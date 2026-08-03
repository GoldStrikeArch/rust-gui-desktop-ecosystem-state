//! "Fetcher" — async & network integration (SPEC-8), vizia 0.4.
//!
//! Architecture notes (research-relevant):
//! - **Runtime**: vizia has *no* executor. Its async story is
//!   `cx.spawn(|proxy| ..)`, which starts a raw OS thread and hands you a
//!   `ContextProxy` that can `emit` events back to the UI thread. So this app
//!   owns a `tokio::runtime::Runtime` explicitly, spawns futures on it, and
//!   moves a `ContextProxy` clone into each task. `ContextProxy` is `Send`
//!   (its `EventProxy` is `Send`), so it travels into `async move` blocks
//!   fine; it is not `Sync`, so each task needs its own clone.
//! - **Debounce + stale protection**: one mechanism does both. Every
//!   keystroke aborts the previous `JoinHandle` and spawns
//!   `sleep(250 ms) -> reqwest`. Aborting during the sleep is the debounce;
//!   aborting after it drops the in-flight reqwest future, which is real
//!   protocol-level cancellation. A generation counter rides along as a
//!   belt-and-braces stale guard and is logged.
//! - **Progress streaming**: `bytes_stream()` + `proxy.emit(Progress(..))`
//!   per chunk. Each emit wakes the vizia event loop through winit's user
//!   event proxy, so the progress bar updates without any polling.
//! - **Cancellation**: `JoinHandle::abort()` drops the `reqwest::Response`
//!   mid-stream, which closes the TCP connection — the server logs
//!   `ABORT /download`, the required proof.
//!
//! With FETCH_SELFTEST=1 the app runs a scripted lifecycle against the local
//! server, prints evidence lines, finishes with `SELFTEST DONE pass=N
//! fail=N` and exits.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use serde::Deserialize;
use tokio::task::JoinHandle;
use vizia::prelude::*;

const DOWNLOAD_SIZE: u64 = 8 * 1024 * 1024;
static SELFTEST: AtomicBool = AtomicBool::new(false);

fn say(line: impl AsRef<str>) {
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{}", line.as_ref());
    let _ = stdout.flush();
}

fn trace(line: impl FnOnce() -> String) {
    if SELFTEST.load(Ordering::Relaxed) {
        say(line());
    }
}

fn main() -> Result<(), ApplicationError> {
    let selftest = std::env::var_os("FETCH_SELFTEST").is_some();
    SELFTEST.store(selftest, Ordering::Relaxed);

    let port = std::env::var("FETCHER_PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or(7878);
    let base = format!("http://127.0.0.1:{port}");

    Application::new(move |cx| {
        cx.add_stylesheet(STYLE).expect("failed to add stylesheet");

        let query = Signal::new(String::new());
        let searching = Signal::new(false);
        let results = Signal::new(Vec::<SearchResult>::new());
        let search_error = Signal::new(None::<String>);
        let download = Signal::new(Download::Idle);
        let flaky = Signal::new(Flaky::Idle);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime");

        // 100 ms tick, only used to drive the scripted self-test.
        let timer = cx.add_timer(Duration::from_millis(100), None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(FetchEvent::SelfTestTick);
            }
        });

        Fetcher {
            base: base.clone(),
            client: reqwest::Client::new(),
            runtime: Arc::new(runtime),
            query,
            searching,
            results,
            search_error,
            download,
            flaky,
            generation: 0,
            search_task: None,
            download_task: None,
            search_completions: 0,
            search_aborts: 0,
            progress_events: 0,
            selftest,
            step: 0,
            ticks: 0,
            pass: 0,
            fail: 0,
        }
        .build(cx);

        if selftest {
            cx.start_timer(timer);
        }

        VStack::new(cx, move |cx| {
            // ---------------- search ----------------
            VStack::new(cx, move |cx| {
                HStack::new(cx, move |cx| {
                    Label::new(cx, "Search").class("section-title");
                    Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                    Label::new(
                        cx,
                        Memo::new(move |_| {
                            if searching.get() {
                                String::from("searching…")
                            } else {
                                format!("{} results", results.get().len())
                            }
                        }),
                    )
                    .class("dim");
                })
                .class("row");

                Textbox::new(cx, query)
                    .class("query")
                    .placeholder("type to search (250 ms debounce)…")
                    .width(Stretch(1.0))
                    .on_edit(|cx, text| cx.emit(FetchEvent::QueryChanged(text)));

                Binding::new(cx, search_error, move |cx| {
                    if let Some(error) = search_error.get() {
                        Label::new(cx, format!("search failed: {error}")).class("error");
                    }
                });

                List::new(cx, results, |cx, _, result| {
                    HStack::new(cx, move |cx| {
                        Label::new(cx, result.map(|r: &SearchResult| format!("#{:03}", r.id)))
                            .class("dim")
                            .width(Pixels(50.0))
                            .hoverable(false);
                        Label::new(cx, result.map(|r: &SearchResult| r.name.clone()))
                            .width(Stretch(1.0))
                            .hoverable(false);
                        Label::new(cx, result.map(|r: &SearchResult| format!("{:.1}", r.score)))
                            .class("dim")
                            .hoverable(false);
                    })
                    .class("result")
                    .hoverable(false);
                })
                .height(Stretch(1.0));
            })
            .class("panel")
            .height(Stretch(1.0));

            // ---------------- download ----------------
            // The panel is rebuilt only when the download *state kind*
            // changes; the bar and the byte counter are bound to signals so
            // 64 progress events do not rebuild 64 view trees.
            let running = Memo::new(move |_| matches!(download.get(), Download::Running { .. }));
            let progress = Memo::new(move |_| match download.get() {
                Download::Running { received, total } => received as f32 / total.max(1) as f32,
                _ => 0.0,
            });
            let bytes_text = Memo::new(move |_| match download.get() {
                Download::Running { received, total } => format!(
                    "{:.1} / {:.1} MiB",
                    received as f64 / 1048576.0,
                    total as f64 / 1048576.0
                ),
                _ => String::new(),
            });
            let idle_text = Memo::new(move |_| download.get().describe());

            VStack::new(cx, move |cx| {
                Label::new(cx, "Download").class("section-title");
                Binding::new(cx, running, move |cx| {
                    if running.get() {
                        HStack::new(cx, move |cx| {
                            ProgressBar::horizontal(cx, progress).width(Stretch(1.0));
                            Label::new(cx, bytes_text).width(Pixels(120.0)).class("dim");
                            Button::new(cx, |cx| Label::new(cx, "Cancel"))
                                .variant(ButtonVariant::Outline)
                                .on_press(|cx| cx.emit(FetchEvent::CancelDownload));
                        })
                        .class("row");
                    } else {
                        HStack::new(cx, move |cx| {
                            Button::new(cx, |cx| Label::new(cx, "Download"))
                                .variant(ButtonVariant::Primary)
                                .on_press(|cx| cx.emit(FetchEvent::StartDownload));
                            Label::new(cx, idle_text).class("dim");
                        })
                        .class("row");
                    }
                });
            })
            .class("panel");

            // ---------------- flaky ----------------
            VStack::new(cx, move |cx| {
                Label::new(cx, "Flaky endpoint").class("section-title");
                Binding::new(cx, flaky, move |cx| {
                    let state = flaky.get();
                    HStack::new(cx, move |cx| {
                        Button::new(cx, |cx| Label::new(cx, "Call /flaky"))
                            .variant(ButtonVariant::Primary)
                            .on_press(|cx| cx.emit(FetchEvent::CallFlaky));
                        match &state {
                            Flaky::Failed { attempts, error } => {
                                Label::new(
                                    cx,
                                    format!("attempt {attempts} failed: {error}"),
                                )
                                .class("error")
                                .width(Stretch(1.0));
                                Button::new(cx, |cx| Label::new(cx, "Retry"))
                                    .variant(ButtonVariant::Outline)
                                    .on_press(|cx| cx.emit(FetchEvent::CallFlaky));
                            }
                            other => {
                                Label::new(cx, other.describe())
                                    .class("dim")
                                    .width(Stretch(1.0));
                            }
                        }
                    })
                    .class("row");
                });
            })
            .class("panel");
        })
        .class("app");
    })
    .title("Fetcher (vizia)")
    .inner_size((700, 560))
    .run()
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct SearchResult {
    id: u64,
    name: String,
    score: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum Download {
    Idle,
    Running { received: u64, total: u64 },
    Done { bytes: u64 },
    Cancelled { received: u64, total: u64 },
    Failed(String),
}

impl Download {
    fn describe(&self) -> String {
        match self {
            Download::Idle => String::from("8 MiB over ~8 s, streamed"),
            Download::Running { received, total } => {
                format!("{received} / {total}")
            }
            Download::Done { bytes } => {
                format!("done — received {:.1} MiB", *bytes as f64 / 1048576.0)
            }
            Download::Cancelled { received, total } => format!(
                "cancelled at {:.1} / {:.1} MiB",
                *received as f64 / 1048576.0,
                *total as f64 / 1048576.0
            ),
            Download::Failed(error) => format!("failed: {error}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Flaky {
    Idle,
    Running { attempts: u32 },
    Failed { attempts: u32, error: String },
    Succeeded { attempts: u32, attempt: u64 },
}

impl Flaky {
    fn describe(&self) -> String {
        match self {
            Flaky::Idle => String::from("fails twice, succeeds on the 3rd"),
            Flaky::Running { attempts } => format!("attempt {attempts}: calling…"),
            Flaky::Failed { attempts, error } => format!("attempt {attempts}: {error}"),
            Flaky::Succeeded { attempts, attempt } => {
                format!("success after {attempts} attempts (server counter {attempt})")
            }
        }
    }
}

struct Fetcher {
    base: String,
    client: reqwest::Client,
    runtime: Arc<tokio::runtime::Runtime>,
    query: Signal<String>,
    searching: Signal<bool>,
    results: Signal<Vec<SearchResult>>,
    search_error: Signal<Option<String>>,
    download: Signal<Download>,
    flaky: Signal<Flaky>,
    generation: u64,
    search_task: Option<JoinHandle<()>>,
    download_task: Option<JoinHandle<()>>,
    search_completions: usize,
    search_aborts: usize,
    progress_events: usize,
    selftest: bool,
    step: usize,
    ticks: usize,
    pass: usize,
    fail: usize,
}

enum FetchEvent {
    QueryChanged(String),
    SearchReady(u64, Result<Vec<SearchResult>, String>),
    StartDownload,
    DownloadProgress(u64, u64),
    DownloadFinished(Result<u64, String>),
    CancelDownload,
    CallFlaky,
    FlakyReady(Result<u64, String>),
    SelfTestTick,
}

#[derive(Deserialize)]
struct Attempt {
    attempt: u64,
}

impl Fetcher {
    fn start_search(&mut self, cx: &mut EventContext) {
        // Debounce + stale protection + real cancellation, all one abort.
        if let Some(handle) = self.search_task.take() {
            handle.abort();
            self.search_aborts += 1;
        }
        self.generation += 1;

        let query = self.query.get();
        if query.is_empty() {
            self.searching.set(false);
            self.results.set(Vec::new());
            self.search_error.set(None);
            return;
        }

        self.searching.set(true);
        let generation = self.generation;
        let client = self.client.clone();
        let base = self.base.clone();
        let mut proxy = cx.get_proxy();

        trace(|| format!("SEARCH_QUEUED gen={generation} q={query:?}"));

        self.search_task = Some(self.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let outcome = async {
                let response = client
                    .get(format!("{base}/search"))
                    .query(&[("q", &query)])
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?;
                response.json::<Vec<SearchResult>>().await.map_err(|e| e.to_string())
            }
            .await;
            let _ = proxy.emit(FetchEvent::SearchReady(generation, outcome));
        }));
    }

    fn start_download(&mut self, cx: &mut EventContext) {
        let client = self.client.clone();
        let url = format!("{}/download", self.base);
        let mut proxy = cx.get_proxy();

        self.progress_events = 0;
        self.download.set(Download::Running { received: 0, total: DOWNLOAD_SIZE });
        trace(|| String::from("DL_START"));

        self.download_task = Some(self.runtime.spawn(async move {
            let outcome = async {
                let response = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?;
                let total = response.content_length().unwrap_or(DOWNLOAD_SIZE);
                let mut received: u64 = 0;
                let mut stream = response.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|e| e.to_string())?;
                    received += chunk.len() as u64;
                    let _ = proxy.emit(FetchEvent::DownloadProgress(received, total));
                }
                Ok(received)
            }
            .await;
            let _ = proxy.emit(FetchEvent::DownloadFinished(outcome));
        }));
    }

    fn call_flaky(&mut self, cx: &mut EventContext) {
        let attempts = match self.flaky.get() {
            Flaky::Failed { attempts, .. } => attempts,
            _ => 0,
        } + 1;
        self.flaky.set(Flaky::Running { attempts });

        let client = self.client.clone();
        let url = format!("{}/flaky", self.base);
        let mut proxy = cx.get_proxy();

        self.runtime.spawn(async move {
            let outcome = async {
                let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(format!("HTTP {status}: {body}"));
                }
                response.json::<Attempt>().await.map(|a| a.attempt).map_err(|e| e.to_string())
            }
            .await;
            let _ = proxy.emit(FetchEvent::FlakyReady(outcome));
        });
    }

    fn check(&mut self, condition: bool, message: impl AsRef<str>) {
        if condition {
            self.pass += 1;
        } else {
            self.fail += 1;
        }
        say(format!(
            "SELFTEST {} {}",
            if condition { "PASS" } else { "FAIL" },
            message.as_ref()
        ));
    }
}

impl Model for Fetcher {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.take(|fetch_event, _| match fetch_event {
            FetchEvent::QueryChanged(text) => {
                self.query.set(text);
                self.start_search(cx);
            }

            FetchEvent::SearchReady(generation, outcome) => {
                let stale = generation != self.generation;
                trace(|| {
                    format!(
                        "SEARCH_READY gen={generation} stale={stale} {}",
                        match &outcome {
                            Ok(results) => format!("n={}", results.len()),
                            Err(error) => format!("err={error:?}"),
                        }
                    )
                });
                if stale {
                    return;
                }
                self.search_completions += 1;
                self.searching.set(false);
                match outcome {
                    Ok(results) => {
                        self.results.set(results);
                        self.search_error.set(None);
                    }
                    Err(error) => {
                        self.results.set(Vec::new());
                        self.search_error.set(Some(error));
                    }
                }
            }

            FetchEvent::StartDownload => self.start_download(cx),

            FetchEvent::DownloadProgress(received, total) => {
                if let Download::Running { .. } = self.download.get() {
                    self.progress_events += 1;
                    self.download.set(Download::Running { received, total });
                    trace(|| format!("DL_PROGRESS {received}/{total}"));
                }
            }

            FetchEvent::DownloadFinished(outcome) => {
                self.download_task = None;
                self.download.set(match outcome {
                    Ok(bytes) => {
                        trace(|| format!("DL_DONE bytes={bytes}"));
                        Download::Done { bytes }
                    }
                    Err(error) => {
                        trace(|| format!("DL_FAILED err={error:?}"));
                        Download::Failed(error)
                    }
                });
            }

            FetchEvent::CancelDownload => {
                if let (Some(handle), Download::Running { received, total }) =
                    (self.download_task.take(), self.download.get())
                {
                    // Dropping the in-flight reqwest Response closes the TCP
                    // connection -> the server logs `ABORT /download`.
                    handle.abort();
                    trace(|| format!("DL_CANCELLED {received}/{total}"));
                    self.download.set(Download::Cancelled { received, total });
                }
            }

            FetchEvent::CallFlaky => self.call_flaky(cx),

            FetchEvent::FlakyReady(outcome) => {
                let attempts = match self.flaky.get() {
                    Flaky::Running { attempts } => attempts,
                    _ => 1,
                };
                self.flaky.set(match outcome {
                    Ok(attempt) => {
                        trace(|| {
                            format!("FLAKY_OK attempts={attempts} server_attempt={attempt}")
                        });
                        Flaky::Succeeded { attempts, attempt }
                    }
                    Err(error) => {
                        trace(|| format!("FLAKY_ERR attempts={attempts} err={error:?}"));
                        Flaky::Failed { attempts, error }
                    }
                });
            }

            FetchEvent::SelfTestTick => {
                if self.selftest {
                    self.ticks += 1;
                    self.drive(cx);
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Scripted lifecycle self-test (FETCH_SELFTEST=1)
//
// A tick-driven state machine. `act` steps do something and advance; `wait`
// steps re-check a predicate every 100 ms until it holds or times out.
// ---------------------------------------------------------------------------

impl Fetcher {
    fn advance(&mut self) {
        self.step += 1;
        self.ticks = 0;
    }

    /// Returns true (and advances) when `ready` holds; fails the run on
    /// timeout so a hang can never be mistaken for a pass.
    fn wait(&mut self, ready: bool, what: &str, timeout_ticks: usize) -> bool {
        if ready {
            self.advance();
            true
        } else {
            if self.ticks > timeout_ticks {
                say(format!("SELFTEST TIMEOUT waiting for {what}"));
                self.fail += 1;
                self.advance();
            }
            false
        }
    }

    fn drive(&mut self, cx: &mut EventContext) {
        match self.step {
            0 => {
                // Reset the server's flaky phase so the probe is deterministic.
                let client = self.client.clone();
                let url = format!("{}/flaky/reset", self.base);
                let health = format!("{}/health", self.base);
                let mut proxy = cx.get_proxy();
                self.runtime.spawn(async move {
                    let _ = client.get(&url).send().await;
                    let ok = match client.get(&health).send().await {
                        Ok(response) => response.text().await.unwrap_or_default() == "ok",
                        Err(_) => false,
                    };
                    let _ = proxy.emit(FetchEvent::SearchReady(
                        u64::MAX,
                        if ok { Ok(Vec::new()) } else { Err(String::from("health failed")) },
                    ));
                    say(format!("HEALTH ok={ok}"));
                });
                self.advance();
            }
            1 => {
                if self.wait(self.ticks > 5, "server health", 60) {
                    self.check(true, "server /health reachable (see HEALTH line)");
                    // Debounce probe: six keystrokes inside one 250 ms window.
                    self.search_completions = 0;
                    for prefix in ["a", "am", "amb", "ambe", "amber"] {
                        self.query.set(String::from(prefix));
                        self.start_search(cx);
                    }
                }
            }
            2 => {
                if self.wait(self.search_completions >= 1, "search results", 60) {
                    let results = self.results.get();
                    self.check(!results.is_empty(), format!("search \"amber\" -> {} results", results.len()));
                    self.check(
                        results.iter().all(|result| result.name.contains("amber")),
                        "all results match the query",
                    );
                    self.check(
                        self.search_completions == 1,
                        format!(
                            "debounce: 5 keystrokes -> {} completed request(s), {} aborted",
                            self.search_completions, self.search_aborts
                        ),
                    );
                }
            }
            3 => {
                // Stale protection: fire A, then B while A is on the wire.
                self.search_completions = 0;
                self.query.set(String::from("mossy"));
                self.start_search(cx);
                self.advance();
            }
            4 => {
                // ~400 ms later: A's 250 ms debounce has elapsed and its
                // request is in flight (the server's delay for "mossy" is
                // deterministic and > 150 ms).
                if self.ticks >= 4 {
                    self.query.set(String::from("prism"));
                    self.start_search(cx);
                    self.advance();
                }
            }
            5 => {
                if self.wait(self.search_completions >= 1, "query B results", 80) {
                    let results = self.results.get();
                    self.check(
                        !results.is_empty()
                            && results.iter().all(|result| result.name.contains("prism"))
                            && self.search_completions == 1,
                        format!(
                            "stale protection: newer query's {} results shown and the \
                             in-flight older request never delivered ({} completion)",
                            results.len(),
                            self.search_completions
                        ),
                    );
                    cx.emit(FetchEvent::StartDownload);
                }
            }
            6 => {
                let received = match self.download.get() {
                    Download::Running { received, .. } => received,
                    _ => 0,
                };
                if self.wait(received > 1_500_000, "download past 1.5 MiB", 150) {
                    self.check(
                        self.progress_events >= 5,
                        format!("progress streamed incrementally ({} chunks)", self.progress_events),
                    );
                    cx.emit(FetchEvent::CancelDownload);
                }
            }
            7 => {
                if let Download::Cancelled { received, total } = self.download.get() {
                    self.check(
                        received < total,
                        format!(
                            "cancelled at {:.1} of {:.1} MiB (server should log ABORT)",
                            received as f64 / 1048576.0,
                            total as f64 / 1048576.0
                        ),
                    );
                    say("SELFTEST CANCELLED_DOWNLOAD");
                    self.advance();
                } else if self.ticks > 40 {
                    self.wait(false, "cancelled state", 40);
                }
            }
            8 => {
                // Give the server a moment to log ABORT, then start /flaky.
                if self.ticks >= 7 {
                    cx.emit(FetchEvent::CallFlaky);
                    self.advance();
                }
            }
            9 => {
                let state = self.flaky.get();
                if self.wait(!matches!(state, Flaky::Running { .. }), "flaky response", 60) {
                    self.check(
                        matches!(state, Flaky::Failed { .. }),
                        format!("flaky first call shows an error state: {}", state.describe()),
                    );
                    cx.emit(FetchEvent::CallFlaky);
                }
            }
            10 => {
                let state = self.flaky.get();
                if self.wait(!matches!(state, Flaky::Running { .. }), "flaky retry", 60) {
                    match state {
                        Flaky::Succeeded { .. } => {}
                        _ => cx.emit(FetchEvent::CallFlaky),
                    }
                    if matches!(state, Flaky::Succeeded { .. }) {
                        self.step = 11;
                    } else {
                        self.step = 10;
                    }
                    self.ticks = 0;
                }
            }
            11 => {
                let state = self.flaky.get();
                self.check(
                    matches!(state, Flaky::Succeeded { .. }),
                    format!("flaky retry UX reached success: {}", state.describe()),
                );
                cx.emit(FetchEvent::StartDownload);
                self.advance();
            }
            12 => {
                let done = matches!(self.download.get(), Download::Done { .. });
                if self.wait(done, "full download", 200) {
                    if let Download::Done { bytes } = self.download.get() {
                        self.check(
                            bytes == DOWNLOAD_SIZE,
                            format!(
                                "full download {:.1} MiB in {} progress events",
                                bytes as f64 / 1048576.0,
                                self.progress_events
                            ),
                        );
                    }
                }
            }
            _ => {
                say(format!("SELFTEST DONE pass={} fail={}", self.pass, self.fail));
                std::process::exit(if self.fail == 0 { 0 } else { 1 });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.app { width: 1s; height: 1s; padding: 12px; vertical-gap: 10px; }

.panel {
    width: 1s;
    height: auto;
    padding: 10px;
    vertical-gap: 8px;
    background-color: #ffffff0e;
    border-width: 1px;
    border-color: #ffffff20;
    corner-radius: 8px;
}

.section-title { height: auto; font-size: 14px; }
.row { height: auto; horizontal-gap: 10px; alignment: center; }
.dim { height: auto; font-size: 12px; color: #8a8a8a; }
.error { height: auto; font-size: 12px; color: #e06363; }
.query { font-size: 13px; }
.result { height: auto; horizontal-gap: 8px; padding: 2px 6px; font-size: 13px; }
"#;
