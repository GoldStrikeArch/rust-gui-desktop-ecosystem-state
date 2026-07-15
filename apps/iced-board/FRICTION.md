# FRICTION — Board (iced =0.14.0)

Reference: SPEC-3.md (rating rubric in SPEC-2.md). Built + verified on macOS
(M4 Pro, rustc 1.96.1): `cargo build --release` clean, binary launched, alive
after 10 s, killed cleanly. It is event-driven with no idle timers. A quick
local read showed approximately 0% CPU, but no raw sampling trace was retained,
so that figure is qualitative rather than a controlled result.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| Cross-column DnD | **hand-rolled in this app** | Iced core has no general cross-container card-reorder widget; `pane_grid` handles only its own panes. `iced_drop 0.2.37` supports Iced 0.14, but it was not discovered or evaluated during this implementation for the SPEC-3 cross-column behavior. This app uses `mouse_area::on_press`, an 8 px threshold, remove-while-dragging model state, per-card `on_move`, lane/tail targets, and a conditional global event subscription. ~110 LoC of mechanics; widget enter/move messages avoid manual rectangle hit-testing. |
| Within-column reorder | **hand-rolled** | Free once the cross-column machinery exists — identical code path (remove → retarget → insert). The remove-while-dragging trick keeps indices stable so there is no special same-column bookkeeping. |
| Drop indicator | **assembled** | A 4 px accent-colored `container` injected into the target column's card list at the insertion index (the column's `spacing` provides the gap). Declarative views make this pleasant: the indicator is just data-driven layout, ~10 LoC. |
| Drag ghost/preview | **assembled** | `pin(ghost).x(cursor.x).y(cursor.y)` layered via a root `stack![]`, fed by the global cursor subscription. The `pin` widget (new in 0.14) makes cursor-anchored overlays trivial; since the ghost contains no interactive widget it never steals hover events from drop targets underneath. No occlusion problem at all. |
| Inline edit (dbl-click, Enter/Esc) | **assembled** | Double-click detection is built into `mouse_area::on_double_click`. Enter = `text_input::on_submit`. Focus on open = `operation::focus(id)` Task. The sharp edge is **Esc**: `text_input` captures the Escape key (uses it to unfocus), so the documented `keyboard::listen()` subscription (ignored events only) never fires — you must use `event::listen_with` and match the raw key event regardless of capture status. |
| Add/delete cards | **built-in** | Plain `button` + conditional `text_input` swap in the view; Enter commits via `on_submit`, empty input ignored, ✕ button per card. Standard Elm-style CRUD, no friction. |
| Drop/reorder animation | **hand-rolled** (approximation) | iced has no layout/FLIP animation: when the order changes, cards *snap* to their new slots. Shipped approximation per the fallback rule: the landed card plays a 200 ms scale + shadow "settle" pop using the built-in `Animation` API + `float` widget, with `window::frames()` subscribed only while it runs. Animating actual positions would require measuring widget bounds and interpolating manual offsets — not attempted. |
| Independent column scrolling | **built-in** | Each lane's card list is its own `scrollable(...).height(Fill)`; independence is automatic. Bonus: `mouse_area` coordinates keep working inside the scrolled viewport, so DnD targeting needed zero scroll-offset math. |

## Helper crates

None were used. Everything in this implementation is an Iced 0.14 built-in
(`mouse_area`, `pin`, `stack`, `float`, `Animation`, `operation::focus`,
`event::listen_with`). `iced_drop 0.2.37` now targets Iced 0.14; it was not
evaluated here, so no claim of ecosystem absence follows from this app.

## Repaint strategy

No timers: the app is fully event-driven and should not request steady idle
repaints. During a drag,
repaints follow the cursor-move message stream; `window::frames()` runs only
for the ~200 ms drop animation. Global event listeners (`listen_with`) are
subscribed conditionally (drag in flight, or an input open for Esc) so idle
hovering publishes no messages.

## Where the time went

1. **Drag targeting model** — deciding that the dragged card leaves the model
   while in flight (stable indices, target starts at source so a stray drop
   is a no-op), and covering the "end of column"/"empty column" cases with a
   tail zone + lane-level `on_enter`.
2. **Input lifecycle** — Enter/Esc/focus for the two inline inputs; finding
   out Esc never reaches `keyboard::listen()` because `text_input` captures
   it.
3. **Ordering edge cases** — reasoning about message order when nested
   `mouse_area`s (card inside lane) both fire on the same cursor move
   (child publishes first; lane `on_enter` fires once, cards refine on every
   move, so any mis-target lasts a single frame).

## Surprises

- Good: `mouse_area::on_double_click` exists (0.14) — expected to hand-roll
  click timing.
- Good: the ghost-overlay problem (drag preview stealing hover from drop
  targets) simply doesn't exist: iced has no occlusion-based hit-testing for
  plain containers, so events pass through the `pin` layer.
- Bad: Esc capture by `text_input` silently defeats the documented
  `keyboard::listen()`; needed source-diving to understand why the
  subscription never fired.
- Bad: no way to animate layout changes — drop animation had to be faked with
  scale/shadow on the settled card.

## Totals

- LoC: 617 (single `src/main.rs`, heavily commented)
- Dependencies: `iced = "=0.14.0"` only, default features (no extras needed —
  unlike the dashboard, which needed `smol` for timers and `canvas` for
  charts).
