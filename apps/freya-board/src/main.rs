//! "Board" — kanban board on Freya 0.4 (SPEC-3).
//!
//! Cross-container drag-and-drop with edit-in-place. The DnD mechanics are
//! Freya's first-party `DragZone`/`DropZone` components; everything else is
//! stock elements plus `use_state` signals.

use std::time::{
    Duration,
    Instant,
};

use freya::{
    animation::*,
    prelude::*,
};

const COLUMNS: [&str; 3] = ["Todo", "Doing", "Done"];
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// Height of the "drop at the end of this column" target below the last card.
const TAIL_ZONE_H: f32 = 140.;

const BG: Color = Color::from_argb(255, 24, 26, 31);
const COLUMN_BG: Color = Color::from_argb(255, 33, 36, 43);
const CARD_BG: Color = Color::from_argb(255, 44, 48, 57);
const TEXT: Color = Color::from_argb(255, 228, 231, 238);
const MUTED: Color = Color::from_argb(255, 141, 149, 165);
const ACCENT: Color = Color::from_argb(255, 122, 162, 247);

fn main() {
    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app)
                .with_title("Board (freya)")
                .with_size(900.0, 600.0)
                .with_background(BG),
        ),
    )
}

// ---------------------------------------------------------------- model

#[derive(Clone, PartialEq)]
struct Card {
    id: u64,
    text: String,
}

/// Where a dragged card would land: column index + insertion index.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Slot {
    column: usize,
    index: usize,
}

fn seed() -> Vec<Vec<Card>> {
    let mut id = 0;
    let mut next = |text: &str| {
        id += 1;
        Card {
            id,
            text: text.to_owned(),
        }
    };
    vec![
        vec![
            next("Write the FRICTION.md template"),
            next("Measure clean build times"),
            next("Collect binary sizes"),
        ],
        vec![next("Port the kanban board"), next("Chase the DnD ghost")],
        vec![next("Pin every framework version")],
    ]
}

// ---------------------------------------------------------------- root

fn app() -> impl IntoElement {
    let mut columns = use_state(seed);
    let mut next_id = use_state(|| 7u64);

    // UI-only state.
    let mut drop_slot = use_state(|| None::<Slot>);
    let mut editing = use_state(|| None::<u64>);
    let mut edit_buffer = use_state(String::new);
    let mut adding = use_state(|| None::<usize>);
    let mut add_buffer = use_state(String::new);
    let mut last_press = use_state(|| None::<(u64, Instant)>);

    let hovered = *drop_slot.read();
    let editing_id = *editing.read();
    let adding_col = *adding.read();

    let move_card = move |card_id: u64, slot: Slot| {
        move_card_into(columns, drop_slot, card_id, slot);
    };

    let column_views: Vec<Element> = COLUMNS
        .iter()
        .enumerate()
        .map(|(col_index, title)| {
            let cards = columns.read()[col_index].clone();
            let count = cards.len();

            let mut body: Vec<Element> = Vec::with_capacity(cards.len() * 2 + 1);
            for (index, card) in cards.iter().enumerate() {
                let card_id = card.id;
                let slot = Slot {
                    column: col_index,
                    index,
                };

                // Insertion line above this card.
                body.push(drop_indicator(hovered == Some(slot)));

                let is_editing = editing_id == Some(card_id);
                let card_body: Element = if is_editing {
                    Input::new(edit_buffer)
                        .placeholder("Card text")
                        .auto_focus(true)
                        .width(Size::fill())
                        .on_submit(move |value: String| {
                            let value = value.trim().to_owned();
                            editing.set(None);
                            if value.is_empty() {
                                return;
                            }
                            for col in columns.write().iter_mut() {
                                if let Some(card) =
                                    col.iter_mut().find(|card| card.id == card_id)
                                {
                                    card.text = value;
                                    break;
                                }
                            }
                        })
                        .on_pre_key_down(move |e: Event<KeyboardEventData>| {
                            escape_aware(&e, move || editing.set(None))
                        })
                        .into()
                } else {
                    CardView {
                        id: card_id,
                        text: card.text.clone(),
                        column: col_index,
                        index,
                        on_press: EventHandler::new(move |_| {
                            // Freya has no double-click event: track the last
                            // press per card and compare timestamps.
                            let now = Instant::now();
                            let is_double = matches!(
                                *last_press.peek(),
                                Some((id, at)) if id == card_id && now.duration_since(at) < DOUBLE_CLICK
                            );
                            last_press.set(Some((card_id, now)));
                            if is_double {
                                let text = columns
                                    .peek()
                                    .iter()
                                    .flatten()
                                    .find(|c| c.id == card_id)
                                    .map(|c| c.text.clone())
                                    .unwrap_or_default();
                                edit_buffer.set(text);
                                editing.set(Some(card_id));
                            }
                        }),
                        on_delete: EventHandler::new(move |_| {
                            for col in columns.write().iter_mut() {
                                col.retain(|card| card.id != card_id);
                            }
                        }),
                    }
                    .into()
                };

                body.push(
                    DropZone::new(
                        DragZone::new(card_id, card_body.clone())
                            .drag_element(
                                rect()
                                    .width(Size::px(250.))
                                    .opacity(0.9)
                                    .child(card_body),
                            )
                            .show_while_dragging(true),
                        move |dragged: u64| move_card(dragged, slot),
                    )
                    .on_drag_over(move |over: bool| {
                        if over {
                            drop_slot.set(Some(slot));
                        } else if *drop_slot.peek() == Some(slot) {
                            drop_slot.set(None);
                        }
                    })
                    .key(card_id)
                    .into(),
                );
            }

            // Tail zone: insert at the end / drop into an empty column.
            let tail = Slot {
                column: col_index,
                index: count,
            };
            body.push(drop_indicator(hovered == Some(tail)));
            // Append target. `DropZone` has no width/height setters — it sizes
            // itself to its child — so "the empty part of the column" has to be
            // an explicitly sized child rather than the column's slack space.
            body.push(
                DropZone::new(
                    rect().width(Size::fill()).height(Size::px(TAIL_ZONE_H)),
                    move |dragged: u64| move_card(dragged, tail),
                )
                .on_drag_over(move |over: bool| {
                    if over {
                        drop_slot.set(Some(tail));
                    } else if *drop_slot.peek() == Some(tail) {
                        drop_slot.set(None);
                    }
                })
                .key(("tail", col_index))
                .into(),
            );

            let adder: Element = if adding_col == Some(col_index) {
                Input::new(add_buffer)
                    .placeholder("New card")
                    .auto_focus(true)
                    .width(Size::fill())
                    .on_submit(move |value: String| {
                        let value = value.trim().to_owned();
                        adding.set(None);
                        add_buffer.set(String::new());
                        if value.is_empty() {
                            return;
                        }
                        let id = *next_id.peek();
                        next_id.set(id + 1);
                        columns.write()[col_index].push(Card { id, text: value });
                    })
                    .on_pre_key_down(move |e: Event<KeyboardEventData>| {
                        escape_aware(&e, move || {
                            adding.set(None);
                            add_buffer.set(String::new());
                        })
                    })
                    .into()
            } else {
                Button::new()
                    .expanded()
                    .flat()
                    .on_press(move |_| {
                        add_buffer.set(String::new());
                        adding.set(Some(col_index));
                    })
                    .child("+ Add card")
                    .into()
            };

            rect()
                .width(Size::flex(1.))
                .height(Size::fill())
                // `Size::flex` on a child only works when the parent opts into
                // `Content::Flex`; without it the child silently falls back.
                .content(Content::flex())
                .background(COLUMN_BG)
                .rounded_lg()
                .padding(Gaps::new_all(10.))
                .spacing(8.)
                .child(
                    rect()
                        .horizontal()
                        .cross_align(Alignment::Center)
                        .spacing(8.)
                        .child(label().text(*title).font_size(15.).color(TEXT))
                        .child(
                            label()
                                .text(count.to_string())
                                .font_size(13.)
                                .color(MUTED),
                        ),
                )
                .child(
                    ScrollView::new()
                        .width(Size::fill())
                        .height(Size::flex(1.))
                        .children(body),
                )
                .child(adder)
                .into()
        })
        .collect();

    rect()
        .expanded()
        .content(Content::flex())
        .background(BG)
        .color(TEXT)
        .padding(Gaps::new_all(12.))
        .spacing(10.)
        .child(
            label()
                .text("drag cards between columns · double-click to edit · ✕ deletes")
                .font_size(12.)
                .color(MUTED),
        )
        .child(
            rect()
                .horizontal()
                .width(Size::fill())
                .height(Size::flex(1.))
                .content(Content::flex())
                .spacing(12.)
                .children(column_views),
        )
}

/// Detach `card_id` from wherever it currently is and re-insert it at `slot`.
fn move_card_into(
    mut columns: State<Vec<Vec<Card>>>,
    mut drop_slot: State<Option<Slot>>,
    card_id: u64,
    slot: Slot,
) {
    drop_slot.set(None);
    let mut cols = columns.write();

    let mut found = None;
    for (c, col) in cols.iter().enumerate() {
        if let Some(i) = col.iter().position(|card| card.id == card_id) {
            found = Some((c, i));
            break;
        }
    }
    let Some((from_col, from_idx)) = found else {
        return;
    };
    let card = cols[from_col].remove(from_idx);

    // The insertion index shifts left when the card was removed from an
    // earlier position in the *same* column.
    let mut index = slot.index;
    if slot.column == from_col && from_idx < index {
        index -= 1;
    }
    let index = index.min(cols[slot.column].len());
    cols[slot.column].insert(index, card);
}

/// Escape cancels the editor; everything else keeps `Input`'s stock behaviour
/// (which is not reachable any other way once `on_pre_key_down` is overridden).
fn escape_aware(e: &Event<KeyboardEventData>, cancel: impl FnOnce()) -> bool {
    match &e.key {
        Key::Named(NamedKey::Escape) => {
            cancel();
            false
        }
        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Shift) => true,
        Key::Named(NamedKey::Tab) => false,
        _ => {
            e.stop_propagation();
            e.prevent_default();
            true
        }
    }
}

/// A 2 px insertion line that only becomes visible while a drag hovers the slot.
fn drop_indicator(active: bool) -> Element {
    rect()
        .width(Size::fill())
        .height(Size::px(if active { 6. } else { 4. }))
        .padding(Gaps::new_symmetric(2., 0.))
        .child(
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .rounded_full()
                .background(if active {
                    ACCENT
                } else {
                    Color::from_argb(0, 0, 0, 0)
                }),
        )
        .into()
}

// ---------------------------------------------------------------- card

#[derive(Clone, PartialEq)]
struct CardView {
    id: u64,
    text: String,
    column: usize,
    index: usize,
    on_press: EventHandler<Event<PressEventData>>,
    on_delete: EventHandler<Event<PressEventData>>,
}

impl Component for CardView {
    fn render(&self) -> impl IntoElement {
        let mut hovering = use_state(|| false);

        // Drop/reorder animation: whenever the card's (column, index) changes —
        // i.e. it was just dropped somewhere — replay a short scale-in.
        let settle = use_animation_with_dependencies(
            &(self.column, self.index),
            |conf, _| {
                conf.on_change(OnChange::Rerun);
                AnimNum::new(0.94, 1.0)
                    .time(180)
                    .ease(Ease::Out)
                    .function(Function::Back)
            },
        );

        rect()
            .width(Size::fill())
            .background(CARD_BG)
            .rounded_md()
            .padding(Gaps::new_symmetric(8., 10.))
            .horizontal()
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .spacing(8.)
            .scale(settle.get().value())
            .border(
                Border::new()
                    .fill(if hovering() {
                        Color::from_argb(255, 72, 78, 92)
                    } else {
                        Color::from_argb(255, 56, 61, 72)
                    })
                    .width(1.0),
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(self.on_press.clone())
            .child(
                label()
                    .text(self.text.clone())
                    .width(Size::flex(1.))
                    .font_size(14.)
                    .color(TEXT),
            )
            .child(
                rect()
                    .width(Size::px(20.))
                    .height(Size::px(20.))
                    .center()
                    .rounded_sm()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Delete card")
                    .on_press(self.on_delete.clone())
                    .child(label().text("✕").font_size(12.).color(MUTED)),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.id)
    }
}
