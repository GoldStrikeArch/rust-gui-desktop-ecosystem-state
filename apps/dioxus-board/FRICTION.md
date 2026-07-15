# FRICTION — dioxus-board ("Board", Dioxus 0.7.9 desktop/webview)

## Capability ratings

| Capability | Rating | Notes |
|---|---|---|
| Cross-column DnD | **hand-rolled** | No DnD support in Dioxus and no desktop-compatible helper crate found. Mouse-event state machine with drag state in a `Signal<Option<Drag>>`: card `onmousedown` arms it, root `onmousemove` applies a 6px threshold + feeds the ghost, root `onmouseup` commits remove→insert across `Vec<CardData>`s, `onmouseleave` cancels. HTML5 `draggable` + `ondragstart/ondragover/ondrop` are all exposed in RSX and `evt.prevent_default()` works in 0.7 (events are handled synchronously), so that route is viable too — but you still hand-roll all the insertion-index math and give up ghost control, so it buys little. |
| Within-column reorder | **hand-rolled** | Same machinery; only extra work is the remove/insert index fixup when source and target share a column (~5 LoC). |
| Drop indicator | **hand-rolled** | The interesting failure: a Dioxus `MouseEvent` gives target-relative offset coords but **not the target element's size**, so "above or below the card's midpoint?" cannot be computed from the event. Workaround: while a drag is live, every card renders two invisible absolutely-positioned half-overlays (extended 5px past the card edges to also cover the flex gaps); `onmousemove` on a half sets the insertion target to (col, i) or (col, i+1). A flex-grow "endzone" per column catches append/empty-column drops. The indicator itself is then trivial — a real 4px div rendered at the target index. |
| Drag ghost/preview | **hand-rolled** | Fixed-position div following the cursor (client coords from root onmousemove), `pointer-events: none`, slight rotation + shadow. ~10 LoC + CSS, but only exists because the mouse-event route was chosen; HTML5 DnD would give a free (but unstylable) snapshot ghost. |
| Inline edit (dbl-click, Enter/Esc) | **assembled** | `ondoubleclick` swaps the card body for an `input`; `onkeydown` matches `Key::Enter`/`Key::Escape`; focus via `onmounted` + async `set_focus(true)` (autofocus attr alone is unreliable in a webview). `stop_propagation()` on the input's mousedown keeps editing from arming a drag. |
| Add/delete cards | **assembled** | Plain signal writes: footer button reveals an input (same Enter/Esc/focus pattern), ✕ button removes by index; `stop_propagation` on the ✕ mousedown so deleting never starts a drag. This tier is where Dioxus feels closest to React — effortless. |
| Drop/reorder animation | **assembled** | CSS keyframe (`settle`: scale 1.05→1 + shadow decay) replayed by making the moved card's bump counter part of its rsx `key`, which forces element recreation on drop. No framework animation API exists; list-reflow (FLIP) animation would be fully hand-rolled, so surrounding cards snap into place (documented approximation). The drop indicator also gets a pop keyframe. |
| Independent column scrolling | **built-in** | `overflow-y: auto` on each column body; the webview does the rest. Zero Rust. (No autoscroll-while-dragging near column edges — would be another hand-rolled timer; noted as a gap.) |

## Helper crates

None. (Searched for a Dioxus DnD helper: the ecosystem's options target
dioxus-web/wasm or are unmaintained pre-0.7; nothing usable for desktop 0.7.)

## Repaint strategy

Write-driven only: signal write → App re-runs → VDOM diff → DOM patch over
IPC. No frame loop. Drag handlers only write the drag signal when the ghost
position or target actually changed (guarded by `.peek()` comparisons), so
idle mouse travel costs nothing; during a drag the tree re-renders per
mousemove (~60/s), which the diff absorbs easily at this size.

## Where the time went

- ~45% the drop-target problem: discovering events don't carry element
  geometry, then designing the half-overlay + endzone scheme (including
  extending overlays past card edges so the gaps between cards are valid
  targets, and `stop_propagation` choreography so root still sees moves).
- ~25% drag/click/dblclick coexistence: movement threshold, stop_propagation
  on ✕ and inputs, suppressing drag while editing.
- ~15% RSX structure: nested `for` loops over `columns.read()` with per-item
  closures means cloning card text into each handler block; view-state
  (target/src_id/ghost) precomputed before `rsx!` to appease borrowck.
- ~15% CSS/polish (settle keyframe, dropline, ghost).

## Surprises

- (+) Key-based animation replay (bump counter in the rsx `key`) is a neat,
  zero-JS way to retrigger CSS keyframes on drop.
- (+) `evt.stop_propagation()` / `evt.prevent_default()` just work
  synchronously in 0.7 desktop — no leftover 0.5-era `prevent_default:`
  attribute weirdness.
- (−) No element geometry on events and no `getBoundingClientRect` shortcut
  (short of `document::eval` or async `onmounted` bookkeeping) is the single
  biggest DnD pain; the overlay trick is clean but non-obvious.
- (−) Everything DnD is DIY: ~120 of the file's lines exist only to move a
  card between two Vecs with good affordances.

## Measurements

- `src/main.rs`: 412 lines (including ~85 lines of CSS-in-Rust and comments).
- Canonical serial clean release build: **31 s**; no-op incremental build:
  **1 s**. The 103.3-second parallel-load run is retained only as a
  noncanonical observation. Dependency graph: **279 unique crate names / 287
  name-version entries including the app**. Binary **6,123,152 bytes raw /
  5,211,416 bytes (5.0 MiB) stripped**.
- First `cargo check`: 0 errors, 1 real warning — the `key:` on the card div
  was silently ignored because a conditional drop-indicator node preceded it
  in the for-loop body ("Keys are only allowed on the first node in the
  block"). That key drives the drop animation and list diffing, so the
  warning was load-bearing; fixed by rendering indicators *after* each card
  (as target i+1) plus one pre-loop indicator for index 0.
- Launch check: release binary ran 11 s in the background, window up. A
  contemporaneous main-process RSS reading was ≈100 MiB, but its raw sample
  was not retained; empty stdout/stderr; clean SIGTERM exit. Scripted UI
  interaction not automated (WKWebView needs Accessibility permissions);
  DnD mechanics verified by construction.
