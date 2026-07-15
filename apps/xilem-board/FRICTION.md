# FRICTION — xilem-board ("Board", xilem 0.4.0 from crates.io)

Observed on macOS 26.5 (M4 Pro): release build, 10 s launch checks, and a
contemporaneous scripted interaction run via synthetic CGEvent mouse/keyboard
input covering drag/reorder, add/edit/delete and column counts. The harness,
raw output and screenshots were not retained, so these interaction details are
narrative evidence rather than a reproducible test artifact.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| Cross-column DnD | **hand-rolled** | Nothing in xilem/masonry 0.4 resembles DnD. Three hand-built pieces: (1) `CardFrame`, a custom masonry `Widget` wrapping each card that captures the pointer and reports drag events in window coordinates; (2) a **geometry registry** — every card and each column's list area write their window-space `Rect` into a shared `Arc<Mutex<HashMap>>` from the *compose* pass (so rects stay correct while columns scroll); (3) app-state hit-testing of that registry on every drag move to find the target column + insertion index. Works, but it re-implements what a real DnD layer would provide, and rect-registry staleness/cleanup is entirely on the app author. |
| Within-column reorder | **hand-rolled** | Same mechanism; insertion index = count of cards (excluding the dragged one) whose registered center-y is above the pointer. |
| Drop indicator | **assembled** | Visual is trivial view composition (a 4 px accent `sized_box` bar spliced into the column's card list at the computed index on each rebuild); the *index computation* rides on the hand-rolled DnD machinery. |
| Drag ghost/preview | **assembled** | Top layer of a window-spanning `zstack`: a card-shaped `sized_box(label)` moved with `transformed(..).translate(cursor − grab_offset)`; source card dimmed via a `post_paint` overlay in `CardFrame`. Stock views compose fine once the (hand-rolled) drag state exists in window coordinates. |
| Inline edit (dbl-click, Enter/Esc) | **hand-rolled** | Double-click: `PointerState.count >= 2` — the data is in the event, but only a custom widget can consume it. Enter-commit: built-in (`text_input(..).on_enter`). **Esc-cancel: no API** — masonry's `TextArea` ignores Escape; rescued by the fact that unhandled text events bubble to ancestors, so `CardFrame` catches the bubbled Escape and emits a cancel action. **Autofocus: no API** — xilem 0.4 has no focus view/API and masonry only allows `set_focus` from an `EventCtx`, so a hand-rolled `AutoFocus` wrapper widget grabs the inner `TextArea`'s `WidgetId` at view-build time (via `TextInput::area_pod()`) and focuses it on the first pointer event that bubbles through after creation (in practice: the mouse-up of the revealing double-click). Also: the caret lands at position 0 — there is no select-all/caret-placement API on the `text_input` view. |
| Add/delete cards | **assembled** | Add: stock `text_button` reveals an `auto_focus(text_input)` row (Enter commits, Esc cancels via the same bubbled-Escape catch, empty input ignored). Delete: the ✕ had to be a **mini `CardFrame` instead of a stock `Button`** — masonry pointer capture is last-wins and `Button` does not mark its Down as handled, so a stock button inside the draggable card frame loses its capture to the ancestor and never fires. My widget sets `set_handled()` on capture; nested frames then work. |
| Drop/reorder animation | **hand-rolled (approximation)** | No layout-position tweening, no FLIP, no transition primitives at any level. Documented equivalent: the dropped card plays a fade-out accent flash driven by `on_anim_frame` (view passes a `flash_seq`; new/changed non-zero value triggers the animation). Animating actual positions would require hand-rolling FLIP on top of the geometry registry. |
| Independent column scrolling | **built-in** | `portal(flex_col(cards))` per column. Two caveats: `Portal` passes its *viewport* max size down as the child's constraints (a known "due for rework" TODO in the source), which combined with Flex handing children its full loosened max made naive "fill the finite cell" widget sizing produce viewport-height cards — fill vs hug must be chosen explicitly per use site; and there is no auto-scroll when dragging near a scrolled column's edge (not implemented — gap). |

## Helper crates

None — `xilem =0.4.0` only. No DnD/animation helper crates exist for xilem.

## Totals

- LoC: **1160** (`main.rs` 412 + `widgets.rs` 748).
- The implementation is event-driven (no timers; animation-frame chains
  self-terminate), so it should idle between input/rebuilds. No controlled
  board-idle CPU dataset was retained.
- Retained serial build log: **26.37 s** clean; no-op incremental build **2 s**.
  The former 34-second table value had no matching retained trace and has been
  corrected to 26 seconds. Dependency graph: **143 unique crate names / 154
  name-version entries including the app**. Binary **12,342,848 bytes raw /
  10,404,040 bytes (9.9 MiB) stripped**.

## Where the time went

1. **DnD architecture** (~35 %): there is no framework hook for "what widget
   is at this point", so drop-target resolution needed the compose-pass
   geometry registry; getting scroll-correct window rects was the key insight.
2. **Layout-constraint debugging** (~25 %): the Portal/Flex constraint
   semantics above produced screen-filling cards and one-character-per-line
   text wrapping until CardFrame grew explicit `fill()/hug()` sizing modes;
   separately, a flexed label does *not* push its row siblings to the edge
   (masonry flex packs measured sizes — `FlexSpacer::Flex` required).
3. **Focus/Escape plumbing** (~20 %): AutoFocus + bubbled-Escape catch.
4. **Custom View boilerplate** (~20 %): two widgets → ~350 LoC of
   build/rebuild/teardown/message plumbing.

## Surprises

- Good: event bubbling + `PointerState.count` + `EventCtx::set_focus` mean the
  raw materials for double-click/Escape/focus hacks exist one layer down.
- Good: `NewWidget.widget` is public, so a wrapper view can reach into the
  child widget it just built (how AutoFocus learns the `TextArea` id).
- Bad: pointer-capture is last-wins and stock widgets don't set "handled" on
  press, so *any* interactive container breaks stock buttons nested inside it.
- Bad: the ✕ glyph (U+2715) renders as tofu with masonry's default font
  setup; had to fall back to "×" (U+00D7).
