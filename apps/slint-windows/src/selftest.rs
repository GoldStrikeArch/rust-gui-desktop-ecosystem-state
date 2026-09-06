// Verification harness (WINDOWS_SELFTEST=1). Everything here is test code.
//
// It drives the app through the same callbacks the UI uses, and — for the
// modality check — through the REAL input pipeline via
// `Window::try_dispatch_event`, so the click has to survive hit-testing.
// Prints `SELFTEST DONE pass=N fail=M` and exits 0.

use crate::{App, close_dialog, open_dialog};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, Timer, TimerMode};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

thread_local! {
    static PASS: Cell<u32> = const { Cell::new(0) };
    static FAIL: Cell<u32> = const { Cell::new(0) };
    static LIGHT: Cell<(f64, f64, f64)> = const { Cell::new((0.0, 0.0, 0.0)) };
}

fn check(name: &str, ok: bool, detail: impl std::fmt::Display) {
    if ok {
        PASS.with(|c| c.set(c.get() + 1));
        println!("PASS {name}: {detail}");
    } else {
        FAIL.with(|c| c.set(c.get() + 1));
        println!("FAIL {name}: {detail}");
    }
}

/// Mean luminance of a window's rendered pixels — used as objective evidence
/// that a theme switch repainted *that* window.
fn luma(w: &slint::Window) -> f64 {
    match w.take_snapshot() {
        Ok(buf) => {
            let px = buf.as_slice();
            px.iter()
                .map(|p| 0.2126 * p.r as f64 + 0.7152 * p.g as f64 + 0.0722 * p.b as f64)
                .sum::<f64>()
                / px.len() as f64
        }
        Err(_) => f64::NAN,
    }
}

fn snapshot(w: &slint::Window, name: &str) {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|a| a.join("evidence")))
        .unwrap_or_else(|| "evidence".into());
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(buf) = w.take_snapshot() {
        let (w, h) = (buf.width(), buf.height());
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for p in buf.as_slice() {
            out.extend_from_slice(&[p.r, p.g, p.b]);
        }
        let path = dir.join(format!("{name}.ppm"));
        let _ = std::fs::write(&path, out);
        println!("SNAPSHOT {}", path.display());
    }
}

fn click(w: &slint::Window, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    let _ = w.try_dispatch_event(WindowEvent::PointerMoved { position });
    let _ = w.try_dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    let _ = w.try_dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

fn key(w: &slint::Window, text: &str) {
    let t: slint::SharedString = text.into();
    let _ = w.try_dispatch_event(WindowEvent::KeyPressed { text: t.clone() });
    let _ = w.try_dispatch_event(WindowEvent::KeyReleased { text: t });
}

pub fn run(app: Rc<App>) {
    let mut steps: Vec<(u64, Box<dyn Fn()>)> = Vec::new();
    let mut at = |ms: u64, f: Box<dyn Fn()>| steps.push((ms, f));

    // 1. one window
    {
        let a = app.clone();
        at(
            400,
            Box::new(move || {
                check(
                    "single-window-at-start",
                    a.main.window().is_visible()
                        && !a.insp.window().is_visible()
                        && !a.prefs.window().is_visible(),
                    "main visible, inspector/prefs hidden",
                );
                println!("SCALE_FACTOR main={}", a.main.window().scale_factor());
            }),
        );
    }
    // 2. inspector opens; second request focuses instead of duplicating
    {
        let a = app.clone();
        at(
            700,
            Box::new(move || {
                a.main.invoke_toggle_inspector();
            }),
        );
    }
    {
        let a = app.clone();
        at(
            1000,
            Box::new(move || {
                check(
                    "inspector-opens",
                    a.insp.window().is_visible() && a.shared().insp_open,
                    "InspectorWindow visible",
                );
                crate::open_inspector(&a); // 2nd request
                check(
                    "inspector-singleton",
                    a.insp.window().is_visible() && a.shared().insp_open,
                    "second request focused the same instance",
                );
            }),
        );
    }
    // 3. preferences window
    {
        let a = app.clone();
        at(
            1300,
            Box::new(move || {
                a.main.invoke_open_prefs();
            }),
        );
    }
    {
        let a = app.clone();
        at(
            1600,
            Box::new(move || {
                check(
                    "three-windows",
                    a.main.window().is_visible()
                        && a.insp.window().is_visible()
                        && a.prefs.window().is_visible(),
                    "main + inspector + preferences",
                );
            }),
        );
    }
    // 4. selection propagates main -> inspector
    {
        let a = app.clone();
        at(
            1800,
            Box::new(move || {
                a.main.invoke_select(2);
            }),
        );
    }
    {
        let a = app.clone();
        at(
            2000,
            Box::new(move || {
                let want = a.projects.row_data(2).unwrap().name;
                check(
                    "selection-to-inspector",
                    a.insp.get_shown_name() == want,
                    format!("inspector shows {:?}", a.insp.get_shown_name()),
                );
            }),
        );
    }
    // 5. typing in the inspector updates the shared model (and the main list)
    {
        let a = app.clone();
        at(
            2200,
            Box::new(move || {
                a.insp.invoke_focus_name();
                for c in ["!", "!"] {
                    key(a.insp.window(), c);
                }
            }),
        );
    }
    {
        let a = app.clone();
        at(
            2500,
            Box::new(move || {
                let model_name = a.projects.row_data(2).unwrap().name;
                check(
                    "inspector-edit-to-model",
                    model_name.contains("!!"),
                    format!("model row 2 = {model_name:?} (typed through the real key pipeline)"),
                );
                check("dirty-flag-set", a.shared().dirty, "dirty = true after edit");
                snapshot(a.main.window(), "main-after-inspector-edit");
                snapshot(a.insp.window(), "inspector-after-edit");
            }),
        );
    }
    // 6. cross-window messages
    {
        let a = app.clone();
        at(
            2700,
            Box::new(move || {
                let before = a.shared().pings;
                a.insp.invoke_ping();
                check(
                    "ping-inspector-to-main",
                    a.shared().pings == before + 1,
                    format!("pings {before} -> {}", a.shared().pings),
                );
                a.main.invoke_pong();
                check("pong-main-to-inspector", a.shared().flash, "flash = true");
            }),
        );
    }
    {
        let a = app.clone();
        at(
            3200,
            Box::new(move || {
                check(
                    "pong-clears-after-300ms",
                    !a.shared().flash,
                    "flash back to false",
                );
            }),
        );
    }
    // 7. modality: with the modal open a real click on Delete must do nothing
    {
        let a = app.clone();
        at(
            3400,
            Box::new(move || {
                open_dialog(&a);
            }),
        );
    }
    {
        let a = app.clone();
        at(
            3800,
            Box::new(move || {
                let before = a.projects.row_count();
                click(a.main.window(), a.main.get_del_x(), a.main.get_del_y());
                let after = a.projects.row_count();
                check(
                    "modal-blocks-parent",
                    before == after,
                    format!(
                        "rows {before} -> {after} after a dispatched click on Delete \
                         (overlay={}, AppKit sheet={})",
                        a.main.get_overlay_enabled(),
                        a.sheet_ok.get()
                    ),
                );
                snapshot(a.main.window(), "main-with-modal");
            }),
        );
    }
    {
        let a = app.clone();
        at(
            4000,
            Box::new(move || {
                close_dialog(&a);
                check(
                    "modal-closes",
                    !a.shared().blocked,
                    "blocked cleared, focus returned to main",
                );
            }),
        );
    }
    // 8. control click without the modal really does delete (control test)
    {
        let a = app.clone();
        at(
            4300,
            Box::new(move || {
                let before = a.projects.row_count();
                click(a.main.window(), a.main.get_del_x(), a.main.get_del_y());
                let after = a.projects.row_count();
                check(
                    "delete-works-without-modal",
                    after == before - 1,
                    format!("rows {before} -> {after} (same dispatched click)"),
                );
            }),
        );
    }
    // 9. preferences restyle every open window. NOTE: Slint's `changed`
    //    handlers are deferred to the next event-loop iteration, so light and
    //    dark have to be sampled in separate steps.
    {
        let a = app.clone();
        at(4600, Box::new(move || { a.prefs.invoke_set_theme(0); }));
    }
    {
        let a = app.clone();
        at(
            4800,
            Box::new(move || {
                LIGHT.with(|c| {
                    c.set((
                        luma(a.main.window()),
                        luma(a.insp.window()),
                        luma(a.prefs.window()),
                    ))
                });
                snapshot(a.main.window(), "main-light");
                a.prefs.invoke_set_theme(1); // Dark
            }),
        );
    }
    {
        let a = app.clone();
        at(
            5000,
            Box::new(move || {
                let (m0, i0, p0) = LIGHT.with(|c| c.get());
                let (m1, i1, p1) = (
                    luma(a.main.window()),
                    luma(a.insp.window()),
                    luma(a.prefs.window()),
                );
                check(
                    "theme-applies-to-all-windows",
                    m1 < m0 - 10.0 && i1 < i0 - 10.0 && p1 < p0 - 10.0,
                    format!(
                        "mean luma light->dark: main {m0:.0}->{m1:.0} insp {i0:.0}->{i1:.0} prefs {p0:.0}->{p1:.0}"
                    ),
                );
                snapshot(a.main.window(), "main-dark");
                a.prefs.invoke_set_compact(true);
            }),
        );
    }
    {
        let a = app.clone();
        at(
            5200,
            Box::new(move || {
                check("compact-rows", a.shared().compact, "compact = true");
                snapshot(a.main.window(), "main-compact-dark");
                a.prefs.invoke_set_theme(0);
                a.prefs.invoke_set_compact(false);
            }),
        );
    }
    // 10. child window closes without quitting the app
    {
        let a = app.clone();
        at(
            5500,
            Box::new(move || {
                a.insp.invoke_request_close();
            }),
        );
    }
    {
        let a = app.clone();
        at(
            5800,
            Box::new(move || {
                check(
                    "child-close-keeps-app-alive",
                    !a.insp.window().is_visible() && a.main.window().is_visible(),
                    "inspector hidden, main still up",
                );
                crate::open_inspector(&a);
            }),
        );
    }
    // 11. close veto
    {
        let a = app.clone();
        at(
            6100,
            Box::new(move || {
                a.set_shared(|s| s.dirty = true);
                a.main.invoke_request_close();
            }),
        );
    }
    {
        let a = app.clone();
        at(
            6400,
            Box::new(move || {
                check(
                    "close-veto-prompts",
                    a.prompt.window().is_visible() && a.main.window().is_visible(),
                    "SavePrompt up, main window survived CloseRequested",
                );
                a.prompt.invoke_cancel();
            }),
        );
    }
    {
        let a = app.clone();
        at(
            6700,
            Box::new(move || {
                check(
                    "close-veto-cancel-keeps-window",
                    a.main.window().is_visible() && !a.prompt.window().is_visible(),
                    "Cancel kept the main window",
                );
                // persistence: move the window, then let Discard save + quit
                a.main
                    .window()
                    .set_position(slint::PhysicalPosition::new(120, 120));
            }),
        );
    }
    {
        let a = app.clone();
        at(
            7000,
            Box::new(move || {
                let p = a.main.window().position();
                println!("MOVED_TO {} {}", p.x, p.y);
                a.main.invoke_request_close();
            }),
        );
    }
    {
        let a = app.clone();
        at(
            7300,
            Box::new(move || {
                a.prompt.invoke_discard(); // saves state + quits
                let saved = std::fs::read_to_string(crate::state_path()).unwrap_or_default();
                check(
                    "persistence-file-written",
                    saved.contains("main ") && saved.contains("insp_open 1"),
                    saved.replace('\n', " | ").trim().to_string(),
                );
                println!(
                    "SELFTEST DONE pass={} fail={}",
                    PASS.with(|c| c.get()),
                    FAIL.with(|c| c.get())
                );
                let _ = slint::quit_event_loop();
            }),
        );
    }

    let timers: Vec<Timer> = steps
        .into_iter()
        .map(|(ms, f)| {
            let t = Timer::default();
            t.start(TimerMode::SingleShot, Duration::from_millis(ms), f);
            t
        })
        .collect();
    std::mem::forget(timers); // verification-only leak
    println!("SELFTEST ARMED");
}
