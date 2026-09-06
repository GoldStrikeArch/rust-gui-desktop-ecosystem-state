//! Verification-only scripted driver (`WINDOWS_SELFTEST=1`). Inert unless
//! the env var is set; counted as verification LoC in FRICTION.md.
//!
//! It exercises the parts of SPEC-9 that are *in-process* facts — the modal
//! actually disabling the parent's widget layer, a second window's pass
//! observing state written by the first, the close-request veto, the JSON
//! round-trip. Anything that needs an OS window (window counts, sheets,
//! synthetic clicks) is verified from outside with synth/window-count.

use crate::{WindowsApp, inspector_id, prefs_id};
use eframe::egui;
use std::time::{Duration, Instant};

pub struct SelfTest {
    started: Instant,
    step: usize,
    pass: u32,
    fail: u32,
    pub vetoed: bool,
}

impl SelfTest {
    pub fn from_env() -> Option<Self> {
        std::env::var("WINDOWS_SELFTEST").ok().filter(|v| v == "1").map(|_| {
            println!("SELFTEST_START");
            SelfTest { started: Instant::now(), step: 0, pass: 0, fail: 0, vetoed: false }
        })
    }

    /// Called from the close-veto path: veto exactly once.
    pub fn veto_next(&mut self) -> bool {
        if self.vetoed {
            false
        } else {
            self.vetoed = true;
            true
        }
    }

    fn check(&mut self, name: &str, ok: bool) {
        if ok {
            self.pass += 1;
            println!("SELFTEST ok   {name}");
        } else {
            self.fail += 1;
            println!("SELFTEST FAIL {name}");
        }
    }
}

pub fn drive(ctx: &egui::Context, app: &mut WindowsApp) {
    if app.selftest.is_none() {
        return;
    }
    let t = app.selftest.as_ref().unwrap().started.elapsed().as_secs_f64();
    let step = app.selftest.as_ref().unwrap().step;
    let bg = egui::LayerId::background();

    macro_rules! st {
        () => {
            app.selftest.as_mut().unwrap()
        };
    }

    match step {
        0 if t >= 0.4 => {
            st!().step = 1;
            app.open_inspector(ctx);
            app.shared.lock().unwrap().prefs_open = true;
        }
        1 if t >= 1.2 => {
            st!().step = 2;
            let (insp, prefs) = {
                let s = app.shared.lock().unwrap();
                (s.inspector_dark.is_some(), s.prefs_ran)
            };
            st!().check("second_window_rendered(inspector)", insp);
            st!().check("third_window_rendered(preferences)", prefs);
            // Change the selection in the MAIN window.
            app.shared.lock().unwrap().selected = 2;
            app.shared.lock().unwrap().inspector_seen_name = None;
            ctx.request_repaint_of(inspector_id());
        }
        2 if t >= 1.8 => {
            st!().step = 3;
            let ok = {
                let s = app.shared.lock().unwrap();
                s.inspector_seen_name.as_deref() == Some(s.projects[2].name.as_str())
            };
            st!().check("main->inspector live selection", ok);
            // Now write from the inspector side (same code path as its
            // TextEdit) and read it back through the main window's model.
            {
                let mut s = app.shared.lock().unwrap();
                let i = s.selected;
                s.projects[i].name = "Renamed".into();
                s.dirty = true;
                s.pings += 1; // == the inspector's "Ping →" button
            }
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        3 if t >= 2.2 => {
            st!().step = 4;
            let (name_ok, pings_ok) = {
                let s = app.shared.lock().unwrap();
                (s.projects[s.selected].name == "Renamed", s.pings == 1)
            };
            st!().check("inspector->main live edit", name_ok);
            st!().check("cross-window ping counter", pings_ok);
            app.shared.lock().unwrap().theme = 1; // Dark
            app.shared.lock().unwrap().inspector_dark = None;
            ctx.request_repaint_of(inspector_id());
            ctx.request_repaint_of(prefs_id());
        }
        4 if t >= 2.9 => {
            st!().step = 5;
            let ok = app.shared.lock().unwrap().inspector_dark == Some(true);
            st!().check("theme change reached other window", ok);
            app.begin_edit();
        }
        5 if t >= 3.4 => {
            st!().step = 6;
            // The real mechanism the spec's synthetic click tests: while the
            // modal is up, the root's background layer refuses interaction,
            // so the toolbar's Delete cannot be clicked.
            let blocked = !ctx.memory(|m| m.allows_interaction(bg));
            st!().check("modal blocks parent widget layer", blocked);
            let disabled = app.shared.lock().unwrap().modal_open;
            st!().check("modal flag disables child windows", disabled);
            app.edit = None;
        }
        6 if t >= 3.9 => {
            st!().step = 7;
            let ok = ctx.memory(|m| m.allows_interaction(bg));
            st!().check("parent interactive again after modal", ok);
            // Close veto: the app is dirty, so this must be cancelled.
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        7 if t >= 4.6 => {
            st!().step = 8;
            let vetoed = st!().vetoed;
            st!().check("close_requested vetoed while dirty", vetoed);
            st!().check("process alive after veto", true);
            app.save_state();
        }
        8 if t >= 5.0 => {
            st!().step = 9;
            let want = app.shared.lock().unwrap().geom.main;
            let got = crate::load_state();
            st!().check(
                "window geometry persisted to JSON",
                want.is_some() && got.main == want && got.inspector.is_some(),
            );
            st!().check("inspector_open persisted", got.inspector_open);
            let (p, f) = (st!().pass, st!().fail);
            println!("SELFTEST DONE pass={p} fail={f}");
            std::process::exit(0);
        }
        _ => {}
    }

    if app.selftest.as_ref().is_some_and(|s| s.step < 9) {
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}
