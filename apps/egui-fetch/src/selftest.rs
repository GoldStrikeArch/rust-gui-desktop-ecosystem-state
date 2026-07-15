//! Verification-only scripted driver (`FETCH_SELFTEST=1`). Inert unless the
//! env var is set; counted as verification LoC in FRICTION.md.
//!
//! Timeline (wall clock since app start):
//! - 0.3 s  type "amber" -> 250 ms debounce -> SEARCH_FIRE/SEARCH_APPLY
//! - 1.4 s  stale demo: fire a slow query then immediately a fast one (the
//!   server's latency is deterministic: 150 + fnv1a(q) % 151 ms, computed
//!   here to pick the pair). The fast (newer) response lands first and is
//!   applied; the slow (older) one must print SEARCH_STALE_DROP.
//! - 2.5 s  start /download (8 MiB over ~8 s)
//! - 5.0 s  cancel mid-stream -> ControlFlow::Break -> server logs ABORT
//! - 6.0 s  /flaky with auto-retry until success (500,500,200 cycle)
//! - then   SELFTEST_DONE once flaky succeeded (app keeps running)

use crate::FetchApp;
use eframe::egui;
use std::time::{Duration, Instant};

pub struct SelfTest {
    started: Instant,
    next_step: usize,
}

impl SelfTest {
    pub fn from_env() -> Option<Self> {
        std::env::var("FETCH_SELFTEST").ok().filter(|v| v == "1").map(|_| {
            println!("SELFTEST_START");
            SelfTest { started: Instant::now(), next_step: 0 }
        })
    }
}

/// FNV-1a, identical to tools/fetcher-server (drives its per-query latency).
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn server_delay_ms(q: &str) -> u64 {
    150 + fnv1a(&q.to_lowercase()) % 151
}

pub fn drive(ctx: &egui::Context, app: &mut FetchApp) {
    let Some(st) = app.selftest.as_ref() else { return };
    let t = st.started.elapsed().as_secs_f64();
    let step = st.next_step;

    match step {
        0 if t >= 0.3 => {
            app.selftest.as_mut().unwrap().next_step = 1;
            // Through the same debounce path as typing.
            app.query = "amber".into();
            app.debounce_until = Some(Instant::now() + Duration::from_millis(250));
        }
        1 if t >= 1.4 => {
            app.selftest.as_mut().unwrap().next_step = 2;
            // Pick the slowest and fastest queries from a candidate set so
            // the older request provably lands after the newer one.
            let candidates = ["amber", "brisk", "coral", "lunar", "prism", "delta"];
            let slow = *candidates.iter().max_by_key(|q| server_delay_ms(q)).unwrap();
            let fast = *candidates.iter().min_by_key(|q| server_delay_ms(q)).unwrap();
            println!(
                "SELFTEST_STALE_DEMO slow={slow:?}({} ms) fast={fast:?}({} ms)",
                server_delay_ms(slow),
                server_delay_ms(fast)
            );
            app.query = slow.into();
            app.fire_search(ctx); // older generation, slower response
            app.query = fast.into();
            app.fire_search(ctx); // newer generation, faster response
        }
        2 if t >= 2.5 => {
            app.selftest.as_mut().unwrap().next_step = 3;
            app.start_download(ctx);
        }
        3 if t >= 5.0 => {
            app.selftest.as_mut().unwrap().next_step = 4;
            println!("SELFTEST_CANCEL_REQUESTED");
            app.dl_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        4 if t >= 6.0 => {
            app.selftest.as_mut().unwrap().next_step = 5;
            app.auto_retry = true;
            app.flaky_attempts = 0;
            app.fire_flaky(ctx);
        }
        5 => {
            let done =
                matches!(*app.flaky.lock().unwrap(), crate::FlakyState::Success { .. });
            if done {
                app.selftest.as_mut().unwrap().next_step = 6;
                println!("SELFTEST_DONE");
            }
        }
        _ => {}
    }

    if app.selftest.as_ref().is_some_and(|st| st.next_step < 6) {
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}
