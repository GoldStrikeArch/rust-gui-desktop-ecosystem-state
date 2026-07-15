// Babel (slint) — SPEC-5 text & i18n stress test.

use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

slint::include_modules!();

const CORPUS: &str = include_str!("../../babel-assets/corpus.txt");
const BIG_DOC_REPEATS: usize = 1000; // ~11k lines

fn corpus_lines() -> Vec<SharedString> {
    CORPUS.lines().map(SharedString::from).collect()
}

fn rss_mib() -> f64 {
    // ps is good enough for a qualitative memory note.
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|kib| kib / 1024.0)
        .unwrap_or(f64::NAN)
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = BabelWindow::new()?;

    let model: Rc<VecModel<SharedString>> = Rc::new(VecModel::from(corpus_lines()));
    ui.set_corpus_lines(ModelRc::from(model.clone()));
    ui.set_doc_info(format!("{} lines · {:.0} MiB RSS", model.row_count(), rss_mib()).into());

    // Seed the editor with the [MIXED] line.
    let mixed = CORPUS
        .lines()
        .find(|l| l.starts_with("[MIXED]"))
        .unwrap_or("[MIXED] missing");
    ui.set_editor_text(mixed.into());

    ui.on_load_big_doc({
        let ui = ui.as_weak();
        let model = model.clone();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let t0 = Instant::now();
            let lines = corpus_lines();
            let mut big = Vec::with_capacity(lines.len() * BIG_DOC_REPEATS);
            for _ in 0..BIG_DOC_REPEATS {
                big.extend(lines.iter().cloned());
            }
            let n = big.len();
            model.set_vec(big);
            let elapsed = t0.elapsed();
            ui.set_doc_info(
                format!(
                    "{} lines (set_vec {} ms) · {:.0} MiB RSS",
                    n,
                    elapsed.as_millis(),
                    rss_mib()
                )
                .into(),
            );
            eprintln!("[babel] big doc: {n} lines in {elapsed:?}, rss {:.0} MiB", rss_mib());
        }
    });

    ui.on_reset_doc({
        let ui = ui.as_weak();
        let model = model.clone();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            model.set_vec(corpus_lines());
            ui.set_doc_info(
                format!("{} lines · {:.0} MiB RSS", model.row_count(), rss_mib()).into(),
            );
        }
    });

    // Deterministic placement so `screencapture -R` can grab the window
    // even on a desktop crowded by other test apps.
    ui.window().set_position(slint::LogicalPosition::new(60.0, 60.0));

    // BABEL_TEST_SCROLL=1: load the big doc after 2 s, then sweep the
    // rendering pane viewport over 4 s (25 steps) to exercise lazy
    // instantiation the same way a fast scrollbar drag would, logging step
    // latency (uninstantiated rows are created synchronously on reveal).
    let scroll_timer = slint::Timer::default();
    let sweep_timer = slint::Timer::default();
    if std::env::var("BABEL_TEST_SCROLL").is_ok() {
        let weak = ui.as_weak();
        scroll_timer.start(slint::TimerMode::SingleShot, Duration::from_secs(2), move || {
            let Some(ui) = weak.upgrade() else { return };
            ui.invoke_load_big_doc();
            let weak2 = ui.as_weak();
            let step = std::cell::Cell::new(0u32);
            let t_start = Instant::now();
            sweep_timer.start(
                slint::TimerMode::Repeated,
                Duration::from_millis(160),
                move || {
                    let Some(ui) = weak2.upgrade() else { return };
                    let i = step.get();
                    if i >= 25 {
                        eprintln!(
                            "[babel] sweep done in {:?}, rss {:.0} MiB",
                            t_start.elapsed(),
                            rss_mib()
                        );
                        std::process::exit(0);
                    }
                    let t = Instant::now();
                    // viewport-y is negative when scrolled down
                    let total = 26.0 * 11.0 * BIG_DOC_REPEATS as f32;
                    ui.set_render_viewport_y(-(total * (i as f32) / 25.0));
                    slint::platform::update_timers_and_animations();
                    eprintln!("[babel] step {i}: set viewport in {:?}", t.elapsed());
                    step.set(i + 1);
                },
            );
        });
    }

    // BABEL_TEST_EDIT=1: drive the editor through the same platform-event
    // pipeline real input uses (slint::platform::WindowEvent dispatched at
    // the window), immune to OS focus stealing. Reports via stderr and exits.
    let edit_timer = slint::Timer::default();
    if std::env::var("BABEL_TEST_EDIT").is_ok() {
        let weak = ui.as_weak();
        edit_timer.start(slint::TimerMode::SingleShot, Duration::from_secs(2), move || {
            let Some(ui) = weak.upgrade() else { return };
            use slint::platform::{Key, WindowEvent};
            let win = ui.window();
            let key = |k: Key| {
                win.dispatch_event(WindowEvent::KeyPressed { text: k.into() });
                win.dispatch_event(WindowEvent::KeyReleased { text: k.into() });
            };
            let type_str = |s: &str| {
                win.dispatch_event(WindowEvent::KeyPressed { text: s.into() });
                win.dispatch_event(WindowEvent::KeyReleased { text: s.into() });
            };

            ui.invoke_focus_editor();
            // Key::End/Home are unmapped on Apple targets (i-slint-core
            // text.rs cfg's them out); go-to-end := select-all + RightArrow.
            let go_end = || {
                ui.invoke_editor_select_all();
                key(Key::RightArrow);
            };

            // T1: backspace over a ZWJ family emoji (one grapheme?)
            ui.set_editor_text("a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}b".into());
            go_end();
            key(Key::Backspace);
            let after_b1 = ui.get_editor_text();
            let mut n = 0;
            while ui.get_editor_text() != "a" && n < 12 {
                key(Key::Backspace);
                n += 1;
            }
            eprintln!("[edit] T1 backspace: after1={:?}, family gone after {} more", after_b1, n);

            // T2: arrow over the family, then insert X
            ui.set_editor_text("a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}b".into());
            go_end();
            key(Key::LeftArrow);
            key(Key::LeftArrow);
            type_str("X");
            eprintln!("[edit] T2 arrow+insert: {:?}", ui.get_editor_text());

            // T3: Shift+Left x5 across the BiDi boundary, replace selection with Y
            ui.set_editor_text("abc \u{5E9}\u{5DC}\u{5D5}\u{5DD} def".into()); // abc שלום def
            go_end();
            win.dispatch_event(WindowEvent::KeyPressed { text: Key::Shift.into() });
            for _ in 0..5 {
                key(Key::LeftArrow);
            }
            win.dispatch_event(WindowEvent::KeyReleased { text: Key::Shift.into() });
            type_str("Y");
            eprintln!("[edit] T3 shift-left-5 + Y: {:?}", ui.get_editor_text());

            // T4: mouse-drag selection across the BiDi boundary of the MIXED
            // line, then delete the selection.
            let mixed = CORPUS.lines().find(|l| l.starts_with("[MIXED]")).unwrap();
            ui.set_editor_text(mixed.into());
            let ex = ui.get_editor_abs_x();
            let ey = ui.get_editor_abs_y();
            let y = ey + 18.0;
            use slint::LogicalPosition as LP;
            use slint::platform::PointerEventButton as PB;
            win.dispatch_event(WindowEvent::PointerPressed {
                position: LP::new(ex + 60.0, y),
                button: PB::Left,
            });
            for i in 1..=12 {
                win.dispatch_event(WindowEvent::PointerMoved {
                    position: LP::new(ex + 60.0 + (i as f32) * 20.0, y),
                });
            }
            win.dispatch_event(WindowEvent::PointerReleased {
                position: LP::new(ex + 300.0, y),
                button: PB::Left,
            });
            key(Key::Backspace);
            eprintln!("[edit] T4 mouse-drag x=+60..+300 then backspace: {:?}", ui.get_editor_text());

            // T5: copy/paste round-trip through the real system clipboard.
            ui.set_editor_text("קפה coffee 커피".into());
            ui.invoke_editor_select_all();
            ui.invoke_editor_copy();
            go_end();
            type_str("|");
            ui.invoke_editor_paste();
            eprintln!("[edit] T5 copy/paste roundtrip: {:?}", ui.get_editor_text());

            std::process::exit(0);
        });
    }

    ui.run()
}
