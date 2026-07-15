# FRICTION — tauri-board ("Board", SPEC-3)

Tauri =2.11.5 / tauri-build =2.6.3 (same pins as ../tauri-app), same manual
no-Node setup. Frontend is vanilla HTML/CSS/JS; **no external JS libraries**.

## Design choice: state lives in JS, on purpose

Iteration 1 (tauri-app) deliberately put state in Rust to exercise the IPC
bridge; this app takes the other idiomatic Tauri shape. Board state is a
plain JS array in the webview because drag-and-drop is frontend-local and
latency-sensitive — `dragover` fires continuously and the drop indicator
must track it synchronously; a Rust round-trip per mutation would add async
JSON hops for zero benefit when there is no backend logic to own. The
consequence is stark: **the entire Rust side is 15 lines** (a
`tauri::Builder` shell), and none of the 8 capabilities below touches the
framework at all — for a pure-frontend app, "Tauri" is a window factory and
the difficulty curve is 100% the web platform's.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| Cross-column DnD | **hand-rolled** | HTML5 drag events (dragstart/dragover/drop) are the primitives, but per-column wiring, midpoint insertion-index math, and the two-splice state move are manual (~50 LoC). **Trap:** requires `"dragDropEnabled": false` in the window config or wry's native drag-drop handler swallows HTML5 DnD. |
| Within-column reorder | **hand-rolled** | Same machinery; the only subtlety is computing the index against the list *excluding* the dragged card so same-column moves land where the indicator showed. |
| Drop indicator | **assembled** | One absolutely plain `<div>` moved with `insertBefore` during `dragover`, removed on leave/drop (~15 LoC + CSS). Flex gap makes it read as an insertion line for free. |
| Drag ghost/preview | **built-in** | HTML5 DnD renders a native snapshot of the dragged card that follows the cursor at zero cost. Only trick: defer adding the dimmed `.dragging` class with `setTimeout(0)` so the snapshot captures the undimmed card. |
| Inline edit (dbl-click, Enter/Esc) | **assembled** | Swap the text span for an `<input>` on dblclick; Enter commits, Esc/blur cancels (~25 LoC). Must set `draggable = false` during the edit or text selection starts a drag. |
| Add/delete cards | **assembled** | Button⇄input swap per column footer (Enter commits and stays open for consecutive adds, Esc/blur closes, empty ignored); delete is a button per card (~35 LoC). |
| Drop/reorder animation | **hand-rolled** | FLIP over persistent card elements: measure rects, mutate state + re-append, apply inverted transforms, let a CSS transition play (~25 LoC). Works across columns because elements are kept in an id→element map and *moved*, not rebuilt. CSS keyframe pop-in for newly added cards. |
| Independent column scrolling | **built-in** | `overflow-y: auto` on each column's card list inside a flex column. Zero logic. |

## Helper crates

None. Direct deps are exactly `tauri` + `tauri-build` — with no commands,
even `serde` isn't needed directly. No Rust crate can help with any of the
above anyway; the webview owns the interaction layer.

## LoC (466 source; 506 including config)

- Rust: **21** (15 `src/main.rs` + 6 `build.rs`)
- Frontend: **445** (13 HTML + 267 JS + 165 CSS)
- Config: 40 (`tauri.conf.json` 33 + capability 7)
- Release binary **8.0 MiB**; **204 unique crate names** — the full Tauri dependency set
  is the fixed price of the window shell even at 15 lines of Rust.

## Where the time went

1. DnD correctness: insertion-index-excluding-dragged-card, indicator
   placement mapping state indices onto DOM positions, dragleave vs child
   `relatedTarget` — classic hand-rolled DnD edge cases.
2. FLIP with persistent element identity: re-rendering by *moving* elements
   (id→element map) instead of rebuilding, so animations and in-flight
   edits survive a render.
3. Headless verification harness (see below).

## Verification

Built release; launched raw binary ~10 s, alive, killed cleanly (no stdout/
stderr noise). Because this app has no IPC to self-report through and the
window can't be clicked headlessly, the frontend was smoke-run in
JavaScriptCore (WKWebView's engine) against a minimal DOM stub: seed render
and counts, add (Enter commits / empty ignored / Esc closes), inline edit
commit + cancel, delete, cross-column drag with indicator-position assert,
and within-column reorder — all asserted green. Remaining visual behavior
(ghost rendering, scrollbars, transitions) was reviewed by construction. The
JavaScriptCore harness and output were not retained, so this is a
collection-time narrative check rather than rerunnable executable evidence.

## Surprises

- The jsc smoke harness caught a real shipped-code bug before launch: the
  id→element cache was never populated (`cardEls.set` missing), so every
  render would have silently *duplicated* every card in the DOM. A
  10-second "window stays alive" check would have passed regardless —
  webview apps can look healthy from the outside while the UI is broken.
- The native HTML5 drag ghost is the single biggest freebie in this whole
  exercise — every non-webview framework in the study has to build that
  cursor-following preview by hand.
- Same `dragDropEnabled: false` trap as tauri-dash; it is the only Tauri
  config line the entire feature set depends on.
