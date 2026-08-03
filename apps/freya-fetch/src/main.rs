//! "Fetcher" — async & network integration (SPEC-8), Freya 0.4.
//!
//! Architecture notes (research-relevant):
//! - Runtime: Freya has its own single-threaded executor. The documented Tokio
//!   pattern (`_docs::tokio_integration`) is `Builder::new_multi_thread()` +
//!   `let _guard = rt.enter();` before `launch`, then keep using **Freya's**
//!   `spawn`. The futures are polled on the UI thread with the Tokio reactor
//!   available, so reqwest works and completions mutate signals *directly* —
//!   no channel, no `Send` bound, no foreign-thread wakeup shim.
//! - Debounce: hand-rolled. Freya has no debounce helper, but `spawn` returns a
//!   `TaskHandle` with `.cancel()`, so "cancel the pending task, spawn a new
//!   one that sleeps 250 ms first" is 5 lines.
//! - Stale protection is **real cancellation**: a new search cancels the
//!   previous task, which drops the in-flight reqwest future (the server logs
//!   the search as cancelled). A generation counter stays on as a
//!   belt-and-braces guard and produces the `stale=` evidence lines.
//! - Cancel download = `TaskHandle::cancel()` → the reqwest `Response` is
//!   dropped mid-`bytes_stream` → TCP close → the server logs `ABORT /download`.
//! - `FETCH_SELFTEST=1` runs a scripted request-lifecycle pass against the
//!   local server through the SAME functions the UI calls, prints evidence
//!   lines and `SELFTEST DONE pass=N fail=M`, then exits.

use std::{
    rc::Rc,
    time::Duration,
};

use freya::prelude::*;
use futures_util::StreamExt;
use serde::Deserialize;

const DEBOUNCE: Duration = Duration::from_millis(250);

const BG: Color = Color::from_argb(255, 251, 251, 252);
const PANEL: Color = Color::WHITE;
const TEXT: Color = Color::from_argb(255, 26, 28, 33);
const MUTED: Color = Color::from_argb(255, 108, 115, 128);
const ACCENT: Color = Color::from_argb(255, 46, 112, 226);
const ERROR: Color = Color::from_argb(255, 176, 42, 42);
const LINE: Color = Color::from_argb(255, 224, 227, 232);

fn base_url() -> String {
    let port = std::env::var("FETCHER_PORT").unwrap_or_else(|_| String::from("7878"));
    format!("http://127.0.0.1:{port}")
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct SearchResult {
    id: u32,
    name: String,
    score: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum Download {
    Idle,
    Running { received: u64, total: u64 },
    Cancelled { received: u64, total: u64 },
    Done { mib: f64 },
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Flaky {
    Idle,
    Running,
    Failed { attempts: u32, error: String },
    Succeeded { attempts: u32, server_attempt: u32 },
}

// ---------------------------------------------------------------- model

#[derive(Clone, Copy)]
struct Fetcher {
    client: State<reqwest::Client>,
    query: State<String>,
    results: State<Vec<SearchResult>>,
    searching: State<bool>,
    search_error: State<Option<String>>,
    generation: State<u64>,
    queued_count: State<u32>,
    stale_seen: State<u32>,
    search_task: State<Option<TaskHandle>>,
    download: State<Download>,
    download_task: State<Option<TaskHandle>>,
    flaky: State<Flaky>,
    flaky_attempts: State<u32>,
    health: State<Option<bool>>,
    selftest: bool,
}

impl Fetcher {
    fn log(&self, line: String) {
        if self.selftest {
            println!("{line}");
        }
    }

    /// Debounced search: cancel whatever is pending and re-arm.
    fn queue_search(&self) {
        let mut this = *self;
        if let Some(task) = this.search_task.peek().clone() {
            task.try_cancel();
        }
        let handle = spawn(async move {
            tokio::time::sleep(DEBOUNCE).await;
            this.run_search().await;
        });
        this.search_task.set(Some(handle));
    }

    async fn run_search(mut self) {
        let query = self.query.peek().clone();
        let generation = *self.generation.peek() + 1;
        self.generation.set(generation);
        // NOTE: `x.set(*x.peek() + 1)` panics — the `ReadRef` returned by
        // `peek()` is an argument temporary and is therefore still alive while
        // `set()` takes the write borrow. Always land the value first.
        let queued = *self.queued_count.peek() + 1;
        self.queued_count.set(queued);
        self.log(format!("SEARCH_QUEUED gen={generation} q={query:?}"));

        if query.trim().is_empty() {
            self.results.set(Vec::new());
            self.searching.set(false);
            return;
        }

        self.searching.set(true);
        self.search_error.set(None);

        let client = self.client.peek().clone();
        let response = client
            .get(format!("{}/search", base_url()))
            .query(&[("q", query.as_str())])
            .send()
            .await
            .and_then(|r| r.error_for_status());

        // A cancelled task never gets here — the future is dropped at an await
        // point — so `stale` is a belt-and-braces guard, not the mechanism.
        let stale = generation != *self.generation.peek();
        match response {
            Ok(response) => match response.json::<Vec<SearchResult>>().await {
                Ok(results) => {
                    self.log(format!(
                        "SEARCH_READY gen={generation} stale={stale} n={}",
                        results.len()
                    ));
                    if stale {
                        let seen = *self.stale_seen.peek() + 1;
                        self.stale_seen.set(seen);
                        return;
                    }
                    self.results.set(results);
                    self.searching.set(false);
                }
                Err(error) => {
                    self.search_error.set(Some(error.to_string()));
                    self.searching.set(false);
                }
            },
            Err(error) => {
                if !stale {
                    self.search_error.set(Some(error.to_string()));
                    self.searching.set(false);
                }
            }
        }
    }

    fn start_download(&self) {
        let mut this = *self;
        if let Some(task) = this.download_task.peek().clone() {
            task.try_cancel();
        }
        this.download.set(Download::Running {
            received: 0,
            total: 0,
        });
        this.log(String::from("DL_START"));

        let handle = spawn(async move {
            let client = this.client.peek().clone();
            let response = match client
                .get(format!("{}/download", base_url()))
                .send()
                .await
                .and_then(|r| r.error_for_status())
            {
                Ok(response) => response,
                Err(error) => {
                    this.download.set(Download::Failed(error.to_string()));
                    return;
                }
            };

            let total = response.content_length().unwrap_or(0);
            let mut received = 0u64;
            let mut stream = response.bytes_stream();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        received += bytes.len() as u64;
                        this.download.set(Download::Running { received, total });
                        this.log(format!("DL_PROGRESS {received}/{total}"));
                    }
                    Err(error) => {
                        this.download.set(Download::Failed(error.to_string()));
                        return;
                    }
                }
            }

            this.log(format!("DL_DONE bytes={received}"));
            this.download.set(Download::Done {
                mib: received as f64 / (1024.0 * 1024.0),
            });
        });
        this.download_task.set(Some(handle));
    }

    /// Cancel by dropping the task: the `Response` goes with it, the TCP
    /// connection closes, and the server logs `ABORT /download`.
    fn cancel_download(&self) {
        let mut this = *self;
        if let Some(task) = this.download_task.peek().clone() {
            task.try_cancel();
        }
        this.download_task.set(None);
        let current = this.download.peek().clone();
        if let Download::Running { received, total } = current {
            this.download.set(Download::Cancelled { received, total });
            this.log(format!("DL_CANCELLED {received}/{total}"));
        }
    }

    fn call_flaky(&self) {
        let mut this = *self;
        let attempts = *this.flaky_attempts.peek() + 1;
        this.flaky_attempts.set(attempts);
        this.flaky.set(Flaky::Running);

        spawn(async move {
            let client = this.client.peek().clone();
            let result = client.get(format!("{}/flaky", base_url())).send().await;
            match result {
                Ok(response) if response.status().is_success() => {
                    #[derive(Deserialize)]
                    struct Attempt {
                        attempt: u32,
                    }
                    let server_attempt = response
                        .json::<Attempt>()
                        .await
                        .map(|a| a.attempt)
                        .unwrap_or(0);
                    this.log(format!(
                        "FLAKY_OK attempts={attempts} server_attempt={server_attempt}"
                    ));
                    this.flaky.set(Flaky::Succeeded {
                        attempts,
                        server_attempt,
                    });
                }
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    let error = format!("HTTP {status}: {}", body.trim());
                    this.log(format!("FLAKY_ERR attempts={attempts} err={error:?}"));
                    this.flaky.set(Flaky::Failed { attempts, error });
                }
                Err(error) => {
                    let error = error.to_string();
                    this.log(format!("FLAKY_ERR attempts={attempts} err={error:?}"));
                    this.flaky.set(Flaky::Failed { attempts, error });
                }
            }
        });
    }
}

// ---------------------------------------------------------------- main

fn main() {
    // Debug builds only: Freya installs a release-mode panic hook that shows a
    // modal "Fatal Error" dialog and exits *before* chaining to the previous
    // hook, so panics never reach stderr in a release run.
    #[cfg(debug_assertions)]
    std::panic::set_hook(Box::new(|info| eprintln!("PANIC: {info}")));
    // `#[tokio::main]` is explicitly discouraged by Freya; build the runtime
    // and hold its guard for the whole process instead.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let _guard = runtime.enter();

    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app)
                .with_title("Fetcher (freya)")
                .with_size(700.0, 560.0)
                .with_background(BG),
        ),
    )
}

fn app() -> impl IntoElement {
    let fetcher = use_hook(|| Fetcher {
        client: State::create(reqwest::Client::new()),
        query: State::create(String::new()),
        results: State::create(Vec::new()),
        searching: State::create(false),
        search_error: State::create(None),
        generation: State::create(0),
        queued_count: State::create(0),
        stale_seen: State::create(0),
        search_task: State::create(None),
        download: State::create(Download::Idle),
        download_task: State::create(None),
        flaky: State::create(Flaky::Idle),
        flaky_attempts: State::create(0),
        health: State::create(None),
        selftest: std::env::var_os("FETCH_SELFTEST").is_some(),
    });

    // Search-as-you-type: the `Input` writes `query`, this effect re-arms the
    // 250 ms debounce. Skip the initial run so startup fires no request.
    let first_run = use_hook(|| Rc::new(std::cell::Cell::new(true)));
    use_side_effect({
        let first_run = first_run.clone();
        move || {
            let _subscribe = fetcher.query.read().len();
            if first_run.replace(false) {
                return;
            }
            fetcher.queue_search();
        }
    });

    if fetcher.selftest {
        use_hook(move || {
            spawn(async move { selftest(fetcher).await });
        });
    }

    let searching = *fetcher.searching.read();
    let results = fetcher.results.read().clone();
    let search_error = fetcher.search_error.read().clone();
    let download = fetcher.download.read().clone();
    let flaky = fetcher.flaky.read().clone();

    rect()
        .expanded()
        .content(Content::flex())
        .background(BG)
        .color(TEXT)
        .padding(Gaps::new_all(12.))
        .spacing(10.)
        // ------------------------------------------------------ search
        .child(
            rect()
                .horizontal()
                .content(Content::flex())
                .spacing(8.)
                .cross_align(Alignment::Center)
                .child(
                    Input::new(fetcher.query)
                        .placeholder("search… (250 ms debounce)")
                        .width(Size::flex(1.)),
                )
                .child(
                    label()
                        .text(if searching {
                            "searching…"
                        } else {
                            "idle"
                        })
                        .width(Size::px(78.))
                        .font_size(12.)
                        .color(if searching { ACCENT } else { MUTED }),
                ),
        )
        .maybe_child(search_error.map(|error| {
            label()
                .text(format!("search failed: {error}"))
                .font_size(12.)
                .color(ERROR)
        }))
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.))
                .content(Content::flex())
                .background(PANEL)
                .rounded_md()
                .border(Border::new().fill(LINE).width(1.))
                .child(
                    ScrollView::new()
                        .width(Size::fill())
                        .height(Size::flex(1.))
                        .children(results.iter().map(|result| {
                            rect()
                                .key(result.id)
                                .horizontal()
                                .content(Content::flex())
                                .width(Size::fill())
                                .padding(Gaps::new_symmetric(5., 10.))
                                .cross_align(Alignment::Center)
                                .child(
                                    label()
                                        .text(result.name.clone())
                                        .width(Size::flex(1.))
                                        .font_size(13.)
                                        .color(TEXT),
                                )
                                .child(
                                    label()
                                        .text(format!("{:.1}", result.score))
                                        .font_size(12.)
                                        .color(MUTED),
                                )
                                .into()
                        })),
                ),
        )
        // ------------------------------------------------------ download
        .child(
            rect()
                .width(Size::fill())
                .background(PANEL)
                .rounded_md()
                .border(Border::new().fill(LINE).width(1.))
                .padding(Gaps::new_all(10.))
                .spacing(6.)
                .child(
                    rect()
                        .horizontal()
                        .spacing(8.)
                        .cross_align(Alignment::Center)
                        .child(
                            Button::new()
                                .compact()
                                .on_press(move |_| fetcher.start_download())
                                .child("Download 8 MiB"),
                        )
                        .child(
                            Button::new()
                                .compact()
                                .on_press(move |_| fetcher.cancel_download())
                                .child("Cancel"),
                        )
                        .child(
                            label()
                                .text(match &download {
                                    Download::Idle => String::from("idle"),
                                    Download::Running { received, total } => format!(
                                        "{:.2} / {:.2} MiB",
                                        *received as f64 / 1048576.0,
                                        *total as f64 / 1048576.0
                                    ),
                                    Download::Cancelled { received, total } => format!(
                                        "cancelled at {:.2} / {:.2} MiB",
                                        *received as f64 / 1048576.0,
                                        *total as f64 / 1048576.0
                                    ),
                                    Download::Done { mib } => format!("done — {mib:.2} MiB"),
                                    Download::Failed(error) => format!("failed: {error}"),
                                })
                                .font_size(12.)
                                .color(MUTED),
                        ),
                )
                .child(ProgressBar::new(match &download {
                    Download::Running { received, total } | Download::Cancelled { received, total }
                        if *total > 0 =>
                    {
                        (*received as f32 / *total as f32) * 100.0
                    }
                    Download::Done { .. } => 100.0,
                    _ => 0.0,
                })),
        )
        // ------------------------------------------------------ flaky
        .child(
            rect()
                .horizontal()
                .width(Size::fill())
                .background(PANEL)
                .rounded_md()
                .border(Border::new().fill(LINE).width(1.))
                .padding(Gaps::new_all(10.))
                .spacing(8.)
                .cross_align(Alignment::Center)
                .child(
                    Button::new()
                        .compact()
                        .on_press(move |_| fetcher.call_flaky())
                        .child(match &flaky {
                            Flaky::Failed { .. } => "Retry /flaky",
                            _ => "Call /flaky",
                        }),
                )
                .child(
                    label()
                        .text(match &flaky {
                            Flaky::Idle => String::from("not called"),
                            Flaky::Running => String::from("calling…"),
                            Flaky::Failed { attempts, error } => {
                                format!("attempt {attempts} failed: {error}")
                            }
                            Flaky::Succeeded {
                                attempts,
                                server_attempt,
                            } => format!(
                                "succeeded on attempt {attempts} (server attempt {server_attempt})"
                            ),
                        })
                        .font_size(12.)
                        .max_lines(2)
                        .color(match &flaky {
                            Flaky::Failed { .. } => ERROR,
                            Flaky::Succeeded { .. } => ACCENT,
                            _ => MUTED,
                        }),
                ),
        )
}

// ---------------------------------------------------------------- self-test

async fn selftest(fetcher: Fetcher) {
    let passed = std::cell::Cell::new(0usize);
    let failed = std::cell::Cell::new(0usize);
    let check = |name: &str, ok: bool| {
        if ok {
            passed.set(passed.get() + 1);
        } else {
            failed.set(failed.get() + 1);
            println!("SELFTEST FAIL {name}");
        }
    };
    let mut fetcher_mut = fetcher;
    let sleep = |ms: u64| tokio::time::sleep(Duration::from_millis(ms));

    // --- health + reset the /flaky cycle -------------------------------
    let client = fetcher.client.peek().clone();
    let ok = client
        .get(format!("{}/health", base_url()))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    let _ = client.get(format!("{}/flaky/reset", base_url())).send().await;
    fetcher_mut.health.set(Some(ok));
    println!("HEALTH {}", if ok { "ok" } else { "FAILED" });
    check("health ok", ok);

    // --- debounce: two keystrokes 100 ms apart → ONE request -----------
    fetcher_mut.query.set(String::from("am"));
    sleep(100).await;
    fetcher_mut.query.set(String::from("amb"));
    sleep(1_200).await;
    check(
        "debounce collapsed burst to 1 request",
        *fetcher.queued_count.peek() == 1,
    );
    check(
        "results delivered for final query",
        !fetcher.results.peek().is_empty()
            && fetcher.results.peek().iter().all(|r| r.name.contains("amb"))
            && !*fetcher.searching.peek(),
    );

    // --- stale protection: a search aborted MID-FLIGHT by the next one --
    // 380 ms is past the 250 ms debounce but inside the server's 150–300 ms
    // artificial latency, so the "co" request is genuinely in flight when the
    // "br" keystroke cancels its task.
    fetcher_mut.query.set(String::from("co"));
    sleep(380).await;
    fetcher_mut.query.set(String::from("br"));
    sleep(1_400).await;
    check(
        "stale protection: latest query wins, no stale applied",
        !fetcher.results.peek().is_empty()
            && fetcher.results.peek().iter().all(|r| r.name.contains("br"))
            && *fetcher.stale_seen.peek() == 0,
    );

    // --- download + cancel at ~1.6 s -----------------------------------
    fetcher.start_download();
    sleep(1_600).await;
    let mid = fetcher.download.peek().clone();
    check(
        "progress streamed monotonically",
        matches!(mid, Download::Running { received, total } if received > 0 && received < total),
    );
    fetcher.cancel_download();
    let at_cancel = match fetcher.download.peek().clone() {
        Download::Cancelled { received, total } => {
            check("cancelled mid-stream", received > 0 && received < total);
            received
        }
        other => {
            check(&format!("cancelled mid-stream (was {other:?})"), false);
            0
        }
    };
    sleep(700).await;
    check(
        "no progress after cancel",
        matches!(fetcher.download.peek().clone(), Download::Cancelled { received, .. } if received == at_cancel),
    );

    // --- full download to completion (~8 s) -----------------------------
    fetcher.start_download();
    sleep(11_000).await;
    check(
        "full download completed with all 8 MiB",
        matches!(fetcher.download.peek().clone(), Download::Done { mib } if (mib - 8.0).abs() < 0.01),
    );

    // --- flaky: 500, 500, then success ----------------------------------
    fetcher.call_flaky();
    sleep(800).await;
    let first_failed = matches!(
        fetcher.flaky.peek().clone(),
        Flaky::Failed { attempts: 1, .. }
    );
    fetcher.call_flaky();
    sleep(800).await;
    let second_failed = matches!(
        fetcher.flaky.peek().clone(),
        Flaky::Failed { attempts: 2, .. }
    );
    check(
        "flaky failed on attempts 1 and 2",
        first_failed && second_failed,
    );
    fetcher.call_flaky();
    sleep(800).await;
    check(
        "flaky succeeded on attempt 3",
        matches!(
            fetcher.flaky.peek().clone(),
            Flaky::Succeeded { attempts: 3, .. }
        ),
    );

    println!("SELFTEST DONE pass={} fail={}", passed.get(), failed.get());
    std::process::exit(if failed.get() == 0 { 0 } else { 1 });
}
