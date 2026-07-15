//! "Fetcher" — async & network integration. Dioxus 0.7 desktop (wry/tao).
//!
//! ASYNC MODEL — dioxus-desktop runs the VirtualDom on a tokio runtime, so
//! plain `reqwest` futures run directly inside `use_resource`/`spawn`; no
//! bridge, channel, or "invoke on UI thread" step exists anywhere in this
//! file. Signals are written straight from async code.
//!
//!   * SEARCH: one `use_resource` is the whole pipeline. It reads `query`
//!     (subscribing the resource to it), sleeps 250 ms (debounce), then GETs
//!     `/search`. Any change to `query` makes use_resource CANCEL the old
//!     task (source: dioxus-hooks 0.7.9 use_resource.rs `task.write().cancel()`
//!     on dependency change) and start a new one:
//!       - change during the sleep  → debounced, no request ever sent;
//!       - change mid-request       → the dropped reqwest future closes the
//!         connection (proved via the server's ABORT log — see FRICTION.md).
//!     Stale responses are therefore impossible by construction: there is no
//!     sequence guard in this app, cancellation is real.
//!   * DOWNLOAD: `spawn` + `Response::chunk()` loop writing a progress
//!     signal per chunk; Cancel = `Task::cancel()` (drops the future → drops
//!     the mid-body Response → TCP close → server logs `ABORT /download`).
//!   * FLAKY: same spawn pattern; error banner + manual Retry, plus an
//!     auto-retry variant with exponential backoff (300/600/1200 ms).
//!
//! Port from FETCHER_PORT (default 7878). Run with FETCH_SELFTEST=1 to drive
//! the same callbacks/signals the UI uses: debounce coalescing, use_resource
//! mid-stream cancellation (the ct_key resource), button-path download
//! cancel, and the 500→500→200 flaky cycle. Verification is stdout +
//! the fetcher-server log.

use std::time::Duration;

use dioxus::core::Task; // not in the prelude in 0.7
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use serde::Deserialize;

fn base_url() -> &'static str {
    static CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        let port = std::env::var("FETCHER_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(7878);
        format!("http://127.0.0.1:{port}")
    })
}

/// Minimal percent-encoding for the query param (no extra crate).
fn urlenc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Deserialize, Clone, PartialEq)]
struct Hit {
    id: u64,
    name: String,
    score: f64,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum DlPhase {
    #[default]
    Idle,
    Running,
    Done,
    Cancelled,
    Error,
}

#[derive(Clone, Default, PartialEq)]
struct DlState {
    phase: DlPhase,
    received: u64,
    total: u64,
    msg: String,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum FlakyPhase {
    #[default]
    Idle,
    Running,
    Failed,
    Success,
}

#[derive(Clone, Default, PartialEq)]
struct FlakyState {
    phase: FlakyPhase,
    attempts: u32,
    msg: String,
}

fn main() {
    // Verification-only: shell-launched apps get no macOS activation, so the
    // window can start occluded → WKWebView throttles → dioxus stops polling
    // the VirtualDom (and ALL tasks) while an edit batch waits to flush
    // (dioxus-desktop edits.rs `edits_in_progress` gate). Keeping the window
    // on top during the self-test sidesteps that; production is unaffected.
    let selftest = std::env::var("FETCH_SELFTEST").is_ok();
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_window(
                WindowBuilder::new()
                    .with_title("Fetcher (dioxus)")
                    .with_inner_size(LogicalSize::new(700.0, 560.0))
                    .with_always_on_top(selftest)
                    .with_resizable(true),
            ),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    let mut query = use_signal(String::new);

    // -- SEARCH: debounce + stale protection in one resource ---------------
    let search = use_resource(move || async move {
        let q = query(); // read = subscribe: any change cancels + restarts
        if q.trim().is_empty() {
            return Ok(Vec::new());
        }
        // Debounce: if `query` changes within 250 ms this task is cancelled
        // here, before any request is sent.
        tokio::time::sleep(Duration::from_millis(250)).await;
        let url = format!("{}/search?q={}", base_url(), urlenc(q.trim()));
        let resp = reqwest::get(&url).await.map_err(|e| e.to_string())?;
        resp.json::<Vec<Hit>>().await.map_err(|e| e.to_string())
    });

    // -- DOWNLOAD: spawn + chunk loop, Task handle for real cancellation ---
    let mut dl = use_signal(DlState::default);
    let mut dl_task = use_signal(|| Option::<Task>::None);
    let start_download = use_callback(move |_: ()| {
        if let Some(t) = dl_task.write().take() {
            t.cancel(); // restart semantics: kill any previous run
        }
        dl.set(DlState { phase: DlPhase::Running, ..Default::default() });
        let t = spawn(async move {
            let url = format!("{}/download", base_url());
            match reqwest::get(&url).await {
                Err(e) => {
                    dl.set(DlState { phase: DlPhase::Error, msg: e.to_string(), ..Default::default() });
                }
                Ok(mut resp) => {
                    dl.write().total = resp.content_length().unwrap_or(0);
                    loop {
                        match resp.chunk().await {
                            Ok(Some(chunk)) => dl.write().received += chunk.len() as u64,
                            Ok(None) => {
                                dl.write().phase = DlPhase::Done;
                                break;
                            }
                            Err(e) => {
                                let mut w = dl.write();
                                w.phase = DlPhase::Error;
                                w.msg = e.to_string();
                                break;
                            }
                        }
                    }
                }
            }
            dl_task.set(None);
        });
        dl_task.set(Some(t));
    });
    let cancel_download = use_callback(move |_: ()| {
        if let Some(t) = dl_task.write().take() {
            // Drops the future → drops the mid-body reqwest Response → the
            // connection closes → the server logs `ABORT /download`.
            t.cancel();
            dl.write().phase = DlPhase::Cancelled;
        }
    });

    // -- FLAKY: manual retry + auto-retry with backoff ----------------------
    let mut flaky = use_signal(FlakyState::default);
    let call_flaky = use_callback(move |_: ()| {
        {
            let mut w = flaky.write();
            w.phase = FlakyPhase::Running;
            w.attempts += 1;
        }
        spawn(async move {
            let outcome = flaky_once().await;
            apply_flaky(&mut flaky, outcome);
        });
    });
    let auto_flaky = use_callback(move |_: ()| {
        flaky.set(FlakyState { phase: FlakyPhase::Running, ..Default::default() });
        spawn(async move {
            let mut backoff = 300u64;
            for attempt in 1..=5u32 {
                flaky.write().attempts = attempt;
                let outcome = flaky_once().await;
                let ok = matches!(outcome, Ok(_));
                apply_flaky(&mut flaky, outcome);
                if ok {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                backoff *= 2;
            }
        });
    });

    // ---- view-model precomputation ----------------------------------------
    let q_now = query();
    let searching = !q_now.trim().is_empty()
        && *search.state().read() == UseResourceState::Pending;
    let dl_v = dl();
    let pct = if dl_v.total > 0 {
        (dl_v.received as f64 / dl_v.total as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    let mib = |b: u64| b as f64 / (1024.0 * 1024.0);
    let dl_running = dl_v.phase == DlPhase::Running;
    let fl_v = flaky();

    rsx! {
        style { {CSS} }
        div { class: "root",
            // ================= SEARCH =================
            div { class: "panel",
                div { class: "ptitle",
                    "Search"
                    if searching {
                        span { class: "searching", " searching\u{2026}" }
                    }
                }
                input {
                    r#type: "text",
                    class: "searchbox",
                    placeholder: "type to search (250 ms debounce)\u{2026}",
                    value: "{query}",
                    oninput: move |evt| query.set(evt.value()),
                }
                div { class: "results",
                    match &*search.read() {
                        None => rsx! { div { class: "muted", "\u{2026}" } },
                        Some(Err(e)) => rsx! { div { class: "error", "search failed: {e}" } },
                        Some(Ok(hits)) if hits.is_empty() => rsx! {
                            div { class: "muted",
                                if q_now.trim().is_empty() { "results appear here" } else { "no matches" }
                            }
                        },
                        Some(Ok(hits)) => rsx! {
                            for h in hits.iter() {
                                div { class: "hit", key: "{h.id}",
                                    span { class: "hitname", "{h.name}" }
                                    span { class: "hitscore", "{h.score:.1}" }
                                }
                            }
                        },
                    }
                }
            }

            // ================= DOWNLOAD =================
            div { class: "panel",
                div { class: "ptitle", "Download (8 MiB over ~8 s)" }
                div { class: "dlrow",
                    button {
                        class: "btn",
                        disabled: dl_running,
                        onclick: move |_| start_download.call(()),
                        "Start"
                    }
                    button {
                        class: "btn danger",
                        disabled: !dl_running,
                        onclick: move |_| cancel_download.call(()),
                        "Cancel"
                    }
                    span { class: "dlstat",
                        match dl_v.phase {
                            DlPhase::Idle => rsx! { "idle" },
                            DlPhase::Running => rsx! { "{mib(dl_v.received):.2} / {mib(dl_v.total):.2} MiB" },
                            DlPhase::Done => rsx! { span { class: "okmsg", "done — {mib(dl_v.received):.2} MiB" } },
                            DlPhase::Cancelled => rsx! { span { class: "warnmsg", "cancelled at {mib(dl_v.received):.2} MiB (connection aborted)" } },
                            DlPhase::Error => rsx! { span { class: "error", "error: {dl_v.msg}" } },
                        }
                    }
                }
                div { class: "barwrap",
                    div { class: "bar", style: "width: {pct}%;" }
                }
            }

            // ================= FLAKY =================
            div { class: "panel",
                div { class: "ptitle", "Flaky endpoint (500, 500, then 200)" }
                div { class: "dlrow",
                    button {
                        class: "btn",
                        disabled: fl_v.phase == FlakyPhase::Running,
                        onclick: move |_| call_flaky.call(()),
                        if fl_v.phase == FlakyPhase::Failed { "Retry" } else { "Call /flaky" }
                    }
                    button {
                        class: "btn",
                        disabled: fl_v.phase == FlakyPhase::Running,
                        onclick: move |_| auto_flaky.call(()),
                        "Auto-retry (backoff)"
                    }
                    span { class: "dlstat", "attempts: {fl_v.attempts}" }
                }
                div { class: "flakymsg",
                    match fl_v.phase {
                        FlakyPhase::Idle => rsx! { span { class: "muted", "not called yet" } },
                        FlakyPhase::Running => rsx! { span { class: "muted", "calling\u{2026}" } },
                        FlakyPhase::Failed => rsx! { span { class: "error", "failed: {fl_v.msg} — try Retry" } },
                        FlakyPhase::Success => rsx! { span { class: "okmsg", "success: {fl_v.msg}" } },
                    }
                }
            }
        }

        // ================= VERIFICATION (FETCH_SELFTEST=1) =================
        // ct_key/ct resource exist to answer THE question: does use_resource's
        // auto-cancel abort the underlying reqwest connection, or only drop
        // the Rust future? The server's ABORT log line is the arbiter.
        {
            let mut ct_key = use_signal(|| 0u32);
            let _ct = use_resource(move || async move {
                if ct_key() != 1 {
                    return 0u64;
                }
                let mut n = 0u64;
                if let Ok(mut resp) = reqwest::get(format!("{}/download", base_url())).await {
                    while let Ok(Some(chunk)) = resp.chunk().await {
                        n += chunk.len() as u64; // ~8 s to finish; will be cancelled mid-stream
                    }
                }
                n
            });
            let _ = use_future(move || async move {
                if std::env::var("FETCH_SELFTEST").is_err() {
                    return;
                }
                let ms = |n: u64| tokio::time::sleep(Duration::from_millis(n));
                // Generous warm-up before the first signal write. Three runs
                // froze here or later: while the (unactivated, occluded)
                // window's webview is throttled, a pending edit batch parks
                // the whole VirtualDom+task loop — see FRICTION.md "The
                // occlusion freeze". with_always_on_top (selftest-only)
                // is what actually fixes it; the warm-up is belt-and-braces.
                ms(3000).await;
                // 1) Debounce: "am" then "amber" 100 ms later — the first task
                //    dies during its debounce sleep; server must log ONE search.
                println!("SELFTEST_SEARCH_BEGIN");
                query.set("am".into());
                ms(100).await;
                query.set("amber".into());
                ms(1200).await;
                match &*search.value().peek() {
                    Some(Ok(hits)) => println!(
                        "SELFTEST_SEARCH_RESULTS {} first={}",
                        hits.len(),
                        hits.first().map(|h| h.name.as_str()).unwrap_or("-")
                    ),
                    Some(Err(e)) => println!("SELFTEST_SEARCH_ERR {e}"),
                    None => println!("SELFTEST_SEARCH_NONE"),
                }
                // 2) THE question: cancel a use_resource mid-stream by writing
                //    its dependency; watch the server log for ABORT.
                println!("SELFTEST_CT_START");
                ct_key.set(1);
                ms(1600).await;
                ct_key.set(2); // dependency change → task cancelled mid-body
                ms(400).await;
                println!("SELFTEST_CT_CANCELLED");
                // 3) Button-path download + cancel (Task::cancel).
                println!("SELFTEST_DL_START");
                start_download.call(());
                ms(2100).await;
                println!("SELFTEST_DL_PROGRESS {} of {}", dl.peek().received, dl.peek().total);
                cancel_download.call(());
                ms(400).await;
                println!(
                    "SELFTEST_DL_CANCELLED phase_cancelled={}",
                    dl.peek().phase == DlPhase::Cancelled
                );
                // 4) Flaky: three manual calls → 500, 500, 200.
                for _ in 0..3 {
                    call_flaky.call(());
                    ms(500).await;
                    let f = flaky.peek().clone();
                    println!(
                        "SELFTEST_FLAKY attempts={} phase={} msg={}",
                        f.attempts,
                        match f.phase {
                            FlakyPhase::Failed => "failed",
                            FlakyPhase::Success => "success",
                            _ => "other",
                        },
                        f.msg
                    );
                }
                println!("SELFTEST_DONE");
            });
        }
    }
}

async fn flaky_once() -> Result<String, String> {
    match reqwest::get(format!("{}/flaky", base_url())).await {
        Ok(r) if r.status().is_success() => {
            Ok(r.text().await.unwrap_or_default())
        }
        Ok(r) => Err(format!("HTTP {}", r.status().as_u16())),
        Err(e) => Err(e.to_string()),
    }
}

fn apply_flaky(flaky: &mut Signal<FlakyState>, outcome: Result<String, String>) {
    let mut w = flaky.write();
    match outcome {
        Ok(body) => {
            w.phase = FlakyPhase::Success;
            w.msg = body;
        }
        Err(e) => {
            w.phase = FlakyPhase::Failed;
            w.msg = e;
        }
    }
}

const CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body { height: 100%; }
body {
  background: #0e1320; color: #e8ecf4; overflow: hidden;
  font-family: system-ui, -apple-system, sans-serif; font-size: 13px;
  -webkit-user-select: none; user-select: none;
}
.root { display: flex; flex-direction: column; gap: 12px; padding: 14px 16px; height: 100vh; }
.panel {
  background: #121a2b; border: 1px solid #263354; border-radius: 10px;
  padding: 12px 14px; display: flex; flex-direction: column; gap: 9px;
}
.panel:first-child { flex: 1; min-height: 0; }
.ptitle { font-size: 13px; font-weight: 600; color: #b8c6e0; letter-spacing: .3px; }
.searching { color: #7aa2ff; font-weight: 400; font-size: 12px; animation: pulse 1s ease infinite; }
@keyframes pulse { 50% { opacity: .35; } }
.searchbox {
  background: #171f31; color: #e8ecf4; border: 1px solid #33456b; border-radius: 6px;
  padding: 6px 10px; font-size: 13px; outline: none; width: 100%;
  -webkit-user-select: text; user-select: text;
}
.searchbox:focus { border-color: #7aa2ff; }
.results { flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; }
.hit {
  display: flex; justify-content: space-between; padding: 4px 8px;
  border-radius: 5px; background: #16203a;
}
.hit:hover { background: #1d2a4a; }
.hitname { color: #dfe7f5; }
.hitscore { color: #7688ad; font-variant-numeric: tabular-nums; }
.muted { color: #5e6f92; padding: 4px 2px; }
.error { color: #ff7a85; }
.okmsg { color: #5ad18b; }
.warnmsg { color: #ffd166; }
.btn {
  background: #22304d; color: #e8ecf4; border: 1px solid #33456b; border-radius: 6px;
  padding: 5px 14px; font-size: 13px; cursor: pointer; transition: background .15s ease;
}
.btn:hover:not(:disabled) { background: #2e4066; }
.btn:disabled { opacity: .45; cursor: default; }
.btn.danger { border-color: #6b2730; }
.btn.danger:hover:not(:disabled) { background: #3a1519; }
.dlrow { display: flex; align-items: center; gap: 10px; }
.dlstat { color: #9db0d0; font-variant-numeric: tabular-nums; }
.barwrap {
  height: 10px; background: #171f31; border: 1px solid #263354; border-radius: 999px;
  overflow: hidden;
}
.bar { height: 100%; background: linear-gradient(90deg, #4a7dff, #7aa2ff); border-radius: 999px; }
.flakymsg { min-height: 18px; }
"#;
