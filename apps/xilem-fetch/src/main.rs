//! "Fetcher" — async & network integration test per apps/SPEC-8.md, built
//! with xilem 0.4.0 against the deterministic server in tools/fetcher-server.
//!
//! Async architecture: three long-lived `worker_raw` views (search, download,
//! flaky), each a tokio task on xilem's own runtime with an
//! `UnboundedReceiver` of commands from UI callbacks and a `MessageProxy` for
//! responses back to the UI thread. All HTTP goes through reqwest.
//!
//! * Debounce: the search worker owns the 250 ms window (`tokio::select!`
//!   between the debounce sleep and the next keystroke command).
//! * Stale protection: requests are serialized per worker, and an in-flight
//!   request is *really cancelled* (its future dropped, closing the
//!   connection) the moment a newer keystroke arrives; a generation counter
//!   guards the UI side as belt-and-braces.
//! * Download cancellation: the Cancel button sends `DlCmd::Cancel`; the
//!   worker drops the streaming `Response` mid-body, which closes the TCP
//!   connection — the server logs `ABORT /download` (the required proof).
//!
//! Evidence lines on stdout: KEYSTROKE, SEARCH_ISSUE, SEARCH_ABORT,
//! SEARCH_DONE, SEARCH_APPLY, STALE_DROP, DL_START, DL_PROGRESS (1 MiB
//! steps), DL_CANCEL, DL_DONE, FLAKY_CALL, FLAKY_RESULT.

use std::time::{Duration, Instant};

use serde::Deserialize;
use xilem::core::{fork, MessageProxy};
use xilem::masonry::properties::types::{CrossAxisAlignment, Length};
use xilem::masonry::properties::Padding;
use xilem::style::Style as _;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use xilem::view::{
    flex_col, flex_row, label, portal, progress_bar, sized_box, text_button, text_input,
    worker_raw, FlexExt as _, FlexSpacer,
};
use xilem::winit::error::EventLoopError;
use xilem::{Color, EventLoop, FontWeight, WidgetView, WindowOptions, Xilem};

const TEXT_MAIN: Color = Color::from_rgb8(0xe6, 0xe9, 0xef);
const TEXT_DIM: Color = Color::from_rgb8(0xa0, 0xa8, 0xb8);
const ACCENT: Color = Color::from_rgb8(0x53, 0xa8, 0xf4);
const OK_GREEN: Color = Color::from_rgb8(0x5f, 0xc2, 0x7e);
const ERR_RED: Color = Color::from_rgb8(0xe8, 0x6a, 0x6a);
const SECTION_BG: Color = Color::from_rgb8(0x23, 0x27, 0x2e);

fn port() -> u16 {
    std::env::var("FETCHER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7878)
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct SearchResult {
    id: u64,
    name: String,
    score: f64,
}

#[derive(Debug, Deserialize)]
struct FlakyOk {
    attempt: u64,
}

// Commands (UI -> worker) and messages (worker -> UI).

struct SearchCmd {
    gen: u64,
    query: String,
}

#[derive(Debug)]
enum SearchMsg {
    Started { gen: u64 },
    Done { gen: u64, result: Result<Vec<SearchResult>, String> },
}

enum DlCmd {
    Start,
    Cancel,
}

#[derive(Debug)]
enum DlMsg {
    Started { total: u64 },
    Progress { received: u64, total: u64 },
    Done { received: u64, secs: f64 },
    Cancelled,
    Failed(String),
}

struct FlakyCmd {
    click: u32,
}

#[derive(Debug)]
enum FlakyMsg {
    Started { click: u32 },
    Result { click: u32, result: Result<u64, String> },
}

// ---------------------------------------------------------------------------
// Workers (tokio tasks on xilem's runtime)
// ---------------------------------------------------------------------------

/// Search: debounce 250 ms, then GET /search?q=. Requests are serialized;
/// a newer keystroke aborts the in-flight request (drops its future).
async fn search_worker(proxy: MessageProxy<SearchMsg>, mut rx: UnboundedReceiver<SearchCmd>) {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/search", port());
    let mut pending: Option<SearchCmd> = None;
    'outer: loop {
        let mut cmd = match pending.take() {
            Some(c) => c,
            None => match rx.recv().await {
                Some(c) => c,
                None => break,
            },
        };
        // Debounce: every newer keystroke restarts the 250 ms window.
        loop {
            tokio::select! {
                () = tokio::time::sleep(Duration::from_millis(250)) => break,
                next = rx.recv() => match next {
                    Some(c) => cmd = c,
                    None => break 'outer,
                },
            }
        }
        if cmd.query.trim().is_empty() {
            let _ = proxy.message(SearchMsg::Done {
                gen: cmd.gen,
                result: Ok(Vec::new()),
            });
            continue;
        }
        println!("SEARCH_ISSUE gen={} q={:?}", cmd.gen, cmd.query);
        let _ = proxy.message(SearchMsg::Started { gen: cmd.gen });
        let request = async {
            let resp = client
                .get(&url)
                .query(&[("q", cmd.query.as_str())])
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            resp.json::<Vec<SearchResult>>()
                .await
                .map_err(|e| e.to_string())
        };
        tokio::pin!(request);
        tokio::select! {
            result = &mut request => {
                println!(
                    "SEARCH_DONE gen={} n={}",
                    cmd.gen,
                    result.as_ref().map(Vec::len).unwrap_or(0)
                );
                let _ = proxy.message(SearchMsg::Done { gen: cmd.gen, result });
            }
            next = rx.recv() => match next {
                Some(c) => {
                    // Newer keystroke while a request is in flight: dropping
                    // `request` here closes the connection — real cancellation.
                    println!("SEARCH_ABORT gen={} superseded_by={}", cmd.gen, c.gen);
                    pending = Some(c);
                }
                None => break,
            },
        }
    }
}

/// Download: GET /download streamed chunk by chunk; Cancel drops the
/// response mid-stream (server logs `ABORT /download`).
async fn download_worker(proxy: MessageProxy<DlMsg>, mut rx: UnboundedReceiver<DlCmd>) {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/download", port());
    loop {
        match rx.recv().await {
            Some(DlCmd::Start) => {}
            Some(DlCmd::Cancel) => continue, // nothing running
            None => return,
        }
        println!("DL_START");
        let t0 = Instant::now();
        let stream = async {
            let mut resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            let total = resp.content_length().unwrap_or(0);
            let _ = proxy.message(DlMsg::Started { total });
            let mut received = 0u64;
            let mut last_logged = 0u64;
            while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
                received += chunk.len() as u64;
                if received - last_logged >= 1024 * 1024 || received == total {
                    println!("DL_PROGRESS {received}/{total}");
                    last_logged = received;
                }
                let _ = proxy.message(DlMsg::Progress { received, total });
            }
            Ok::<u64, String>(received)
        };
        tokio::pin!(stream);
        loop {
            tokio::select! {
                res = &mut stream => {
                    match res {
                        Ok(received) => {
                            let secs = t0.elapsed().as_secs_f64();
                            println!("DL_DONE bytes={received} secs={secs:.2}");
                            let _ = proxy.message(DlMsg::Done { received, secs });
                        }
                        Err(e) => {
                            println!("DL_FAILED {e}");
                            let _ = proxy.message(DlMsg::Failed(e));
                        }
                    }
                    break;
                }
                cmd = rx.recv() => match cmd {
                    Some(DlCmd::Cancel) => {
                        // Break drops `stream` (and the Response inside it):
                        // the TCP connection closes mid-body and the server
                        // logs `ABORT /download`.
                        println!("DL_CANCEL after {:.2}s", t0.elapsed().as_secs_f64());
                        let _ = proxy.message(DlMsg::Cancelled);
                        break;
                    }
                    Some(DlCmd::Start) => {} // already running; ignore
                    None => return,
                },
            }
        }
    }
}

/// Flaky: GET /flaky per click; 500s surface as a visible error with Retry.
async fn flaky_worker(proxy: MessageProxy<FlakyMsg>, mut rx: UnboundedReceiver<FlakyCmd>) {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/flaky", port());
    while let Some(FlakyCmd { click }) = rx.recv().await {
        println!("FLAKY_CALL click={click}");
        let _ = proxy.message(FlakyMsg::Started { click });
        let result = async {
            let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
            let status = resp.status();
            if status.is_success() {
                let ok: FlakyOk = resp.json().await.map_err(|e| e.to_string())?;
                Ok(ok.attempt)
            } else {
                let body = resp.text().await.unwrap_or_default();
                Err(format!("HTTP {} — {}", status.as_u16(), body))
            }
        }
        .await;
        match &result {
            Ok(n) => println!("FLAKY_RESULT click={click} ok attempt={n}"),
            Err(e) => println!("FLAKY_RESULT click={click} err={e:?}"),
        }
        let _ = proxy.message(FlakyMsg::Result { click, result });
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum DlState {
    Idle,
    Running { received: u64, total: u64 },
    Done { received: u64, secs: f64 },
    Cancelled { received: u64, total: u64 },
    Failed(String),
}

#[derive(Debug)]
enum FlakyState {
    Idle,
    Loading { click: u32 },
    Failed { click: u32, error: String },
    Success { click: u32, server_attempt: u64 },
}

struct App {
    // search
    query: String,
    /// Bumped on every keystroke; commands and results carry it.
    search_gen: u64,
    /// Highest generation whose result has been applied.
    applied_gen: u64,
    searching: bool,
    results: Vec<SearchResult>,
    search_error: Option<String>,
    search_tx: Option<UnboundedSender<SearchCmd>>,
    // download
    dl: DlState,
    dl_tx: Option<UnboundedSender<DlCmd>>,
    // flaky
    flaky: FlakyState,
    flaky_clicks: u32,
    flaky_tx: Option<UnboundedSender<FlakyCmd>>,
}

impl App {
    fn new() -> Self {
        Self {
            query: String::new(),
            search_gen: 0,
            applied_gen: 0,
            searching: false,
            results: Vec::new(),
            search_error: None,
            search_tx: None,
            dl: DlState::Idle,
            dl_tx: None,
            flaky: FlakyState::Idle,
            flaky_clicks: 0,
            flaky_tx: None,
        }
    }

    fn on_query_edit(&mut self, value: String) {
        if value == self.query {
            return;
        }
        self.query = value;
        self.search_gen += 1;
        self.searching = true;
        println!("KEYSTROKE gen={} q={:?}", self.search_gen, self.query);
        if let Some(tx) = &self.search_tx {
            let _ = tx.send(SearchCmd {
                gen: self.search_gen,
                query: self.query.clone(),
            });
        }
    }

    fn on_search_msg(&mut self, msg: SearchMsg) {
        match msg {
            SearchMsg::Started { .. } => {}
            SearchMsg::Done { gen, result } => {
                // The worker serializes requests, so out-of-order results are
                // impossible by construction; the generation guard is
                // belt-and-braces (and would catch a stale late response).
                if gen < self.applied_gen {
                    println!("STALE_DROP gen={gen} applied_gen={}", self.applied_gen);
                    return;
                }
                self.applied_gen = gen;
                match result {
                    Ok(results) => {
                        println!("SEARCH_APPLY gen={gen} n={}", results.len());
                        self.results = results;
                        self.search_error = None;
                    }
                    Err(e) => {
                        self.results.clear();
                        self.search_error = Some(e);
                    }
                }
                if gen == self.search_gen {
                    self.searching = false;
                }
            }
        }
    }

    fn on_dl_msg(&mut self, msg: DlMsg) {
        match msg {
            DlMsg::Started { total } => self.dl = DlState::Running { received: 0, total },
            DlMsg::Progress { received, total } => {
                self.dl = DlState::Running { received, total };
            }
            DlMsg::Done { received, secs } => self.dl = DlState::Done { received, secs },
            DlMsg::Cancelled => {
                let (received, total) = match self.dl {
                    DlState::Running { received, total } => (received, total),
                    _ => (0, 0),
                };
                self.dl = DlState::Cancelled { received, total };
            }
            DlMsg::Failed(e) => self.dl = DlState::Failed(e),
        }
    }

    fn on_flaky_msg(&mut self, msg: FlakyMsg) {
        match msg {
            FlakyMsg::Started { click } => self.flaky = FlakyState::Loading { click },
            FlakyMsg::Result { click, result } => {
                self.flaky = match result {
                    Ok(server_attempt) => FlakyState::Success { click, server_attempt },
                    Err(error) => FlakyState::Failed { click, error },
                };
            }
        }
    }

    fn call_flaky(&mut self) {
        self.flaky_clicks += 1;
        if let Some(tx) = &self.flaky_tx {
            let _ = tx.send(FlakyCmd {
                click: self.flaky_clicks,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn section_title(text: &'static str) -> impl WidgetView<App> + use<> {
    label(text)
        .text_size(13.0)
        .weight(FontWeight::BOLD)
        .color(ACCENT)
}

fn search_section(state: &App) -> impl WidgetView<App> + use<> {
    let status: (String, Color) = if state.searching {
        ("searching…".into(), TEXT_DIM)
    } else if let Some(err) = &state.search_error {
        (format!("error: {err}"), ERR_RED)
    } else if state.query.trim().is_empty() {
        ("type to search".into(), TEXT_DIM)
    } else {
        (format!("{} result(s)", state.results.len()), TEXT_DIM)
    };

    let rows: Vec<_> = state
        .results
        .iter()
        .map(|r| {
            flex_row((
                sized_box(label(format!("#{}", r.id)).text_size(12.0).color(TEXT_DIM))
                    .width(Length::px(56.0)),
                label(r.name.clone()).text_size(13.0).color(TEXT_MAIN),
                FlexSpacer::Flex(1.0),
                label(format!("{:.1}", r.score)).text_size(12.0).color(TEXT_DIM),
            ))
            .gap(Length::px(8.0))
            .must_fill_major_axis(true)
        })
        .collect();

    flex_col((
        section_title("Search (250 ms debounce, stale-safe)"),
        flex_row((
            sized_box(
                text_input(state.query.clone(), |state: &mut App, value| {
                    state.on_query_edit(value);
                })
                .placeholder("Search names…"),
            )
            .width(Length::px(300.0)),
            label(status.0).text_size(12.0).color(status.1),
        ))
        .gap(Length::px(10.0)),
        sized_box(portal(
            flex_col(rows)
                .gap(Length::px(2.0))
                .cross_axis_alignment(CrossAxisAlignment::Fill),
        ))
        .height(Length::px(190.0))
        .background_color(SECTION_BG)
        .corner_radius(6.0)
        .padding(Padding::all(6.0)),
    ))
    .gap(Length::px(6.0))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
}

fn download_section(state: &App) -> impl WidgetView<App> + use<> {
    let running = matches!(state.dl, DlState::Running { .. });
    let (progress, detail, detail_color): (Option<f64>, String, Color) = match &state.dl {
        DlState::Idle => (Some(0.0), "idle".into(), TEXT_DIM),
        DlState::Running { received, total } => (
            (*total > 0).then(|| *received as f64 / *total as f64),
            format!("{:.2} / {:.2} MiB", mib(*received), mib(*total)),
            TEXT_MAIN,
        ),
        DlState::Done { received, secs } => (
            Some(1.0),
            format!("done — {:.2} MiB in {secs:.1}s", mib(*received)),
            OK_GREEN,
        ),
        DlState::Cancelled { received, total } => (
            (*total > 0).then(|| *received as f64 / *total as f64),
            format!("cancelled at {:.2} / {:.2} MiB", mib(*received), mib(*total)),
            ERR_RED,
        ),
        DlState::Failed(e) => (Some(0.0), format!("failed: {e}"), ERR_RED),
    };

    flex_col((
        section_title("Download (8 MiB streamed, live progress, real abort)"),
        flex_row((
            text_button(
                if running { "Downloading…" } else { "Start download" },
                |state: &mut App| {
                    if !matches!(state.dl, DlState::Running { .. }) {
                        if let Some(tx) = &state.dl_tx {
                            let _ = tx.send(DlCmd::Start);
                        }
                    }
                },
            ),
            text_button("Cancel", |state: &mut App| {
                if matches!(state.dl, DlState::Running { .. }) {
                    if let Some(tx) = &state.dl_tx {
                        let _ = tx.send(DlCmd::Cancel);
                    }
                }
            }),
            progress_bar(progress).flex(1.0),
            label(detail).text_size(12.0).color(detail_color),
        ))
        .gap(Length::px(10.0))
        .must_fill_major_axis(true),
    ))
    .gap(Length::px(6.0))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
}

fn flaky_section(state: &App) -> impl WidgetView<App> + use<> {
    let (button_text, status, color): (&'static str, String, Color) = match &state.flaky {
        FlakyState::Idle => ("Call /flaky", "not called yet".into(), TEXT_DIM),
        FlakyState::Loading { click } => ("Calling…", format!("call #{click} in flight…"), TEXT_DIM),
        FlakyState::Failed { click, error } => (
            "Retry",
            format!("call #{click} failed: {error}"),
            ERR_RED,
        ),
        FlakyState::Success { click, server_attempt } => (
            "Call /flaky",
            format!("call #{click} succeeded (server attempt {server_attempt})"),
            OK_GREEN,
        ),
    };

    flex_col((
        section_title("Flaky endpoint (500, 500, then 200 — manual retry)"),
        flex_row((
            text_button(button_text, |state: &mut App| {
                if !matches!(state.flaky, FlakyState::Loading { .. }) {
                    state.call_flaky();
                }
            }),
            label(status).text_size(12.0).color(color),
        ))
        .gap(Length::px(10.0)),
    ))
    .gap(Length::px(6.0))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
}

fn app_logic(state: &mut App) -> impl WidgetView<App> + use<> {
    let body = flex_col((
        search_section(state),
        download_section(state),
        flaky_section(state),
        FlexSpacer::Flex(1.0),
        label(format!("server: 127.0.0.1:{}", port()))
            .text_size(11.0)
            .color(TEXT_DIM),
    ))
    .gap(Length::px(16.0))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .must_fill_major_axis(true)
    .padding(14.0);

    fork(
        body,
        (
            worker_raw(
                search_worker,
                |state: &mut App, tx| state.search_tx = Some(tx),
                |state: &mut App, msg| state.on_search_msg(msg),
            ),
            worker_raw(
                download_worker,
                |state: &mut App, tx| state.dl_tx = Some(tx),
                |state: &mut App, msg| state.on_dl_msg(msg),
            ),
            worker_raw(
                flaky_worker,
                |state: &mut App, tx| state.flaky_tx = Some(tx),
                |state: &mut App, msg| state.on_flaky_msg(msg),
            ),
        ),
    )
}

fn main() -> Result<(), EventLoopError> {
    let mut window = WindowOptions::new("Fetcher (xilem)")
        .with_initial_inner_size(xilem::dpi::LogicalSize::new(700.0, 560.0))
        .with_resizable(true);
    // Verification aid on a shared desktop (same pattern as xilem-grid).
    if std::env::var("FETCH_TOPMOST").as_deref() == Ok("1") {
        window = window.with_window_level(xilem::winit::window::WindowLevel::AlwaysOnTop);
    }
    Xilem::new_simple(App::new(), app_logic, window).run_in(EventLoop::with_user_event())
}
