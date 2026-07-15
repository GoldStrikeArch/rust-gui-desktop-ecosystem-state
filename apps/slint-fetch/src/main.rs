// Fetcher (slint) — SPEC-8: async & network integration.
//
// Async model: a background tokio runtime (1 worker thread) owns ALL network
// I/O via a pooled reqwest::Client; task results are marshalled onto the UI
// thread with `Weak::upgrade_in_event_loop`. slint::spawn_local was rejected:
// reqwest futures need a tokio reactor and panic on Slint's winit event loop
// (proven by `src/bin/spawnlocal_probe.rs` — observed panic).
//
// Debounce: a single-shot `slint::Timer` on the UI thread, restarted on every
// keystroke (250 ms). Stale protection is two layers: (1) the previous
// in-flight search task is aborted via its tokio AbortHandle, and (2) a
// monotonically increasing sequence number is checked on the UI thread before
// applying results (covers responses already queued for delivery).
// Cancellation of /download is REAL: JoinHandle::abort_handle().abort() drops
// the in-flight hyper response body -> TCP connection closes -> the server
// logs `ABORT /download after N/64 chunks`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};
use tokio::task::AbortHandle;

slint::include_modules!();

fn base_url() -> String {
    let port = std::env::var("FETCHER_PORT").unwrap_or_else(|_| "7878".into());
    format!("http://127.0.0.1:{port}")
}

#[derive(serde::Deserialize)]
struct ApiResult {
    id: u64,
    name: String,
    score: f64,
}

struct NetCtx {
    client: reqwest::Client,
    rt: tokio::runtime::Handle,
    base: String,
    // search stale protection
    search_seq: Arc<AtomicU64>,
    search_inflight: RefCell<Option<AbortHandle>>,
    dispatch_count: Cell<u64>, // instrumentation (selftest)
    // download cancellation
    dl_abort: RefCell<Option<AbortHandle>>,
}

/// Issue GET /search?q=. `abort_prev=false` is used by the selftest to
/// demonstrate that the sequence guard alone also suppresses stale responses.
fn dispatch_search(ctx: &Rc<NetCtx>, ui_weak: &Weak<MainWindow>, query: SharedString, abort_prev: bool) {
    let my_seq = ctx.search_seq.fetch_add(1, Ordering::SeqCst) + 1;
    ctx.dispatch_count.set(ctx.dispatch_count.get() + 1);
    if abort_prev {
        if let Some(h) = ctx.search_inflight.borrow_mut().take() {
            h.abort();
        }
    }
    if let Some(ui) = ui_weak.upgrade() {
        ui.set_searching(true);
    }

    let client = ctx.client.clone();
    let url = format!("{}/search", ctx.base);
    let seq = ctx.search_seq.clone();
    let ui_weak = ui_weak.clone();
    let handle = ctx.rt.spawn(async move {
        let outcome: Result<Vec<ApiResult>, String> = async {
            let resp = client
                .get(&url)
                .query(&[("q", query.as_str())])
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())
        }
        .await;

        let q = query.clone();
        ui_weak
            .upgrade_in_event_loop(move |ui| {
                // stale guard (runs on the UI thread)
                if my_seq != seq.load(Ordering::SeqCst) {
                    println!("STALE_DROPPED seq={my_seq} q={q:?}");
                    return;
                }
                ui.set_searching(false);
                match outcome {
                    Ok(results) => {
                        println!("SEARCH_APPLIED seq={my_seq} q={q:?} n={}", results.len());
                        ui.set_search_status(
                            format!("{} results for \"{q}\"", results.len()).into(),
                        );
                        let rows: Vec<SearchResult> = results
                            .into_iter()
                            .map(|r| SearchResult {
                                id: r.id as i32,
                                name: r.name.into(),
                                score: format!("{:.1}", r.score).into(),
                            })
                            .collect();
                        ui.set_results(ModelRc::new(VecModel::from(rows)));
                    }
                    Err(e) => {
                        ui.set_search_status(format!("error: {e}").into());
                        ui.set_results(ModelRc::new(VecModel::from(Vec::<SearchResult>::new())));
                    }
                }
            })
            .ok();
    });
    *ctx.search_inflight.borrow_mut() = Some(handle.abort_handle());
}

fn start_download(ctx: &Rc<NetCtx>, ui_weak: &Weak<MainWindow>) {
    if let Some(ui) = ui_weak.upgrade() {
        ui.set_dl_state(1);
        ui.set_dl_progress(0.0);
        ui.set_dl_status("connecting…".into());
    }
    let client = ctx.client.clone();
    let url = format!("{}/download", ctx.base);
    let ui_weak = ui_weak.clone();
    let handle = ctx.rt.spawn(async move {
        let fail = |ui_weak: &Weak<MainWindow>, msg: String| {
            ui_weak
                .upgrade_in_event_loop(move |ui| {
                    ui.set_dl_state(4);
                    ui.set_dl_status(format!("error: {msg}").into());
                })
                .ok();
        };
        let mut resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return fail(&ui_weak, e.to_string()),
        };
        let total = resp.content_length().unwrap_or(0);
        let mut got: u64 = 0;
        let mut chunk_n: u64 = 0;
        const MIB: f64 = 1024.0 * 1024.0;
        loop {
            match resp.chunk().await {
                Ok(Some(c)) => {
                    got += c.len() as u64;
                    chunk_n += 1;
                    if chunk_n % 16 == 1 {
                        println!("DL_CHUNK {got}/{total}");
                    }
                    let frac = if total > 0 { got as f32 / total as f32 } else { 0.0 };
                    let status =
                        format!("{:.1} / {:.1} MiB", got as f64 / MIB, total as f64 / MIB);
                    ui_weak
                        .upgrade_in_event_loop(move |ui| {
                            if ui.get_dl_state() == 1 {
                                ui.set_dl_progress(frac);
                                ui.set_dl_status(status.into());
                            }
                        })
                        .ok();
                }
                Ok(None) => {
                    println!("DL_DONE {got}/{total}");
                    ui_weak
                        .upgrade_in_event_loop(move |ui| {
                            ui.set_dl_state(2);
                            ui.set_dl_progress(1.0);
                            ui.set_dl_status(
                                format!("done — {:.1} MiB", got as f64 / MIB).into(),
                            );
                        })
                        .ok();
                    return;
                }
                Err(e) => return fail(&ui_weak, e.to_string()),
            }
        }
    });
    *ctx.dl_abort.borrow_mut() = Some(handle.abort_handle());
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    let ctx = Rc::new(NetCtx {
        client: reqwest::Client::new(), // pooled; server probes must run serially because /flaky is process-global
        rt: rt.handle().clone(),
        base: base_url(),
        search_seq: Arc::new(AtomicU64::new(0)),
        search_inflight: RefCell::new(None),
        dispatch_count: Cell::new(0),
        dl_abort: RefCell::new(None),
    });

    let ui = MainWindow::new().expect("failed to create window");

    // --- search-as-you-type: 250 ms debounce on the UI thread ---
    let debounce = Rc::new(Timer::default());
    {
        let ctx = ctx.clone();
        let ui_weak = ui.as_weak();
        let debounce = debounce.clone();
        ui.on_search_edited(move |text| {
            let ctx = ctx.clone();
            let ui_weak = ui_weak.clone();
            // start() on a running timer restarts it -> classic debounce
            debounce.start(TimerMode::SingleShot, Duration::from_millis(250), move || {
                dispatch_search(&ctx, &ui_weak, text.clone(), true);
            });
        });
    }

    // --- download + real cancel ---
    {
        let ctx = ctx.clone();
        let ui_weak = ui.as_weak();
        ui.on_download_clicked(move || start_download(&ctx, &ui_weak));
    }
    {
        let ctx = ctx.clone();
        let ui_weak = ui.as_weak();
        ui.on_cancel_clicked(move || {
            if let Some(h) = ctx.dl_abort.borrow_mut().take() {
                h.abort(); // drops the hyper body -> TCP close -> server ABORT log
                println!("CANCEL_ISSUED");
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_dl_state(3);
                    ui.set_dl_status("cancelled".into());
                }
            }
        });
    }

    // --- flaky endpoint with manual retry ---
    {
        let ctx = ctx.clone();
        let ui_weak = ui.as_weak();
        ui.on_flaky_clicked(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_flaky_state(1);
                ui.set_flaky_status("calling /flaky…".into());
            }
            let client = ctx.client.clone();
            let url = format!("{}/flaky", ctx.base);
            let ui_weak = ui_weak.clone();
            ctx.rt.spawn(async move {
                let outcome: Result<(u16, String), String> = async {
                    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
                    let code = resp.status().as_u16();
                    let body = resp.text().await.unwrap_or_default();
                    Ok((code, body))
                }
                .await;
                ui_weak
                    .upgrade_in_event_loop(move |ui| match outcome {
                        Ok((200, body)) => {
                            println!("FLAKY_OK {}", body.trim());
                            ui.set_flaky_state(3);
                            ui.set_flaky_status(format!("success: {}", body.trim()).into());
                        }
                        Ok((code, _)) => {
                            println!("FLAKY_ERR {code}");
                            ui.set_flaky_state(2);
                            ui.set_flaky_status(format!("HTTP {code} — click Retry").into());
                        }
                        Err(e) => {
                            println!("FLAKY_ERR {e}");
                            ui.set_flaky_state(2);
                            ui.set_flaky_status(format!("error: {e} — click Retry").into());
                        }
                    })
                    .ok();
            });
        });
    }

    if std::env::var("FETCH_SELFTEST").as_deref() == Ok("1") {
        run_selftest(&ui, ctx.clone());
    }

    ui.run().expect("event loop failed");
    // rt dropped here shuts the worker down
    drop(rt);
}

// ---------------------------------------------------------------------------
// Verification harness (FETCH_SELFTEST=1). Drives search via REAL key events
// (debounce proof), demonstrates the stale-guard with two overlapping
// requests of known server latency, cancels a download mid-stream (server
// must log ABORT), and walks /flaky through 500,500,200.
// Everything below is verification code, not production code.
// ---------------------------------------------------------------------------

/// FNV-1a, identical to the server's — used to pick a slow and a fast query.
fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Render the window to <app-dir>/verify-snapshot.ppm (pixel evidence).
fn save_snapshot(ui: &MainWindow) {
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|a| a.join("verify-snapshot.ppm")))
        .unwrap();
    match ui.window().take_snapshot() {
        Ok(buf) => {
            let (w, h) = (buf.width(), buf.height());
            let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
            for px in buf.as_slice() {
                out.extend_from_slice(&[px.r, px.g, px.b]);
            }
            std::fs::write(&path, out).unwrap();
            println!("SNAPSHOT_SAVED {} {w}x{h}", path.display());
        }
        Err(e) => println!("SNAPSHOT_FAILED {e}"),
    }
}

fn run_selftest(ui: &MainWindow, ctx: Rc<NetCtx>) {
    use slint::platform::WindowEvent;

    fn type_key(ui: &MainWindow, text: &str) {
        let t: SharedString = text.into();
        ui.window().try_dispatch_event(WindowEvent::KeyPressed { text: t.clone() }).unwrap();
        ui.window().try_dispatch_event(WindowEvent::KeyReleased { text: t }).unwrap();
    }

    // pick a slow and a fast query by replicating the server's latency fn
    let mut slow = ("a".to_string(), 0u64);
    let mut fast = ("a".to_string(), u64::MAX);
    for c1 in b'a'..=b'z' {
        for c2 in [None, Some(b'x')] {
            let q = match c2 {
                None => (c1 as char).to_string(),
                Some(_) => format!("{}x", c1 as char),
            };
            let lat = 150 + fnv(&q) % 151;
            if lat > slow.1 {
                slow = (q.clone(), lat);
            }
            if lat < fast.1 {
                fast = (q, lat);
            }
        }
    }
    println!("STALE_TEST slow={:?}({}ms) fast={:?}({}ms)", slow.0, slow.1, fast.0, fast.1);

    let ui_weak = ui.as_weak();
    let mk_ui = |f: Box<dyn Fn(&MainWindow)>| {
        let u = ui_weak.clone();
        Box::new(move || f(&u.unwrap())) as Box<dyn Fn()>
    };

    let c1 = ctx.clone();
    let c2 = ctx.clone();
    let c3 = ctx.clone();
    let u_stale = ui_weak.clone();
    let slow_q = slow.0.clone();
    let fast_q = fast.0.clone();
    let steps: Vec<(u64, Box<dyn Fn()>)> = vec![
        // debounce proof: two keystrokes 100 ms apart -> ONE dispatch
        (600, mk_ui(Box::new(|ui| type_key(ui, "a")))),
        (700, mk_ui(Box::new(|ui| type_key(ui, "m")))),
        (1800, Box::new(move || {
            println!("DEBOUNCE_DISPATCHES {}", c1.dispatch_count.get());
        })),
        // stale-guard proof: slow query then fast query, no aborting
        (2200, Box::new(move || {
            let slow_q: SharedString = slow_q.as_str().into();
            let fast_q: SharedString = fast_q.as_str().into();
            dispatch_search(&c2, &u_stale, slow_q, false);
            let c = c2.clone();
            let u = u_stale.clone();
            let t = Timer::default();
            t.start(TimerMode::SingleShot, Duration::from_millis(40), move || {
                dispatch_search(&c, &u, fast_q.clone(), false);
            });
            std::mem::forget(t); // verification-only leak
        })),
        (3400, mk_ui(Box::new(|ui| {
            println!("SEARCH_FINAL_STATUS {:?}", ui.get_search_status());
        }))),
        // download + mid-stream cancel (server must log ABORT)
        (3600, mk_ui(Box::new(|ui| ui.invoke_download_clicked()))),
        (6600, mk_ui(Box::new(|ui| ui.invoke_cancel_clicked()))),
        (7100, mk_ui(Box::new(|ui| {
            println!("DL_FINAL state={} status={:?} progress={:.2}",
                ui.get_dl_state(), ui.get_dl_status(), ui.get_dl_progress());
        }))),
        // flaky: 500, 500, 200
        (7300, mk_ui(Box::new(|ui| ui.invoke_flaky_clicked()))),
        (8100, mk_ui(Box::new(|ui| {
            println!("FLAKY_STATUS_1 {:?}", ui.get_flaky_status());
            ui.invoke_flaky_clicked();
        }))),
        (8900, mk_ui(Box::new(|ui| {
            println!("FLAKY_STATUS_2 {:?}", ui.get_flaky_status());
            ui.invoke_flaky_clicked();
        }))),
        (9700, mk_ui(Box::new(|ui| {
            println!("FLAKY_STATUS_3 {:?}", ui.get_flaky_status());
        }))),
        // pixel evidence: results list, cancelled download bar, flaky success
        (10000, mk_ui(Box::new(|ui| save_snapshot(ui)))),
        (10800, Box::new(move || {
            let _ = c3.dispatch_count.get(); // keep ctx alive until the end
            println!("SELFTEST_DONE");
            let _ = slint::quit_event_loop();
        })),
    ];

    let timers: Vec<Timer> = steps
        .into_iter()
        .map(|(ms, f)| {
            let t = Timer::default();
            t.start(TimerMode::SingleShot, Duration::from_millis(ms), f);
            t
        })
        .collect();
    std::mem::forget(timers); // verification-only leak
    println!("SELFTEST_ARMED base={}", base_url());
}
