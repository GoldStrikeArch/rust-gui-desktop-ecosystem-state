# FRICTION — Board (floem git @ 778bb5f2)

Reference: SPEC-3.md. Built + verified on macOS (M4 Pro, rustc 1.96.1):
`cargo build --release` clean (locked rebuild too), launched, alive after
10 s, killed cleanly, nothing on stdout/stderr.

Version note: same pinned git rev as apps/floem-app (crates.io 0.2.0 stale;
`main` unpublishable — forked `floem-winit`). See apps/floem-app/GAPS.md.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| Cross-column DnD | **built-in** | Cards are `.draggable_with_config()` with the card id as `custom_data`; any card or column tail registered for `DragTargetEnter` receives that id and calls one `move_card` helper. Cross-container works exactly like within-container — floem's drag system has no notion of container boundaries to fight. ~30 LoC including the model helper. |
| Within-column reorder | **built-in** | Same `DragTargetEnter` handler; hovering a sibling card moves the dragged card into its position (live reflow). No index math beyond a `position()` lookup. |
| Drop indicator | **assembled** | Live reflow makes the board itself the preview; the dragged card's slot is styled (accent border + tinted background) via a `dragging: RwSignal<Option<u64>>` set in `DragStart`/`DragEnd`/`DragCancel` listeners. A dedicated insertion-line API does not exist, but ~10 LoC of styling on top of the built-in events suffices. |
| Drag ghost/preview | **built-in** | floem re-paints the dragged view at the cursor automatically; `dragging_style` styles that ghost (shadow, accent border, translucency). Zero mechanics code — compare the iced port's `pin`-in-`stack` + global cursor subscription. |
| Inline edit (dbl-click, Enter/Esc) | **assembled** | `dyn_container` swaps a card row for a `TextInput` when `editing == Some(id)`; `DoubleClick` is a first-class typed listener; Enter is the `TextInputEnter` custom event; Esc is a `KeyDown` check. `ViewId::request_focus()` focuses the fresh input (pattern lifted from the upstream todo-complex example). |
| Add/delete cards | **built-in** | Buttons + signal updates; the reveal-on-demand add input is the same `dyn_container` + `request_focus` pattern; Enter/Esc identical to inline edit. |
| Drop/reorder animation | **built-in*** | On release floem animates the ghost into place with a configurable easing (`Spring::snappy()` here) — genuinely pleasant and free. The asterisk: the *other* cards snap when the list reflows mid-drag; there is no FLIP/layout animation for reflow (same gap as every other framework tested so far). |
| Independent column scrolling | **built-in** | Each column body is `.scroll()` with `flex_grow(1)`. One layout papercut: the inner stack needs `min_height_full()` so the tail drop target fills short columns. |

## Helper crates

None. Everything above is stock floem.

## Design notes / caveats

- **Live-reflow commit model**: the card is moved in the model while
  dragging, so releasing anywhere "commits" the last hovered slot, and Esc
  (which floem treats as drag-cancel) does NOT restore the original order —
  matching the iced port's behavior, and acceptable per spec, but a
  transactional model would need to defer the move until `DragTargetDrop`.
- Card text inside a keyed `dyn_stack` row must be re-derived reactively
  (`Label::derived` + signal lookup) or an inline edit won't refresh the
  row, since keyed diffing keeps the old view. Classic fine-grained-
  reactivity gotcha, cost ~15 minutes.

## Where the time went

1. **Editing/adding state choreography** — which signal owns the buffer,
   commit-vs-cancel paths, focusing the freshly created input.
2. **Column layout** — getting scroll + tail-drop-target + `min_height_full`
   to cooperate in taffy flexbox.
3. **DnD itself was nearly free** — the dash app's card reorder generalized
   to cross-column in one sitting.

## Surprises

- Good: cross-container DnD costs the same as within-container; the drag
  system is container-agnostic. This was the spec's hardest capability and
  floem's easiest.
- Good: typed `DoubleClick` listener exists (several frameworks make you
  hand-time double clicks).
- Bad: no transactional drop or FLIP reflow animation; live reflow is the
  only ergonomic pattern the event set naturally supports.

## Totals

- LoC: 353 (single `src/main.rs`)
- Dependencies added: none beyond the pinned floem git rev
