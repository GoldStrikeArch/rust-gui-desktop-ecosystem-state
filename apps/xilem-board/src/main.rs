//! "Board" — kanban task board per apps/SPEC-3.md, built with xilem 0.4.0.
//!
//! Drag-and-drop architecture (all hand-rolled; xilem 0.4 has no DnD):
//! * Every card is wrapped in a custom `CardFrame` masonry widget that
//!   captures the pointer on press and reports drag events in window
//!   coordinates to app state.
//! * Every card widget and each column's card-list area report their window
//!   rectangles into a shared `GeomRegistry` from the compose pass (so the
//!   rects stay correct while columns scroll). During a drag, app state
//!   hit-tests the registry to find the target column + insertion index.
//! * The drop indicator is a 4 px accent bar view inserted into the target
//!   column at the computed index on each rebuild.
//! * The drag ghost is a top zstack layer: a card-shaped view moved with a
//!   render transform (`transformed(..).translate(cursor - grab_offset)`).
//! * On drop the card is moved in the model and its new widget plays a
//!   fade-out flash animation (masonry `on_anim_frame`) — xilem 0.4 has no
//!   layout-position tweening, so this is the documented equivalent
//!   transition for "cards animate on drop".
//!
//! Repaint strategy: no timers; repaints happen only on input events, view
//! rebuilds after state changes, and transient `on_anim_frame` chains while
//! hover/flash animations are in flight.

mod widgets;

use xilem::core::one_of::Either;
use xilem::masonry::kurbo::{Point, Size, Vec2};
use xilem::masonry::properties::types::{CrossAxisAlignment, Length, UnitPoint};
use xilem::style::Style as _;
use xilem::view::{
    flex_col, flex_row, label, portal, sized_box, text_button, text_input, transformed, zstack,
    FlexExt as _, FlexSpacer,
};
use xilem::winit::error::EventLoopError;
use xilem::{EventLoop, FontWeight, InsertNewline, WidgetView, WindowOptions, Xilem};

use widgets::{auto_focus, card_frame, CardEvent, GeomRegistry, ACCENT, COLUMN_BG, TEXT_DIM};

const COLUMN_NAMES: [&str; 3] = ["Todo", "Doing", "Done"];
/// Geometry-registry keys for the three column card-list areas.
const COL_KEY: u64 = 1_000_000_000;

struct Card {
    id: u64,
    text: String,
}

struct DragState {
    card_id: u64,
    /// Current pointer position, window coords.
    cur: Point,
    /// Pointer offset inside the card at press time.
    grab: Vec2,
    /// Size of the dragged card (the ghost copies it).
    size: Size,
    /// (column, insertion index among the column's cards excluding the
    /// dragged card) currently under the pointer.
    target: Option<(usize, usize)>,
    text: String,
}

struct Board {
    columns: Vec<Vec<Card>>,
    next_id: u64,
    /// (card id, edit buffer) while a card is edited inline.
    editing: Option<(u64, String)>,
    /// (column, input buffer) while a new card is being typed.
    adding: Option<(usize, String)>,
    drag: Option<DragState>,
    /// Bumped on every completed drop; drives the flash animation.
    drop_seq: u64,
    /// Card that should play the drop flash.
    flashed: u64,
    geoms: GeomRegistry,
}

impl Board {
    fn new() -> Self {
        let mut next_id = 0;
        let mut card = |text: &str| {
            next_id += 1;
            Card { id: next_id, text: text.to_string() }
        };
        let columns = vec![
            vec![card("Write FRICTION.md"), card("Add column auto-scroll")],
            vec![card("Hand-roll drag-and-drop")],
            vec![card("Pin xilem 0.4.0")],
        ];
        Self {
            columns,
            next_id,
            editing: None,
            adding: None,
            drag: None,
            drop_seq: 0,
            flashed: 0,
            geoms: GeomRegistry::default(),
        }
    }

    fn find_card(&self, id: u64) -> Option<(usize, usize)> {
        for (c, col) in self.columns.iter().enumerate() {
            if let Some(i) = col.iter().position(|card| card.id == id) {
                return Some((c, i));
            }
        }
        None
    }

    fn delete_card(&mut self, id: u64) {
        if let Some((c, i)) = self.find_card(id) {
            self.columns[c].remove(i);
        }
        if matches!(&self.editing, Some((eid, _)) if *eid == id) {
            self.editing = None;
        }
    }

    fn commit_edit(&mut self) {
        if let Some((id, buf)) = self.editing.take() {
            let text = buf.trim();
            if !text.is_empty() {
                if let Some((c, i)) = self.find_card(id) {
                    self.columns[c][i].text = text.to_string();
                }
            }
        }
    }

    fn commit_add(&mut self) {
        if let Some((col, buf)) = &self.adding {
            let text = buf.trim().to_string();
            if text.is_empty() {
                // Empty input is ignored; keep the editor open.
                return;
            }
            let col = *col;
            self.next_id += 1;
            self.columns[col].push(Card { id: self.next_id, text });
            self.adding = Some((col, String::new()));
        }
    }

    /// Recompute the drop target from the geometry registry.
    fn update_drag_target(&mut self) {
        let Some(drag) = &mut self.drag else { return };
        let geoms = self.geoms.lock().unwrap();
        let p = drag.cur;
        drag.target = None;
        for (c, cards) in self.columns.iter().enumerate() {
            let Some(rect) = geoms.get(&(COL_KEY + c as u64)) else { continue };
            if p.x < rect.x0 || p.x > rect.x1 {
                continue;
            }
            // Insertion index: count cards (except the dragged one) whose
            // center is above the pointer.
            let mut idx = 0;
            for card in cards {
                if card.id == drag.card_id {
                    continue;
                }
                if let Some(r) = geoms.get(&card.id) {
                    if p.y > (r.y0 + r.y1) / 2.0 {
                        idx += 1;
                    }
                }
            }
            drag.target = Some((c, idx));
            break;
        }
    }

    fn finish_drag(&mut self) {
        if let Some(drag) = self.drag.take() {
            if let Some((tc, ti)) = drag.target {
                if let Some((fc, fi)) = self.find_card(drag.card_id) {
                    let card = self.columns[fc].remove(fi);
                    let ti = ti.min(self.columns[tc].len());
                    self.flashed = card.id;
                    self.drop_seq += 1;
                    self.columns[tc].insert(ti, card);
                }
            }
        }
    }

    fn card_event(&mut self, id: u64, event: CardEvent) {
        match event {
            CardEvent::Clicked => {}
            CardEvent::DoubleClicked => {
                self.adding = None;
                if let Some((c, i)) = self.find_card(id) {
                    let text = self.columns[c][i].text.clone();
                    self.editing = Some((id, text));
                }
            }
            CardEvent::DragStart { window, size, grab, .. } => {
                self.editing = None;
                let text = self
                    .find_card(id)
                    .map(|(c, i)| self.columns[c][i].text.clone())
                    .unwrap_or_default();
                self.drag = Some(DragState {
                    card_id: id,
                    cur: window,
                    grab,
                    size,
                    target: None,
                    text,
                });
                self.update_drag_target();
            }
            CardEvent::DragMove { window } => {
                if let Some(drag) = &mut self.drag {
                    drag.cur = window;
                }
                self.update_drag_target();
            }
            CardEvent::DragEnd { .. } => self.finish_drag(),
            CardEvent::DragCancelled => self.drag = None,
            CardEvent::EscapePressed => {
                if matches!(&self.editing, Some((eid, _)) if *eid == id) {
                    self.editing = None;
                }
            }
        }
    }
}

fn card_view(state: &Board, card: &Card) -> impl WidgetView<Board> + use<> {
    let id = card.id;
    let editing_this = matches!(&state.editing, Some((eid, _)) if *eid == id);

    let content = if editing_this {
        let buf = state.editing.as_ref().unwrap().1.clone();
        Either::B(auto_focus(
            text_input(buf, move |state: &mut Board, value| {
                if let Some((_, buf)) = &mut state.editing {
                    *buf = value;
                }
            })
            .insert_newline(InsertNewline::Never)
            .on_enter(move |state: &mut Board, _| state.commit_edit()),
        ))
    } else {
        Either::A(
            flex_row((
                label(card.text.clone()),
                // flex spacer pushes the ✕ to the card's right edge (a flexed
                // label would leave the ✕ right after the text: masonry flex
                // packs children by their actual, not allotted, sizes).
                FlexSpacer::Flex(1.0),
                // ✕ delete: a mini CardFrame rather than a stock button —
                // a stock Button inside CardFrame would lose the pointer
                // capture to its ancestor (see widgets.rs).
                card_frame(label("×").text_size(12.0), move |state: &mut Board, event| {
                    if matches!(event, CardEvent::Clicked) {
                        state.delete_card(id);
                    }
                })
                .hug()
                .padding(3.0),
            ))
            .must_fill_major_axis(true),
        )
    };

    let dragging_this = matches!(&state.drag, Some(d) if d.card_id == id);
    card_frame(content, move |state: &mut Board, event| {
        state.card_event(id, event);
    })
    .dimmed(dragging_this)
    .flash_seq(if state.flashed == id { state.drop_seq } else { 0 })
    .report_geometry(id, state.geoms.clone())
}

/// 4 px accent bar marking where the dragged card will land.
fn drop_indicator<State: 'static>() -> impl WidgetView<State> + use<State> {
    sized_box(label(""))
        .height(Length::px(4.0))
        .expand_width()
        .background_color(ACCENT)
        .corner_radius(2.0)
}

fn column_view(state: &Board, col: usize) -> impl WidgetView<Board> + use<> {
    let cards = &state.columns[col];

    let target_idx = state.drag.as_ref().and_then(|d| match d.target {
        Some((c, i)) if c == col => Some(i),
        _ => None,
    });
    let drag_id = state.drag.as_ref().map(|d| d.card_id);

    // Card list with the drop indicator spliced in at the target index
    // (index counted over the cards excluding the dragged one).
    let mut items = Vec::new();
    let mut placed = false;
    let mut k = 0usize;
    for card in cards {
        if Some(card.id) != drag_id {
            if target_idx == Some(k) && !placed {
                items.push(Either::B(drop_indicator()));
                placed = true;
            }
            k += 1;
        }
        items.push(Either::A(card_view(state, card)));
    }
    if target_idx.is_some() && !placed {
        items.push(Either::B(drop_indicator()));
    }

    // Independent scrolling; the plain CardFrame wrapper reports this
    // area's window rect for drop hit-testing.
    let list = card_frame(
        portal(
            flex_col(items)
                .gap(Length::px(8.0))
                .cross_axis_alignment(CrossAxisAlignment::Fill),
        ),
        |_: &mut Board, _| {},
    )
    .plain()
    .fill()
    .padding(0.0)
    .report_geometry(COL_KEY + col as u64, state.geoms.clone());

    let header = flex_row((
        label(COLUMN_NAMES[col]).text_size(15.0).weight(FontWeight::BOLD).flex(1.0),
        label(format!("{}", cards.len())).text_size(13.0).color(TEXT_DIM),
    ))
    .must_fill_major_axis(true);

    let add = match &state.adding {
        Some((c, buf)) if *c == col => {
            let buf = buf.clone();
            Either::B(
                card_frame(
                    auto_focus(
                        text_input(buf, move |state: &mut Board, value| {
                            if let Some((_, buf)) = &mut state.adding {
                                *buf = value;
                            }
                        })
                        .placeholder("New card - Enter adds, Esc cancels")
                        .insert_newline(InsertNewline::Never)
                        .on_enter(move |state: &mut Board, _| state.commit_add()),
                    ),
                    move |state: &mut Board, event| {
                        if matches!(event, CardEvent::EscapePressed) {
                            state.adding = None;
                        }
                    },
                )
                .plain()
                .padding(0.0),
            )
        }
        _ => Either::A(text_button("+ Add card", move |state: &mut Board| {
            state.editing = None;
            state.adding = Some((col, String::new()));
        })),
    };

    sized_box(
        flex_col((header, list.flex(1.0), add))
            .gap(Length::px(8.0))
            .cross_axis_alignment(CrossAxisAlignment::Fill),
    )
    .expand_height()
    .background_color(COLUMN_BG)
    .corner_radius(8.0)
    .padding(8.0)
}

fn app_logic(state: &mut Board) -> impl WidgetView<Board> + use<> {
    let body = flex_row((
        column_view(state, 0).flex(1.0),
        column_view(state, 1).flex(1.0),
        column_view(state, 2).flex(1.0),
    ))
    .gap(Length::px(12.0))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .must_fill_major_axis(true)
    .padding(12.0);

    // Drag ghost: a card-shaped top layer following the cursor.
    let ghost = match &state.drag {
        Some(drag) => Either::A(
            transformed(
                sized_box(label(drag.text.clone()))
                    .width(Length::px(drag.size.width))
                    .background_color(widgets::CARD_BG_HOVER)
                    .border(ACCENT, 1.0)
                    .corner_radius(8.0)
                    .padding(10.0),
            )
            .translate(drag.cur.to_vec2() - drag.grab),
        ),
        None => Either::B(label("")),
    };

    zstack((body, ghost)).alignment(UnitPoint::TOP_LEFT)
}

fn main() -> Result<(), EventLoopError> {
    let window = WindowOptions::new("Board (xilem)")
        .with_initial_inner_size(xilem::dpi::LogicalSize::new(900.0, 600.0))
        .with_resizable(true);
    Xilem::new_simple(Board::new(), app_logic, window).run_in(EventLoop::with_user_event())
}
