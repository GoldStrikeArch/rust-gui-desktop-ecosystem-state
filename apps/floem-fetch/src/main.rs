//! "Fetcher" — async & network integration (SPEC-8), floem git @ 778bb5f2.
//!
//! Architecture notes (research-relevant):
//! - Runtime: floem has NO executor. The sanctioned pattern (upstream
//!   tokio-timer example) is `Runtime::new()` +
//!   `block_on(block_in_place(floem::launch))`, after which `tokio::spawn`
//!   works from UI closures. reqwest futures run entirely on tokio worker
//!   threads; completions cross back with `create_ext_action` (floem's
//!   foreign-thread → reactive-graph wakeup), streamed download progress
//!   with `update_signal_from_channel`.
//! - Debounce: floem's BUILT-IN `debounce_action(signal, 250ms, action)`.
//! - Stale protection is REAL cancellation: each new search aborts the
//!   previous tokio task via its `AbortHandle`, dropping the in-flight
//!   reqwest future (the server logs the search as cancelled). A generation
//!   counter stays on as a belt-and-braces guard (`stale=` evidence lines).
//! - Cancel download = `AbortHandle::abort()` → the reqwest `Response` is
//!   dropped mid-`bytes_stream` → TCP close → the server logs
//!   `ABORT /download` (the required proof of non-UI cancellation).
//! - `FETCH_SELFTEST=1` runs a scripted request-lifecycle pass against the
//!   local server through the SAME closures the UI calls, prints evidence
//!   lines and `SELFTEST DONE pass=N fail=M`, then exits.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use floem::Application;
use floem::action::{debounce_action, exec_after};
use floem::ext_event::{create_ext_action, update_signal_from_channel};
use floem::kurbo::Size;
use floem::prelude::*;
use floem::reactive::{Effect, Scope, WriteSignal};
use floem::window::WindowConfig;
use futures::StreamExt;
use serde::Deserialize;
use tokio::task::AbortHandle;

const DOWNLOAD_SIZE: u64 = 8 * 1024 * 1024; // fallback if no Content-Length

static SELFTEST: AtomicBool = AtomicBool::new(false);

fn trace(line: impl FnOnce() -> String) {
    if SELFTEST.load(Ordering::Relaxed) {
        println!("{}", line());
    }
}

const PANEL_BG: Color = Color::from_rgb8(0xf4, 0xf4, 0xf6);
const BORDER: Color = Color::from_rgb8(0xc9, 0xc9, 0xd2);
const GREEN: Color = Color::from_rgb8(0x1d, 0x7a, 0x33);
const AMBER: Color = Color::from_rgb8(0xc9, 0x8a, 0x0b);
const RED: Color = Color::from_rgb8(0xc2, 0x33, 0x2e);
const ACCENT: Color = Color::from_rgb8(0x3b, 0x6f, 0xe0);
const TEXT_DIM: Color = Color::from_rgb8(0x70, 0x70, 0x7a);

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct SearchResult {
    id: u64,
    name: String,
    score: f64,
}

#[derive(Clone, PartialEq)]
enum Download {
    Idle,
    Running { received: u64, total: u64 },
    Done { mib: f64 },
    Cancelled { received: u64, total: u64 },
    Failed(String),
}

#[derive(Clone, PartialEq)]
enum Flaky {
    Idle,
    Running { attempts: u32 },
    Failed { attempts: u32, error: String },
    Succeeded { attempts: u32, attempt: u64 },
}

#[derive(Clone, Copy)]
struct Fetcher {
    query: RwSignal<String>,
    searching: RwSignal<bool>,
    results: RwSignal<Vec<SearchResult>>,
    search_error: RwSignal<Option<String>>,
    generation: RwSignal<u64>,
    search_abort: RwSignal<Option<AbortHandle>>,
    queued_count: RwSignal<u64>,
    stale_seen: RwSignal<u64>,
    download: RwSignal<Download>,
    download_abort: RwSignal<Option<AbortHandle>>,
    flaky: RwSignal<Flaky>,
}

fn base_url() -> String {
    let port = std::env::var("FETCHER_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(7878);
    format!("http://127.0.0.1:{port}")
}

fn client() -> reqwest::Client {
    // One shared client (connection pool) for the whole app.
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new).clone()
}

impl Fetcher {
    fn new() -> Self {
        Self {
            query: RwSignal::new(String::new()),
            searching: RwSignal::new(false),
            results: RwSignal::new(Vec::new()),
            search_error: RwSignal::new(None),
            generation: RwSignal::new(0),
            search_abort: RwSignal::new(None),
            queued_count: RwSignal::new(0),
            stale_seen: RwSignal::new(0),
            download: RwSignal::new(Download::Idle),
            download_abort: RwSignal::new(None),
            flaky: RwSignal::new(Flaky::Idle),
        }
    }

    /// Fire a /search request for the current query. Called by the debounced
    /// query watcher AND directly by the self-test's stale-protection probe.
    fn start_search(&self) {
        // Real cancellation: abort the previous task whether it is still
        // waiting or already on the wire (drops the reqwest future).
        if let Some(handle) = self.search_abort.get_untracked() {
            handle.abort();
        }
        let generation = self.generation.get_untracked() + 1;
        self.generation.set(generation);

        let query = self.query.get_untracked();
        if query.is_empty() {
            self.searching.set(false);
            self.results.update(|r| r.clear());
            self.search_error.set(None);
            return;
        }

        self.searching.set(true);
        self.queued_count.update(|c| *c += 1);
        trace(|| format!("SEARCH_QUEUED gen={generation} q={query:?}"));

        let this = *self;
        let send = create_ext_action(
            Scope::new(),
            move |result: Result<Vec<SearchResult>, String>| {
                this.search_ready(generation, result);
            },
        );

        let handle = tokio::spawn(async move {
            let result = async {
                client()
                    .get(format!("{}/search", base_url()))
                    .query(&[("q", &query)])
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json::<Vec<SearchResult>>()
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            send(result);
        })
        .abort_handle();
        self.search_abort.set(Some(handle));
    }

    fn search_ready(&self, generation: u64, result: Result<Vec<SearchResult>, String>) {
        // Belt-and-braces stale guard; with task abortion this should never
        // fire for an old generation (logged as evidence either way).
        let stale = generation != self.generation.get_untracked();
        trace(|| {
            format!(
                "SEARCH_READY gen={generation} stale={stale} {}",
                match &result {
                    Ok(results) => format!("n={}", results.len()),
                    Err(error) => format!("err={error:?}"),
                }
            )
        });
        if stale {
            self.stale_seen.update(|s| *s += 1);
            return;
        }

        self.searching.set(false);
        match result {
            Ok(results) => {
                self.results.set(results);
                self.search_error.set(None);
            }
            Err(error) => {
                self.results.update(|r| r.clear());
                self.search_error.set(Some(error));
            }
        }
    }

    fn start_download(&self, progress_writer: WriteSignal<Option<(u64, u64)>>) {
        trace(|| String::from("DL_START"));
        self.download.set(Download::Running { received: 0, total: DOWNLOAD_SIZE });

        let this = *self;
        let done = create_ext_action(Scope::new(), move |result: Result<u64, String>| {
            this.download_abort.set(None);
            this.download.set(match result {
                Ok(bytes) => {
                    trace(|| format!("DL_DONE bytes={bytes}"));
                    Download::Done { mib: bytes as f64 / (1024.0 * 1024.0) }
                }
                Err(error) => {
                    trace(|| format!("DL_FAILED err={error:?}"));
                    Download::Failed(error)
                }
            });
        });

        // Progress crosses threads through a plain std channel wired into a
        // signal (floem's update_signal_from_channel disposes itself when
        // the sender drops — including on abort).
        let (tx, rx) = std::sync::mpsc::channel::<(u64, u64)>();
        update_signal_from_channel(progress_writer, rx);

        let handle = tokio::spawn(async move {
            let result = async {
                let response = client()
                    .get(format!("{}/download", base_url()))
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
                    let _ = tx.send((received, total));
                }
                Ok(received)
            }
            .await;
            done(result);
        })
        .abort_handle();
        self.download_abort.set(Some(handle));
    }

    fn cancel_download(&self) {
        if let Download::Running { received, total } = self.download.get_untracked()
            && let Some(handle) = self.download_abort.get_untracked()
        {
            // Aborting the task drops the in-flight reqwest Response ->
            // TCP close -> the server logs `ABORT /download`.
            handle.abort();
            self.download_abort.set(None);
            trace(|| format!("DL_CANCELLED {received}/{total}"));
            self.download.set(Download::Cancelled { received, total });
        }
    }

    fn call_flaky(&self) {
        let attempts = match self.flaky.get_untracked() {
            Flaky::Failed { attempts, .. } => attempts,
            _ => 0,
        } + 1;
        self.flaky.set(Flaky::Running { attempts });

        #[derive(Deserialize)]
        struct Attempt {
            attempt: u64,
        }

        let this = *self;
        let send = create_ext_action(Scope::new(), move |result: Result<u64, String>| {
            this.flaky.set(match result {
                Ok(attempt) => {
                    trace(|| format!("FLAKY_OK attempts={attempts} server_attempt={attempt}"));
                    Flaky::Succeeded { attempts, attempt }
                }
                Err(error) => {
                    trace(|| format!("FLAKY_ERR attempts={attempts} err={error:?}"));
                    Flaky::Failed { attempts, error }
                }
            });
        });

        tokio::spawn(async move {
            let result = async {
                let response = client()
                    .get(format!("{}/flaky", base_url()))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(format!("HTTP {status}: {body}"));
                }
                response
                    .json::<Attempt>()
                    .await
                    .map(|a| a.attempt)
                    .map_err(|e| e.to_string())
            }
            .await;
            send(result);
        });
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

fn main() {
    SELFTEST.store(std::env::var_os("FETCH_SELFTEST").is_some(), Ordering::Relaxed);

    // Multi-threaded runtime is required: the main thread is not a real
    // tokio task, and reqwest needs a live reactor (upstream tokio-timer
    // example pattern).
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        tokio::task::block_in_place(|| {
            Application::new()
                .window(
                    |_| app_view(),
                    Some(
                        WindowConfig::default()
                            .title("Fetcher (floem)")
                            .size(Size::new(700.0, 560.0)),
                    ),
                )
                .run()
        })
    });
}

fn app_view() -> impl IntoView {
    let fetcher = Fetcher::new();

    // Download progress lands in this signal from the channel thread.
    let progress: RwSignal<Option<(u64, u64)>> = RwSignal::new(None);
    Effect::new(move |_| {
        if let Some((received, total)) = progress.get() {
            if matches!(fetcher.download.get_untracked(), Download::Running { .. }) {
                trace(|| format!("DL_PROGRESS {received}/{total}"));
                fetcher.download.set(Download::Running { received, total });
            }
        }
    });

    // 250 ms debounce on the query signal — floem built-in.
    debounce_action(fetcher.query, Duration::from_millis(250), move || {
        fetcher.start_search();
    });

    if SELFTEST.load(Ordering::Relaxed) {
        selftest(fetcher, progress);
    }

    Stack::vertical((
        search_section(fetcher),
        download_section(fetcher, progress),
        flaky_section(fetcher),
    ))
    .style(|s| s.flex_col().gap(12.0).padding(14.0).size_full())
}

fn section_style(s: floem::style::Style) -> floem::style::Style {
    s.padding(10.0)
        .width_full()
        .background(PANEL_BG)
        .border(1.0)
        .border_color(BORDER)
        .border_radius(8.0)
}

fn search_section(fetcher: Fetcher) -> impl IntoView {
    let header = Stack::horizontal((
        Label::new("Search").style(|s| s.font_size(15.0)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Label::derived(move || {
            if fetcher.searching.get() {
                String::from("searching…")
            } else {
                format!("{} results", fetcher.results.with(|r| r.len()))
            }
        })
        .style(|s| s.font_size(12.0).color(TEXT_DIM)),
    ))
    .style(|s| s.items_center().width_full());

    let input = TextInput::new(fetcher.query)
        .placeholder("type to search (250 ms debounce)…")
        .style(|s| s.width_full().padding(8.0));

    let list = dyn_stack(
        move || fetcher.results.get(),
        |result| result.id,
        move |result| {
            Stack::horizontal((
                Label::new(format!("#{:03}", result.id))
                    .style(|s| s.font_size(13.0).color(TEXT_DIM).width(50.0)),
                Label::new(result.name).style(|s| s.font_size(13.0)),
                Empty::new().style(|s| s.flex_grow(1.0)),
                Label::new(format!("{:.1}", result.score))
                    .style(|s| s.font_size(13.0).color(TEXT_DIM)),
            ))
            .style(|s| s.gap(8.0).padding_vert(2.0).padding_horiz(6.0).width_full())
        },
    )
    .style(|s| s.flex_col().width_full());

    let error = dyn_container(
        move || fetcher.search_error.get(),
        move |error| match error {
            Some(error) => Label::new(format!("search failed: {error}"))
                .style(|s| s.font_size(13.0).color(RED))
                .into_any(),
            None => Empty::new().into_any(),
        },
    );

    Stack::vertical((
        header,
        input,
        error,
        list.scroll().style(|s| s.width_full().flex_grow(1.0).min_height(0.0)),
    ))
    .style(|s| {
        section_style(s.flex_col().gap(8.0))
            .flex_grow(1.0)
            .min_height(0.0)
    })
}

fn download_section(fetcher: Fetcher, progress: RwSignal<Option<(u64, u64)>>) -> impl IntoView {
    const MIB: f64 = 1024.0 * 1024.0;

    let controls = dyn_container(
        move || fetcher.download.get(),
        move |download| match download {
            Download::Running { received, total } => {
                let ratio = received as f64 / total as f64;
                Stack::horizontal((
                    // Progress bar: a filled track (no built-in progress bar
                    // widget in floem — two nested styled views).
                    Empty::new()
                        .style(move |s| {
                            s.height(16.0)
                                .width_pct(ratio * 100.0)
                                .background(ACCENT)
                                .border_radius(4.0)
                        })
                        .container()
                        .style(|s| {
                            s.height(16.0)
                                .flex_grow(1.0)
                                .background(BORDER.with_alpha(0.5))
                                .border_radius(4.0)
                        }),
                    Label::new(format!(
                        "{:.1} / {:.1} MiB",
                        received as f64 / MIB,
                        total as f64 / MIB
                    ))
                    .style(|s| s.font_size(13.0).width(110.0)),
                    Button::new("Cancel").action(move || fetcher.cancel_download()),
                ))
                .style(|s| s.gap(10.0).items_center().width_full())
                .into_any()
            }
            state => {
                let label = match state {
                    Download::Idle => Label::new("8 MiB over ~8 s, streamed")
                        .style(|s| s.font_size(13.0).color(TEXT_DIM)),
                    Download::Done { mib } => Label::new(format!("done — received {mib:.1} MiB"))
                        .style(|s| s.font_size(13.0).color(GREEN)),
                    Download::Cancelled { received, total } => Label::new(format!(
                        "cancelled at {:.1} / {:.1} MiB",
                        received as f64 / MIB,
                        total as f64 / MIB
                    ))
                    .style(|s| s.font_size(13.0).color(AMBER)),
                    Download::Failed(error) => Label::new(format!("failed: {error}"))
                        .style(|s| s.font_size(13.0).color(RED)),
                    Download::Running { .. } => unreachable!(),
                };
                Stack::horizontal((
                    Button::new("Download").action(move || fetcher.start_download(progress.write_only())),
                    label,
                ))
                .style(|s| s.gap(10.0).items_center())
                .into_any()
            }
        },
    );

    Stack::vertical((Label::new("Download").style(|s| s.font_size(15.0)), controls))
        .style(|s| section_style(s.flex_col().gap(8.0)))
}

fn flaky_section(fetcher: Fetcher) -> impl IntoView {
    let status = dyn_container(
        move || fetcher.flaky.get(),
        move |flaky| match flaky {
            Flaky::Idle => Label::new("fails twice, succeeds on the 3rd")
                .style(|s| s.font_size(13.0).color(TEXT_DIM))
                .into_any(),
            Flaky::Running { attempts } => Label::new(format!("attempt {attempts}: calling…"))
                .style(|s| s.font_size(13.0).color(TEXT_DIM))
                .into_any(),
            Flaky::Failed { attempts, error } => Stack::horizontal((
                Label::new(format!("attempt {attempts} failed: {error}"))
                    .style(|s| s.font_size(13.0).color(RED)),
                Button::new("Retry").action(move || fetcher.call_flaky()),
            ))
            .style(|s| s.gap(10.0).items_center())
            .into_any(),
            Flaky::Succeeded { attempts, attempt } => Label::new(format!(
                "success after {attempts} attempts (server counter {attempt})"
            ))
            .style(|s| s.font_size(13.0).color(GREEN))
            .into_any(),
        },
    );

    Stack::vertical((
        Label::new("Flaky endpoint").style(|s| s.font_size(15.0)),
        Stack::horizontal((
            Button::new("Call /flaky").action(move || {
                if !matches!(fetcher.flaky.get_untracked(), Flaky::Running { .. }) {
                    fetcher.call_flaky();
                }
            }),
            status,
        ))
        .style(|s| s.gap(10.0).items_center()),
    ))
    .style(|s| section_style(s.flex_col().gap(8.0)))
}

// ---------------------------------------------------------------------------
// Scripted self-test (FETCH_SELFTEST=1): 10 checks, then exit.
// ---------------------------------------------------------------------------

static PASS: AtomicUsize = AtomicUsize::new(0);
static FAIL: AtomicUsize = AtomicUsize::new(0);

fn check(name: &str, ok: bool) {
    if ok {
        PASS.fetch_add(1, Ordering::Relaxed);
    } else {
        FAIL.fetch_add(1, Ordering::Relaxed);
        println!("CHECK-FAIL {name}");
    }
}

fn after(ms: u64, f: impl FnOnce() + 'static) {
    exec_after(Duration::from_millis(ms), move |_| f());
}

fn selftest(fetcher: Fetcher, progress: RwSignal<Option<(u64, u64)>>) {
    // Track progress monotonicity for check 5.
    let progress_events: RwSignal<Vec<u64>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        if let Some((received, _)) = progress.get() {
            progress_events.update(|v| v.push(received));
        }
    });

    // t=0.5s: health + flaky reset (serial determinism).
    after(500, move || {
        let health: RwSignal<Option<bool>> = RwSignal::new(None);
        let send = create_ext_action(Scope::new(), move |ok: bool| health.set(Some(ok)));
        tokio::spawn(async move {
            let ok = async {
                let body = client()
                    .get(format!("{}/health", base_url()))
                    .send()
                    .await
                    .ok()?
                    .text()
                    .await
                    .ok()?;
                let _ = client().get(format!("{}/flaky/reset", base_url())).send().await;
                Some(body == "ok")
            }
            .await
            .unwrap_or(false);
            send(ok);
        });
        after(700, move || {
            let ok = health.get_untracked() == Some(true);
            println!("HEALTH {}", if ok { "ok" } else { "FAILED" });
            check("health ok", ok);
        });
    });

    // t=2.0s: debounce — two keystrokes 100 ms apart → ONE request.
    after(2_000, move || fetcher.query.set(String::from("am")));
    after(2_100, move || fetcher.query.set(String::from("amb")));
    after(3_300, move || {
        check("debounce collapsed burst to 1 request", fetcher.queued_count.get_untracked() == 1);
        let ok = fetcher.results.with_untracked(|r| {
            !r.is_empty() && r.iter().all(|res| res.name.contains("amb"))
        }) && !fetcher.searching.get_untracked();
        check("results delivered for final query", ok);
    });

    // t=4.0s: stale protection — two searches fired back-to-back (bypassing
    // the debounce), the first aborted MID-FLIGHT by the second.
    after(4_000, move || {
        fetcher.query.set(String::from("co"));
        fetcher.start_search();
        after(60, move || {
            fetcher.query.set(String::from("br"));
            fetcher.start_search();
        });
    });
    after(5_200, move || {
        let ok = fetcher
            .results
            .with_untracked(|r| !r.is_empty() && r.iter().all(|res| res.name.contains("br")))
            && fetcher.stale_seen.get_untracked() == 0;
        check("stale protection: latest query wins, no stale applied", ok);
    });

    // t=6.0s: download + cancel at ~1.6s (~1.6 MiB of 8 MiB).
    after(6_000, move || fetcher.start_download(progress.write_only()));
    after(7_600, move || {
        let events = progress_events.get_untracked();
        check(
            "progress streamed monotonically",
            events.len() >= 2 && events.windows(2).all(|w| w[0] <= w[1]),
        );
        fetcher.cancel_download();
        let ok = matches!(
            fetcher.download.get_untracked(),
            Download::Cancelled { received, total } if received > 0 && received < total
        );
        check("cancelled mid-stream", ok);
        let at_cancel = progress_events.with_untracked(|v| v.len());
        after(700, move || {
            check(
                "no progress after cancel",
                progress_events.with_untracked(|v| v.len()) == at_cancel,
            );
        });
    });

    // t=9.0s: full download to completion (~8 s).
    after(9_000, move || fetcher.start_download(progress.write_only()));
    after(19_000, move || {
        let ok = matches!(
            fetcher.download.get_untracked(),
            Download::Done { mib } if (mib - 8.0).abs() < 0.01
        );
        check("full download completed with all 8 MiB", ok);

        // Flaky: 500, 500, then success (server counter cycle of 3,
        // /flaky/reset was called during the health step).
        fetcher.call_flaky();
        after(800, move || {
            let first_failed =
                matches!(fetcher.flaky.get_untracked(), Flaky::Failed { attempts: 1, .. });
            fetcher.call_flaky();
            after(800, move || {
                let second_failed =
                    matches!(fetcher.flaky.get_untracked(), Flaky::Failed { attempts: 2, .. });
                check("flaky failed on attempts 1 and 2", first_failed && second_failed);
                fetcher.call_flaky();
                after(800, move || {
                    check(
                        "flaky succeeded on attempt 3",
                        matches!(
                            fetcher.flaky.get_untracked(),
                            Flaky::Succeeded { attempts: 3, .. }
                        ),
                    );
                    println!(
                        "SELFTEST DONE pass={} fail={}",
                        PASS.load(Ordering::Relaxed),
                        FAIL.load(Ordering::Relaxed)
                    );
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    std::process::exit(if FAIL.load(Ordering::Relaxed) == 0 { 0 } else { 1 });
                });
            });
        });
    });
}
