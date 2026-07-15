# SPEC-2: "Pulse" — a live metrics dashboard (interactivity test)

Iteration 2 of the experiment. Where SPEC.md (todo app) tested forms/lists,
this tests the **interactivity difficulty curve**: drag-and-drop, live data,
custom drawing, hover interactions, animation. Build it as idiomatically as
possible; the *effort profile* is the research output.

## Functional requirements

1. **Window** titled `Pulse (<framework>)`, ~900×640, resizable.
2. **Metric card grid**: 6 cards (e.g. CPU, Memory, Network In, Network Out,
   Disk, Requests). Each card shows: metric name, the current value as a big
   number, and a **sparkline** of the last 60 samples.
3. **Drag-and-drop reorder**: cards can be dragged to a new position in the
   grid; the grid reflows. A visible cue (ghost, elevation, or slot indicator)
   shows where the card will land.
4. **Live synthetic data**: all 6 metrics update from a synthetic generator
   (smooth random walk) ticking at a configurable rate, default **10 Hz**.
   Ticking must not require user input (tests timers/subscriptions/async and
   the repaint model).
5. **Main chart**: a large line chart of the *selected* metric (click a card
   to select it; selected card visibly highlighted), plotting the last ~300
   samples, scrolling as data arrives.
6. **Hover interaction**: hovering the main chart shows a crosshair (or
   marker) + tooltip with the value and sample index/time at the cursor.
7. **Controls row**: a **pause/resume** button and a **tick-rate slider**
   (1–60 Hz) with the current rate displayed.
8. **Animation**: at least one deliberate animation — animated card reorder,
   hover elevation transition, or a smooth value/needle transition. Document
   what animation primitives the framework offers (tweening? springs?
   per-frame callbacks? CSS?).

## Implementation rules

- Independent crate at `apps/<framework>-dash/`, package `<framework>-dash`,
  runnable via `cargo run --release`. NOT a workspace member.
- **Pin the same framework version as iteration 1** (see the sibling
  `apps/<framework>-app/Cargo.toml`); crib project setup from it (esp. tauri
  config / slint build script).
- Framework-ecosystem **Rust helper crates are allowed** (e.g. egui_plot,
  a DnD helper crate, an animation crate) — *needing* one is itself a finding;
  record every helper crate and why in FRICTION.md. Webview frameworks may
  hand-roll Canvas2D/DOM but must use **no external JS libraries**.
- **Fallback rule**: if a capability can't be expressed, ship the closest
  approximation (e.g. up/down buttons instead of DnD) and record the gap in
  FRICTION.md — gaps are data, not failures.
- Verify on macOS: build release, launch ~10 s, confirm alive, kill. Interact
  if scriptable; otherwise verify by construction and say so.

## FRICTION.md (required, per app)

For each capability below, a rating + 1–3 sentence note:

| Capability | Rating |
|---|---|
| DnD card reorder | built-in / assembled / hand-rolled / not-achievable |
| Live data (timer @ 10 Hz) | " |
| Sparklines + main chart | " |
| Hover crosshair + tooltip | " |
| Slider control | " |
| Click-to-select | " |
| Animation | " |

Ratings: **built-in** = a widget/API does it directly; **assembled** = compose
existing framework primitives (< ~30 LoC per capability); **hand-rolled** =
you implemented the mechanics yourself (hit-testing, interpolation, redraw
scheduling…); **not-achievable** = shipped a documented approximation.
Also record: helper crates used, total LoC, roughly where the time went, and
anything that surprised you (good or bad).

## Reference machine

Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
