# FRICTION.md — Board (egui 0.35 + eframe)

App: `apps/egui-board/` · package `egui-board` · `cargo run --release`
Verified on macOS: release build clean (no warnings), launched in background,
alive after 10 s, killed. 4 egui_kittest unit tests (add/Enter, Esc/empty,
delete, drop-index math) + 2 hit-test experiment tests documenting the DnD
click-swallowing trap (`tests/hit_experiment.rs`).

**No helper crates.** Deliberate contrast to egui-dash: built on core egui's
DnD payload plumbing (`egui::DragAndDrop`, modeled on `Ui::dnd_drag_source`).
`egui_dnd` was considered and rejected — it sorts a *single* list; there is
no cross-container story (hello_egui ships no multi-list example for it).

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| Cross-column DnD | **hand-rolled** (payload store built-in) | Core egui gives exactly one primitive pair: a typed payload store auto-cleared on release/Escape, plus a drag-source helper. Everything else — which column is hovered, insertion index from card rects, deferred list surgery — is yours (~60 LoC). |
| Within-column reorder | **hand-rolled** | Falls out of the same code path; the only subtlety is decrementing the insert index when a card is dropped below its own old position (unit-tested). |
| Drop indicator | **hand-rolled** | After laying out a column, compare pointer.y against collected card rects → insertion index; paint an accent `hline` + dot into the gap with `painter_at(zone)`. Painting an *overlay* avoids the immediate-mode chicken-and-egg of inserting a gap before you know the index. |
| Drag ghost/preview | **assembled** | The floating-card ghost is genuinely built in: render the card to an `Order::Tooltip` layer and `ctx.transform_layer_shapes` translates it to the cursor — but see the trap below; I had to fork `dnd_drag_source` (~25 LoC) to make it usable. |
| Inline edit (dbl-click, Enter/Esc) | **assembled** | `Label::sense(Sense::click())` → `double_clicked()` swaps the label for a `TextEdit`; commit = `lost_focus + Enter`, cancel = Escape (egui already drops focus on Esc). Focus must be hand-managed with a "just opened" flag + `request_focus()`. |
| Add/delete cards | **assembled** | Button → inline `TextEdit` (same Enter/Esc idiom); ✕ button per card; mutations deferred to end-of-frame to keep the borrow checker out of the per-column closures. |
| Drop/reorder animation | **assembled** | Drop-flash: prime `ctx.animate_value_with_time(id, 1.0, 0.0)` (time 0 = snap) at drop, tween back to 0 over 0.7 s to lerp the card fill. egui's tween helpers self-request repaints. No position/FLIP animation — cards *teleport* to their new slot; a slide-in would be fully hand-rolled (store prev rects, offset shapes per frame). |
| Independent column scrolling | **built-in** | One `ScrollArea::vertical().id_salt(col)` per column. Zero friction. |

## The headline trap (cost: ~40% of total time)

A click on a card's ✕ button silently never fired — caught only because an
egui_kittest test failed; visually the app "worked".

Two interacting causes in egui 0.35:

1. **Hit test refuses to click through drag-only widgets.** `dnd_drag_source`
   registers its `Sense::drag()` interact *after* (on top of) the card's
   children, and `hit_test.rs` deliberately returns `click: None` when the
   topmost hit senses only drag ("it would be confusing if clicking a
   drag-widget would actually click something else below it"). Every button
   inside a stock drag source is therefore inert.
2. **Drag-only widgets are "dragged" from the moment of pointer press** (no
   movement threshold — that only applies to click+drag widgets), so the
   stock helper enters ghost mode during a plain click: the card teleports to
   the cursor for the duration of the press and its widgets return empty
   `Response`s (Tooltip-layer widgets don't interact).

Fix (both in `drag_source()` in main.rs): put the drag sense on the card's
*container* via `UiBuilder::sense(Sense::drag())` — the container response
registers *under* the children (the ScrollArea-background pattern), so
buttons win clicks while the card wins drags — and gate ghost mode + payload
on `pointer.is_decidedly_dragging()`. Also gate drop handling on the same,
or a plain click counts as a zero-distance drop.

Verdict: the egui demo's DnD recipe works only for cards with no interactive
children; real kanban cards need the forked pattern above.

## LoC / time

~438 LoC app (+91 test, +78 experiment). Time: ~40% diagnosing the
click-swallowing trap (ending with reading `hit_test.rs`/`interaction.rs`),
~25% column/indicator/drop geometry, ~20% inline-edit + focus/keyboard
handling, ~15% layout polish and tests.

## Surprises

- Good: the payload store is typed, global, and auto-cleared on release and
  Escape (drag-cancel via Esc worked without any code).
- Good: deferred-mutation style (collect `pending_*`, apply after the column
  loop) resolves all borrow-checker friction; edition-2024 disjoint field
  capture means the big UI closure "just works" against separate fields.
- Bad: the two-part DnD trap above; invisible without an interaction test.
- Bad/neutral: no built-in way to animate layout position changes — drop
  animation downgraded to a flash (documented approximation).
