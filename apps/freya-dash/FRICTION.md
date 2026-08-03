# FRICTION — Pulse (freya =0.4.0)

Reference: SPEC-2.md. Built + verified on macOS 26.5.2 (M4 Pro, rustc 1.96.1):
`cargo build --release` clean (no warnings), `cargo build --locked --release`
reproduces, binary launched and stayed alive >12 s. Interactions were driven
with synthetic CoreGraphics mouse events (`CGEventPost`) and verified from
window-scoped screenshots, so the ratings below are **observed**, not assumed.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| DnD card reorder | **built-in** | `DragZone::new(payload, child).drag_element(ghost)` + `DropZone::new(child, on_drop)` ship in `freya::components`. Payloads are typed (`use_drag::<T>()` keys a root-scope context by `T`), the 4 px drag threshold is configurable, and the drop handler just gets the payload back. Whole reorder feature = ~30 LoC including the `Vec` permutation. Verified: dragging the CPU card onto the Memory slot swapped them on screen. |
| Live data (timer @ 10 Hz) | **assembled** | `spawn(async move { loop { … } })` is first-party and updates signals directly from the task (single-threaded reactivity, so no channel plumbing). But Freya ships **no timer/interval primitive** — `spawn` takes a bare future and the prelude exports nothing time-related. The fix is to depend on `async-io` and use `Timer::after`, which is exactly what `freya-animation` does internally; it is already in the tree, so it costs no new compilation, only knowledge. Rate changes are read with `.peek()` inside the loop, so re-rating needs no re-subscription. |
| Sparklines + main chart | **hand-rolled** | The `canvas()` element hands you the raw `skia_safe::Canvas`, so scaling, gridlines, the area fill and the polyline are all written here (~110 LoC of drawing). There *is* a first-party charting path — the `plot` feature re-exports `plotters` plus `freya-plotters-backend`, a real plotters backend for this same canvas — but it was not used: the point of the round is to measure the drawing story, and plotters would have hidden it. Skia's path API in this version is `PathBuilder` + `.snapshot()`/`.detach()`, not `Path::move_to` (skia-safe 0.98 moved it); the compile error names `Handle<SkPath>` and offers no hint. |
| Hover crosshair + tooltip | **assembled** | `canvas().on_pointer_move(…)` gives `element_location` already in element-local logical px — no manual hit-testing, no scroll-offset math. The crosshair + snapped marker are drawn in Skia; the **tooltip is a real element** (`rect` + two `label`s, `Position::new_absolute()`, `Layer::Overlay`, `interactive(false)`) because `CanvasContext` exposes a `FontCollection` but no text measurement, so drawing the tooltip in Skia would mean building a `ParagraphBuilder` by hand. Observed working (screenshot shows `#131 / 57.05 %`). |
| Slider control | **built-in** | `Slider::new(on_moved).value(percent)` — one line. Caveat: it is hard-wired to a 0..100 **percentage**, so a 1–60 Hz control needs the two mapping expressions itself, and there are no steps/ticks. |
| Click-to-select | **built-in** | `.on_press(handler)` on the card rect. `on_press` is the high-level "activate" event: it fires for left-click, touch, *and* keyboard activation on a focused element, so a11y comes free. Selection highlight is a `Border` swap. |
| Animation | **built-in** | `use_animation(\|conf\| { conf.on_change(OnChange::Rerun); AnimNum::new(0., target).time(160).ease(Ease::Out).function(Function::Quad) })` from `freya::animation`. It owns its own clock (an `async-io` task), re-runs when a signal read inside the closure changes, and you just read `.get().value()` — no frame subscription, no `Instant` threading. Used for the hover elevation (background lerp + shadow y/blur). There is no layout/FLIP animation, so the grid reflow on drop snaps rather than slides — documented gap. |

## The sharpest edge: canvases that never repaint

`RenderCallback`'s `PartialEq` returns `true` unconditionally, and
`CanvasElement::changed()` is `self != other`. A `canvas()` whose *only*
changing input is the data captured by its closure therefore diffs as
**unchanged**: the old element stays in the tree and the render pipeline keeps
invoking the *first* closure forever. Observed directly — the six sparklines
were frozen at their seed values while the main chart animated, because the
main chart's canvas also carried `on_pointer_move`/`on_sized` handlers and
`Callback::eq` returns `false` unconditionally, which forced its element to be
replaced every render.

The workaround in this app is a one-line helper:

```rust
fn live_canvas(on_render: RenderCallback) -> Canvas {
    canvas(on_render).on_wheel(|_| {})   // any handler makes the element differ
}
```

Nothing in the docs mentions this; there is no `Canvas` equivalent of iced's
`canvas::Cache::clear()`. Note also `render_pipeline.rs` carries a
`// TODO: Use incremental rendering` — every painted frame redraws the entire
tree, so "canvas is stale" is a diffing problem, not a painting one.

## Helper crates

- **async-io 2.6.0** — `Timer::after` for the 10 Hz tick loop. Needed because
  Freya's executor exposes no timer; already a transitive dependency of
  `freya-animation`, so it adds no build cost.
- Feature flag **`engine`** on `freya` itself — without it the Skia types
  (`Paint`, `PathBuilder`, `SkColor`) that `canvas()`'s callback requires are
  not nameable, so the callback cannot be written at all. Easy to miss: the
  `canvas` component is available without the flag.
- No `rand` (10-line xorshift64\*), no DnD crate, no plotting crate.

## Repaint / cost profile

- Freya repaints when a signal a component read is mutated. The tick loop
  writes `metrics` at the configured rate, so at 10 Hz the whole tree is
  re-rendered and (per the TODO above) fully re-painted 10×/s.
- Hover crosshair: writing the `cursor` signal re-renders `MainChart` only.
- Hover animation: `use_animation` runs its own task for ~160 ms per hover.
- Self-observed while running at 10 Hz (`ps -o %cpu=,rss=`): **4.2–5.2 % CPU**,
  **RSS ≈ 99.6 MiB**.

## Where the time went

1. **Canvas staleness.** Diagnosing why sparklines froze while the main chart
   updated — the difference turned out to be the presence of event handlers.
2. **Skia API archaeology.** `PathBuilder` vs `Path`, `Color → SkColor`
   conversion, and the fact that `ctx.size` is already divided by the scale
   factor (so everything is in logical px).
3. Layout vocabulary — `Size::flex(1.)` vs `Size::fill()`, and that `DropZone`
   wraps its child in an `auto`-sized rect (so children need explicit sizes).

## Surprises

- Good: **drag-and-drop is a first-party component**, typed and ~30 LoC to use.
  Of the frameworks in this cohort that is the least-effort DnD by a wide
  margin.
- Good: `use_animation` needs no frame scheduling from the app at all.
- Good: `on_press` unifies mouse/touch/keyboard activation, and every element
  has AccessKit fields (`a11y_role`, `a11y_alt`) inline.
- Bad: the canvas staleness above.
- Bad: `Slider` is percentage-only with no range/step; `Input` is single-line.
- Bad: debug builds silently inject the FPS overlay plugin
  (`#[cfg(debug_assertions)]` inside `freya::prelude::launch`), which is
  confusing the first time you see it.

## Totals

- LoC: 624 (single `src/main.rs`, heavily commented)
- Dependencies added: `async-io 2.6.0`; `freya` feature `engine`
