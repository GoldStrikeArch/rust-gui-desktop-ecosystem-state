# FRICTION — Pulse (iced =0.14.0)

Reference: SPEC-2.md. Built + verified on macOS (M4 Pro, rustc 1.96.1):
`cargo build --release` clean, binary launched, alive after 10 s, killed
cleanly. The controlled 30-second runtime sample recorded **3.5% average CPU**
at 10 Hz; an earlier quick observation of ~1.5% was preliminary and is not
used as the canonical result.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| DnD card reorder | **hand-rolled in this app** | Iced core has no general sortable-card DnD widget. `iced_drop 0.2.37` supports Iced 0.14, but it was not discovered or evaluated during this implementation and was not tested against the wrapped-grid reorder interaction. This app assembles mechanics from `mouse_area` per card (`on_press` arms the drag, `on_enter` retargets the slot with live grid reflow), a conditional global `event::listen_with` subscription for cursor/release, an 8 px threshold, and a `pin` ghost. ~90 LoC of mechanics, with no manual hit-testing because widget enter messages carry the target. |
| Live data (timer @ 10 Hz) | **built-in*** | `iced::time::every(Duration) → Subscription` is exactly the right shape and rate changes are just a new Duration (subscriptions re-key automatically). The asterisk: `time::every` does **not compile** with default features — it only exists in the `tokio`/`smol` executor backends, so you must add `features = ["smol"]` (found via the upstream stopwatch example; the compile error gives no hint). |
| Sparklines + main chart | **hand-rolled in this app** | `canvas` widget + `Program` trait is the path chosen here, so this implementation writes scaling, gridlines, axis labels, polylines and interaction itself (~150 LoC). Compatible helpers now exist: `plotters-iced2 0.14.0` targets Iced 0.14, and `iced_plot 0.5.0` depends on Iced 0.14. Neither was evaluated in this app, so the experiment measures the manual-canvas choice rather than ecosystem absence. |
| Hover crosshair + tooltip | **hand-rolled** | Done inside the chart's `canvas::Program`: widget-local `State = Option<Point>`, `update()` on `CursorMoved` returns `Action::request_redraw()` (repaints only this widget), `draw()` renders crosshair + snapped marker + tooltip box/text. Index snapping, tooltip placement/flipping, and text measurement (approximated by char count) are all manual. The built-in `tooltip` widget can't follow a cursor inside a canvas. |
| Slider control | **built-in** | `slider(1..=60, hz, Message::RateChanged)` — one line, integer steps by default. |
| Click-to-select | **built-in** | `mouse_area::on_press` (already there for drag); selection + accent border via `container` style. Trivial. |
| Animation | **assembled** | iced 0.14 ships a first-party `Animation<T>` API (wrapping the `lilt` crate): `Animation::new(false).quick()`, `go_mut(state, now)`, `interpolate(a, b, now)`. Used for hover elevation (scale + shadow via the new `float` widget, exactly like the official gallery example). The catch: *you* schedule frames — subscribe to `window::frames()` only while `is_animating(now)`, and thread `Instant` through everything via `iced::application::timed`. No layout/FLIP animation, so the grid reflow during drag snaps rather than slides (documented gap). |

## Helper crates

None beyond Iced itself were used. `rand` was avoided with a 10-line
xorshift*; charts are raw `canvas`; DnD is hand-rolled; animation is
first-party (`iced::Animation`, internally `lilt`). Current compatible
alternatives include `plotters-iced2 0.14.0`, `iced_plot 0.5.0`, and
`iced_drop 0.2.37`; they were missed during implementation and do not change
which code this experiment exercised.

## Repaint strategy (for later CPU measurement)

- iced repaints only when a message/subscription fires. The 10 Hz tick is the
  only steady message source; each tick clears the 7 `canvas::Cache`s so
  geometry is re-tessellated at most once per tick.
- Crosshair movement repaints via canvas-local `Action::request_redraw()`.
- `window::frames()` (per-frame redraws) is subscribed **only** while a hover
  animation is in flight (~200 ms per hover).
- Drag cursor tracking subscription exists only while a drag is armed.
- Controlled 30-second result: 3.5% average CPU at 10 Hz on the M4 Pro
  (`measurements/runtime.csv`; raw per-second samples were not retained).

## Where the time went

1. **DnD design** — deciding on message-level mechanics (mouse_area messages +
   gated global listener + pin ghost) that avoid manual hit-testing; the
   actual code was quick once designed.
2. **Chart drawing** — the usual manual-plotting yak: y-scaling, right-aligned
   scrolling window, snapping, tooltip edge-flipping.
3. **API archaeology** — 0.14 is recent; several key pieces (`float`, `pin`,
   `application::timed`, `Animation`, `time::every` feature gate) are best
   documented by the repo's version-matched examples, not by guides.

## Surprises

- Good: `iced::application::timed` + `Animation` + `window::frames()` is a
  coherent, power-friendly animation story — new in 0.14 and clearly designed
  together (the gallery example is the Rosetta stone).
- Good: `mouse_area::on_enter` firing during an in-flight drag makes drop
  targeting free — no geometry math, no scroll-offset bookkeeping.
- Bad: `time::every` silently missing without an executor feature; the error
  is just "cannot find function `every`".
- Bad: no text measurement in `canvas` — tooltip width is estimated from
  character count.

## Totals

- LoC: 777 (single `src/main.rs`, heavily commented)
- Dependencies added: none beyond `iced = "=0.14.0"` (+ `canvas`, `smol` features)
