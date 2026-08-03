//! "Board" — kanban task board (SPEC-3), floem git main @ 778bb5f2.
//!
//! Architecture notes (research-relevant):
//! - Cross-container DnD rides on floem's built-in drag system: every card is
//!   `.draggable_with_config()` (custom_data = card id) and every card AND
//!   every column tail is a drag target. `DragTargetEnter` performs a LIVE
//!   move of the dragged card into the hovered slot, so the board itself is
//!   the drop preview; the source card's slot is styled as the drop
//!   indicator. The ghost following the cursor and the spring animation on
//!   release are framework built-ins (`dragging_style`, `DragConfig`).
//! - State: one `RwSignal<Vec<Card>>` per column. `dyn_stack` keyed by card
//!   id diffs each column; card text is re-derived reactively so an inline
//!   edit updates the existing row view in place.

use floem::Application;
use floem::event::DragConfig;
use floem::kurbo::Size;
use floem::prelude::*;
use floem::style::Transition;
use floem::window::WindowConfig;

const BG_COLUMN: Color = Color::from_rgb8(0xf0, 0xf0, 0xf3);
const BG_CARD: Color = Color::from_rgb8(0xff, 0xff, 0xff);
const BORDER: Color = Color::from_rgb8(0xc9, 0xc9, 0xd2);
const ACCENT: Color = Color::from_rgb8(0x3b, 0x6f, 0xe0);
const TEXT_DIM: Color = Color::from_rgb8(0x70, 0x70, 0x7a);
const TEXT_MAIN: Color = Color::from_rgb8(0x20, 0x20, 0x28);

#[derive(Clone, PartialEq, Eq, Hash)]
struct Card {
    id: u64,
    text: String,
}

/// All board state. Signals are `Copy`, so this struct is freely captured.
#[derive(Clone, Copy)]
struct Board {
    columns: [RwSignal<Vec<Card>>; 3],
    /// Card id currently being dragged (styles its slot as drop indicator).
    dragging: RwSignal<Option<u64>>,
    /// Card id currently in inline-edit mode + its edit buffer.
    editing: RwSignal<Option<u64>>,
    edit_buf: RwSignal<String>,
    /// Column index with an open "+ Add card" input + its buffer.
    adding: RwSignal<Option<usize>>,
    add_buf: RwSignal<String>,
    next_id: RwSignal<u64>,
}

const COLUMN_NAMES: [&str; 3] = ["Todo", "Doing", "Done"];

impl Board {
    fn new() -> Self {
        let seed = |ids: &[(u64, &str)]| {
            RwSignal::new(
                ids.iter()
                    .map(|(id, text)| Card { id: *id, text: text.to_string() })
                    .collect::<Vec<_>>(),
            )
        };
        Self {
            columns: [
                seed(&[(0, "Write SPEC-3 app"), (1, "Sketch column layout")]),
                seed(&[(2, "Port DnD to floem")]),
                seed(&[(3, "Pick a framework list")]),
            ],
            dragging: RwSignal::new(None),
            editing: RwSignal::new(None),
            edit_buf: RwSignal::new(String::new()),
            adding: RwSignal::new(None),
            add_buf: RwSignal::new(String::new()),
            next_id: RwSignal::new(4),
        }
    }

    fn card_text(&self, id: u64) -> String {
        for column in &self.columns {
            if let Some(text) =
                column.with(|cards| cards.iter().find(|c| c.id == id).map(|c| c.text.clone()))
            {
                return text;
            }
        }
        String::new()
    }

    fn locate(&self, id: u64) -> Option<(usize, usize)> {
        for (ci, column) in self.columns.iter().enumerate() {
            if let Some(pos) =
                column.with_untracked(|cards| cards.iter().position(|c| c.id == id))
            {
                return Some((ci, pos));
            }
        }
        None
    }

    /// Move card `id` so it sits at `to_pos` in column `to_col` (live reflow
    /// while dragging). Positions are recomputed from scratch each call.
    fn move_card(&self, id: u64, to_col: usize, to_pos: Option<usize>) {
        let Some((from_col, from_pos)) = self.locate(id) else { return };
        let card = self.columns[from_col].with_untracked(|c| c[from_pos].clone());

        if from_col == to_col {
            self.columns[from_col].update(|cards| {
                cards.remove(from_pos);
                let pos = to_pos.unwrap_or(cards.len()).min(cards.len());
                cards.insert(pos, card.clone());
            });
        } else {
            self.columns[from_col].update(|cards| {
                cards.remove(from_pos);
            });
            self.columns[to_col].update(|cards| {
                let pos = to_pos.unwrap_or(cards.len()).min(cards.len());
                cards.insert(pos, card.clone());
            });
        }
    }

    fn delete(&self, id: u64) {
        for column in &self.columns {
            column.update(|cards| cards.retain(|c| c.id != id));
        }
    }

    fn commit_edit(&self) {
        if let Some(id) = self.editing.get_untracked() {
            let text = self.edit_buf.with_untracked(|s| s.trim().to_string());
            if !text.is_empty() {
                for column in &self.columns {
                    column.update(|cards| {
                        if let Some(card) = cards.iter_mut().find(|c| c.id == id) {
                            card.text = text.clone();
                        }
                    });
                }
            }
        }
        self.editing.set(None);
    }

    fn commit_add(&self) {
        if let Some(ci) = self.adding.get_untracked() {
            let text = self.add_buf.with_untracked(|s| s.trim().to_string());
            if !text.is_empty() {
                let id = self.next_id.get_untracked();
                self.next_id.set(id + 1);
                self.columns[ci].update(|cards| cards.push(Card { id, text }));
            }
        }
        self.add_buf.set(String::new());
        self.adding.set(None);
    }
}

fn main() {
    Application::new()
        .window(
            |_| app_view(),
            Some(
                WindowConfig::default()
                    .title("Board (floem)")
                    .size(Size::new(900.0, 600.0)),
            ),
        )
        .run();
}

fn app_view() -> impl IntoView {
    let board = Board::new();

    Stack::horizontal((
        column_view(board, 0),
        column_view(board, 1),
        column_view(board, 2),
    ))
    .style(|s| s.gap(12.0).padding(12.0).size_full().items_start())
}

fn column_view(board: Board, ci: usize) -> impl IntoView {
    let cards = board.columns[ci];

    let header = Stack::horizontal((
        Label::new(COLUMN_NAMES[ci]).style(|s| s.font_size(16.0).color(TEXT_MAIN)),
        Label::derived(move || format!("{}", cards.with(|c| c.len())))
            .style(|s| s.font_size(13.0).color(TEXT_DIM)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Button::new("+ Add card").action(move || {
            board.add_buf.set(String::new());
            board.adding.set(Some(ci));
        }),
    ))
    .style(|s| s.gap(8.0).items_center().width_full());

    // Inline add input, revealed by the button above. Enter commits,
    // Esc cancels; empty input is ignored by commit_add.
    let add_input = dyn_container(
        move || board.adding.get() == Some(ci),
        move |open| {
            if open {
                let input = TextInput::new(board.add_buf)
                    .placeholder("Card text…")
                    .into_view();
                input.id().request_focus();
                input
                    .on_event_stop(TextInputEnter::listener(), move |_, _| board.commit_add())
                    .on_event_stop(listener::KeyDown, move |_, event| {
                        if event.key == Key::Named(NamedKey::Escape) {
                            board.add_buf.set(String::new());
                            board.adding.set(None);
                        }
                    })
                    .style(|s| s.width_full().padding(8.0))
                    .into_any()
            } else {
                Empty::new().into_any()
            }
        },
    )
    .style(|s| s.width_full());

    let list = dyn_stack(
        move || cards.get(),
        |card| card.id,
        move |card| card_view(board, card.id),
    )
    .style(|s| s.flex_col().gap(8.0).width_full());

    // Tail drop target: dropping/hovering in the empty space below the cards
    // appends the dragged card to the end of this column.
    let tail = Empty::new()
        .style(|s| s.height(60.0).flex_grow(1.0).width_full())
        .on_event_stop(listener::DragTargetEnter, move |_, enter| {
            if let Some(data) = &enter.custom_data
                && let Some(dragged) = data.downcast_ref::<u64>()
            {
                board.move_card(*dragged, ci, None);
            }
        });

    let body = Stack::vertical((list, tail))
        .style(|s| s.flex_col().gap(8.0).width_full().min_height_full());

    Stack::vertical((
        header,
        add_input,
        body.scroll().style(|s| s.flex_grow(1.0).width_full()),
    ))
    .style(|s| {
        s.flex_col()
            .gap(10.0)
            .padding(10.0)
            .width_pct(32.0)
            .height_full()
            .background(BG_COLUMN)
            .border(1.0)
            .border_color(BORDER)
            .border_radius(10.0)
    })
}

fn card_view(board: Board, id: u64) -> impl IntoView {
    dyn_container(
        move || board.editing.get() == Some(id),
        move |is_editing| {
            if is_editing {
                edit_view(board, id).into_any()
            } else {
                display_view(board, id).into_any()
            }
        },
    )
    .style(|s| s.width_full())
}

/// Normal card: label + ✕, draggable, double-click to edit, drop target.
fn display_view(board: Board, id: u64) -> impl IntoView {
    Stack::horizontal((
        Label::derived(move || board.card_text(id))
            .style(|s| s.flex_grow(1.0).color(TEXT_MAIN)),
        Button::new("✕").action(move || board.delete(id)),
    ))
    .style(move |s| {
        let is_drag_source = board.dragging.get() == Some(id);
        s.gap(8.0)
            .items_center()
            .width_full()
            .padding(10.0)
            .border_radius(8.0)
            .background(BG_CARD)
            .border(1.0)
            .border_color(BORDER)
            .transition_background(Transition::linear(150.millis()))
            .hover(|s| s.background(Color::from_rgb8(0xf7, 0xf7, 0xfb)))
            // Drop indicator: the source card's (live-reflowed) slot gets an
            // accent border + dimmed body — this is where the card will land.
            .apply_if(is_drag_source, |s| {
                s.border_color(ACCENT)
                    .border(1.5)
                    .background(ACCENT.with_alpha(0.08))
            })
    })
    // Ghost that follows the cursor (painted by floem's drag system).
    .dragging_style(|s| {
        s.box_shadow_blur(16.0)
            .box_shadow_color(Color::BLACK.with_alpha(0.35))
            .border(1.0)
            .border_color(ACCENT)
            .border_radius(8.0)
            .background(BG_CARD.with_alpha(0.95))
    })
    .on_event_stop(listener::DoubleClick, move |_, _| {
        board.edit_buf.set(board.card_text(id));
        board.editing.set(Some(id));
    })
    // Live reflow: a dragged card entering this card takes its position.
    .on_event_stop(listener::DragTargetEnter, move |_, enter| {
        if let Some(data) = &enter.custom_data
            && let Some(dragged) = data.downcast_ref::<u64>()
            && *dragged != id
            && let Some((col, pos)) = board.locate(id)
        {
            board.move_card(*dragged, col, Some(pos));
        }
    })
    .on_event_cont(listener::DragStart, move |_, _| {
        board.dragging.set(Some(id));
    })
    .on_event_cont(listener::DragEnd, move |_, _| board.dragging.set(None))
    .on_event_cont(listener::DragCancel, move |_, _| board.dragging.set(None))
    .draggable_with_config(move || {
        DragConfig::default()
            .with_threshold(6.0)
            .with_custom_data(id)
            // Release animation: spring the ghost into its slot (built-in).
            .with_easing(floem::easing::Spring::snappy())
    })
}

/// Inline editor: Enter commits, Esc cancels.
fn edit_view(board: Board, _id: u64) -> impl IntoView {
    let input = TextInput::new(board.edit_buf).into_view();
    input.id().request_focus();
    input
        .on_event_stop(TextInputEnter::listener(), move |_, _| board.commit_edit())
        .on_event_stop(listener::KeyDown, move |_, event| {
            if event.key == Key::Named(NamedKey::Escape) {
                board.editing.set(None);
            }
        })
        .style(|s| s.width_full().padding(10.0).border_color(ACCENT))
}
