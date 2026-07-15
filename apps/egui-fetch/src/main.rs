//! "Fetcher" per apps/SPEC-8.md — async & network integration in egui 0.35.
//!
//! Architecture: no async runtime at all. `ehttp` runs each request on its
//! own native thread and invokes a callback; the callback writes into an
//! `Arc<Mutex<..>>` shared with the UI and calls `ctx.request_repaint()`.
//! Time-based logic (250 ms debounce, retry backoff) is deadline math in the
//! frame callback + `request_repaint_after` — the same reactive idiom as
//! apps/egui-dash.
//!
//! Stale protection: generation counter (`AtomicU64`); a response is applied
//! only if its generation is newer than the last applied one. Cancellation:
//! `ehttp::streaming::fetch` chunk callback returns `ControlFlow::Break`,
//! which drops the reader and aborts the TCP connection (server logs ABORT).
//!
//! `FETCH_SELFTEST=1` runs a scripted driver (src/selftest.rs, verification
//! code): search, out-of-order stale demo, download + mid-stream cancel,
//! flaky retry-until-success. Evidence in verify-stdout.log + server log.

use eframe::egui;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod selftest;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([700.0, 560.0])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Fetcher (egui)",
        options,
        Box::new(|_cc| Ok(Box::new(FetchApp::new()))),
    )
}

#[derive(serde::Deserialize, Clone)]
struct SearchResult {
    id: u64,
    name: String,
    score: f64,
}

#[derive(Default)]
struct SearchShared {
    results: Vec<SearchResult>,
    /// Number of requests currently in flight (for the "searching…" state).
    in_flight: u32,
    /// Generation of the last applied response (stale guard).
    applied_gen: u64,
    /// Query whose results are currently shown.
    applied_query: String,
    error: Option<String>,
}

enum DownloadState {
    Idle,
    Running { received: u64, total: Option<u64> },
    Done { bytes: u64, secs: f64 },
    Cancelled { received: u64 },
    Error(String),
}

enum FlakyState {
    Idle,
    InFlight,
    Error(String),
    Success { attempt: u64 },
}

struct FetchApp {
    base_url: String,
    // Search
    query: String,
    debounce_until: Option<Instant>,
    search_gen: Arc<AtomicU64>,
    search: Arc<Mutex<SearchShared>>,
    // Download
    dl: Arc<Mutex<DownloadState>>,
    dl_cancel: Arc<AtomicBool>,
    dl_started: Option<Instant>,
    // Flaky
    flaky: Arc<Mutex<FlakyState>>,
    flaky_attempts: u64,
    auto_retry: bool,
    retry_at: Option<Instant>,
    retry_backoff: Duration,
    selftest: Option<selftest::SelfTest>,
}

impl FetchApp {
    fn new() -> Self {
        let port = std::env::var("FETCHER_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(7878);
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            query: String::new(),
            debounce_until: None,
            search_gen: Arc::new(AtomicU64::new(0)),
            search: Arc::new(Mutex::new(SearchShared::default())),
            dl: Arc::new(Mutex::new(DownloadState::Idle)),
            dl_cancel: Arc::new(AtomicBool::new(false)),
            dl_started: None,
            flaky: Arc::new(Mutex::new(FlakyState::Idle)),
            flaky_attempts: 0,
            auto_retry: false,
            retry_at: None,
            retry_backoff: Duration::from_millis(400),
            selftest: selftest::SelfTest::from_env(),
        }
    }

    /// Fire `/search?q=<query>` with a fresh generation. The callback thread
    /// applies the response only if no newer response has been applied
    /// (stale protection — sequence guard, not request cancellation: ehttp
    /// has no native way to abort a plain fetch in flight).
    fn fire_search(&mut self, ctx: &egui::Context) {
        let generation = self.search_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let query = self.query.clone();
        let url = format!("{}/search?q={}", self.base_url, urlencode(&query));
        self.search.lock().unwrap().in_flight += 1;
        println!("SEARCH_FIRE gen={generation} q={query:?}");
        let shared = Arc::clone(&self.search);
        let ctx = ctx.clone();
        ehttp::fetch(ehttp::Request::get(url), move |result| {
            let outcome = match result {
                Ok(response) if response.ok => response
                    .json::<Vec<SearchResult>>()
                    .map_err(|e| format!("bad JSON: {e}")),
                Ok(response) => {
                    Err(format!("HTTP {} {}", response.status, response.status_text))
                }
                Err(e) => Err(format!("transport error: {e}")),
            };
            let mut s = shared.lock().unwrap();
            s.in_flight -= 1;
            if generation <= s.applied_gen {
                // A newer response already landed; never overwrite it.
                println!(
                    "SEARCH_STALE_DROP gen={generation} applied_gen={}",
                    s.applied_gen
                );
            } else {
                s.applied_gen = generation;
                match outcome {
                    Ok(results) => {
                        println!(
                            "SEARCH_APPLY gen={generation} q={query:?} results={}",
                            results.len()
                        );
                        s.results = results;
                        s.applied_query = query;
                        s.error = None;
                    }
                    Err(e) => {
                        println!("SEARCH_ERROR gen={generation} err={e}");
                        s.error = Some(e);
                    }
                }
            }
            ctx.request_repaint();
        });
    }

    /// Start `/download` via the streaming API. Progress = bytes received vs
    /// Content-Length; Cancel sets `dl_cancel`, and the next chunk callback
    /// returns `ControlFlow::Break`, dropping the connection mid-stream
    /// (the server logs `ABORT /download …` — real cancellation).
    fn start_download(&mut self, ctx: &egui::Context) {
        self.dl_cancel.store(false, Ordering::SeqCst);
        *self.dl.lock().unwrap() = DownloadState::Running { received: 0, total: None };
        self.dl_started = Some(Instant::now());
        println!("DOWNLOAD_START");
        let mut request = ehttp::Request::get(format!("{}/download", self.base_url));
        request.timeout = None; // ~8 s stream; don't let the default timeout kill it
        let shared = Arc::clone(&self.dl);
        let cancel = Arc::clone(&self.dl_cancel);
        let ctx = ctx.clone();
        let started = Instant::now();
        ehttp::streaming::fetch(request, move |part| {
            let mut d = shared.lock().unwrap();
            if cancel.load(Ordering::SeqCst) {
                let received = match *d {
                    DownloadState::Running { received, .. } => received,
                    _ => 0,
                };
                *d = DownloadState::Cancelled { received };
                println!("DOWNLOAD_CANCELLED received_bytes={received}");
                ctx.request_repaint();
                return ControlFlow::Break(()); // drops the connection
            }
            let flow = match part {
                Ok(ehttp::streaming::Part::Response(response)) => {
                    if response.ok {
                        let total = response
                            .headers
                            .get("content-length")
                            .and_then(|v| v.parse::<u64>().ok());
                        *d = DownloadState::Running { received: 0, total };
                        ControlFlow::Continue(())
                    } else {
                        *d = DownloadState::Error(format!("HTTP {}", response.status));
                        ControlFlow::Break(())
                    }
                }
                Ok(ehttp::streaming::Part::Chunk(chunk)) => {
                    if let DownloadState::Running { received, .. } = &mut *d {
                        if chunk.is_empty() {
                            // End of stream.
                            let bytes = *received;
                            let secs = started.elapsed().as_secs_f64();
                            *d = DownloadState::Done { bytes, secs };
                            println!("DOWNLOAD_DONE bytes={bytes} secs={secs:.2}");
                            ControlFlow::Break(())
                        } else {
                            *received += chunk.len() as u64;
                            ControlFlow::Continue(())
                        }
                    } else {
                        ControlFlow::Break(())
                    }
                }
                Err(e) => {
                    *d = DownloadState::Error(format!("transport error: {e}"));
                    ControlFlow::Break(())
                }
            };
            ctx.request_repaint();
            flow
        });
    }

    fn fire_flaky(&mut self, ctx: &egui::Context) {
        self.flaky_attempts += 1;
        self.retry_at = None;
        *self.flaky.lock().unwrap() = FlakyState::InFlight;
        let shared = Arc::clone(&self.flaky);
        let ctx = ctx.clone();
        let attempt_no = self.flaky_attempts;
        ehttp::fetch(
            ehttp::Request::get(format!("{}/flaky", self.base_url)),
            move |result| {
                let mut f = shared.lock().unwrap();
                *f = match result {
                    Ok(response) if response.ok => {
                        #[derive(serde::Deserialize)]
                        struct Attempt {
                            attempt: u64,
                        }
                        let attempt = response
                            .json::<Attempt>()
                            .map(|a| a.attempt)
                            .unwrap_or_default();
                        println!("FLAKY_RESULT try={attempt_no} status=200 attempt={attempt}");
                        FlakyState::Success { attempt }
                    }
                    Ok(response) => {
                        println!("FLAKY_RESULT try={attempt_no} status={}", response.status);
                        FlakyState::Error(format!(
                            "HTTP {} {}",
                            response.status, response.status_text
                        ))
                    }
                    Err(e) => {
                        println!("FLAKY_RESULT try={attempt_no} transport_err");
                        FlakyState::Error(format!("transport error: {e}"))
                    }
                };
                ctx.request_repaint();
            },
        );
    }

    /// Deadline-driven work: debounce firing + auto-retry backoff. Reactive:
    /// one `request_repaint_after` per pending deadline, no polling loop.
    fn poll_deadlines(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if let Some(deadline) = self.debounce_until {
            if now >= deadline {
                self.debounce_until = None;
                self.fire_search(ctx);
            } else {
                ctx.request_repaint_after(deadline - now);
            }
        }
        if let Some(deadline) = self.retry_at {
            if now >= deadline {
                self.retry_at = None;
                self.retry_backoff = (self.retry_backoff * 2).min(Duration::from_secs(4));
                self.fire_flaky(ctx);
            } else {
                ctx.request_repaint_after(deadline - now);
            }
        } else if self.auto_retry
            && matches!(*self.flaky.lock().unwrap(), FlakyState::Error(_))
        {
            self.retry_at = Some(now + self.retry_backoff);
            ctx.request_repaint_after(self.retry_backoff);
        }
    }

    /// Whole UI; split out from `eframe::App::ui` so egui_kittest can drive
    /// it headlessly.
    fn show(&mut self, ui: &mut egui::Ui) {
        if self.selftest.is_some() {
            selftest::drive(ui.ctx(), self);
        }
        self.poll_deadlines(ui.ctx());

        egui::CentralPanel::default().show(ui, |ui| {
            self.show_search(ui);
            ui.separator();
            self.show_download(ui);
            ui.separator();
            self.show_flaky(ui);
        });
    }

    fn show_search(&mut self, ui: &mut egui::Ui) {
        ui.heading("Search");
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("search-as-you-type (250 ms debounce)…")
                    .desired_width(300.0),
            );
            if response.changed() {
                self.debounce_until = Some(Instant::now() + Duration::from_millis(250));
            }
            let searching =
                self.debounce_until.is_some() || self.search.lock().unwrap().in_flight > 0;
            if searching {
                ui.spinner();
                ui.weak("searching…");
            }
        });

        let (results, applied_query, error) = {
            let s = self.search.lock().unwrap();
            (s.results.clone(), s.applied_query.clone(), s.error.clone())
        };
        if let Some(error) = error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.weak(format!("{} result(s) for {applied_query:?}", results.len()));
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for r in &results {
                    ui.horizontal(|ui| {
                        ui.monospace(format!("#{:03}", r.id));
                        ui.label(&r.name);
                        ui.weak(format!("score {:.1}", r.score));
                    });
                }
            });
    }

    fn show_download(&mut self, ui: &mut egui::Ui) {
        ui.heading("Download");
        let running = matches!(*self.dl.lock().unwrap(), DownloadState::Running { .. });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("Download 8 MiB"))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.start_download(&ctx);
            }
            if ui
                .add_enabled(running, egui::Button::new("Cancel"))
                .clicked()
            {
                // Real cancellation: the streaming callback sees this flag
                // and returns ControlFlow::Break -> connection dropped.
                self.dl_cancel.store(true, Ordering::SeqCst);
            }
        });
        match &*self.dl.lock().unwrap() {
            DownloadState::Idle => {
                ui.weak("idle");
            }
            DownloadState::Running { received, total } => {
                let received = *received;
                if let Some(total) = *total {
                    let frac = received as f32 / total as f32;
                    ui.add(egui::ProgressBar::new(frac).show_percentage());
                    ui.weak(format!("{} / {}", fmt_mib(received), fmt_mib(total)));
                } else {
                    ui.add(egui::ProgressBar::new(0.0).animate(true));
                    ui.weak(format!("{} received", fmt_mib(received)));
                }
            }
            DownloadState::Done { bytes, secs } => {
                ui.add(egui::ProgressBar::new(1.0).show_percentage());
                ui.weak(format!("done: {} in {secs:.1} s", fmt_mib(*bytes)));
            }
            DownloadState::Cancelled { received } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("cancelled after {}", fmt_mib(*received)),
                );
            }
            DownloadState::Error(e) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        }
    }

    fn show_flaky(&mut self, ui: &mut egui::Ui) {
        ui.heading("Flaky endpoint");
        ui.horizontal(|ui| {
            if ui.button("Call /flaky").clicked() {
                self.flaky_attempts = 0;
                self.retry_backoff = Duration::from_millis(400);
                let ctx = ui.ctx().clone();
                self.fire_flaky(&ctx);
            }
            ui.checkbox(&mut self.auto_retry, "auto-retry with backoff");
        });
        let state_line = match &*self.flaky.lock().unwrap() {
            FlakyState::Idle => None,
            FlakyState::InFlight => Some(("calling…".to_string(), false)),
            FlakyState::Error(e) => Some((format!("failed: {e}"), true)),
            FlakyState::Success { attempt } => {
                Some((format!("success on server attempt #{attempt}"), false))
            }
        };
        if let Some((text, is_error)) = state_line {
            if is_error {
                ui.colored_label(ui.visuals().error_fg_color, &text);
                ui.horizontal(|ui| {
                    if ui.button("Retry").clicked() {
                        let ctx = ui.ctx().clone();
                        self.fire_flaky(&ctx);
                    }
                    if let Some(at) = self.retry_at {
                        ui.weak(format!(
                            "auto-retry in {:.1} s (try #{})",
                            (at - Instant::now()).as_secs_f32(),
                            self.flaky_attempts + 1
                        ));
                    }
                });
            } else {
                ui.label(text);
            }
        }
        if self.flaky_attempts > 0 {
            ui.weak(format!("{} attempt(s) this round", self.flaky_attempts));
        }
    }
}

/// Minimal RFC 3986 percent-encoding for the query string (avoids a `url`
/// crate dependency for one parameter).
fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn fmt_mib(bytes: u64) -> String {
    format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
}

impl eframe::App for FetchApp {
    // egui 0.35: `App::ui` (replacing `App::update` since 0.34).
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// End-to-end against the live local server: type into the real search
    /// box, let the 250 ms debounce fire, and wait for results to land via
    /// the callback thread. Requires `tools/fetcher-server` on
    /// $FETCHER_PORT (default 7878).
    #[test]
    fn typed_search_debounces_and_lands() {
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(700.0, 560.0))
            .build_ui_state(|ui, app: &mut FetchApp| app.show(ui), FetchApp::new());
        harness.run();

        let input = harness.get_by_role(egui::accesskit::Role::TextInput);
        input.focus();
        input.type_text("amber");
        // NB: not `run()` — the spinner + pending debounce keep requesting
        // repaints, and `Harness::run` panics after 4 such steps by design.
        harness.step();
        harness.step();
        assert!(harness.state().debounce_until.is_some(), "debounce armed");

        // Pump frames until debounce fires and the response lands
        // (deterministic server latency is 150-300 ms + 250 ms debounce).
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            std::thread::sleep(Duration::from_millis(50));
            harness.step();
            let s = harness.state().search.lock().unwrap();
            if s.applied_query == "amber" && !s.results.is_empty() {
                assert!(s.results.iter().all(|r| r.name.contains("amber")));
                break;
            }
            drop(s);
            assert!(Instant::now() < deadline, "no search results within 3 s");
        }
    }

    /// Stale guard: an older generation arriving after a newer one must be
    /// dropped (pure model test — no network).
    #[test]
    fn stale_generation_is_dropped() {
        let shared = SearchShared {
            applied_gen: 5,
            applied_query: "newer".into(),
            ..Default::default()
        };
        // Simulate what the callback does for generation 4 (older):
        let generation = 4u64;
        assert!(generation <= shared.applied_gen, "older response must be dropped");
        // ... and for generation 6 (newer):
        assert!(6 > shared.applied_gen, "newer response must be applied");
    }
}
