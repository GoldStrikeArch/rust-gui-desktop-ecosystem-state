# FRICTION — Pulse (vizia =0.4.0)

Reference: SPEC-2.md. Built + verified on macOS 26.5.2 (M4 Pro, rustc 1.96.1):
`cargo build --release` clean (no warnings), `cargo build --locked --release`
clean, binary launched, window pixel-verified, alive well past the 10 s bar,
killed cleanly. No fallback was needed — every SPEC-2 requirement is
implemented for real.

Evidence labels: **observed** (seen in the launched app without synthetic
input), **synthetic-input** (driven with CGEvent clicks/drags scoped to this
app's window, verified from window-scoped `screencapture -l` shots),
**source-only**, **unexercised**.

## Headline numbers

- **3.5 → 6.0 % average CPU at 10 Hz** — 10 × 1 s `ps -o pcpu=` samples of the
  running release binary averaged **6.03 %**. Every tick writes 6
  `Signal<VecDeque<f32>>` buffers, which invalidates 6 sparkline views + the
  main chart; vizia repaints only the dirty views but still runs a full
  layout/draw pass per tick.
- **RSS 106–107 MiB** steady (`ps -o rss= -p <pid>` / 1024).
- Release binary **21.9 MiB** (Skia is statically linked).

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| DnD card reorder | **built-in** | synthetic-input | The standout finding of this app: vizia core has real drag-and-drop. `.on_drag(\|ex\| ex.set_drop_data(ex.current()))` marks the view `DRAGGABLE` and fires when the pointer leaves a *pressed* card; `.on_drop(\|ex, data\| ..)` fires on release over any view; `ex.has_drop_data()` is available inside `on_over` for the slot highlight. Total DnD mechanics in this app: **~14 lines of modifiers** plus the model's `order.update(remove/insert)`. Proven: a synthetic drag of the "Memory" card from slot 1 to slot 3 reflowed the grid to CPU / Network In / Network Out — Memory / Disk / Requests. No helper crate, no hit-testing, no cursor-tracking subscription. |
| Live data (timer @ 10 Hz) | **built-in** | observed | `cx.add_timer(Duration, None, \|cx, action\| ..)` + `cx.start_timer/stop_timer` is core API with **no feature flag and no executor choice** (contrast iced 0.14, where `time::every` silently doesn't exist without the `smol`/`tokio` feature). Rate changes are in-place: `cx.modify_timer(t, \|s\| s.set_interval(d))` — the timer is not torn down and rebuilt. `TimerAction::{Start, Stop, Tick(delta)}` even gives you the elapsed delta. |
| Sparklines + main chart | **assembled** | observed | No plotting crate in the vizia ecosystem, but also no canvas *layer* to set up: vizia renders with Skia and `vizia::vg` re-exports `skia-safe`, so `impl View { fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) }` hands you the same `skia_safe::Canvas` the framework itself draws with, already transformed into the view's coordinate space. ~100 LoC for both chart types (scaling, gridlines, area fill, polyline). Redraw is per-view and declarative: `.bind(metric.samples, \|mut h\| h.needs_redraw())` — no cache invalidation bookkeeping. Trap: `skia-safe 0.93`'s `Path` is immutable, so geometry goes through `vg::PathBuilder` + `snapshot()/detach()`; `vg::Path::new()` compiles and then has no `move_to`. |
| Hover crosshair + tooltip | **assembled** | synthetic-input | `.on_mouse_move(\|cx, x, y\| ..)` on the chart view + `.on_hover_out(..)` to clear; index snapping is 3 lines; the crosshair and marker are drawn in the same Skia `draw()`. The **tooltip is a real `VStack` of `Label`s** absolutely positioned over the canvas via `left(Pixels(..))`/`top(Pixels(..))` bound to a signal, so it gets the framework's own text shaping (no `measure_text` needed, which is exactly the thing that forces character-count estimates in canvas-only frameworks). Verified: tooltip reads "59.5 / sample 159" with crosshair and dot snapped to the line. |
| Slider control | **built-in** | synthetic-input | `Slider::new(cx, signal).on_change(\|cx, v\| ..)`. One catch: vizia's `Slider` is **always normalised 0.0–1.0**, so a 1–60 Hz range needs mapping in both directions (`hz.map(\|v\| (v-1.0)/59.0)` in, `v*59.0+1.0` out). Verified: dragging the handle set the label to "45 Hz" and the timer interval followed. |
| Click-to-select | **built-in** | synthetic-input | `.on_press(\|cx\| cx.emit(..))` + `.toggle_class("selected", signal.map(..))` and one CSS rule. Verified: clicking the Network In card moved the accent border and switched the main chart title/series. |
| Animation | **built-in** | observed | vizia has a **CSS animation system**: `transition: background-color 180ms, border-color 180ms, scale 180ms, shadow 180ms;` on `.card`, with `:hover`, `.selected` and `.drop-target` variants. No `Instant` threading, no per-frame subscription, no animation crate — the framework interpolates the style properties itself. `@keyframes` + `cx.play_animation_for(..)` exists for imperative one-shots (unused here). The gap: like every other framework in this cohort, there is **no layout/FLIP animation**, so the grid reflow after a drop snaps rather than slides. |

## Three traps worth recording

1. **`on_press` on a container needs `hoverable(false)` children.** vizia only
   runs the action when `cx.current == meta.target`, and the target of a
   press is the *hovered* entity. A click landing on the card's `Label` or
   sparkline therefore never reached the card's press/drag handlers. Marking
   every child `.hoverable(false)` (the idiom used by vizia's own `list`
   example) fixes it. Silent failure: no warning, no compile error.
2. **`on_mouse_move` and `cx.bounds()` are in PHYSICAL pixels; `Pixels(..)`
   is logical.** On this 2× display the tooltip initially rendered at twice
   the cursor offset, i.e. pinned to the bottom-right corner. Fix: divide by
   `cx.scale_factor()` when converting an event coordinate into a layout
   coordinate.
3. **Event *ordering* around `on_drop`.** `on_drop` runs while
   `WindowEvent::MouseUp` is still propagating from the card to the root
   model, and it only *queues* the app event. A root-level MouseUp handler
   that cleared the drag state directly therefore ran first and the queued
   drop found nothing to move. Fix: queue a `DragEnd` event from the MouseUp
   handler so the order stays Drop → DragEnd.

## Helper crates

**None.** `vizia = "=0.4.0"` with default features only. Timers, drag & drop,
custom Skia drawing, CSS transitions and the slider are all in core; the PRNG
is a 10-line xorshift* (same shape as the iced/egui cohort apps).

## LoC

- `src/main.rs`: **743** lines total, heavily commented, no verification
  hooks compiled in (the self-test evidence above came from external CGEvent
  synthesis + window screenshots, not from in-app instrumentation).
- Rough split: ~110 lines of Skia drawing, ~90 lines of CSS, ~150 lines of
  model/event, ~250 lines of view construction, the rest doc comments.

## Where the time went

1. **Coordinate spaces and the hover/target rule** (traps 1–3 above) — every
   one of them fails silently, and all three had to be found by printing
   `WindowEvent`s from the model.
2. **Skia API archaeology** — vizia pins `skia-safe 0.93`, whose `Path` is
   immutable; the vizia examples use `vg::Path::rect(..)` only, so the
   builder pattern had to be found in the skia-safe source.
3. The features SPEC-2 treats as hard (DnD, timers, animation) were the
   *fastest* parts — roughly 30 lines between them.

## Surprises

- Good: drag-and-drop, a live-rate timer, and CSS transitions are all core.
  This is the only framework in the cohort where none of SPEC-2's three
  "hard" capabilities needed a helper crate or hand-rolled mechanics.
- Good: `Signal`/`Memo` fine-grained reactivity means a metric tick
  invalidates exactly the one sparkline that changed — no view-function
  re-run, no diffing pass.
- Bad: 6 % CPU at 10 Hz is high for what is being redrawn — vizia still runs
  a full style/layout pass per event cycle.
- Bad: the framework is unusually silent about mistakes. `cx.emit` from a
  model that should reach a child, a press handler on a container, an event
  coordinate used as a layout coordinate — all compile and all do nothing
  visible.
