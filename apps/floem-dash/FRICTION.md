# FRICTION — Pulse (floem git @ 778bb5f2)

Reference: SPEC-2.md. Built + verified on macOS (M4 Pro, rustc 1.96.1):
`cargo build --release` clean (locked rebuild too), binary launched, alive
after 10 s, killed cleanly. A 12-second `ps` sample at the default 10 Hz
measured **2.7% average CPU**.

Version note: same pinned git rev as apps/floem-app (crates.io 0.2.0 is 20
months stale; `main` is unpublishable because it depends on the forked
`floem-winit`). See apps/floem-app/GAPS.md.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| DnD card reorder | **built-in** | This is floem's standout cell. `.draggable_with_config()` on the card gives a drag with threshold, an automatic ghost (floem re-paints the dragged view at the cursor — `dragging_style` styles it), `DragTargetEnter` events on the other cards carry the source's `custom_data` for live grid reflow (the reflowed grid is the slot indicator), and release animates with a configurable spring easing. ~25 LoC total, no manual hit-testing, no global pointer subscription (the iced port hand-rolled ~90 LoC for the same interaction). |
| Live data (timer @ 10 Hz) | **assembled** | floem has NO interval/subscription primitive — only one-shot `exec_after(Duration, cb)`. The upstream `timer` example's own pattern is an `Effect` + `exec_after` + trigger-signal chain that re-arms itself; ~15 LoC and the rate is re-read each round so slider changes just work. Slightly galling that the framework's own example must hand-build an interval. |
| Sparklines + main chart | **hand-rolled** | No chart widget or ecosystem helper for this rev (git main — crates.io chart helpers target floem 0.2). The `canvas` view + kurbo `BezPath` through the `Renderer` trait: scaling, gridlines, axis labels, polylines all manual (~110 LoC). BUT: the paint closure is signal-tracked — reading `samples.get()` inside it makes repaint scheduling fully automatic, no cache/damage management at all (iced needed 7 explicit `canvas::Cache`s). |
| Hover crosshair + tooltip | **hand-rolled** | Pointer position lands in an `RwSignal<Option<Point>>` via `on_event_cont(listener::PointerMove/PointerLeave)`; positions are already view-local. The paint closure reads the signal, so the crosshair repaints reactively. Genuine win over iced: `TextLayout::new_with_text(...).size()` gives REAL text measurement for the tooltip box (iced estimated width from char count). Snapping/edge-flipping still manual. |
| Slider control | **built-in*** | `Slider::new_ranged(value_fn, 1.0..=60.0).step(1.0)`. The asterisk: changes arrive as a typed custom event (`on_event_stop(SliderChanged::listener(), ...)` reading `changed.value`), not a plain callback — and the doc example in the source (`event.state.value`) doesn't compile; the extractor already unwraps to `&SliderState`. Minor API-churn papercut. |
| Click-to-select | **built-in** | `on_event_stop(listener::Click, ...)` + reactive border style via `selected` signal. (`on_click_stop` exists but is deprecated at this rev.) |
| Animation | **built-in** | Two primitives used deliberately: (1) style transitions — `transition_background(Transition::linear(200.millis()))` + `hover()` styles gives tweened hover elevation with zero scheduling code; (2) the drag-release spring (`easing::Spring::snappy()`) that animates the ghost into place. floem also has a full keyframe `Animation` API (`.animation(|a| a.keyframe(...))`) with springs/bezier easings — not needed here. Animation is clearly a first-class subsystem, unlike iced's schedule-your-own-frames model. |

## Helper crates

None. DnD, animation, slider are framework built-ins; charts are raw canvas;
`rand` avoided with a 10-line xorshift* (same as the iced port).

## Repaint strategy (for CPU comparison)

- floem repaints only views whose tracked signals changed. Each tick writes 6
  value signals + 6 sample signals → 6 sparkline canvases + 1 chart canvas +
  7 labels repaint; everything else is untouched.
- Crosshair movement writes only the `hover` signal → only the chart canvas
  repaints.
- Hover/drag animations are driven internally by floem's transition system;
  no app-side frame scheduling exists anywhere in this file.
- Measured: 2.7% average CPU at 10 Hz over 12 s (iced port: 3.5% over 30 s).

## Where the time went

1. **API archaeology** — this rev deprecates the documented API surface
   (`empty()` → `Empty::new()`, `on_click_stop` → typed `Click` listener),
   and the slider's change-event shape had to be read from the source.
2. **Chart drawing** — the usual manual-plotting work, though signal-tracked
   canvases removed the whole cache/invalidations layer iced needed.
3. **DnD took minutes, not hours** — the widget-gallery `draggable` example
   is a direct template for sortable cards.

## Surprises

- Good: built-in drag-and-drop with ghost + spring release is unique among
  the frameworks measured so far; the SPEC's hardest capability was the
  easiest cell in this app.
- Good: signal-tracked canvas paint closures — reactive repaint for custom
  drawing with zero bookkeeping.
- Good: real text measurement (`TextLayout::size()`) usable inside canvas.
- Bad: no interval timer primitive; the framework's own example hand-rolls
  the re-arming `exec_after` chain.
- Bad: doc/example drift on `main` — in-source doc examples that don't
  compile against the same rev (slider event), deprecated constructors
  everywhere the docs still use them.

## Totals

- LoC: 421 (single `src/main.rs`)
- Dependencies added: none beyond the pinned floem git rev
