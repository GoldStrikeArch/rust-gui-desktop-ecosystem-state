//! "Board" — kanban task board per apps/SPEC-3.md, in egui 0.35 (eframe).
//!
//! No helper crates: the drag-and-drop is built on egui's own DnD payload
//! API (`Ui::dnd_drag_source` + `egui::DragAndDrop`), which paints the
//! dragged card at the cursor (ghost) on a floating layer for free. The
//! insertion index, drop indicator line, and the actual list surgery are
//! hand-rolled from card rects.
//!
//! Repaint model: nothing here ticks, so egui only repaints on input; the
//! drop-flash animation uses `Context::animate_value_with_time`, which
//! requests its own repaints while in flight.

use eframe::egui::{
    self, Align, Color32, CornerRadius, Id, Key, Label, Layout, Margin, Rect, RichText, Sense,
    Stroke, StrokeKind, TextEdit, epaint::Shadow,
};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Board (egui)", // window title
        options,
        Box::new(|_cc| Ok(Box::new(BoardApp::new()))),
    )
}

struct Card {
    id: u64,
    text: String,
}

struct Column {
    name: &'static str,
    cards: Vec<Card>,
}

/// What travels with a drag; stored in egui's `DragAndDrop` plugin, which
/// auto-clears it on pointer release or Escape.
#[derive(Clone, Copy)]
struct DragPayload {
    card_id: u64,
    from_col: usize,
}

struct BoardApp {
    columns: Vec<Column>,
    next_id: u64,
    /// Open "+ Add card" input: (column index, draft text).
    adding: Option<(usize, String)>,
    /// In-place edit: (card id, draft text).
    editing: Option<(u64, String)>,
    /// A TextEdit was just opened and should grab focus this frame.
    want_focus: bool,
    /// Card to flash after a drop (animates 1 → 0 then clears).
    flash: Option<u64>,
}

fn flash_id(card_id: u64) -> Id {
    Id::new(("drop-flash", card_id))
}

/// Like `Ui::dnd_drag_source`, but reworked around two egui 0.35 traps that
/// egui_kittest exposed (a click on a card's ✕ button never registered):
///
/// 1. `dnd_drag_source` adds its `Sense::drag()` interact *after* (on top
///    of) the card's children, and egui's hit test deliberately refuses to
///    click through a topmost drag-only widget (`hit_test.rs`: "it would be
///    confusing if clicking a drag-widget would actually click something
///    else below it") — so every button inside the card goes inert.
///    Fix: put the drag sense on the *container* via `UiBuilder::sense`,
///    which registers it *under* the children (the ScrollArea-background
///    pattern), letting buttons win the click while the card wins the drag.
/// 2. A drag-only widget counts as "dragged" from the moment of pointer
///    press, so the stock helper enters ghost mode (contents re-rendered on
///    an `Order::Tooltip` layer, where responses are empty) during a plain
///    click. Fix: gate ghost mode + payload on `is_decidedly_dragging`.
fn drag_source<R>(
    ui: &mut egui::Ui,
    is_dragged: bool,
    payload: DragPayload,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let ctx = ui.ctx().clone();
    if is_dragged {
        egui::DragAndDrop::set_payload(&ctx, payload); // keep the payload alive

        // Paint the body to a floating layer and translate it to the cursor
        // (the ghost); the layout slot in the column is still occupied.
        let layer_id = egui::LayerId::new(
            egui::Order::Tooltip,
            Id::new(("card-ghost", payload.card_id)),
        );
        let inner = ui.scope_builder(egui::UiBuilder::new().layer_id(layer_id), add_contents);
        if let Some(pointer_pos) = ctx.pointer_interact_pos() {
            let delta = pointer_pos - inner.response.rect.center();
            ctx.transform_layer_shapes(
                layer_id,
                egui::emath::TSTransform::from_translation(delta),
            );
        }
        inner
    } else {
        let inner = ui.scope_builder(
            egui::UiBuilder::new().sense(Sense::drag()),
            add_contents,
        );
        if inner.response.dragged() && ui.input(|i| i.pointer.is_decidedly_dragging()) {
            egui::DragAndDrop::set_payload(&ctx, payload);
        }
        inner.response.clone().on_hover_cursor(egui::CursorIcon::Grab);
        inner
    }
}

impl BoardApp {
    fn new() -> Self {
        let mut next_id = 0;
        let mut card = |text: &str| {
            next_id += 1;
            Card { id: next_id, text: text.to_owned() }
        };
        let columns = vec![
            Column {
                name: "Todo",
                cards: vec![card("Sketch friction rubric"), card("Write SPEC-4")],
            },
            Column { name: "Doing", cards: vec![card("Build kanban board")] },
            Column { name: "Done", cards: vec![card("Ship iteration 1")] },
        ];
        Self {
            columns,
            next_id,
            adding: None,
            editing: None,
            want_focus: false,
            flash: None,
        }
    }

    fn new_card(&mut self, text: String) -> Card {
        self.next_id += 1;
        Card { id: self.next_id, text }
    }

    /// Move `payload`'s card to `target` column at `insert` position,
    /// adjusting the index when reordering within the same column.
    fn apply_drop(&mut self, payload: DragPayload, target: usize, mut insert: usize) {
        let DragPayload { card_id, from_col } = payload;
        let Some(pos) = self.columns[from_col].cards.iter().position(|c| c.id == card_id)
        else {
            return;
        };
        let card = self.columns[from_col].cards.remove(pos);
        if from_col == target && pos < insert {
            insert -= 1; // removal above the insertion point shifted it
        }
        insert = insert.min(self.columns[target].cards.len());
        self.columns[target].cards.insert(insert, card);
        self.flash = Some(card_id);
    }

    /// Whole UI; split out from `eframe::App::ui` so tests can drive it.
    fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();

        // Id of the card being dragged, if any. Gated on "decidedly
        // dragging" (movement-based) so a plain click is never treated as a
        // zero-distance drag-and-drop; see `drag_source` for the details.
        let dragged_card: Option<u64> = egui::DragAndDrop::payload::<DragPayload>(&ctx)
            .filter(|_| ui.input(|i| i.pointer.is_decidedly_dragging()))
            .map(|p| p.card_id);
        let dragging = dragged_card.is_some();
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let released = ui.input(|i| i.pointer.any_released());

        let mut pending_drop: Option<(usize, usize)> = None; // (column, index)
        let mut pending_delete: Option<u64> = None;
        let mut start_edit: Option<(u64, String)> = None;
        let mut clear_flash = false;

        let n_columns = self.columns.len();
        ui.columns(n_columns, |cols| {
            for (ci, col_ui) in cols.iter_mut().enumerate() {
                // -- Header: name + live count --------------------------------
                col_ui.horizontal(|ui| {
                    ui.label(RichText::new(self.columns[ci].name).heading().strong());
                    ui.label(
                        RichText::new(format!("({})", self.columns[ci].cards.len())).weak(),
                    );
                });
                col_ui.separator();

                // Everything below the header is this column's drop zone.
                let zone = col_ui.available_rect_before_wrap();
                let mut card_rects: Vec<Rect> = Vec::new();

                // -- Cards (independent scrolling per column) ----------------
                egui::ScrollArea::vertical()
                    .id_salt(("column-scroll", ci))
                    .auto_shrink([false, false])
                    .show(col_ui, |ui| {
                        ui.add_space(2.0);
                        for pos in 0..self.columns[ci].cards.len() {
                            let card_id = self.columns[ci].cards[pos].id;

                            // ---- In-place editor for this card? ----
                            if matches!(self.editing, Some((id, _)) if id == card_id) {
                                let (_, draft) = self.editing.as_mut().unwrap();
                                let response = ui.add(
                                    TextEdit::singleline(draft).desired_width(f32::INFINITY),
                                );
                                if self.want_focus {
                                    response.request_focus();
                                    self.want_focus = false;
                                }
                                card_rects.push(response.rect);
                                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                                let escape = ui.input(|i| i.key_pressed(Key::Escape));
                                if escape {
                                    self.editing = None; // cancel
                                } else if response.lost_focus() {
                                    if enter {
                                        let (_, draft) = self.editing.take().unwrap();
                                        let trimmed = draft.trim();
                                        if !trimmed.is_empty() {
                                            self.columns[ci].cards[pos].text =
                                                trimmed.to_owned();
                                        }
                                    } else {
                                        self.editing = None; // click-away cancels
                                    }
                                }
                                continue;
                            }

                            // ---- Normal card: a drag source ----
                            let is_dragged = dragged_card == Some(card_id);
                            let flash_t = if self.flash == Some(card_id) {
                                let t = ui.ctx().animate_value_with_time(
                                    flash_id(card_id),
                                    0.0,
                                    0.7,
                                );
                                if t <= 0.01 {
                                    clear_flash = true;
                                }
                                t
                            } else {
                                0.0
                            };

                            let inner = drag_source(
                                ui,
                                is_dragged,
                                DragPayload { card_id, from_col: ci },
                                |ui| {
                                    let visuals = ui.visuals();
                                    let accent = visuals.selection.bg_fill;
                                    let fill = visuals
                                        .faint_bg_color
                                        .lerp_to_gamma(accent, 0.6 * flash_t);
                                    let mut frame = egui::Frame::new()
                                        .fill(fill)
                                        .stroke(Stroke::new(
                                            1.0,
                                            visuals.widgets.noninteractive.bg_stroke.color,
                                        ))
                                        .corner_radius(6.0)
                                        .inner_margin(Margin::same(8));
                                    if is_dragged {
                                        // Ghost following the cursor: elevate it.
                                        frame = frame.shadow(Shadow {
                                            offset: [0, 4],
                                            blur: 12,
                                            spread: 1,
                                            color: Color32::from_black_alpha(110),
                                        });
                                    }
                                    frame.show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    if ui.small_button("✕").clicked() {
                                                        pending_delete = Some(card_id);
                                                    }
                                                    ui.with_layout(
                                                        Layout::left_to_right(Align::Center),
                                                        |ui| {
                                                            // Click sense so we can
                                                            // catch double-clicks.
                                                            let text = &self.columns[ci]
                                                                .cards[pos]
                                                                .text;
                                                            let resp = ui.add(
                                                                Label::new(text.as_str())
                                                                    .sense(Sense::click()),
                                                            );
                                                            if resp.double_clicked() {
                                                                start_edit = Some((
                                                                    card_id,
                                                                    text.clone(),
                                                                ));
                                                            }
                                                        },
                                                    );
                                                },
                                            );
                                        });
                                    });
                                },
                            );
                            card_rects.push(inner.response.rect);

                            if is_dragged {
                                // The visuals moved to the cursor layer; mark
                                // the hole the card came from.
                                ui.painter().rect_stroke(
                                    inner.response.rect,
                                    CornerRadius::same(6),
                                    Stroke::new(
                                        1.0,
                                        ui.visuals().widgets.noninteractive.bg_stroke.color,
                                    ),
                                    StrokeKind::Inside,
                                );
                            }
                        }

                        // -- "+ Add card" affordance ------------------------
                        match &mut self.adding {
                            Some((col, draft)) if *col == ci => {
                                let response = ui.add(
                                    TextEdit::singleline(draft)
                                        .hint_text("Card text…")
                                        .desired_width(f32::INFINITY),
                                );
                                if self.want_focus {
                                    response.request_focus();
                                    self.want_focus = false;
                                }
                                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                                let escape = ui.input(|i| i.key_pressed(Key::Escape));
                                if escape {
                                    self.adding = None; // cancel
                                } else if response.lost_focus() {
                                    if enter {
                                        let (_, draft) = self.adding.take().unwrap();
                                        let trimmed = draft.trim().to_owned();
                                        if !trimmed.is_empty() {
                                            let card = self.new_card(trimmed);
                                            self.columns[ci].cards.push(card);
                                        }
                                        // (empty input is ignored, editor closes)
                                    } else {
                                        self.adding = None; // click-away cancels
                                    }
                                }
                            }
                            _ => {
                                if ui.button("+ Add card").clicked() {
                                    self.adding = Some((ci, String::new()));
                                    self.want_focus = true;
                                }
                            }
                        }
                    });

                // -- Drop indicator + drop handling ---------------------------
                if dragging
                    && let Some(p) = pointer
                    && zone.contains(p)
                {
                    let insert = card_rects
                        .iter()
                        .position(|r| p.y < r.center().y)
                        .unwrap_or(card_rects.len());
                    let y = if card_rects.is_empty() {
                        zone.top() + 4.0
                    } else if insert == 0 {
                        card_rects[0].top() - 3.0
                    } else if insert == card_rects.len() {
                        card_rects[insert - 1].bottom() + 3.0
                    } else {
                        0.5 * (card_rects[insert - 1].bottom() + card_rects[insert].top())
                    };
                    // Insertion line, clipped to the column.
                    let painter = col_ui.painter_at(zone);
                    let accent = col_ui.visuals().selection.bg_fill;
                    let x = zone.x_range().shrink(4.0);
                    painter.hline(x, y, Stroke::new(3.0, accent));
                    painter.circle_filled(egui::pos2(x.min, y), 4.0, accent);

                    if released {
                        pending_drop = Some((ci, insert));
                    }
                }
            }
        });

        // -- Deferred mutations (kept out of the per-column borrow scope) ----
        if clear_flash {
            self.flash = None;
        }
        if let Some((id, text)) = start_edit {
            self.editing = Some((id, text));
            self.want_focus = true;
        }
        if let Some(id) = pending_delete {
            for column in &mut self.columns {
                column.cards.retain(|c| c.id != id);
            }
        }
        if let Some((target, insert)) = pending_drop
            && let Some(payload) = egui::DragAndDrop::take_payload::<DragPayload>(&ctx)
        {
            self.apply_drop(*payload, target, insert);
            // Prime the drop flash at full strength: animation time 0 snaps
            // the value to 1.0; the per-card code above tweens it back to 0.
            ctx.animate_value_with_time(flash_id(payload.card_id), 1.0, 0.0);
        }
    }
}

impl eframe::App for BoardApp {
    // Since egui 0.34, `App::ui` (replacing `App::update`) hands us the root
    // `Ui` of the viewport.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| self.show(ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn new_harness() -> Harness<'static, BoardApp> {
        Harness::new_ui_state(|ui, app: &mut BoardApp| app.show(ui), BoardApp::new())
    }

    #[test]
    fn add_card_via_inline_input_enter_commits() {
        let mut harness = new_harness();
        harness.run();

        // Three "+ Add card" buttons, one per column; take the first (Todo).
        harness.get_all_by_label("+ Add card").next().unwrap().click();
        harness.run();

        let input = harness.get_by_role(egui::accesskit::Role::TextInput);
        input.focus();
        input.type_text("Review egui friction notes");
        harness.run();
        harness.key_press(Key::Enter);
        harness.run();

        let todo = &harness.state().columns[0];
        assert_eq!(todo.cards.len(), 3);
        assert_eq!(todo.cards[2].text, "Review egui friction notes");
        assert!(harness.state().adding.is_none());
    }

    #[test]
    fn escape_cancels_and_empty_input_is_ignored() {
        let mut harness = new_harness();
        harness.run();

        harness.get_all_by_label("+ Add card").next().unwrap().click();
        harness.run();
        harness.key_press(Key::Escape);
        harness.run();
        assert!(harness.state().adding.is_none());
        assert_eq!(harness.state().columns[0].cards.len(), 2);

        // Empty commit: open the editor and press Enter with no text.
        harness.get_all_by_label("+ Add card").next().unwrap().click();
        harness.run();
        harness.get_by_role(egui::accesskit::Role::TextInput).focus();
        harness.run();
        harness.key_press(Key::Enter);
        harness.run();
        assert_eq!(harness.state().columns[0].cards.len(), 2, "empty input ignored");
    }

    #[test]
    fn delete_button_removes_the_card() {
        let mut harness = new_harness();
        harness.run();
        let before: usize = harness.state().columns.iter().map(|c| c.cards.len()).sum();
        harness.get_all_by_label("✕").next().unwrap().click();
        harness.run();
        let after: usize = harness.state().columns.iter().map(|c| c.cards.len()).sum();
        assert_eq!(after, before - 1);
    }

    /// The list surgery for drops is the bug-prone part; test it directly.
    #[test]
    fn apply_drop_moves_and_reorders_correctly() {
        let mut app = BoardApp::new();
        let id = app.columns[0].cards[0].id; // "Sketch friction rubric"

        // Cross-column: Todo[0] -> Doing at index 1 (after existing card).
        app.apply_drop(DragPayload { card_id: id, from_col: 0 }, 1, 1);
        assert_eq!(app.columns[0].cards.len(), 1);
        assert_eq!(app.columns[1].cards.len(), 2);
        assert_eq!(app.columns[1].cards[1].id, id);

        // Within-column reorder upward.
        app.apply_drop(DragPayload { card_id: id, from_col: 1 }, 1, 0);
        assert_eq!(app.columns[1].cards[0].id, id);

        // Within-column reorder downward: index must self-adjust.
        app.apply_drop(DragPayload { card_id: id, from_col: 1 }, 1, 2);
        assert_eq!(app.columns[1].cards[1].id, id, "drop below itself lands at end");

        // Out-of-range insert clamps.
        app.apply_drop(DragPayload { card_id: id, from_col: 1 }, 2, 99);
        assert_eq!(app.columns[2].cards.last().unwrap().id, id);
    }
}

