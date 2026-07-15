# SPEC-3: "Board" — a kanban task board (drag-and-drop test)

The purest "moving things around" test: cross-container drag-and-drop with
edit-in-place. Build idiomatically; the effort profile is the research output.

## Functional requirements

1. **Window** titled `Board (<framework>)`, ~900×600, resizable.
2. **Three columns**: "Todo", "Doing", "Done". Each column header shows the
   column name and a live card count. Start with 3–4 seed cards spread across
   columns.
3. **Add**: each column has an "+ Add card" affordance revealing an inline
   text input (Enter commits, Esc cancels, empty input is ignored).
4. **Delete**: each card has a ✕ that removes it.
5. **Cross-column drag-and-drop**: drag a card from any column and drop it
   into any other column at a chosen position.
6. **Within-column reorder**: drag a card up/down inside its own column.
7. **Drop indicator**: while dragging, show where the card will land
   (insertion line, gap, or slot highlight).
8. **Drag ghost/preview**: the dragged card visibly follows the cursor (or a
   clear equivalent affordance).
9. **Inline edit**: double-click a card to edit its text in place (Enter
   commits, Esc cancels).
10. **Animation**: cards animate on drop/reorder (or a documented equivalent
    transition).
11. **Scrolling**: columns scroll independently when cards overflow.

## Implementation rules

Same as SPEC-2: independent crate at `apps/<framework>-board/` (package
`<framework>-board`), same pinned framework version as `apps/<framework>-app/`
(crib setup from it), Rust helper crates allowed and recorded, no external JS
libraries for webviews, fallback rule applies (approximation + FRICTION.md
entry), build + 10 s launch check on macOS.

## FRICTION.md (required, per app)

Rating (built-in / assembled / hand-rolled / not-achievable) + short note for
each capability:

| Capability |
|---|
| Cross-column DnD |
| Within-column reorder |
| Drop indicator |
| Drag ghost/preview |
| Inline edit (dbl-click, Enter/Esc) |
| Add/delete cards |
| Drop/reorder animation |
| Independent column scrolling |

Also: helper crates used + why, total LoC, where the time went, surprises.
See SPEC-2.md for the rating rubric.

## Reference machine

Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
