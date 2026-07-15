# FRICTION — xilem-dash ("Pulse", xilem 0.4.0 from crates.io)

Observed on macOS 26.5 (M4 Pro): release build, 10 s launch checks, and a
contemporaneous scripted interaction run via synthetic CGEvent mouse/keyboard
input (card click-select, drag-reorder, chart hover, pause/resume and slider).
The harness, raw output and screenshots from that run were not retained, so
these interaction details are narrative evidence rather than a reproducible
test artifact.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| DnD card reorder | **hand-rolled** | No DnD anywhere in xilem/masonry 0.4. Wrote a custom masonry `Widget` (`CardFrame`, ~250 LoC + ~180 LoC xilem `View` plumbing) that captures the pointer, applies a 5 px drag threshold, and reports window-coordinate drag events to app state; target slot computed from uniform grid math; the dragged card follows the cursor via the stock `transformed(..).translate(..)` view and the target cell paints a hint border. Caveat: a render-transformed card does not change paint order, so the "ghost" can pass under later siblings (the board app fixes this with a `zstack` overlay). |
| Live data (timer @ 10 Hz) | **built-in** | `task_raw` view + bundled tokio: an async loop `sleep(period); proxy.message(())`. First-class and clean (same pattern as the bundled `stopwatch`/`variable_clock` examples). Two quirks: task views are *not* rebuilt when captured state changes, so the mutable tick rate had to be smuggled in as an `Arc<AtomicU64>`; pause is expressed by removing the task from the tree (`running.then(|| task_raw(...))` under `fork`), which is elegant but non-obvious. |
| Sparklines + main chart | **hand-rolled** | No chart/plot widget or ecosystem crate for xilem. Both are custom masonry widgets painting vello `BezPath`s. Fine-grained: `rebuild` diffs the sample vector and calls `request_paint_only`. Drawing *text* inside a custom widget (axis min/max labels, tooltip) has no helper — you rebuild the ~10 lines of parley `ranged_builder` → `break_all_lines` → `render_text` code that `Label` uses internally, via `ctx.text_contexts()`. |
| Hover crosshair + tooltip | **hand-rolled** | `on_pointer_event(Move/Leave)` storing a widget-local hover point + `request_paint_only`; crosshair, marker and tooltip (rounded rect + parley text) drawn in `paint`. Widget-local state avoids re-running `app_logic` on every mouse move — but there is no tooltip primitive, no overlay layer, and text must be measured with a "draw transparent, then draw real" dry run (or a second layout pass). |
| Slider control | **built-in** | `slider(1.0, 60.0, hz, cb).step(1.0)` — works, keyboard accessible. Surprise: masonry's `Slider::layout` clamps its own width to `bc.max().width.clamp(100.0, 200.0)`, so `.flex(1.0)` silently does nothing; the slider is always ≤ 200 px and there is no property to widen it. This cost significant automation-debugging time (clicks landing right of x+200 hit nothing). |
| Click-to-select | **hand-rolled** | No generic tap/gesture/pointer view for containers. `button(child)` does accept an arbitrary child view, but it draws button chrome and — worse — its own pointer capture conflicts with the same surface needing drag (masonry: the last widget to capture wins, and events bubble to ancestors). Selection is just the `Clicked` event of the same `CardFrame` widget that implements dragging. |
| Animation | **hand-rolled** | Only primitive: per-widget `on_anim_frame(interval)` + `request_anim_frame` chains (Masonry level). No tween/spring/transition/keyframe API, nothing view-level. Implemented an eased hover-elevation animation (exponential approach, vello blurred-rect shadow) inside `CardFrame`. Anim frames stop when the animation settles, so idle cost is zero. |

## Helper crates

None. Dependencies: `xilem =0.4.0` only (**143 unique crate names / 154
name-version entries including the app**). Even the
random-walk PRNG is a 6-line xorshift* to avoid `rand`. There is no xilem
ecosystem to lean on (no plot/DnD/animation helper crates exist for it).

## Totals

- LoC: **1233** (`main.rs` 294 + `widgets.rs` 939 — over half the app is
  hand-rolled widget/view infrastructure, not app logic).
- Canonical clean release build **28 s**; no-op incremental build **2 s**.
  Binary **11,903,856 bytes raw / 10,186,056 bytes (9.7 MiB) stripped**.
- Controlled 30-second sample at 10 Hz: **9.9% CPU**, **12.4% peak CPU** and
  **106 MiB total process-tree RSS**. The earlier ~3.5–9% observation was a
  preliminary range; raw per-second samples were not retained.

## Repaint strategy

Tick → `proxy.message` → `app_logic` re-run + view-tree diff → only changed
widgets (`set_samples`, labels) invalidate paint → vello full-scene GPU
re-render. Chart hover redraws are widget-local (`request_paint_only`, no
rebuild). Animations use `on_anim_frame` chains that self-terminate at rest.

## Where the time went

1. **API archaeology** (~30 %): 0.4.0 has no hosted docs matching the release;
   everything came from reading the vendored crate sources + bundled examples
   (git main has a different API, so web docs actively mislead).
2. **Custom View plumbing** (~30 %): every custom widget needs a ~120–180 LoC
   xilem `View` impl (build/rebuild/teardown/message + ViewId routing for
   children) — pure boilerplate copied from `xilem/src/view/*.rs` patterns.
3. **Constraint-semantics surprises** (~20 %): masonry `Flex` hands non-flex
   children its own loosened max in *both* axes, so "fill if bounded" widget
   sizing explodes; flex children are packed at *measured* size (a flexed
   label does not push siblings to the edge — you need `FlexSpacer::Flex`).
4. **Slider width clamp goose chase** (~20 % — mostly automation debugging).

## Surprises

- Good: `slider`, `task`/`task_raw`, `zstack`, `transformed`, `grid` all exist
  in 0.4.0 — more stock views than expected; the timer story is genuinely nice.
- Good: events bubble to ancestor widgets and `PointerState.count` gives
  double-click detection for free (at the masonry level).
- Bad: the boundary is sharp — the moment you leave the stock view list
  (charts, tooltips, DnD, any animation) you drop into full masonry `Widget`
  implementation plus a hand-written xilem `View` wrapper.
- Bad: `Slider` hard-caps its width at 200 px; no API to change it.
