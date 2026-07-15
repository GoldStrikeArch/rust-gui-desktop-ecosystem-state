//! Minimal experiments pinning down how a click resolves when a button sits
//! inside a drag-sensing container (the kanban card situation).
//!
//! Finding (egui 0.35): a drag-only `ui.interact` registered AFTER the
//! children sits on top of them, and egui's hit test deliberately refuses to
//! click "through" a topmost drag-only widget (see `hit_test.rs`), so the
//! button never receives the click. Putting the drag sense on the container
//! itself (`UiBuilder::sense`) registers it UNDER the children — the pattern
//! ScrollArea uses for its background — and then both gestures work.

use eframe::egui::{Id, Sense, UiBuilder};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

#[derive(Default)]
struct State {
    plain_clicks: u32,
    over_clicks: u32,
    under_clicks: u32,
}

#[test]
fn drag_interact_on_top_swallows_button_clicks() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut State| {
            // Case A: plain button (sanity).
            if ui.small_button("plain").clicked() {
                state.plain_clicks += 1;
            }
            // Case B: drag-sense interact registered AFTER the children,
            // i.e. on top of them (what `Ui::dnd_drag_source` does).
            let inner = ui.scope(|ui| {
                if ui.small_button("over").clicked() {
                    state.over_clicks += 1;
                }
            });
            let _ = ui.interact(inner.response.rect, Id::new("drag-over"), Sense::drag());
        },
        State::default(),
    );
    harness.run();

    harness.get_by_label("plain").click();
    harness.run();
    assert_eq!(harness.state().plain_clicks, 1, "sanity: plain button clicks");

    // Documented trap: the click is swallowed by the drag-only overlay.
    harness.get_by_label("over").click();
    harness.run();
    assert_eq!(
        harness.state().over_clicks, 0,
        "egui 0.35 hit test does NOT click through a topmost drag-only widget"
    );
}

#[test]
fn drag_sense_on_container_keeps_buttons_clickable() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut State| {
            // The fix: drag sense on the container Ui itself, which registers
            // the drag widget UNDER its children.
            let _ = ui.scope_builder(UiBuilder::new().sense(Sense::drag()), |ui| {
                if ui.small_button("under").clicked() {
                    state.under_clicks += 1;
                }
            });
        },
        State::default(),
    );
    harness.run();

    harness.get_by_label("under").click();
    harness.run();
    assert_eq!(
        harness.state().under_clicks, 1,
        "container-sense pattern must keep inner buttons clickable"
    );
}
