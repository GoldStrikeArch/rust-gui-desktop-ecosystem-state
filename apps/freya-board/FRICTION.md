# FRICTION — Board (freya =0.4.0)

Reference: SPEC-3.md. Built + verified on macOS 26.5.2 (M4 Pro, rustc 1.96.1):
`cargo build --release` clean, `cargo build --locked --release` reproduces,
binary launched and stayed alive. **Every capability below was exercised with
synthetic CoreGraphics mouse events and `System Events` keystrokes and checked
against window-scoped screenshots**, not asserted by construction.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| Cross-column DnD | **built-in** | `DropZone::new(DragZone::new(card_id, body).drag_element(ghost), on_drop)`. The payload is typed (`u64` here) and comes straight back in the drop handler; the whole reorder engine is ~35 LoC of `Vec` surgery. Observed: "Collect binary sizes" dragged from Todo into Doing, counts updated 3/2/1 → 2/3/1. |
| Within-column reorder | **built-in** | Same mechanism — every card is wrapped in a `DropZone` that means "insert *before* me", and the index is corrected when the source was earlier in the same column. Observed: card dragged from Doing index 2 to index 0. |
| Drop indicator | **assembled** | `DropZone::on_drag_over(bool)` fires on enter/leave *only while a drag of that payload type is in flight*, which is exactly the signal you want; it drives a `drop_slot` signal, and a 4→6 px pill between cards paints accent when it matches. ~20 LoC. |
| Drag ghost/preview | **built-in** | `DragZone::drag_element(...)` renders any element at the cursor, offset by the grab point, on `Layer::Overlay` with `interactive(false)` (which correctly propagates to children, so the ghost never eats the drop). `show_while_dragging(true)` keeps the original in place. Zero code beyond describing the ghost. |
| Inline edit (dbl-click, Enter/Esc) | **hand-rolled** | Freya 0.4 has **no double-click event** — no `on_double_click`, no click count in `MouseEventData`. Implemented by remembering `(card_id, Instant)` of the last press and comparing against a 400 ms window. Enter is `Input::on_submit`; Escape needs `Input::on_pre_key_down`, which *replaces* the widget's stock key filter, so the app has to re-implement the default arms (`Enter`/`Shift` → pass, `Tab` → skip, everything else → `stop_propagation` + `prevent_default`) around its own Escape case. Observed: double-click opened the editor, typing + Enter committed ("v2Collect binary sizes"), Escape on the add-card editor cancelled it. |
| Add/delete cards | **built-in** | `Button` + a per-column `adding: Option<usize>` signal that swaps the button for an `auto_focus(true)` `Input`; delete is `.on_press` on a 20×20 rect carrying `a11y_role(Button)` + `a11y_alt`. Observed working. |
| Drop/reorder animation | **built-in** | `use_animation_with_dependencies(&(column, index), …)` with `OnChange::Rerun` replays an `AnimNum::new(0.94, 1.0).time(180).ease(Ease::Out).function(Function::Back)` scale-in whenever a card's position changes — i.e. exactly on drop. No frame scheduling, no `Instant` threading. There is **no layout/FLIP animation**, so neighbours snap into their new slots instead of sliding; the moved card is the only thing that animates (documented gap). |
| Independent column scrolling | **built-in** | One `ScrollView` per column. |

## Two sharp edges found here

**1. `Ref` held across a `write()` aborts the app with a modal dialog.**
`State::peek()`/`read()` return a `Ref`. Writing to the same signal while one is
alive panics — and a panic on the UI thread surfaces as a **blocking
`CFUserNotificationDisplayAlert` "Fatal Error" panel**, which freezes the whole
app (the window keeps its last frame; every later event is queued behind the
modal). The trap is `if let Some(x) = *state.peek() { state.write() … }`: the
scrutinee temporary is still alive inside the body. The fix is to copy the
value into a local first. Worth knowing that the failure mode is a hung window
plus a system alert rather than a stderr backtrace.

**2. `DropZone` cannot cover a container's slack space.**
`DropZone` renders `rect().width(Size::auto()).height(Size::auto())` around its
child and exposes **no width/height setters**, so it always shrink-wraps. "Drop
anywhere in the empty part of this column" therefore cannot be expressed by
sizing a zone; this app gives each column an explicit 140 px tail zone below the
last card instead. The obvious alternative — handling the drop by hand on the
column rect via `use_drag::<T>()` + `on_mouse_up` — did **not** work: an
`on_mouse_up` listener on an ancestor rect never fired for clicks in the
column's empty area, while `on_pointer_down` on the same rect fired every time
(logged from a debug build). Bubbling for released events looks unreliable
outside the `DropZone` path; the built-in components work, so this is filed as
a limitation rather than a hard blocker.

**3. `Size::flex` silently no-ops.** A child sized `Size::flex(1.)` collapses
unless the *parent* opts into `.content(Content::flex())`. There is no warning;
the first build of this app rendered a single full-width column.

## Helper crates

**None.** `freya = "=0.4.0"` with default features covers DnD, scrolling,
inputs, buttons and animation. `freya::animation` is compiled in unconditionally
(not feature-gated), which is why no extra flag appears in `Cargo.toml`.

## Where the time went

1. **The empty-column drop target** — three attempts (column-level
   `on_mouse_up`, `on_pointer_press`, then the explicit tail zone), including
   the `Ref`-across-`write` panic that made the app hang behind a modal alert.
2. Double-click emulation and re-implementing `Input`'s default key filter to
   get Escape.
3. `Content::flex()` discovery.

## Surprises

- Good: typed, first-party `DragZone`/`DropZone` with a working ghost and an
  `on_drag_over` hook — the least code of any framework in this cohort for a
  cross-container kanban.
- Good: `use_animation_with_dependencies` makes "animate when this thing moved"
  a two-line declaration.
- Bad: no double-click; `Input` is single-line only and its key handling is
  all-or-nothing.
- Bad: the panic-to-modal-dialog behaviour, which turns a small borrow mistake
  into a frozen app.

## Totals

- LoC: 462 (single `src/main.rs`)
- Dependencies added: none
