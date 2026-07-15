# FRICTION — dioxus-dash ("Pulse", Dioxus 0.7.9 desktop/webview)

## Capability ratings

| Capability | Rating | Notes |
|---|---|---|
| DnD card reorder | **hand-rolled** | No DnD widget/helper in Dioxus. Built a mouse-event state machine (onmousedown arm → 6px threshold on root onmousemove → per-card onmousemove picks the target slot → root onmouseup commits) with drag state in a `Signal<Option<Drag>>`. HTML5 `draggable`/`ondragstart` attributes exist in RSX and `evt.prevent_default()` works in 0.7, but you'd still hand-roll the target math, and you lose control of the ghost — mouse events were the cleaner path. `.card * { pointer-events: none }` is required so events always target the card (offset coords are target-relative). |
| Live data (timer @ 10 Hz) | **assembled** | `use_future` + a loop with `tokio::time::sleep(1/hz)`; the loop re-reads the rate/paused signals every lap so the slider takes effect without restarting the task. Dioxus runs on tokio but re-exports no timer — you must add tokio yourself (feature `time`). ~12 LoC. |
| Sparklines + main chart | **hand-rolled** | No chart widget anywhere in the ecosystem for Dioxus desktop. Charts are SVG `polyline`/`line`/`text`/`circle` elements in RSX with points strings regenerated from the signal each tick; scaling/axis/scroll math is manual (~40 LoC of geometry helpers). Chose SVG-in-RSX over Canvas2D-via-`document::eval` deliberately: SVG stays inside the declarative signal→diff model (a tick diffs to one `points` attribute swap per chart), while Canvas needs hand-written JS strings driven imperatively from Rust. The main chart is drawn in pixel space; the `onresize` event (ResizeObserver-backed, new in 0.6+) supplies the real element width so hover math is exact. |
| Hover crosshair + tooltip | **assembled** | `onmousemove` on the SVG gives `element_coordinates()`; mapping x→sample index and rendering a `line`+`circle`+absolutely-positioned tooltip div is ~25 LoC. Caveat: offset coordinates are relative to the *event target*, so all SVG children need `pointer-events: none` or the numbers jump. |
| Slider control | **built-in** | `input { r#type: "range", min, max, value, oninput }` — plain HTML range input; the webview renders and drags it natively. |
| Click-to-select | **built-in** | An `onclick` would be one line. Because mousedown also arms the drag, "click" is instead detected as press+release without crossing the 6px threshold (3 LoC in onmouseup). |
| Animation | **assembled** | CSS only: transitions on the cards (hover lift + shadow, selected border, drop-target scale) and a keyframe pop-in on the drag ghost, all in a `style {}` block. Dioxus itself has **no animation primitives** — no tweening, springs, or per-frame callback API on desktop; anything CSS can't express (e.g. FLIP-animated grid reflow after reorder, tweened numbers) would need a hand-rolled timer loop writing signals. The reorder itself therefore snaps (documented gap; the ghost/indicator carry the affordance). |

## Helper crates

- `tokio` (features = ["time"]) — dioxus-desktop already *runs* on a tokio
  runtime but re-exports no sleep/interval API, so any timed loop needs the
  crate as a direct dependency. That a 10 Hz ticker requires adding an async
  runtime crate to Cargo.toml is itself a (small) finding.
- No chart, DnD, or animation helper exists for Dioxus desktop that I could
  find; everything above is hand-assembled from HTML/SVG/CSS + signals.

## Repaint strategy (for later CPU% measurement)

Write-driven, no frame loop: each tick writes the `metrics` signal → the
single App component re-runs → VDOM diff → minimal DOM edits over the
webview IPC (six sparkline `points` swaps, one main-chart polyline, a few
text nodes) → WebKit repaints. Paused = zero re-renders. At 60 Hz the whole
App closure re-executes 60×/s and rebuilds ~2 KB of SVG point strings per
frame — fine at this tree size; the idiomatic scaling lever would be
splitting cards into child components so each subscribes to its own slice.
Mousemove handlers deliberately avoid writing signals unless a value
actually changed, otherwise every pixel of cursor travel would re-render.

## Where the time went

- ~40% DnD state machine details: click-vs-drag threshold, ghost following
  the cursor via root onmousemove, making events target the card
  (`pointer-events: none` on children), cancel-on-window-leave.
- ~30% chart geometry + hover mapping (pixel-space vs viewBox coordinate
  mismatch is a trap; solved with `onresize` + drawing in pixel space).
- ~20% RSX borrow discipline: precomputing view-model structs before `rsx!`
  because holding `.read()` guards while building nested closures fights the
  borrow checker; `.peek()` vs `.read()` vs `.write()` choices.
- ~10% styling.

## Surprises

- (+) `onresize` (ResizeObserver-backed) fires on mount too — one handler
  gives exact chart width at startup and on window resize; no eval/JS needed.
- (+) The whole interactive surface worked on the first run once it compiled;
  webview CSS (grid, transitions, tabular-nums) does a lot of free work.
- (−) MouseEvent carries offset coordinates but not the target's size, so
  "above or below the element midpoint?" is unanswerable from the event alone
  — a real limitation for DnD math (worked around here; see dioxus-board for
  the overlay-halves workaround).
- (−) 10–60 Hz ticking requires adding tokio to Cargo.toml even though the
  framework already ships and runs it internally.

## Measurements

- `src/main.rs`: 524 lines (including ~90 lines of CSS-in-Rust and comments).
- Canonical serial clean release build: **33 s**; no-op incremental build:
  **1 s**. The 103.7-second parallel-load run is retained only as a
  noncanonical observation. Dependency graph: **279 unique crate names / 287
  name-version entries including the app**. Binary **6,156,656 bytes raw /
  5,246,872 bytes (5.0 MiB) stripped**.
- First `cargo check` after writing the code: **0 errors, 0 warnings.**
- Launch check: release binary ran 11 s in the background, window up, ticking
  at 10 Hz; empty stdout/stderr; clean SIGTERM exit. The controlled 30-second
  sample measured **11.9% CPU**, **14.2% peak CPU** and **208 MiB total
  process-tree RSS**. Earlier ~1.5%/~96 MiB observations were preliminary
  lifetime/main-process readings; raw per-second samples were not retained.
  Scripted UI interaction was not
  automated (driving WKWebView needs macOS Accessibility permissions);
  DnD/hover/slider behavior verified by construction as in iteration 1.
