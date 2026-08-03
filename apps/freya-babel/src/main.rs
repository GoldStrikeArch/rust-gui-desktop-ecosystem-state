//! "Babel" — text & i18n stress test on Freya 0.4 (SPEC-5).
//!
//! Left pane renders the shared multilingual corpus read-only; right pane is a
//! multi-line editor seeded with the [MIXED] line. "Load big doc" swaps the
//! rendering pane to the corpus repeated 1,000× (≈11k lines) behind a
//! `VirtualScrollView`.
//!
//! Env hooks (verification only):
//!   BABEL_SELFTEST=1        grapheme / caret / selection probes to stderr
//!   BABEL_SELFTEST_BIG=1    load the big document at startup
//!   BABEL_SELFTEST_SHOT=<p> screencapture the window to <p> after 3 s

use std::{
    cell::RefCell,
    rc::Rc,
    time::{
        Duration,
        Instant,
    },
};

use async_io::Timer;
use freya::{
    prelude::*,
    text_edit::{
        EditableConfig,
        EditableEvent,
        EditorHistory,
        EditorLine,
        RopeEditor,
        TextEditor,
        TextSelection,
        UseEditable,
        use_editable,
    },
};

/// The shared corpus, embedded verbatim (never retyped).
const CORPUS: &str = include_str!("../../babel-assets/corpus.txt");
const BIG_REPEATS: usize = 1_000;

const BG: Color = Color::from_argb(255, 252, 252, 253);
const PANEL: Color = Color::WHITE;
const TEXT: Color = Color::from_argb(255, 24, 26, 31);
const MUTED: Color = Color::from_argb(255, 110, 117, 130);
const LINE_BG: Color = Color::from_argb(255, 244, 245, 247);

fn corpus_lines() -> Vec<&'static str> {
    CORPUS.lines().filter(|l| !l.trim().is_empty()).collect()
}

fn mixed_line() -> String {
    corpus_lines()
        .into_iter()
        .find(|l| l.starts_with("[MIXED]"))
        .unwrap_or_default()
        .to_string()
}

fn main() {
    if std::env::var("BABEL_SELFTEST").is_ok() {
        selftest();
    }

    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app)
                .with_title("Babel (freya)")
                .with_size(800.0, 600.0)
                .with_background(BG),
        ),
    )
}

// ---------------------------------------------------------------- app

fn app() -> impl IntoElement {
    let mut big = use_state(|| false);
    let mut note = use_state(String::new);
    let editable = use_editable(mixed_line, || {
        EditableConfig::new().with_allow_changes(true)
    });

    use_hook(move || {
        if std::env::var("BABEL_SELFTEST_BIG").is_ok() {
            let started = Instant::now();
            big.set(true);
            note.set(format!(
                "big doc built in {} ms",
                started.elapsed().as_millis()
            ));
            eprintln!("selftest big-doc: state flipped in {:?}", started.elapsed());
        }
        if let Ok(path) = std::env::var("BABEL_SELFTEST_SHOT") {
            spawn(async move {
                Timer::after(Duration::from_secs(3)).await;
                screenshot_window(path);
            });
        }
    });

    let is_big = *big.read();

    rect()
        .expanded()
        .content(Content::flex())
        .background(BG)
        .color(TEXT)
        .padding(Gaps::new_all(10.))
        .spacing(8.)
        .child(
            rect()
                .horizontal()
                .spacing(8.)
                .cross_align(Alignment::Center)
                .child(
                    Button::new()
                        .compact()
                        .on_press(move |_| {
                            let started = Instant::now();
                            big.toggle();
                            let elapsed = started.elapsed();
                            note.set(format!(
                                "{} ({} ms)",
                                if *big.peek() {
                                    format!("{} lines", corpus_lines().len() * BIG_REPEATS)
                                } else {
                                    String::from("corpus")
                                },
                                elapsed.as_millis()
                            ));
                            eprintln!("big-doc toggle -> {} in {elapsed:?}", *big.peek());
                        })
                        .child(if is_big { "Show corpus" } else { "Load big doc" }),
                )
                .child(
                    label()
                        .text(if is_big {
                            format!(
                                "rendering pane: corpus ×{BIG_REPEATS} ({} lines), virtualized",
                                corpus_lines().len() * BIG_REPEATS
                            )
                        } else {
                            String::from("rendering pane: apps/babel-assets/corpus.txt")
                        })
                        .font_size(11.)
                        .color(MUTED),
                )
                .child(label().text(note.read().clone()).font_size(11.).color(MUTED)),
        )
        .child(
            rect()
                .horizontal()
                .width(Size::fill())
                .height(Size::flex(1.))
                .content(Content::flex())
                .spacing(8.)
                // ------------------------------------------- rendering pane
                .child(
                    rect()
                        .width(Size::flex(3.))
                        .height(Size::fill())
                        .content(Content::flex())
                        .background(PANEL)
                        .rounded_md()
                        .border(Border::new().fill(MUTED.with_a(60)).width(1.))
                        .padding(Gaps::new_all(6.))
                        .child(if is_big {
                            big_document_view()
                        } else {
                            corpus_view()
                        }),
                )
                // ------------------------------------------- editing pane
                .child(
                    rect()
                        .width(Size::flex(2.))
                        .height(Size::fill())
                        .content(Content::flex())
                        .background(PANEL)
                        .rounded_md()
                        .border(Border::new().fill(MUTED.with_a(60)).width(1.))
                        .padding(Gaps::new_all(6.))
                        .spacing(4.)
                        .child(
                            label()
                                .text("editable — mouse + Shift+arrow selection, ⌘C/⌘V")
                                .font_size(10.)
                                .color(MUTED),
                        )
                        .child(TextArea {
                            editable,
                            color: TEXT,
                            font_size: 15.,
                        }),
                ),
        )
}

/// The 11-line corpus: real wrapping paragraphs, one element per line.
fn corpus_view() -> Element {
    ScrollView::new()
        .width(Size::fill())
        .height(Size::flex(1.))
        .spacing(4.)
        .children(corpus_lines().into_iter().enumerate().map(|(i, line)| {
            rect()
                .key(i)
                .width(Size::fill())
                .background(if i % 2 == 0 { PANEL } else { LINE_BG })
                .padding(Gaps::new_symmetric(2., 4.))
                .child(
                    paragraph()
                        .width(Size::fill())
                        .font_size(15.)
                        .line_height(1.45)
                        .color(TEXT)
                        .span(line),
                )
                .into()
        }))
        .into()
}

/// ~11k lines. `VirtualScrollView` only builds the visible rows, so this stays
/// interactive; the trade-off is a fixed row height, so rows are single-line.
fn big_document_view() -> Element {
    let lines = corpus_lines();
    let total = lines.len() * BIG_REPEATS;
    VirtualScrollView::new_with_data(lines, move |index: usize, lines: &Vec<&'static str>| {
        let line = lines[index % lines.len()];
        rect()
            .width(Size::fill())
            .height(Size::px(24.))
            .background(if index % 2 == 0 { PANEL } else { LINE_BG })
            .padding(Gaps::new_symmetric(0., 4.))
            .child(
                paragraph()
                    .width(Size::fill())
                    .font_size(14.)
                    .max_lines(1)
                    .color(TEXT)
                    .span(format!("{:>6}  {line}", index + 1)),
            )
            .into()
    })
    .length(total)
    .item_size(24.)
    .width(Size::fill())
    .height(Size::flex(1.))
    .into()
}

fn screenshot_window(path: String) {
    Platform::get().with_window(None, move |window| {
        let scale = window.scale_factor();
        let Ok(pos) = window.outer_position() else {
            eprintln!("screenshot: FAILED (no outer_position)");
            return;
        };
        let size = window.outer_size();
        let region = format!(
            "{},{},{},{}",
            (pos.x as f64 / scale).round() as i32,
            (pos.y as f64 / scale).round() as i32,
            (size.width as f64 / scale).round() as i32,
            (size.height as f64 / scale).round() as i32,
        );
        match std::process::Command::new("screencapture")
            .args(["-x", "-o", &format!("-R{region}"), &path])
            .status()
        {
            Ok(s) if s.success() => eprintln!("screenshot: saved {path}"),
            Ok(s) => eprintln!("screenshot: FAILED (status {s})"),
            Err(error) => eprintln!("screenshot: FAILED: {error}"),
        }
    });
}

// ---------------------------------------------------------------- self-test

/// Probes run against `RopeEditor` — the *same* editor core the UI drives, so
/// caret motion / deletion / selection here are faithful to arrow keys and
/// Backspace in the widget. No window or runtime required.
fn selftest() {
    fn editor(text: &str) -> RopeEditor {
        RopeEditor::new(
            text.to_string(),
            TextSelection::new_cursor(0),
            4,
            EditorHistory::new(Duration::from_millis(10)),
        )
    }
    fn press(ed: &mut RopeEditor, key: Key, modifiers: Modifiers) {
        ed.process_key(&key, &modifiers, false, true, false, false);
    }
    let right = Key::Named(NamedKey::ArrowRight);
    let backspace = Key::Named(NamedKey::Backspace);

    // --- ZWJ family emoji: caret motion --------------------------------
    let family = "a👨‍👩‍👧‍👦b";
    let mut ed = editor(family);
    let mut positions = vec![ed.cursor_pos()];
    for _ in 0..3 {
        press(&mut ed, right.clone(), Modifiers::empty());
        positions.push(ed.cursor_pos());
    }
    let family_utf16 = "👨\u{200d}👩\u{200d}👧\u{200d}👦".encode_utf16().count();
    eprintln!(
        "selftest family cluster: 1 grapheme, {} chars, {family_utf16} utf16 units (spans offsets 1..{})",
        "👨\u{200d}👩\u{200d}👧\u{200d}👦".chars().count(),
        1 + family_utf16
    );
    eprintln!("selftest caret utf16 offsets over a|family|b: {positions:?}");
    eprintln!(
        "selftest caret verdict: {}",
        if positions.contains(&(1 + family_utf16)) {
            "GRAPHEME-ATOMIC (family crossed in one step)"
        } else if positions.iter().any(|p| *p == 2) {
            "PER-UTF16-UNIT (offset 2 lands INSIDE the first surrogate pair)"
        } else {
            "PER-SCALAR (family needs several steps)"
        }
    );

    // --- Backspace over the family -------------------------------------
    let mut ed = editor(family);
    for _ in 0..3 {
        press(&mut ed, right.clone(), Modifiers::empty());
    }
    press(&mut ed, backspace.clone(), Modifiers::empty()); // deletes 'b'
    press(&mut ed, backspace.clone(), Modifiers::empty()); // should delete the family
    let after = ed.rope().to_string();
    eprintln!(
        "selftest backspace over family: {after:?} (chars {}) -> {}",
        after.chars().count(),
        if after == "a" {
            "CLEAN (whole grapheme)"
        } else {
            "CORRUPTED/partial"
        }
    );

    // --- Shift+Right selection across the BiDi boundary ----------------
    let mixed = mixed_line();
    let mut ed = editor(&mixed);
    for _ in 0..16 {
        press(&mut ed, right.clone(), Modifiers::SHIFT);
    }
    eprintln!(
        "selftest select 16x right: {:?} -> {:?}",
        ed.get_selection_range(),
        ed.get_selected_text()
    );
    for _ in 0..6 {
        press(&mut ed, right.clone(), Modifiers::SHIFT);
    }
    eprintln!(
        "selftest select 22x right (into Arabic): {:?} -> {:?}",
        ed.get_selection_range(),
        ed.get_selected_text()
    );

    // --- Insertion round-trip of a multi-script string -----------------
    let mut ed = editor("start ");
    for _ in 0..6 {
        press(&mut ed, right.clone(), Modifiers::empty());
    }
    let pasted = "שלום 世界 👨‍👩‍👧‍👦";
    ed.insert(pasted, ed.utf16_cu_to_char(ed.cursor_pos()));
    eprintln!("selftest insert round-trip: {:?}", ed.rope().to_string());

    // --- Grapheme inventory of the corpus ------------------------------
    for line in corpus_lines() {
        let tag = line.split(']').next().unwrap_or("").trim_start_matches('[');
        eprintln!(
            "selftest corpus [{tag}]: {} chars, {} utf16, {} bytes",
            line.chars().count(),
            line.encode_utf16().count(),
            line.len()
        );
    }
    eprintln!("selftest done");
}

// ---------------------------------------------------------------- text area

/// Multi-line editor assembled from `use_editable` (Freya's `Input` is
/// single-line only). One `paragraph` per line with a persistent
/// `ParagraphHolder` so a click can be mapped back to a character offset.
#[derive(Clone, PartialEq)]
struct TextArea {
    editable: UseEditable,
    color: Color,
    font_size: f32,
}

impl Component for TextArea {
    fn render(&self) -> impl IntoElement {
        let mut editable = self.editable;
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut area = use_state(Area::default);
        let mut dragging = use_state(|| false);
        let holders = use_hook(|| Rc::new(RefCell::new(Vec::<ParagraphHolder>::new())));

        let is_focused = focus().is_focused();

        let (lines, cursor_row, cursor_col) = {
            let editor = editable.editor().read();
            let n = editor.len_lines();
            holders
                .borrow_mut()
                .resize_with(n, ParagraphHolder::default);
            let lines: Vec<(String, Option<(usize, usize)>)> = (0..n)
                .map(|i| {
                    (
                        editor
                            .line(i)
                            .map(|l| l.text.trim_end_matches('\n').to_string())
                            .unwrap_or_default(),
                        editor.get_visible_selection(EditorLine::Paragraph(i)),
                    )
                })
                .collect();
            (lines, editor.cursor_row(), editor.cursor_col())
        };

        let color = self.color;
        let font_size = self.font_size;
        let holders_for_down = holders.clone();
        let holders_for_move = holders.clone();

        let line_elements: Vec<Element> = lines
            .into_iter()
            .enumerate()
            .map(|(index, (text, selection))| {
                let holders = holders_for_down.clone();
                let holder = holders.borrow()[index].clone();
                paragraph()
                    .key(index)
                    .width(Size::fill())
                    .holder(holder)
                    .color(color)
                    .font_size(font_size)
                    .line_height(1.45)
                    .cursor_index((is_focused && index == cursor_row).then_some(cursor_col))
                    .cursor_color(color)
                    .highlights(selection.map(|s| vec![s]))
                    .span(if text.is_empty() {
                        String::from(" ")
                    } else {
                        text
                    })
                    .on_focus_press(move |e: Event<FocusPressEventData>| {
                        e.stop_propagation();
                        e.prevent_default();
                        dragging.set(true);
                        editable.process_event(EditableEvent::Down {
                            location: e.element_location(),
                            editor_line: EditorLine::Paragraph(index),
                            holder: &holders.borrow()[index],
                        });
                        a11y_id.request_focus();
                    })
                    .into()
            })
            .collect();

        rect()
            .expanded()
            .content(Content::flex())
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::MultilineTextInput)
            .a11y_alt("Editable multilingual text")
            .on_focus_press(move |e: Event<FocusPressEventData>| {
                e.stop_propagation();
                e.prevent_default();
                a11y_id.request_focus();
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                e.stop_propagation();
                editable.process_event(EditableEvent::KeyDown {
                    key: &e.key,
                    modifiers: e.modifiers,
                });
            })
            .on_key_up(move |e: Event<KeyboardEventData>| {
                editable.process_event(EditableEvent::KeyUp { key: &e.key });
            })
            .on_global_pointer_move(move |e: Event<PointerEventData>| {
                if !*dragging.peek() {
                    return;
                }
                let origin = area.peek().origin;
                let mut location = e.global_location();
                location.x -= origin.x as f64;
                location.y -= origin.y as f64;
                let row = {
                    let editor = editable.editor().peek();
                    editor.cursor_row().min(editor.len_lines().saturating_sub(1))
                };
                let holder = holders_for_move.borrow()[row].clone();
                editable.process_event(EditableEvent::Move {
                    location,
                    editor_line: EditorLine::Paragraph(row),
                    holder: &holder,
                });
            })
            .on_global_pointer_press(move |_: Event<PointerEventData>| {
                if *dragging.peek() {
                    dragging.set(false);
                    editable.process_event(EditableEvent::Release);
                }
            })
            .child(
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::flex(1.))
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new_all(6.))
                            .on_sized(move |e: Event<SizedEventData>| area.set(e.visible_area))
                            .children(line_elements),
                    ),
            )
    }
}
