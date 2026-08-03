//! "Board" — kanban board with cross-column drag & drop (SPEC-3), vizia 0.4.
//!
//! Architecture notes (research-relevant):
//! - **DnD is core.** `.on_drag(|ex| ex.set_drop_data(ex.current()))` arms a
//!   drag when the pointer leaves a pressed DRAGGABLE view; `.on_over(..)`
//!   fires continuously over any view while the drag is live (gated on
//!   `ex.has_drop_data()`), which is the drop indicator; `.on_drop(..)` fires
//!   on release. No hit-testing, no global cursor subscription, no helper
//!   crate. The only hand-rolled piece is the ghost that follows the cursor.
//! - **Insertion position** comes from the drop target itself: dropping on
//!   card `i` of column `c` inserts before it, and each column has a tail
//!   drop zone that appends.
//! - **Inline edit** is a `Textbox` swapped in for the `Label` by a
//!   `Binding` on the `editing` signal; Enter is `on_submit(.., true)` and
//!   Esc is `on_cancel` — both first-class.
//! - **Animation** is CSS: the drop indicator grows via `transition: height`,
//!   and cards have hover/drag elevation transitions.

use vizia::prelude::*;

const COLUMNS: [&str; 3] = ["Todo", "Doing", "Done"];

fn main() -> Result<(), ApplicationError> {
    Application::new(|cx| {
        cx.add_stylesheet(STYLE).expect("failed to add stylesheet");

        let board = Board::new();
        let columns = board.columns;
        let dragging = board.dragging;
        let drop_at = board.drop_at;
        let editing = board.editing;
        let adding = board.adding;
        let draft = board.draft;
        let cursor = board.cursor;
        board.build(cx);

        let ghost_text = Memo::new(move |_| {
            dragging
                .get()
                .and_then(|(col, index)| {
                    columns.get()[col].get(index).map(|card: &Card| card.text.clone())
                })
                .unwrap_or_default()
        });

        HStack::new(cx, |cx| {
            for column in 0..COLUMNS.len() {
                // Per-column memo: only the column whose cards actually
                // changed rebuilds its card views.
                let cards = Memo::new(move |_| columns.get()[column].clone());
                let count = Memo::new(move |_| columns.get()[column].len().to_string());

                VStack::new(cx, |cx| {
                    HStack::new(cx, |cx| {
                        Label::new(cx, COLUMNS[column]).class("column-title");
                        Label::new(cx, count).class("column-count");
                    })
                    .class("column-header");

                    // Independent scrolling per column — a plain ScrollView.
                    ScrollView::new(cx, move |cx| {
                        VStack::new(cx, move |cx| {
                            Binding::new(cx, cards, move |cx| {
                                let list = cards.get();
                                for (index, card) in list.iter().enumerate() {
                                    drop_line(cx, column, index, drop_at);
                                    card_view(
                                        cx, column, index, card.clone(), editing, draft,
                                    );
                                }
                                let end = list.len();
                                drop_line(cx, column, end, drop_at);

                                // Tail zone: a stretchy element that catches
                                // drops below the last card and appends.
                                Element::new(cx)
                                    .class("tail-zone")
                                    .on_over(move |cx| {
                                        if cx.has_drop_data() {
                                            cx.emit(BoardEvent::DragOver(column, end));
                                        }
                                    })
                                    .on_drop(move |cx, _| {
                                        cx.emit(BoardEvent::Drop(column, end))
                                    });
                            });
                        })
                        .class("card-stack");
                    })
                    .class("column-body");

                    // "+ Add card" affordance: button, or an inline textbox
                    // while this column is the one being added to.
                    Binding::new(cx, adding, move |cx| {
                        if adding.get() == Some(column) {
                            Textbox::new(cx, draft)
                                .class("draft")
                                .placeholder("Card text…")
                                .width(Stretch(1.0))
                                .on_edit(|cx, text| cx.emit(BoardEvent::SetDraft(text)))
                                .on_submit(move |cx, text, enter| {
                                    if enter {
                                        cx.emit(BoardEvent::CommitAdd(column, text));
                                    } else {
                                        cx.emit(BoardEvent::CancelAdd);
                                    }
                                })
                                .on_cancel(|cx| cx.emit(BoardEvent::CancelAdd))
                                .on_build(|cx| {
                                    cx.focus();
                                    cx.emit(TextEvent::StartEdit);
                                });
                        } else {
                            Button::new(cx, |cx| Label::new(cx, "+ Add card"))
                                .variant(ButtonVariant::Text)
                                .class("add-button")
                                .width(Stretch(1.0))
                                .on_press(move |cx| cx.emit(BoardEvent::BeginAdd(column)));
                        }
                    });
                })
                .class("column")
                .toggle_class(
                    "column-active",
                    drop_at.map(move |d| matches!(d, Some((c, _)) if *c == column)),
                );
            }
        })
        .class("board")
        .on_mouse_move(|cx, x, y| cx.emit(BoardEvent::Cursor(x, y)));

        // Drag ghost — the one hand-rolled part of the DnD story.
        Label::new(cx, ghost_text)
            .class("ghost")
            .hoverable(false)
            .position_type(PositionType::Absolute)
            .left(cursor.map(|c| Pixels(c.0 + 10.0)))
            .top(cursor.map(|c| Pixels(c.1 + 10.0)))
            .display(dragging.map(|d| if d.is_some() { Display::Flex } else { Display::None }));
    })
    .title("Board (vizia)")
    .inner_size((900, 600))
    .run()
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

/// Insertion indicator between two cards. Height 0 normally, 6 px when it is
/// the live drop slot — animated by a CSS `transition`, which is what makes
/// the surrounding cards slide apart.
fn drop_line(cx: &mut Context, column: usize, index: usize, drop_at: Signal<Option<Slot>>) {
    Element::new(cx).class("drop-line").hoverable(false).toggle_class(
        "active",
        drop_at.map(move |d| *d == Some((column, index))),
    );
}

fn card_view(
    cx: &mut Context,
    column: usize,
    index: usize,
    card: Card,
    editing: Signal<Option<Slot>>,
    draft: Signal<String>,
) {
    HStack::new(cx, move |cx| {
        // Label <-> Textbox swap driven by a Binding on `editing`.
        Binding::new(cx, editing, {
            let text = card.text.clone();
            move |cx| {
                if editing.get() == Some((column, index)) {
                    Textbox::new(cx, draft)
                        .class("card-edit")
                        .width(Stretch(1.0))
                        .on_edit(|cx, value| cx.emit(BoardEvent::SetDraft(value)))
                        .on_submit(move |cx, value, enter| {
                            if enter {
                                cx.emit(BoardEvent::CommitEdit(column, index, value));
                            } else {
                                cx.emit(BoardEvent::CancelEdit);
                            }
                        })
                        .on_cancel(|cx| cx.emit(BoardEvent::CancelEdit))
                        .on_build(|cx| {
                            cx.focus();
                            cx.emit(TextEvent::StartEdit);
                        });
                } else {
                    Label::new(cx, text.clone())
                        .class("card-text")
                        .width(Stretch(1.0))
                        .text_wrap(true)
                        .hoverable(false);
                }
            }
        });

        Button::new(cx, |cx| Label::new(cx, "✕"))
            .variant(ButtonVariant::Text)
            .class("card-delete")
            .on_press(move |cx| cx.emit(BoardEvent::Delete(column, index)));
    })
    .class("card")
    // Every non-interactive child is hoverable(false) (see FRICTION.md):
    // vizia only runs press/drag actions when the acted-on view is itself
    // the hovered entity, so a click on the label would otherwise never
    // reach the card.
    .on_double_click(move |cx, _| cx.emit(BoardEvent::BeginEdit(column, index)))
    .on_drag(move |cx| {
        cx.set_drop_data(cx.current());
        cx.emit(BoardEvent::DragStart(column, index));
    })
    .on_over(move |cx| {
        if cx.has_drop_data() {
            cx.emit(BoardEvent::DragOver(column, index));
        }
    })
    .on_drop(move |cx, _| cx.emit(BoardEvent::Drop(column, index)));
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
struct Card {
    text: String,
}

/// (column, index)
type Slot = (usize, usize);

struct Board {
    columns: Signal<Vec<Vec<Card>>>,
    dragging: Signal<Option<Slot>>,
    drop_at: Signal<Option<Slot>>,
    editing: Signal<Option<Slot>>,
    adding: Signal<Option<usize>>,
    draft: Signal<String>,
    cursor: Signal<(f32, f32)>,
}

impl Board {
    fn new() -> Self {
        let seed = |texts: &[&str]| texts.iter().map(|t| Card { text: (*t).into() }).collect();
        Self {
            columns: Signal::new(vec![
                seed(&["Draft the RFC", "Chase the flaky test", "Write release notes"]),
                seed(&["Port the parser", "Review PR #412"]),
                seed(&["Ship 0.4.0"]),
            ]),
            dragging: Signal::new(None),
            drop_at: Signal::new(None),
            editing: Signal::new(None),
            adding: Signal::new(None),
            draft: Signal::new(String::new()),
            cursor: Signal::new((0.0, 0.0)),
        }
    }
}

enum BoardEvent {
    Cursor(f32, f32),
    DragStart(usize, usize),
    DragOver(usize, usize),
    Drop(usize, usize),
    DragEnd,
    Delete(usize, usize),
    BeginEdit(usize, usize),
    CommitEdit(usize, usize, String),
    CancelEdit,
    BeginAdd(usize),
    CommitAdd(usize, String),
    CancelAdd,
    SetDraft(String),
}

impl Model for Board {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.take(|board_event, _| match board_event {
            BoardEvent::Cursor(x, y) => {
                if self.dragging.get().is_some() {
                    let scale = cx.scale_factor();
                    // Event coordinates are physical pixels; `Pixels(..)` is
                    // logical — see FRICTION.md.
                    self.cursor.set((x / scale, y / scale));
                }
            }

            BoardEvent::DragStart(column, index) => {
                self.dragging.set(Some((column, index)));
                let scale = cx.scale_factor();
                self.cursor
                    .set((cx.mouse().cursor_x / scale, cx.mouse().cursor_y / scale));
            }

            BoardEvent::DragOver(column, index) => {
                if self.dragging.get().is_some() {
                    self.drop_at.set(Some((column, index)));
                }
            }

            BoardEvent::Drop(column, index) => {
                if let Some((from_column, from_index)) = self.dragging.get() {
                    self.columns.update(|columns| {
                        if from_index >= columns[from_column].len() {
                            return;
                        }
                        let card = columns[from_column].remove(from_index);
                        // Removing first shifts everything after it down by
                        // one within the same column.
                        let mut target = index.min(columns[column].len() + 1);
                        if column == from_column && from_index < target {
                            target -= 1;
                        }
                        let target = target.min(columns[column].len());
                        columns[column].insert(target, card);
                    });
                }
            }

            BoardEvent::DragEnd => {
                self.dragging.set(None);
                self.drop_at.set(None);
            }

            BoardEvent::Delete(column, index) => {
                self.columns.update(|columns| {
                    if index < columns[column].len() {
                        columns[column].remove(index);
                    }
                });
                self.editing.set(None);
            }

            BoardEvent::BeginEdit(column, index) => {
                let text = self.columns.get()[column][index].text.clone();
                self.draft.set(text);
                self.editing.set(Some((column, index)));
            }

            BoardEvent::CommitEdit(column, index, text) => {
                let trimmed = text.trim().to_owned();
                if !trimmed.is_empty() {
                    self.columns.update(|columns| {
                        if let Some(card) = columns[column].get_mut(index) {
                            card.text = trimmed;
                        }
                    });
                }
                self.editing.set(None);
                self.draft.set(String::new());
            }

            BoardEvent::CancelEdit => {
                self.editing.set(None);
                self.draft.set(String::new());
            }

            BoardEvent::BeginAdd(column) => {
                self.draft.set(String::new());
                self.editing.set(None);
                self.adding.set(Some(column));
            }

            BoardEvent::CommitAdd(column, text) => {
                let trimmed = text.trim().to_owned();
                if !trimmed.is_empty() {
                    self.columns.update(|columns| columns[column].push(Card { text: trimmed }));
                }
                self.draft.set(String::new());
                self.adding.set(None);
            }

            BoardEvent::CancelAdd => {
                self.adding.set(None);
                self.draft.set(String::new());
            }

            BoardEvent::SetDraft(text) => self.draft.set(text),
        });

        // A release anywhere ends the drag. This must *queue* `DragEnd`
        // rather than clear the state inline: `on_drop` runs during the same
        // MouseUp propagation and only queues its own `Drop`, so clearing
        // here directly would make every drop a no-op (see FRICTION.md).
        event.map(|window_event, _| {
            if let WindowEvent::MouseUp(MouseButton::Left) = window_event {
                if self.dragging.get().is_some() {
                    cx.emit(BoardEvent::DragEnd);
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.board {
    width: 1s;
    height: 1s;
    padding: 12px;
    horizontal-gap: 12px;
}

.column {
    width: 1s;
    height: 1s;
    padding: 8px;
    vertical-gap: 8px;
    background-color: #ffffff0e;
    border-width: 1px;
    border-color: #ffffff1c;
    corner-radius: 10px;
    transition: background-color 160ms, border-color 160ms;
}

.column.column-active {
    background-color: #4f9df71a;
    border-color: #4f9df766;
}

.column-header {
    height: auto;
    horizontal-gap: 8px;
    alignment: center;
}

.column-title { font-size: 14px; font-weight: bold; height: auto; width: 1s; }

.column-count {
    height: auto;
    min-width: 24px;
    padding: 2px 7px;
    font-size: 12px;
    background-color: #ffffff1c;
    corner-radius: 9px;
    text-align: center;
}

.column-body { width: 1s; height: 1s; }
.card-stack { width: 1s; height: auto; }

.card {
    width: 1s;
    height: auto;
    min-height: 40px;
    padding: 8px;
    horizontal-gap: 6px;
    alignment: center;
    background-color: #ffffff14;
    border-width: 1px;
    border-color: #ffffff20;
    corner-radius: 7px;
    transition: background-color 150ms, scale 150ms, shadow 150ms;
}

.card:hover {
    background-color: #ffffff22;
    scale: 1.015;
    shadow: 0px 3px 10px #00000055;
}

.card-text { font-size: 13px; height: auto; }
.card-edit { font-size: 13px; height: auto; }
.card-delete { width: 22px; height: 22px; padding: 0px; font-size: 12px; }

/* The drop indicator: zero-height by default, animated open when it is the
   live insertion slot. The transition is what makes cards slide apart. */
.drop-line {
    width: 1s;
    height: 0px;
    background-color: #4f9df7;
    corner-radius: 2px;
    transition: height 140ms;
}

.drop-line.active { height: 6px; }

.tail-zone { width: 1s; height: 60px; }

.add-button { height: 30px; font-size: 12px; }
.draft { height: auto; font-size: 13px; }

.ghost {
    width: 180px;
    height: 34px;
    padding: 8px;
    font-size: 12px;
    background-color: #4f9df7dd;
    corner-radius: 6px;
    shadow: 0px 6px 16px #00000088;
}
"#;
