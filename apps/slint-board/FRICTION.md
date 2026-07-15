# FRICTION — slint-board (Board, Slint =1.17.1)

Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2. Built with default slint
features (winit backend, femtovg GL renderer).

Headline finding: **Slint 1.17 ships first-class DnD elements** — `DragArea`
(source: press threshold, `dragging` state, `data-transfer` payload,
`drag-finished`) and `DropArea` (target: `can-drop(DropEvent)` streamed on
every drag-move with a *local cursor position*, `dropped(DropEvent)`,
`has-drag`). This app uses them (the sibling slint-dash hand-rolls TouchArea
dragging for comparison). They model payload transfer, not visual reorder — so
ghost, indicator, and insertion math are still assembled on top.

## Capability ratings

| Capability | Rating | Notes |
|---|---|---|
| Cross-column DnD | **assembled** | `DragArea` per card + `DropArea` per column did the heavy lifting (threshold, grab, target negotiation, cancel semantics). The payload is a `data-transfer` value that is deliberately opaque in the DSL — a Rust `pure callback` builds it (`DataTransfer::from("col:idx")`) and the drop handler parses it back. That host-side round-trip is boilerplate-y but honest. |
| Within-column reorder | **assembled** | Same mechanism; `can-drop` gives a column-local cursor position on every move → `round(y / row-h)` insertion index. Rust adjusts the index by −1 for same-column moves past the source. No list-reorder widget exists. |
| Drop indicator | **assembled** | Accent-outlined slot Rectangle at the insertion index + a real gap that opens (cards below shift by one row height). ~15 lines. |
| Drag ghost/preview | **hand-rolled** | The built-in `drag-image` only accepts a *bitmap*, and there's no element→image rendering from the DSL, so a live-looking ghost can't use it. Workaround: a pseudo-ghost Rectangle at window level driven by positions reported to `can-drop` (`DropArea.absolute-position + event.position`). Works because the 3 columns tile the window; the ghost freezes over dead zones (paddings) and there's no cursor-grab-offset (DragArea doesn't expose the press point). Biggest gap in the new API. |
| Inline edit (dbl-click, Enter/Esc) | **assembled** | `TouchArea.double-clicked` (built-in) toggles a conditional `LineEdit` overlay; `init => focus()+select-all()`; Enter = `accepted`. Esc has no LineEdit callback — a wrapping `FocusScope` catches the unhandled Escape via key-event bubbling. That trick is idiomatic but non-obvious. |
| Add/delete cards | **built-in / assembled** | LineEdit `accepted` + VecModel push/remove: trivial. Same FocusScope-for-Esc trick as inline edit. |
| Drop/reorder animation | **assembled** | Cards are absolutely positioned by index (`y: i * row-h + gap-shift`) with `animate y { 150ms, ease-out }` — the gap opening/closing and post-drop settling animate declaratively. Caveat: the repeater binds rows to fixed element instances, so a naive layout-based list can't animate reorder at all; you must position by index yourself. Model insert/remove itself is an instant data swap (no FLIP-style move animation). |
| Independent column scrolling | **built-in** | `Flickable { interactive: false }` — wheel/trackpad scrolling still works (verified in i-slint-core source: Wheel events bypass the `interactive` check) while press-drag flicking is disabled so it can't steal card drags. Without that flag, Flickable's drag-to-flick would fight the DnD. No auto-scroll while dragging near column edges (not implemented). |

## Helper crates

None.

## Repaint strategy

Purely event-driven: repaints only on property changes from interaction
(hover, drag-move, edits, model updates); femtovg redraws the full window per
dirty frame. No controlled board-idle CPU dataset was retained.

## LoC

- Rust: 129 (`src/main.rs`) + 3 (`build.rs`)
- Slint DSL: 367 (`ui/main.slint`)
- Total: 499

## Measurements

- Canonical clean release build **48 s**; no-op incremental build **7 s**.
- Dependency graph: **302 unique crate names / 311 name-version entries
  including the app**.
- Binary: **16,092,816 bytes raw / 14,288,600 bytes (13.6 MiB) stripped**.

## Where the time went

1. ~35% understanding the DragArea/DropArea contract (opaque `data-transfer`,
   `can-drop`/`dropped` return-value negotiation, `drag-finished` vs `dropped`
   responsibilities, `changed dragging` as the "drag started/ended" signal —
   there is no explicit start callback).
2. ~25% ghost workaround (drag-image is bitmap-only) + insertion-index
   geometry through Flickable viewport offsets.
3. ~15% inline edit focus/Escape handling.
4. Rest: styling, models, column component.

## Surprises

- Good: DnD elements exist at all (new-ish; modeled after platform DnD with
  copy/move/link actions and future inter-app support) — cancel handling,
  thresholds and target negotiation came for free.
- Good: `Flickable.interactive: false` keeping wheel scroll is exactly the
  needed switch for drag-inside-scroll UIs.
- Bad: no way to feed a rendered element into `drag-image`, and no press-point
  offset on `DragArea` — a faithful drag ghost still ends up hand-rolled.
- Bad: `data-transfer` being DSL-opaque forces a Rust round-trip even for a
  purely internal move ("col:idx" stringly-typed payload).
