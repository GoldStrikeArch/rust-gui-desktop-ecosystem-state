//! "Fetcher" — async & network integration in gpui 0.2.2 (SPEC-8).
//!
//! ## The executor-interop story (the core finding)
//!
//! gpui has its own executors (foreground = the platform main loop, background
//! = GCD on macOS) and re-exports an `HttpClient` **trait**
//! (`gpui::http_client`, wired via `Application::with_http_client` /
//! `cx.http_client()`) — but the only implementation the published crate
//! ships is `NullHttpClient`, which returns "No HttpClient available". Zed's
//! real reqwest-backed impl (`reqwest_client`) is not on crates.io. So this
//! app BYOs the same architecture Zed uses internally:
//!
//! - one std thread runs a **current-thread tokio runtime** (reqwest/hyper
//!   need tokio's reactor; gpui's executors can't drive them),
//! - requests run there via `Handle::spawn`; the returned **`JoinHandle` is
//!   itself a future and is awaited directly inside a gpui `cx.spawn` task**
//!   on the main thread (tokio wakers are cross-thread; no glue needed),
//! - streaming progress crosses over on an executor-agnostic
//!   `futures::channel::mpsc` drained by a gpui task that does
//!   `entity.update(...) + cx.notify()`,
//! - **cancellation is drop-based end to end**: gpui `Task`s cancel when
//!   dropped, and each gpui task holds an `AbortOnDrop` guard around the
//!   tokio `JoinHandle` — dropping the gpui task aborts the tokio task,
//!   which drops the reqwest response mid-stream, which closes the TCP
//!   connection (the server logs `ABORT /download …`, the required proof).
//!
//! Debounce (250 ms) is a gpui background timer inside the search task;
//! replacing `search_task` drops the old task, so a pending debounce simply
//! never fires and an in-flight request is aborted (stale protection is
//! therefore *real cancellation*, with a sequence guard kept as
//! belt-and-braces for the final race window).
//!
//! The search box is the minimal hand-rolled `on_key_down` input from
//! gpui-app/gpui-board (gpui ships no text-input widget; no IME/selection).
//!
//! `FETCH_SELFTEST=1` drives the same methods the UI events call on a timed
//! script (typing cadence, superseded search, download + mid-stream cancel,
//! full download, flaky retry-until-success) and prints evidence to stdout.

use std::time::Duration;

use futures::StreamExt as _;
use gpui::{
    App, Application, Bounds, Context, FocusHandle, KeyDownEvent, SharedString, TitlebarOptions,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, relative, rgb, size,
};
use serde::Deserialize;

const ACCENT: u32 = 0x3b82f6;

// ---------------------------------------------------------------------------
// Tokio bridge
// ---------------------------------------------------------------------------

/// Handle to the dedicated network runtime + a pooled reqwest client.
#[derive(Clone)]
struct Net {
    handle: tokio::runtime::Handle,
    client: reqwest::Client,
    base: String,
}

impl Net {
    /// Spawn a parked current-thread tokio runtime on its own std thread.
    fn start() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("tokio-net".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                tx.send(rt.handle().clone()).unwrap();
                // Park forever; the runtime drives all spawned request tasks.
                rt.block_on(std::future::pending::<()>());
            })
            .expect("spawn tokio thread");
        let handle = rx.recv().expect("tokio handle");
        let port = std::env::var("FETCHER_PORT").unwrap_or_else(|_| "7878".into());
        Net {
            handle,
            client: reqwest::Client::new(),
            base: format!("http://127.0.0.1:{port}"),
        }
    }
}

/// Aborts the wrapped tokio task when dropped. Held inside gpui tasks so that
/// dropping the gpui `Task` (gpui's native cancellation) tears the HTTP
/// request down for real (connection close, not just UI state).
struct AbortOnDrop(tokio::task::AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Minimal query percent-encoding (enough for a search box).
fn urlencode(q: &str) -> String {
    let mut out = String::new();
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
struct SearchHit {
    id: u64,
    name: String,
    score: f64,
}

enum DlState {
    Idle,
    Running { received: u64, total: u64 },
    Done { received: u64 },
    Cancelled { received: u64 },
    Failed(String),
}

enum DlEvent {
    Start { total: u64 },
    Progress { received: u64 },
    Done { received: u64 },
    Failed(String),
}

enum FlakyState {
    Idle,
    Loading,
    Failed { status: u16, body: String },
    Ok { attempt: u64 },
}

struct FetchApp {
    net: Net,
    // search
    search_input: String,
    search_focus: FocusHandle,
    search_seq: u64,
    search_task: Option<gpui::Task<()>>,
    searching: bool,
    results: Vec<SearchHit>,
    search_error: Option<String>,
    // download
    dl: DlState,
    dl_task: Option<gpui::Task<()>>,
    // flaky
    flaky: FlakyState,
    flaky_tries: u32,
    flaky_task: Option<gpui::Task<()>>,
}

impl FetchApp {
    fn new(net: Net, cx: &mut Context<Self>) -> Self {
        if std::env::var("FETCH_SELFTEST").is_ok() {
            Self::spawn_selftest(cx);
        }
        Self {
            net,
            search_input: String::new(),
            search_focus: cx.focus_handle(),
            search_seq: 0,
            search_task: None,
            searching: false,
            results: Vec::new(),
            search_error: None,
            dl: DlState::Idle,
            dl_task: None,
            flaky: FlakyState::Idle,
            flaky_tries: 0,
            flaky_task: None,
        }
    }

    // -- search: 250 ms debounce + drop-cancellation + seq guard ------------

    fn schedule_search(&mut self, cx: &mut Context<Self>) {
        self.search_seq += 1;
        let seq = self.search_seq;
        // Dropping the previous task cancels a pending debounce outright and
        // aborts an already-sent request via its AbortOnDrop guard.
        if self.search_task.take().is_some() && self.searching {
            println!("SEARCH_CANCEL in_flight_before {seq}");
            self.searching = false;
        }
        let q = self.search_input.trim().to_string();
        if q.is_empty() {
            self.results.clear();
            self.search_error = None;
            cx.notify();
            return;
        }
        let timer = cx.background_executor().timer(Duration::from_millis(250));
        let net = self.net.clone();
        self.search_task = Some(cx.spawn(async move |this, cx| {
            timer.await; // debounce: a replaced task never gets past this line
            let _ = this.update(cx, |t, cx| {
                t.searching = true;
                cx.notify();
            });
            println!("SEARCH_SENT {seq} {q}");
            let url = format!("{}/search?q={}", net.base, urlencode(&q));
            let jh = net.handle.spawn(async move {
                let resp = net.client.get(&url).send().await?;
                let status = resp.status().as_u16();
                let body = resp.bytes().await?;
                Ok::<_, reqwest::Error>((status, body))
            });
            let _guard = AbortOnDrop(jh.abort_handle());
            let out = jh.await;
            let _ = this.update(cx, |t, cx| {
                if seq != t.search_seq {
                    // Belt-and-braces: abort should make this unreachable.
                    println!("SEARCH_STALE_DROPPED {seq}");
                    return;
                }
                t.searching = false;
                match out {
                    Ok(Ok((200, body))) => match serde_json::from_slice::<Vec<SearchHit>>(&body) {
                        Ok(hits) => {
                            println!("SEARCH_RESULT {seq} {} applied", hits.len());
                            t.results = hits;
                            t.search_error = None;
                        }
                        Err(e) => t.search_error = Some(format!("bad JSON: {e}")),
                    },
                    Ok(Ok((code, _))) => t.search_error = Some(format!("HTTP {code}")),
                    Ok(Err(e)) => t.search_error = Some(e.to_string()),
                    Err(_) => t.search_error = Some("aborted".into()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn set_search_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.search_input = text.to_string();
        self.schedule_search(cx);
    }

    fn on_search_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if ks.modifiers.platform || ks.modifiers.control || ks.modifiers.function {
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.search_input.pop().is_some() {
                    self.schedule_search(cx);
                }
            }
            "escape" => {
                if !self.search_input.is_empty() {
                    self.search_input.clear();
                    self.schedule_search(cx);
                }
            }
            _ => {
                if let Some(c) = &ks.key_char {
                    self.search_input.push_str(c);
                    self.schedule_search(cx);
                }
            }
        }
    }

    // -- download: streamed progress + real mid-stream cancellation ---------

    fn start_download(&mut self, cx: &mut Context<Self>) {
        if matches!(self.dl, DlState::Running { .. }) {
            return;
        }
        println!("DL_START");
        self.dl = DlState::Running { received: 0, total: 0 };
        let net = self.net.clone();
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<DlEvent>();
        let jh = net.handle.spawn(async move {
            let url = format!("{}/download", net.base);
            let run = async {
                let resp = net.client.get(&url).send().await?;
                let total = resp.content_length().unwrap_or(0);
                let _ = tx.unbounded_send(DlEvent::Start { total });
                let mut stream = resp.bytes_stream();
                let mut received = 0u64;
                while let Some(chunk) = stream.next().await {
                    received += chunk?.len() as u64;
                    let _ = tx.unbounded_send(DlEvent::Progress { received });
                }
                Ok::<u64, reqwest::Error>(received)
            };
            match run.await {
                Ok(received) => {
                    let _ = tx.unbounded_send(DlEvent::Done { received });
                }
                Err(e) => {
                    let _ = tx.unbounded_send(DlEvent::Failed(e.to_string()));
                }
            }
        });
        // Drain progress on the gpui side; dropping this task aborts the
        // tokio task -> drops the reqwest response mid-stream -> TCP close ->
        // the server logs `ABORT /download` (the SPEC-8 proof).
        self.dl_task = Some(cx.spawn(async move |this, cx| {
            let _guard = AbortOnDrop(jh.abort_handle());
            while let Some(ev) = rx.next().await {
                let _ = this.update(cx, |t, cx| {
                    t.apply_dl_event(ev);
                    cx.notify();
                });
            }
        }));
        cx.notify();
    }

    fn apply_dl_event(&mut self, ev: DlEvent) {
        match ev {
            DlEvent::Start { total } => {
                if let DlState::Running { received, .. } = self.dl {
                    self.dl = DlState::Running { received, total };
                }
            }
            DlEvent::Progress { received } => {
                if let DlState::Running { total, .. } = self.dl {
                    if received / (1024 * 1024) > (received.saturating_sub(128 * 1024)) / (1024 * 1024) {
                        println!("DL_PROGRESS {received}");
                    }
                    self.dl = DlState::Running { received, total };
                }
            }
            DlEvent::Done { received } => {
                println!("DL_DONE {received}");
                self.dl = DlState::Done { received };
            }
            DlEvent::Failed(e) => {
                println!("DL_FAILED {e}");
                self.dl = DlState::Failed(e);
            }
        }
    }

    fn cancel_download(&mut self, cx: &mut Context<Self>) {
        if let DlState::Running { received, .. } = self.dl {
            self.dl_task = None; // drop => abort => connection close => server ABORT
            self.dl = DlState::Cancelled { received };
            println!("DL_CANCEL {received}");
            cx.notify();
        }
    }

    // -- flaky: visible error + manual retry until success ------------------

    fn call_flaky(&mut self, cx: &mut Context<Self>) {
        if matches!(self.flaky, FlakyState::Loading) {
            return;
        }
        self.flaky_tries += 1;
        self.flaky = FlakyState::Loading;
        println!("FLAKY_CALL try={}", self.flaky_tries);
        let net = self.net.clone();
        let jh = net.handle.spawn(async move {
            let url = format!("{}/flaky", net.base);
            let resp = net.client.get(&url).send().await?;
            let status = resp.status().as_u16();
            let body = resp.text().await?;
            Ok::<_, reqwest::Error>((status, body))
        });
        self.flaky_task = Some(cx.spawn(async move |this, cx| {
            let _guard = AbortOnDrop(jh.abort_handle());
            let out = jh.await;
            let _ = this.update(cx, |t, cx| {
                t.flaky = match out {
                    Ok(Ok((200, body))) => {
                        let attempt = serde_json::from_str::<serde_json::Value>(&body)
                            .ok()
                            .and_then(|v| v.get("attempt").and_then(|a| a.as_u64()))
                            .unwrap_or(0);
                        println!("FLAKY_OK attempt={attempt}");
                        FlakyState::Ok { attempt }
                    }
                    Ok(Ok((status, body))) => {
                        println!("FLAKY_ERR http={status}");
                        FlakyState::Failed { status, body }
                    }
                    Ok(Err(e)) => {
                        println!("FLAKY_ERR net={e}");
                        FlakyState::Failed { status: 0, body: e.to_string() }
                    }
                    Err(_) => FlakyState::Failed { status: 0, body: "aborted".into() },
                };
                cx.notify();
            });
        }));
        cx.notify();
    }

    // -- self-test (verification only; drives the same methods as the UI) ---

    fn spawn_selftest(cx: &mut Context<Self>) {
        let exec = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let type_text = |txt: &'static str| {
                let this = this.clone();
                let exec = exec.clone();
                let mut cx = cx.clone();
                async move {
                    for i in 1..=txt.len() {
                        let _ = this.update(&mut cx, |t, cx| t.set_search_text(&txt[..i], cx));
                        exec.timer(Duration::from_millis(60)).await;
                    }
                }
            };
            exec.timer(Duration::from_secs(2)).await;
            println!("SELFTEST debounce: typing 'amber' at 60ms cadence (expect 1 SEARCH_SENT)");
            type_text("amber").await;
            exec.timer(Duration::from_millis(800)).await;
            println!("SELFTEST stale: 'prism' typed while 'amber…a' request could be in flight");
            // Replace query, wait just past debounce+send, then replace again
            // while the request is mid-flight (server latency 150-300 ms).
            let _ = this.update(cx, |t, cx| t.set_search_text("a", cx));
            exec.timer(Duration::from_millis(330)).await; // debounce 250 + in-flight ~80
            let _ = this.update(cx, |t, cx| t.set_search_text("prism", cx));
            exec.timer(Duration::from_millis(900)).await;
            println!("SELFTEST download + mid-stream cancel");
            let _ = this.update(cx, |t, cx| t.start_download(cx));
            exec.timer(Duration::from_millis(3000)).await;
            let _ = this.update(cx, |t, cx| t.cancel_download(cx));
            exec.timer(Duration::from_millis(500)).await;
            println!("SELFTEST flaky retries");
            // The server's flaky counter is global (and shared with sibling
            // agents' runs), so retry until success rather than exactly 3×.
            for _ in 0..6 {
                let _ = this.update(cx, |t, cx| t.call_flaky(cx));
                exec.timer(Duration::from_millis(700)).await;
                let ok = this
                    .update(cx, |t, _| matches!(t.flaky, FlakyState::Ok { .. }))
                    .unwrap_or(false);
                if ok {
                    break;
                }
            }
            println!("SELFTEST full download");
            let _ = this.update(cx, |t, cx| t.start_download(cx));
            exec.timer(Duration::from_millis(9500)).await;
            println!("SELFTEST_DONE");
        })
        .detach();
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

fn button(id: &'static str, label: &'static str, color: u32) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(color))
        .text_color(gpui::white())
        .text_sm()
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .child(label)
}

fn panel_label(text: &'static str) -> impl IntoElement {
    div()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(0x64748b))
        .child(text)
}

impl FetchApp {
    fn render_search(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.search_focus.is_focused(window);
        let input = div()
            .id("search-input")
            .track_focus(&self.search_focus)
            .on_key_down(cx.listener(Self::on_search_key))
            .flex_1()
            .h(px(30.))
            .px_2()
            .flex()
            .items_center()
            .bg(gpui::white())
            .border_1()
            .border_color(if focused { rgb(ACCENT) } else { rgb(0xd1d5db) })
            .rounded_md()
            .cursor_text()
            .text_sm()
            .child(if self.search_input.is_empty() {
                div().text_color(rgb(0x9ca3af)).child("Search (250 ms debounce)…")
            } else {
                div().text_color(rgb(0x111827)).child(self.search_input.clone())
            })
            .when(focused, |d| d.child(div().w(px(1.5)).h(px(16.)).bg(rgb(0x111827))));

        let status: SharedString = if self.searching {
            "searching…".into()
        } else if let Some(err) = &self.search_error {
            format!("error: {err}").into()
        } else if self.search_input.trim().is_empty() {
            "type to search".into()
        } else {
            format!("{} result(s)", self.results.len()).into()
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .min_h(px(0.))
            .child(panel_label("SEARCH"))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(input)
                    .child(
                        div()
                            .w(px(130.))
                            .text_xs()
                            .text_color(if self.search_error.is_some() {
                                rgb(0xdc2626)
                            } else {
                                rgb(0x64748b)
                            })
                            .child(status),
                    ),
            )
            .child(
                div()
                    .id("results")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0xe2e8f0))
                    .bg(gpui::white())
                    .child(div().flex().flex_col().children(self.results.iter().enumerate().map(
                        |(i, hit)| {
                            div()
                                .flex()
                                .justify_between()
                                .px_2()
                                .py_1()
                                .text_sm()
                                .when(i % 2 == 1, |d| d.bg(rgb(0xf8fafc)))
                                .child(
                                    div()
                                        .text_color(rgb(0x111827))
                                        .child(format!("#{} {}", hit.id, hit.name)),
                                )
                                .child(
                                    div()
                                        .text_color(rgb(0x64748b))
                                        .child(format!("score {:.1}", hit.score)),
                                )
                        },
                    ))),
            )
    }

    fn render_download(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (frac, caption, running) = match &self.dl {
            DlState::Idle => (0.0, "idle".to_string(), false),
            DlState::Running { received, total } => {
                let frac = if *total > 0 { *received as f32 / *total as f32 } else { 0.0 };
                (
                    frac,
                    format!(
                        "{:.1} / {:.1} MiB ({:.0}%)",
                        *received as f64 / 1048576.0,
                        *total as f64 / 1048576.0,
                        frac * 100.0
                    ),
                    true,
                )
            }
            DlState::Done { received } => {
                (1.0, format!("done — {:.1} MiB", *received as f64 / 1048576.0), false)
            }
            DlState::Cancelled { received } => (
                0.0,
                format!("cancelled at {:.1} MiB (connection aborted)", *received as f64 / 1048576.0),
                false,
            ),
            DlState::Failed(e) => (0.0, format!("failed: {e}"), false),
        };

        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_1()
            .child(panel_label("DOWNLOAD (8 MiB over ~8 s)"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(if running {
                        button("dl-cancel", "Cancel", 0xdc2626)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_download(cx)))
                    } else {
                        button("dl-start", "Download", ACCENT)
                            .on_click(cx.listener(|this, _, _, cx| this.start_download(cx)))
                    })
                    .child(
                        div().flex_1().h(px(10.)).rounded_full().bg(rgb(0xe2e8f0)).child(
                            div()
                                .h_full()
                                .w(relative(frac))
                                .rounded_full()
                                .bg(rgb(if running { ACCENT } else { 0x16a34a })),
                        ),
                    )
                    .child(div().w(px(220.)).text_xs().text_color(rgb(0x475569)).child(caption)),
            )
    }

    fn render_flaky(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (msg, color): (SharedString, u32) = match &self.flaky {
            FlakyState::Idle => ("not called yet".into(), 0x64748b),
            FlakyState::Loading => ("calling…".into(), 0x64748b),
            FlakyState::Failed { status, body } => {
                (format!("HTTP {status}: {body} (try {})", self.flaky_tries).into(), 0xdc2626)
            }
            FlakyState::Ok { attempt } => (
                format!("success on server attempt {attempt} (after {} tries)", self.flaky_tries)
                    .into(),
                0x16a34a,
            ),
        };
        let retry = matches!(self.flaky, FlakyState::Failed { .. });

        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_1()
            .child(panel_label("FLAKY (500, 500, then 200)"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        button("flaky", if retry { "Retry" } else { "Call /flaky" }, if retry {
                            0xd97706
                        } else {
                            ACCENT
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.call_flaky(cx))),
                    )
                    .child(div().text_sm().text_color(rgb(color)).child(msg)),
            )
    }
}

impl Render for FetchApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .bg(rgb(0xf8fafc))
            .child(self.render_search(window, cx))
            .child(div().h(px(1.)).bg(rgb(0xe2e8f0)))
            .child(self.render_download(cx))
            .child(div().h(px(1.)).bg(rgb(0xe2e8f0)))
            .child(self.render_flaky(cx))
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(rgb(0x94a3b8))
                    .child(format!("server: {}", self.net.base)),
            )
    }
}

// ---------------------------------------------------------------------------

fn main() {
    let net = Net::start();
    Application::new().run(move |cx: &mut App| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(700.), px(560.)), cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Fetcher (gpui)")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let app = FetchApp::new(net, cx);
                    app.search_focus.focus(window);
                    app
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
