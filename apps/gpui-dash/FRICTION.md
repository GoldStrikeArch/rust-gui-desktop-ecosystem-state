# FRICTION — Pulse (gpui =0.2.2)

Reference: SPEC-2.md. Built + verified on macOS (M4 Pro, rustc 1.96.1):
`cargo build --release` clean (crates.io gpui, `runtime_shaders` feature — see
apps/gpui-app/GAPS.md for that setup trap), binary launched, alive after 10 s
at ~2.1% CPU while ticking at 10 Hz / 75 MiB RSS, killed cleanly, empty log.

Authorship note: this app was built across two agent sessions (the first was
interrupted); ratings below are from a full audit of the code as it stands,
and the "where the time went" section is partly inferred from the code's
structure and comments rather than a single continuous experience.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| DnD card reorder | **assembled** | gpui ships real, typed drag-and-drop — the same machinery Zed uses for tab dragging: `.on_drag(payload, ghost_ctor)` starts a drag and paints the returned entity under the cursor every frame, `.drag_over::<CardDrag>()` restyles the hovered slot (the drop cue), `.on_drop::<CardDrag>()` delivers the payload to the slot it landed on. No sortable-grid widget, so the actual reflow is app code — a `Vec<usize>` slot→metric indirection with remove/insert (~12 LoC) — but there is zero manual hit-testing or cursor tracking. |
| Live data (timer @ 10 Hz) | **assembled** | No declarative timer/subscription API. The idiom is an entity-owned task: `cx.spawn(async move …)` looping on `BackgroundExecutor::timer(Duration)`. ~20 LoC including the useful parts: the loop re-reads `hz` each lap (slider changes take effect next tick, no task restart) and `this.update(…)` returning `Err` when the entity drops gives clean teardown for free. |
| Sparklines + main chart | **hand-rolled in this app** | `canvas()` element with a paint closure: `PathBuilder::stroke(px)` → `window.paint_path(path, color)`, gridlines via `paint_quad(fill(…))`. This implementation writes y-scaling, the right-aligned scrolling sample window and gridline placement (~120 LoC). Current alternatives include gpui-component charts, gpui-d3rs, gpui-px and plotters-gpui; they were not evaluated for this app. |
| Hover crosshair + tooltip | **hand-rolled** | `on_mouse_move` stores the window-coords cursor; nearest-sample snapping and the crosshair + marker are painted inside the canvas closure; the tooltip is an absolutely-positioned `div` with manual edge-flip near the right border. The sharp edge: event handlers cannot query an element's bounds, so the chart records its bounds during paint into an `Rc<Cell<Bounds<Pixels>>>` that the next frame's render reads for the snap/tooltip math (one frame of staleness, invisible in practice). |
| Slider control | **hand-rolled** | gpui core has no high-level slider. Track/fill/thumb are absolutely-positioned `div`s; click-to-jump via `on_mouse_down`; thumb dragging rides the DnD system — `on_drag` with an *invisible* ghost entity purely to get `on_drag_move` events, which stream the captured cursor position plus the listener's own bounds (gpui exposes no plain mouse-capture API). ~70 LoC including a zero-cost "bounds probe" canvas that records the track rect for the click math. |
| Click-to-select | **built-in** | `.on_click(…)` on the card sets `selected`; highlight is a conditional border color. Trivial. |
| Animation | **built-in** | `AnimationExt::with_animation(id, Animation::new(dur).with_easing(ease_in_out), \|el, delta\| …)` — a per-frame re-style tween with easing, restarted by changing the element id (keyed with an epoch counter). Used for the drop-settle fade when a card lands. What gpui offers: duration+easing tweens via this element wrapper and raw `window.request_animation_frame`; no springs, and no layout/FLIP animation — the grid reflow itself snaps (the settle fade is the drop feedback). The header's "live" pulse dot is deliberately driven off the data tick instead of a repeating animation, so it costs no extra frames. |

## Helper crates

None — `gpui = "=0.2.2"` only (plus its `runtime_shaders` feature for the
Xcode 26 Metal-toolchain workaround). `rand` was avoided with a 10-line
xorshift64. gpui itself covers DnD and tweening; chart/component alternatives
existed at audit time but were not selected or compatibility-tested here, so
the chart/slider/tooltip result describes this implementation, not an absence
across the ecosystem.

## Repaint strategy (for later CPU measurement)

- Event-driven: the 10 Hz tick task calls `cx.notify()`, which marks the
  window dirty; gpui re-runs `render()` for the entity tree and repaints the
  whole window scene once per dirty frame (retained scene, no partial damage).
- Hover/slider/drag interactions notify per mouse event (gpui itself also
  refreshes every frame while a drag is active so the ghost tracks).
- The oneshot 320 ms drop-settle animation is the only thing that requests
  frames outside of events, and only while running.
- Observed: ~2.1% CPU at 10 Hz with the window idle on M4 Pro.

## Where the time went

1. **Chart math** — the usual manual-plotting yak (y-scaling, right-aligned
   scrolling window, hover snapping, tooltip edge-flip); gpui gives you a
   good path rasterizer and nothing above it.
2. **Bounds plumbing** — discovering that neither render nor event closures
   can ask for an element's bounds, then threading `Rc<Cell<Bounds>>` probes
   (chart + slider track) recorded at paint time; `DragMoveEvent.bounds`
   helps only during drags.
3. **Slider-as-drag design** — realizing the DnD system with an invisible
   ghost is the sanctioned way to get captured pointer streaming, and
   unwrapping `Pixels` (its inner f32 is private in 0.2.2; everything goes
   through a `f32::from(Pixels)` helper).

## Surprises

- Good: a "no-widgets" framework has the best *native* DnD story of the
  frameworks tried so far — typed payloads, framework-painted ghost entity,
  `drag_over` styling. SPEC-2's hardest capability was the easy one here.
- Good: `with_animation` keyed by element id is a tidy oneshot pattern —
  bump an epoch in the id and the animation restarts, no timers to manage.
- Bad: no element-bounds query API — the canvas "bounds probe" workaround is
  needed for any geometry-aware interaction (chart hover, slider click).
- Bad: everything a widget toolkit would give you (slider, tooltip
  positioning, buttons) is your problem — known from iteration 1, and the
  slider alone cost more code than the entire drag-and-drop feature.

## Totals

- LoC: 733 (single `src/main.rs`, heavily commented)
- Binary: 5.3 MiB release (unstripped; 5,513,136 bytes)
- Dependencies: `gpui = "=0.2.2"` with `runtime_shaders` — no helper crates.
- Verification: build + 10 s launch-alive check re-run at audit time;
  interactions verified during development and by code review (scripted
  keystroke/mouse injection is blocked by macOS automation permissions in
  this environment — see apps/gpui-app/GAPS.md).
