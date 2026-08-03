# FRICTION — Board (vizia =0.4.0)

Reference: SPEC-3.md. Built + verified on macOS 26.5.2 (M4 Pro, rustc 1.96.1):
`cargo build --release` and `cargo build --locked --release` clean (no
warnings), binary launched, window pixel-verified, alive past the 10 s bar,
killed cleanly. **No fallback was needed** — every one of SPEC-3's eleven
requirements is implemented for real and was exercised with synthetic input.

Evidence labels: **observed**, **synthetic-input** (CGEvent clicks / drags /
keystrokes scoped to this app's window, verified from window-scoped
`screencapture -l` shots), **source-only**, **unexercised**.

RSS after the full interaction sequence: **99.4 MiB** (`ps -o rss= -p <pid>`
/ 1024). Release binary: 21.9 MiB (Skia statically linked).

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Cross-column DnD | **built-in** | synthetic-input | vizia core ships drag-and-drop. Three modifiers do the whole thing: `.on_drag(\|ex\| ex.set_drop_data(ex.current()))` (marks the view `DRAGGABLE`, fires when the pointer leaves the pressed card), `.on_over(..)` gated on `ex.has_drop_data()`, `.on_drop(\|ex, data\| ..)`. Proven: dragging "Draft the RFC" from Todo into Doing moved the card and the header counts went 3/2/1 → 2/3/1. |
| Within-column reorder | **built-in** | synthetic-input | Exactly the same handler; the model just adjusts the target index by one when the source precedes the destination in the same column. Proven: "Review PR #412" dragged from index 2 to index 0 inside Doing. |
| Drop indicator | **assembled** | synthetic-input | A zero-height `Element` between every pair of cards with `.toggle_class("active", drop_at.map(..))`, plus a `.column-active` class on the whole column. The insertion *position* is free because the drop target is the card you are over — no geometry maths. Mid-drag screenshot shows the blue 6 px insertion bar between "Port the parser" and "Review PR #412" and the highlighted destination column. |
| Drag ghost/preview | **hand-rolled** | synthetic-input | The one genuinely manual part: vizia arms and routes the drag but renders nothing, so the ghost is an absolutely positioned `Label` whose `left`/`top` are bound to a cursor signal fed by the root's `.on_mouse_move`. ~12 lines including the physical→logical scale conversion. Mid-drag screenshot shows the blue "Draft the RFC" ghost chasing the pointer. |
| Inline edit (dbl-click, Enter/Esc) | **built-in** | synthetic-input | `.on_double_click(..)` is a core action modifier; the card's `Label` is swapped for a `Textbox` by a `Binding` on the `editing` signal, and the `Textbox` gives Enter and Esc directly: `on_submit(\|cx, text, enter\|)` where `enter == true` means the Enter key (`false` means focus loss), and `on_cancel` is wired to Escape inside the widget. `.on_build(\|cx\| { cx.focus(); cx.emit(TextEvent::StartEdit); })` puts the caret in it and selects the existing text. Proven: double-click opened the editor with the text selected, typing + Enter committed. |
| Add/delete cards | **built-in** | synthetic-input | "+ Add card" swaps to an inline `Textbox` under the same `Binding` pattern; empty/whitespace input is rejected in the model. Proven: added "Cut a release tag" to Done (count 1 → 2); Esc while typing "THROWAWAY" left the count unchanged (13, not 14); the ✕ button removed a card (Todo 3 → 2). |
| Drop/reorder animation | **built-in** | observed | CSS. `.drop-line { height: 0px; transition: height 140ms; }` / `.drop-line.active { height: 6px; }` — the framework interpolates the layout property, so the surrounding cards genuinely slide apart and back. `.card` also has `transition: background-color, scale, shadow` for hover elevation, and `.column` transitions its highlight. No animation crate, no per-frame callback, no `Instant` threading. The gap (shared with the whole cohort): there is no FLIP/layout-position animation, so a card that lands in a new column appears there instantly rather than flying. |
| Independent column scrolling | **built-in** | synthetic-input | One stock `ScrollView` per column. Proven: after adding 10 filler cards, scrolling with the wheel over the Todo column moved only that column (its scrollbar visible, first visible card became "Write release notes") while Doing and Done stayed put. |

## Traps found (all silent failures)

1. **`on_press` / `on_drag` / `on_double_click` need `hoverable(false)`
   children.** vizia only runs an action when the acted-on view is the
   *hovered* entity (`cx.current == meta.target`). A click that lands on a
   card's `Label` never reaches the card. Marking non-interactive children
   `.hoverable(false)` is the fix and is what vizia's own examples do.
2. **`on_drop` ordering.** `on_drop` runs while `WindowEvent::MouseUp` is
   still propagating up to the root model, and it only *queues* the app
   event. A root MouseUp handler that cleared the drag state inline made
   every drop a silent no-op. Fix: queue a `DragEnd` event instead so the
   order is Drop → DragEnd.
3. **Event coordinates are physical, layout units are logical.**
   `on_mouse_move`/`cx.bounds()` are in device pixels; `Pixels(..)` is
   logical. The ghost was initially offset by 2× until divided by
   `cx.scale_factor()`.

## Helper crates

**None.** `vizia = "=0.4.0"`, default features only. DnD, double-click,
Enter/Esc text commit/cancel, per-column scrolling and the drop animation
are all core.

## LoC

`src/main.rs`: **499** lines total (heavily commented), of which roughly
~150 lines are CSS, ~150 model/events, ~180 view construction. **No
verification hooks are compiled in** — all evidence above came from external
CGEvent synthesis plus window screenshots, so production LoC == total LoC.

## Where the time went

1. The three silent-failure traps above (~60 % of the time). Each one
   compiles, runs, and does nothing.
2. Deciding the insertion-position model. Once "the card you are over *is*
   the drop target" clicked, index handling was ~10 lines.
3. The DnD itself was ~20 minutes. This is the first framework in the cohort
   where cross-container drag & drop needed no helper crate, no manual
   hit-testing and no cursor-tracking subscription.

## Surprises

- Good: `on_drag`/`on_over`/`on_drop`/`has_drop_data` in core, and
  `DropData::File(PathBuf)` means the *same* API also receives Finder file
  drops (used in apps/vizia-tray).
- Good: `transition: height` on a layout property actually animates layout —
  most immediate-mode/retained toolkits in this cohort animate paint only.
- Good: `Textbox::on_submit`'s second argument distinguishes Enter from
  focus loss, and `on_cancel` is Escape. SPEC-3's "Enter commits, Esc
  cancels" is two closures.
- Bad: nothing warns you when an action modifier can never fire because a
  child is eating the hover. This cost more time than every SPEC-3 feature
  combined.
- Neutral: `Binding::new` rebuilds a whole subtree on change, so the
  Label↔Textbox swap and the per-column card list are coarse-grained
  compared to `Signal`-level updates. At kanban scale this is invisible.
