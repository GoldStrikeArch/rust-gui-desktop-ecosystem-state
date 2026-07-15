# FRICTION — Board (gpui =0.2.2)

Reference: SPEC-3.md (rating rubric in SPEC-2.md). Built + verified on macOS
(M4 Pro, rustc 1.96.1): `cargo build --release` clean (crates.io gpui,
`runtime_shaders` feature — see apps/gpui-app/GAPS.md), binary launched,
alive after 10 s at ~0.2% CPU / 75 MiB RSS (no timers — the app is fully
event-driven), killed cleanly, empty log.

Authorship note: built across two agent sessions (the first was interrupted
mid-way). Cross-column DnD and within-column reorder were verified
interactively in the first session; inline edit, add/delete, and the drop
animation were audited by code review + launch check in the second (scripted
UI injection is blocked by macOS automation permissions here, per
apps/gpui-app/GAPS.md). "Where the time went" is partly inferred from the
code's structure and comments.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| Cross-column DnD | **assembled** | gpui's native typed DnD (the machinery Zed's tab/panel dragging uses): `.on_drag(CardDrag{…}, ghost_ctor)` starts the drag, `.on_drag_move::<CardDrag>` fires on every registered listener for each mouse move while that payload type is in flight (capture phase, handing each listener its *own bounds* — no manual hit-testing), `.on_drop::<CardDrag>` on the column body commits. App code computes the insertion index and does the model surgery (~45 LoC total across the wrappers). |
| Within-column reorder | **assembled** | Same code path as cross-column; the only extra is the classic index adjustment (`if from_col == to_col && from_ix < ix { ix -= 1 }`). Effectively free once cross-column works. |
| Drop indicator | **assembled** | Each card's wrapper compares the cursor to its own midline (`on_drag_move` gives it its bounds) and sets `(column, index)`; a 3 px absolutely-positioned accent line renders above/below the target card, and empty columns show a dashed "Drop cards here" zone that lights up. ~20 LoC, all declarative styling driven by one `Option<(usize, usize)>` in the model. |
| Drag ghost/preview | **built-in** | The `on_drag` closure returns a ghost *entity* (`Render` impl); gpui paints it under the cursor every frame, anchored at the grab offset inside the original card. No occlusion problem — the ghost never steals events from drop targets. This is a direct framework API. |
| Inline edit (dbl-click, Enter/Esc) | **hand-rolled** | Double-click detection is free (`ClickEvent::click_count() == 2`) and focus is `FocusHandle::focus`; the *text editing* is not — gpui ships no text-input widget, and the sanctioned path (`EntityInputHandler`, per the bundled input.rs example) is ~750 lines. Same call as iteration 1: a minimal raw `on_key_down` input (`key_char` append, backspace pop, Enter commits, Esc cancels) shared between edit and add. Consequences: no IME composition, no cursor movement/selection/clipboard, caret is a static bar. |
| Add/delete cards | **hand-rolled** | The list mutation itself is trivial ("+ Add card" styled div swaps to the input; ✕ per card with `cx.stop_propagation()` so delete doesn't double-click-edit; empty commit ignored per spec) — but the rating follows the inline text input it depends on, which is the same hand-rolled widget as above. |
| Drop/reorder animation | **assembled** | Cards *snap* to their new layout — gpui has no layout/FLIP animation. Shipped transition per the fallback rule: the landed card plays a 260 ms opacity settle via the built-in `with_animation` tween, restarted by keying the element id with a drop-epoch counter (~8 LoC). Animating actual positions would require capturing per-card bounds across frames and interpolating manual offsets — not attempted; documented gap. |
| Independent column scrolling | **built-in** | `.overflow_y_scroll()` on an `.id()`'d div per column body. Independence is automatic, and DnD targeting keeps working inside the scrolled viewport with zero scroll-offset math because `on_drag_move` bounds are in window coordinates. |

## Helper crates

None — `gpui = "=0.2.2"` only (plus `runtime_shaders`). Nothing else was
needed for DnD. `gpui-component` is published on crates.io and includes input
and editor components; it was not evaluated or adopted here. Zed's own UI
crate remains GPL and unpublished. The hand-rolled editor therefore measures
the selected core-only dependency set, not proof that no ecosystem helper
exists.

## Repaint strategy

Fully event-driven: `cx.notify()` on model changes; idle CPU ~0.2%. While a
drag is active gpui itself refreshes the window every mouse move so the ghost
tracks the cursor (drop-target updates are coalesced — `set_drop_target` only
notifies on change). The oneshot 260 ms settle animation is the only
frame-requesting construct, and only while it runs.

## Where the time went

1. **Drag-targeting model** — the two-layer `on_drag_move` design: the column
   body sets an end-of-column fallback and each card's wrapper overwrites it
   with a midline-refined index. This leans on gpui dispatching capture-phase
   listeners parent-before-child on the same move — the load-bearing ordering
   fact, discovered by experiment, not docs.
2. **Inline input lifecycle** — one shared buffer + `FocusHandle` serving
   both add and edit, and the interaction rules around it (add closes edit
   and vice versa, delete-while-editing clears state, ✕ click must
   `stop_propagation` past the card's click/dbl-click listener).
3. **Drop-index edge cases** — same-column index shift after removal,
   clamping to column length, ignoring a drop when no target was set.

## Surprises

- Good: SPEC-3's headline feature (cross-container DnD with positional drop)
  is the *easiest* part in gpui — typed payloads, framework ghost, per-listener
  bounds on every drag move. Roughly 70 LoC end-to-end for capabilities 5–8.
- Good: `on_drag_move` handing each listener its own `Bounds` removes the
  need for any bounds bookkeeping — the exact thing the dashboard had to
  solve with `Rc<Cell<Bounds>>` probes outside of drags.
- Bad: no text-input widget means "double-click to edit" — trivial in any
  widget toolkit — inherits the full cost of a hand-rolled editor; the
  minimal one shipped has no IME/selection/clipboard.
- Bad: no layout animation primitives, so drop/reorder motion cannot be
  expressed; the opacity settle is an approximation (same finding as iced).

## Totals

- LoC: 515 (single `src/main.rs`, heavily commented)
- Binary: 5.1 MiB release (unstripped; 5,304,464 bytes)
- Dependencies: `gpui = "=0.2.2"` with `runtime_shaders` — no helper crates.
