//! "Babel" — text & i18n stress test (SPEC-5), floem git @ 778bb5f2.
//!
//! Text-stack notes (research-relevant):
//! - floem `main` renders text with parley + fontique (swash shaping):
//!   system-font discovery and per-script fallback come from fontique. No
//!   fonts are bundled by this app.
//! - The stale crates.io 0.2.0 used cosmic-text; the text stack was swapped
//!   wholesale on `main` — one more reason the version pin matters.
//! - The editor pane is the Lapce editor core (xi-rope + its own cursor /
//!   selection / clipboard logic), NOT the same code path as `Label`s.
//! - Verification hooks (env vars, stderr output):
//!   BABEL_SHOT=path      save a window screenshot after 3 s (macOS
//!                        `screencapture -R` on the window bounds — floem
//!                        has no capture API)
//!   BABEL_SELFTEST=1     drive the LIVE editor through the same
//!                        `run_command` path as arrow keys / backspace and
//!                        print grapheme/caret/selection/clipboard probes
//!   BABEL_SCROLLTEST=1   auto-load the big doc, scroll one step per frame
//!                        for 5 s, print the achieved frame rate

use std::time::{Duration, Instant};

use floem::Application;
use floem::WindowIdExt;
use floem::action::{exec_after, exec_after_animation_frame};
use floem::kurbo::{Point, Size};
use floem::prelude::*;
use floem::ui_events::keyboard::Modifiers;
use floem::views::VirtualVector;
use floem::views::editor::command::Command;
use floem::views::editor::core::command::{EditCommand, MoveCommand};
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::core::editor::EditType;
use floem::views::editor::core::selection::Selection;
use floem::window::{WindowConfig, WindowId};
use floem::{Clipboard, views::editor::Editor};

const CORPUS: &str = include_str!("../../babel-assets/corpus.txt");
const BIG_REPEATS: usize = 1_000;

fn corpus_lines() -> Vec<String> {
    CORPUS.lines().map(str::to_owned).collect()
}

fn mixed_line() -> &'static str {
    CORPUS
        .lines()
        .find(|line| line.starts_with("[MIXED]"))
        .expect("corpus has a [MIXED] line")
}

fn main() {
    // The screenshot run opens a taller window so the full corpus pane fits
    // unwrapped (SPEC-5 allows resizing for the capture).
    let size = if std::env::var("BABEL_SHOT").is_ok() {
        Size::new(1150.0, 780.0)
    } else {
        Size::new(800.0, 600.0)
    };

    Application::new()
        .window(
            app_view,
            Some(WindowConfig::default().title("Babel (floem)").size(size)),
        )
        .run();
}

fn app_view(window_id: WindowId) -> impl IntoView {
    let lines: RwSignal<Vec<String>> = RwSignal::new(corpus_lines());
    let big = RwSignal::new(false);
    let status = RwSignal::new(String::from("11 corpus lines — system fonts, no bundling"));
    // Reactive scroll target for the scripted scroll test.
    let scroll_target: RwSignal<Option<Point>> = RwSignal::new(None);

    // ----- editor pane (Lapce editor core), seeded with [MIXED] -------------
    let editor_view = text_editor(mixed_line());
    let editor = editor_view.editor().clone();
    let doc = editor_view.doc();

    let load_big = move || {
        let corpus = corpus_lines();
        let big_lines: Vec<String> = std::iter::repeat_with(|| corpus.clone())
            .take(BIG_REPEATS)
            .flatten()
            .collect();
        let count = big_lines.len();
        let started = Instant::now();
        lines.set(big_lines);
        big.set(true);
        eprintln!("load-big: {count} lines");
        exec_after_animation_frame(move |_| {
            let elapsed = started.elapsed();
            status.set(format!("big doc: {count} lines — first frame in {elapsed:.2?}"));
            eprintln!("first-frame-after-load: {elapsed:.2?}");
        });
    };

    // ----- verification hooks ------------------------------------------------
    if std::env::var("BABEL_SELFTEST").is_ok() {
        let editor = editor.clone();
        let doc = doc.clone();
        exec_after(Duration::from_millis(500), move |_| {
            selftest(&editor, doc);
        });
    }

    if let Ok(path) = std::env::var("BABEL_SHOT") {
        exec_after(Duration::from_secs(3), move |_| {
            take_screenshot(window_id, &path);
        });
    }

    if std::env::var("BABEL_SCROLLTEST").is_ok() {
        exec_after(Duration::from_secs(1), move |_| {
            load_big();
            // One scroll step per rendered frame for 5 s → achieved fps.
            exec_after(Duration::from_secs(1), move |_| {
                let started = Instant::now();
                let frames = RwSignal::new(0u32);
                fn step(
                    started: Instant,
                    frames: RwSignal<u32>,
                    scroll_target: RwSignal<Option<Point>>,
                    status: RwSignal<String>,
                ) {
                    exec_after_animation_frame(move |_| {
                        let n = frames.get_untracked() + 1;
                        frames.set(n);
                        scroll_target.set(Some(Point::new(0.0, f64::from(n) * 120.0)));
                        let elapsed = started.elapsed();
                        if elapsed >= Duration::from_secs(5) {
                            let fps = f64::from(n) / elapsed.as_secs_f64();
                            eprintln!("scroll-test: {fps:.1} fps ({n} frames / {elapsed:.2?})");
                            status.set(format!("scroll test: {fps:.1} fps"));
                        } else {
                            step(started, frames, scroll_target, status);
                        }
                    });
                }
                step(started, frames, scroll_target, status);
            });
        });
    }

    // ----- UI ----------------------------------------------------------------
    let controls = Stack::horizontal((
        Button::new(Label::derived(move || {
            if big.get() { "Big doc loaded".to_string() } else { "Load big doc (×1000)".to_string() }
        }))
        .action(load_big),
        Button::new("Reset").action(move || {
            lines.set(corpus_lines());
            big.set(false);
            status.set(String::from("11 corpus lines"));
        }),
        Label::derived(move || status.get()).style(|s| s.font_size(13.0)),
    ))
    .style(|s| s.gap(10.0).items_center().width_full());

    // Rendering pane: VirtualStack (windowed) — a plain stack would create
    // and shape all 11,000 Labels up front on big-doc load.
    let rendering = VirtualStack::full(
        move || lines.enumerate(),
        |(i, _)| *i,
        |(_, line): (usize, String)| Label::new(line).style(|s| s.font_size(15.0)),
    )
    .style(|s| s.flex_col().gap(6.0).padding(10.0).width_full())
    .scroll()
    .scroll_to(move || scroll_target.get())
    // min_height(0) is LOAD-BEARING: without it taffy sizes the scroll to its
    // min-content height, the clip never applies, and the VirtualStack
    // materializes EVERY line (11k labels, 1.9 GiB RSS) — see FRICTION.md
    // and the identical trap in floem-grid.
    .style(|s| {
        s.flex_grow(3.0)
            .flex_basis(0)
            .height_full()
            .min_height(0.0)
            .border(1.0)
            .border_radius(6.0)
    });

    let editor_pane = Stack::vertical((
        Label::new("Editor — seeded with [MIXED]; selection / caret / IME here")
            .style(|s| s.font_size(12.0)),
        editor_view.style(|s| s.flex_grow(1.0).width_full().border(1.0).border_radius(6.0)),
    ))
    .style(|s| s.flex_col().gap(6.0).flex_grow(2.0).flex_basis(0).height_full());

    Stack::vertical((
        controls,
        Stack::horizontal((rendering, editor_pane))
            .style(|s| s.gap(12.0).width_full().flex_grow(1.0).min_height(0.0)),
    ))
    .style(|s| s.flex_col().gap(10.0).padding(12.0).size_full())
}

// ---------------------------------------------------------------------------
// Self-test probes — driven through `Document::run_command`, the SAME entry
// point the keyboard handler uses, against the live on-screen editor.
// ---------------------------------------------------------------------------

fn selftest(editor: &Editor, doc: std::rc::Rc<dyn floem::views::editor::text::Document>) {
    let set_text = |text: &str| {
        let len = doc.text().len();
        doc.edit_single(
            Selection::region(0, len, CursorAffinity::Backward),
            text,
            EditType::Paste,
        );
    };
    let get_text = || {
        let rope = doc.text();
        rope.slice_to_cow(0..rope.len()).into_owned()
    };
    let run = |cmd: Command, mods: Modifiers| {
        doc.run_command(editor, &cmd, None, mods);
    };
    let offset = || editor.cursor.get_untracked().offset();
    let selection = || editor.cursor.get_untracked().get_selection();

    // --- ZWJ family emoji: caret motion + backspace ------------------------
    set_text("a👨‍👩‍👧‍👦b");
    run(Command::Move(MoveCommand::DocumentStart), Modifiers::default());
    let mut offsets = vec![offset()];
    for _ in 0..3 {
        run(Command::Move(MoveCommand::Right), Modifiers::default());
        offsets.push(offset());
    }
    eprintln!("selftest caret byte-offsets over a|family|b: {offsets:?}");

    run(Command::Move(MoveCommand::DocumentEnd), Modifiers::default());
    run(Command::Edit(EditCommand::DeleteBackward), Modifiers::default());
    run(Command::Edit(EditCommand::DeleteBackward), Modifiers::default());
    let after = get_text();
    eprintln!(
        "selftest backspace over family: {:?} (chars {}) -> {}",
        after,
        after.chars().count(),
        if after == "a" { "CLEAN (whole grapheme)" } else { "CORRUPTED/partial" }
    );

    // --- Shift+Right selection across the BiDi boundary --------------------
    set_text(mixed_line());
    run(Command::Move(MoveCommand::DocumentStart), Modifiers::default());
    for _ in 0..16 {
        run(Command::Move(MoveCommand::Right), Modifiers::SHIFT);
    }
    eprintln!("selftest select 16x shift-right: {:?}", selection());
    for _ in 0..6 {
        run(Command::Move(MoveCommand::Right), Modifiers::SHIFT);
    }
    eprintln!("selftest select 22x shift-right (into Arabic): {:?}", selection());

    // --- Clipboard round-trip through the REAL system clipboard ------------
    set_text("start ");
    run(Command::Move(MoveCommand::DocumentEnd), Modifiers::default());
    let payload = "שלום 世界 👨‍👩‍👧‍👦";
    match Clipboard::set_contents(payload.to_string()) {
        Ok(()) => {
            run(Command::Edit(EditCommand::ClipboardPaste), Modifiers::default());
            let text = get_text();
            eprintln!(
                "selftest clipboard paste round-trip: {:?} -> {}",
                text,
                if text == format!("start {payload}") { "OK" } else { "MISMATCH" }
            );
        }
        Err(error) => eprintln!("selftest clipboard: set_contents FAILED: {error:?}"),
    }

    // Leave the editor in its documented seeded state.
    set_text(mixed_line());
}

/// floem has no window-capture API (iced has `window::screenshot`). Instead:
/// resolve the native NSWindow's `windowNumber` (== CGWindowID) through the
/// new `WindowIdExt::with_window_handle` and hand it to macOS
/// `screencapture -l`, which captures ONLY this window even when occluded.
fn window_number(window_id: WindowId) -> Option<i64> {
    window_id
        .with_window_handle(|handle| match handle.as_raw() {
            raw_window_handle::RawWindowHandle::AppKit(h) => {
                let view = h.ns_view.as_ptr() as *mut objc2::runtime::AnyObject;
                unsafe {
                    let win: *mut objc2::runtime::AnyObject = objc2::msg_send![&*view, window];
                    if win.is_null() {
                        None
                    } else {
                        let num: isize = objc2::msg_send![&*win, windowNumber];
                        Some(num as i64)
                    }
                }
            }
            _ => None,
        })
        .flatten()
}

fn take_screenshot(window_id: WindowId, path: &str) {
    let mut args: Vec<String> = vec!["-x".into(), "-o".into()];
    match window_number(window_id) {
        Some(num) => args.push(format!("-l{num}")),
        None => {
            // Fallback: region capture from the window bounds.
            let Some(bounds) = window_id.bounds_on_screen_including_frame() else {
                eprintln!("screenshot: FAILED: no window handle or bounds");
                return;
            };
            args.push(format!(
                "-R{},{},{},{}",
                bounds.x0,
                bounds.y0,
                bounds.width(),
                bounds.height()
            ));
        }
    }
    args.push(path.to_string());
    match std::process::Command::new("screencapture").args(&args).status() {
        Ok(s) if s.success() => eprintln!("screenshot: saved {path}"),
        Ok(s) => eprintln!("screenshot: FAILED: screencapture exited {s}"),
        Err(error) => eprintln!("screenshot: FAILED: {error}"),
    }
}
