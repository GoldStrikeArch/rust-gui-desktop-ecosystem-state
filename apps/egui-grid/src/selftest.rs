//! Verification-only scripted driver (`GRID_SELFTEST=1`). Not compiled out
//! of the binary, but inert unless the env var is set; counted as
//! verification LoC in FRICTION.md.
//!
//! Timeline (wall clock since app start):
//! - 0.5 s  filter "a"      -> FILTER_MS 1 <ms>
//! - 1.0 s  filter "ambe"   -> FILTER_MS 4 <ms>
//! - 1.5 s  clear filter    -> FILTER_MS 0 <ms>, then sort by Value asc
//! - 2.0-6.0 s  jump-scroll through the full 100k range (exercises
//!   virtualization; RSS is sampled externally via `ps` before/after)
//! - 6.0 s  prints SELFTEST_SCROLL_DONE and goes quiet (app keeps running)

use crate::GridApp;
use eframe::egui;
use std::time::{Duration, Instant};

pub struct SelfTest {
    started: Instant,
    next_step: usize,
}

impl SelfTest {
    pub fn from_env() -> Option<Self> {
        std::env::var("GRID_SELFTEST").ok().filter(|v| v == "1").map(|_| {
            println!("SELFTEST_START");
            SelfTest { started: Instant::now(), next_step: 0 }
        })
    }
}

pub fn drive(ctx: &egui::Context, app: &mut GridApp) {
    let Some(st) = app.selftest.as_mut() else { return };
    let t = st.started.elapsed().as_secs_f64();
    let step = st.next_step;

    match step {
        0 if t >= 0.5 => {
            app.selftest.as_mut().unwrap().next_step = 1;
            app.filter = "a".into();
            app.apply_filter();
        }
        1 if t >= 1.0 => {
            app.selftest.as_mut().unwrap().next_step = 2;
            app.filter = "ambe".into();
            app.apply_filter();
        }
        2 if t >= 1.5 => {
            app.selftest.as_mut().unwrap().next_step = 3;
            app.filter.clear();
            app.apply_filter();
            app.toggle_sort(3); // Value asc — sorted long scroll below
            println!("SELFTEST_SCROLL_BEGIN");
        }
        3 => {
            if t >= 6.0 {
                app.selftest.as_mut().unwrap().next_step = 4;
                println!("SELFTEST_SCROLL_DONE");
            } else if t >= 2.0 {
                // Jump-scroll: a new window of rows is laid out every frame.
                let frac = (t - 2.0) / 4.0;
                let target = ((app.visible.len() - 1) as f64 * frac) as usize;
                app.pending_scroll = Some(target);
            }
        }
        _ => {}
    }

    if app.selftest.as_ref().is_some_and(|st| st.next_step < 4) {
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}
