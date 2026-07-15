# FRICTION.md — Pulse (egui 0.35 + eframe)

App: `apps/egui-dash/` · package `egui-dash` · `cargo run --release`
Verified on macOS: release build clean (no warnings), launched in background,
alive after 10 s, killed. 3 egui_kittest tests drive pause/resume, tick rate,
and click-to-select through the AccessKit tree.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| DnD card reorder | **assembled** (helper crate) | `egui_dnd 0.16` `dnd(ui, id).show_vec_sized(&mut order, size, …)` inside `horizontal_wrapped` gives grid reorder with live reflow, animated swaps and a floating dragged card in ~10 LoC of glue. Core egui alone would be hand-rolled (see egui-board). Items are identified by hash-id; I reorder a `Vec<usize>` of stable metric identities so selection/animation state survives reorders. |
| Live data (timer @ 10 Hz) | **assembled** | No timer API in egui at all — the idiom is deadline math in the frame callback: run due ticks from wall clock, then `ctx.request_repaint_after(time_to_next_deadline)`. ~15 LoC incl. catch-up cap. Reactive, not a busy loop: paused ⇒ zero repaints when idle. |
| Sparklines + main chart | **assembled** (helper crate) | `egui_plot 0.36` (version offset! the 0.36 release is the one tracking egui 0.35). Sparkline = `Plot` with axes/grid/interaction off, 6 per frame is fine. Main chart auto-follows the data window since bounds recompute each frame. No retained state to sync — immediate mode's best moment. |
| Hover crosshair + tooltip | **built-in / assembled** | Crosshair is literally `Plot::show_crosshair(true)`. Value+index snapping is assembled: `pointer_coordinate()` → nearest sample → `vline` + `points` marker inside the plot closure, tooltip via `response.on_hover_ui_at_pointer` (~15 LoC). `label_formatter` with `HoverPosition::NearDataPoint {index, ..}` exists but only fires near the line, so I snapped manually. |
| Slider control | **built-in** | `egui::Slider::new(&mut hz, 1.0..=60.0).suffix(" Hz")`. One line. |
| Click-to-select | **built-in** | egui_dnd's `Handle::sense(Sense::click())` makes the whole card both drag handle and click target; `response.clicked()` selects. Selected highlight = conditional `Frame::stroke`. |
| Animation | **assembled** | Two: (1) egui_dnd's built-in swap/return tweens on reorder; (2) hover-elevation shadow via `ctx.animate_bool(id, hovered)` — egui's animation primitives are id-keyed tween helpers (`animate_bool[_with_time[_and_easing]]`, `animate_value_with_time`) that auto-request repaints. No springs/keyframes; anything fancier is per-frame DIY. Drawing a shadow *under* an already-drawn widget needs the `painter.add(Shape::Noop)` placeholder + `painter.set(idx, …)` two-pass trick. |

## Helper crates & why

- `egui_plot = "=0.36.0"` — charts are not in core egui (extracted from egui
  proper years ago). Chose it over hand-rolled painter lines to get axes,
  bounds management and the built-in crosshair. **Trap:** version number is
  offset from egui (checked crates.io metadata: egui_plot 0.36.0 → egui
  ^0.35.0; egui_plot 0.35.0 → egui 0.34!).
- `egui_dnd = "=0.16.0"` (pulls `egui_animation`) — sortable-list DnD with
  animations. Same version-offset check needed (0.16.0 → egui ^0.35.0).

## Repaint strategy (headline)

- Unpaused: every frame runs due ticks then schedules **exactly one** wakeup
  via `ctx.request_repaint_after(next_deadline - now)` → frame rate ≈ tick
  rate (10 fps at 10 Hz), not display-rate repainting.
- Paused: no request issued → egui repaints only on input events (0% idle CPU
  expected).
- `animate_bool`/egui_dnd animations self-request repaints while in flight,
  so hover/drag animation stays smooth even when paused.

## LoC / time

~374 LoC app (+57 test). Rough time split: ~35% verifying version-matched
APIs (egui 0.35 renamed `TopBottomPanel`→`Panel`; `App::update`→`App::ui`
since 0.34 — pre-2026 examples are all wrong), ~25% card/DnD/selection
wiring, ~20% chart + hover snapping, ~20% tick/repaint design + tests.

## Surprises

- Good: `Plot::show_crosshair(true)` and `HoverPosition::NearDataPoint`
  (with sample index) exist — hover interactions are nearly free.
- Good: scrolling live charts need zero plumbing; auto-bounds re-derive from
  the data window every frame.
- Bad: kittest's `Harness::run` panics on a perpetually-animating app by
  design (runs until "no repaint requested", max 4 steps) — live-ticking UIs
  must be driven with explicit `step()`.
- Bad: helper-crate version numbers don't match egui's; every crate must be
  cross-checked against crates.io dependency metadata.
