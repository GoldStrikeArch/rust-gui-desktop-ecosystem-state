# Verification report — framework-level interaction traps (round-2 apps + todo-round GAPS)

Agent domain: I-1 … I-9 from the brief. Verified 2026-08-30.
Pinned versions read from each app's `Cargo.lock`:
freya 0.4.0 / freya-core 0.4.1 / freya-components 0.4.1 / torin 0.4.1 / ragnarok 0.4.1;
vizia 0.4.0 / vizia_core 0.4.0; egui 0.35.0; masonry 0.4.0 / masonry_core 0.4.0 / xilem 0.4.0;
iced 0.14.0 / iced_widget 0.14.2 / iced_futures 0.14.0; dioxus 0.7.9 / dioxus-html 0.7.9;
wry 0.53.5 / tauri-utils 2.9.3; slint 1.17.1 / i-slint-compiler 1.17.1; gpui 0.2.2.

Upstream clones (shallow, in this dir):
- `freya/` @ `b57fe8a` (2026-08-27)
- `xilem/` @ `b81d8d7` (2026-08-28)
- `vizia/` @ `426d2e7` (2026-08-21)

Registry sources: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/`.

Repo evidence base URL used in drafts:
`https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/<path>`

---

## I-1(a) freya — `DropZone` has no width/height setters, always shrink-wraps

**CLAIM.** `apps/freya-board/FRICTION.md` "sharp edge 2": *"`DropZone` renders
`rect().width(Size::auto()).height(Size::auto())` around its child and exposes no
width/height setters, so it always shrink-wraps."* Mirrored in
`report/data/interactive-rows.md:376`.

**EVIDENCE CHECK.** Supported. The app's workaround (an explicit 140 px tail zone per
column) is visible in `apps/freya-board/src/main.rs`.

**ROOT CAUSE — CONFIRMED.**
`freya-components-0.4.1/src/drag_drop.rs:171-204`:
```rust
pub struct DropZone<T: 'static + PartialEq + Clone> {
    children: Element,
    on_drop: EventHandler<T>,
    on_drag_over: Option<EventHandler<bool>>,
    width: Size,      // set to Size::auto() in new(), never settable
    height: Size,
    key: DiffKey,
}
```
The `width`/`height` fields exist and are wired into `render()` (line 228-231) but the
only builder method on the type is `on_drag_over`. Freya's own `docking.rs` works around
it by passing `rect().expanded()` as the *child* (`docking.rs:534, 597`).

**UPSTREAM STATUS — FIXED ON MAIN, UNRELEASED.**
`crates/freya-components/src/drag_drop.rs` on main now has
`impl LayoutExt for DropZone<T>` + `impl ContainerExt for DropZone<T>` + `ChildrenExt`,
which brings `.width()/.height()/.expanded()`. Introduced by
`2b21c88` (2026-08-15) *"feat(components): Implement ChildrenExt in DragZone and DropZone (#2151)"*,
tracked by issue **marc2332/freya#2147** ("DropZone doesn't use ChildrenExt", closed).
Not in any released 0.4.x.

**FILE? NO.** Already reported and fixed upstream.

**Corpus note.** The FRICTION/dashboard text should gain "fixed on freya main (#2151),
unreleased as of freya-components 0.4.1".

---

## I-1(b) freya — `on_mouse_up` on an ancestor rect never fires; `on_pointer_down` does

**CLAIM.** `apps/freya-board/FRICTION.md` sharp edge 2 (cont.) and
`report/data/interactive-rows.md:376`: *"an `on_mouse_up` listener on an ancestor never
fired for clicks in the empty area while `on_pointer_down` on the same rect fired every
time … Bubbling for released events looks unreliable outside the `DropZone` path."*

**EVIDENCE CHECK.** The observation is narrative ("logged from a debug build"); no log
artifact was retained in the repo. The FRICTION file already hedges it ("looks
unreliable … filed as a limitation rather than a hard blocker").

**ROOT CAUSE — UNVERIFIED (the stated asymmetry is not reproduced by the source).**
I traced the whole dispatch path and found *no* mechanism that treats `MouseUp`
differently from `MouseDown`/`PointerDown`:

- `freya-core-0.4.1/src/events/name.rs:248` — `does_bubble()` is true for `MouseUp`,
  `MouseDown` and `PointerDown` alike.
- `ragnarok-0.4.1/src/measurement.rs:142-183` — for each derived event name, potential
  nodes are walked deepest-layer-first and the event is emitted to the **first listener
  found**, then `continue 'event` (`is_emitted_once()`). Identical for both.
- `freya-core-0.4.1/src/runner.rs:519-545` — real ancestor bubbling then happens inside
  `Runner::handle_event`, from that entry node up the element path, unless
  `stop_propagation()`.
- `ragnarok-0.4.1/src/nodes_state.rs:164-165` — the "only emit release events for
  already-pressed nodes" filter applies to `is_released()`, which in freya is
  **`PointerPress` only** (`name.rs` `is_released`), *not* `MouseUp`.
- Layer numbers equal tree depth (`freya-core-0.4.1/src/data.rs:255-265`:
  `layer = parent_layer + relative + 1`), so deepest-first ordering is deterministic
  across depths. Within one depth the container is `FxHashSet` (`layers.rs:42`), so
  sibling order is arbitrary — but siblings do not overlap here.

The one code-located mechanism that *does* kill `on_mouse_up` bubbling is I-1(b'), below,
and it applies to clicks **on cards** (inside a `DropZone`), not to a column's empty
space. My best guess is that the original observation was that case, mis-attributed to
the empty area.

**UPSTREAM STATUS.** n/a.

**FILE? NO** — not as stated.

**Corpus correction (recommended).** Replace "Bubbling for released events looks
unreliable" with the verified statement: *"every element inside a `DropZone` swallows
`MouseUp` — `DropZone` calls `stop_propagation()` on every mouse-up, drag or not — so a
column-level `on_mouse_up` never sees clicks that land on a card. The empty-area case was
not reproduced from source and should be treated as unconfirmed."*

---

## I-1(b′) freya — `DropZone` calls `stop_propagation()` on *every* mouse-up (new finding)

**ROOT CAUSE — CONFIRMED.** `freya-components-0.4.1/src/drag_drop.rs:212-226`:
```rust
let on_mouse_up = {
    move |e: Event<MouseEventData>| {
        e.stop_propagation();                      // <- unconditional
        if let Some(current_drags) = &*drags.read() { on_drop.call(...); }
        ...
    }
};
```
`stop_propagation()` runs before the "is a drag actually in flight?" check, so an ordinary
click anywhere inside a `DropZone`'s subtree stops `MouseUp` from reaching any ancestor
listener.

**UPSTREAM STATUS — STILL PRESENT ON MAIN**
(`freya/crates/freya-components/src/drag_drop.rs`, `on_mouse_up`, @ b57fe8a). No matching
issue in the tracker (searched `DropZone`, `DragZone`).

**FILE? YES — marc2332/freya.**

> **Title:** `DropZone` swallows every `mouse up`, even when no drag is in flight
>
> **Body:**
> freya-components 0.4.1 (and `main` @ b57fe8a), macOS 26.5, freya 0.4.0.
>
> `DropZone`'s mouse-up handler calls `e.stop_propagation()` before it checks whether a
> drag of `T` is actually in progress:
>
> ```rust
> // crates/freya-components/src/drag_drop.rs
> let on_mouse_up = move |e: Event<MouseEventData>| {
>     e.stop_propagation();
>     let payload = (*drags.read()).clone();
>     if let Some(payload) = payload { on_drop.call(payload); ... }
> };
> ```
>
> Effect: an ordinary click on anything inside a `DropZone` never reaches an ancestor's
> `on_mouse_up`. In a kanban where every card is wrapped in a `DropZone` (the idiomatic
> "insert before me" pattern), a column-level `on_mouse_up` handler is dead for all clicks
> on cards, with no warning.
>
> **Expected:** propagation is stopped only when the `DropZone` actually consumes a drop.
>
> **Repro:** a `rect().on_mouse_up(|_| println!("column"))` containing a
> `DropZone::new(card, |_: u64| {})`; click the card — nothing prints. Move the handler
> to `on_pointer_press` and it fires.
>
> **Suggested fix:** move `e.stop_propagation()` inside the `if let Some(payload)` arm.

---

## I-1(c) freya/torin — `Size::flex` silently no-ops unless the parent sets `Content::flex()`

**CLAIM.** `apps/freya-board/FRICTION.md` sharp edge 3: *"A child sized `Size::flex(1.)`
collapses unless the parent opts into `.content(Content::flex())`. There is no warning;
the first build of this app rendered a single full-width column."*

**ROOT CAUSE — CONFIRMED (with a wording correction).**
- `torin-0.4.1/src/measure.rs:644` and `:673` — the flex distribution pass is gated
  entirely on `parent_node.content.is_flex()`. Flex grow factors are only *collected*
  (line 644-656) and only *applied* (line 673-692) inside that gate.
- `torin-0.4.1/src/values/size.rs:273` — outside that gate `Size::Flex` still evaluates:
  `Self::Flex(_) | Self::FillMinimum if phase == Phase::Final => Some(available_parent)`.
  So a flex child behaves **exactly like `Size::fill()`**: the first child takes the whole
  available main-axis extent and the rest get nothing. That is precisely the "single
  full-width column" symptom, and it is a more accurate description than "collapses".
- The rustdoc on `Size::Flex` (`size.rs:172-180`) says only *"Flex grow factor, fills the
  available space proportionally in the final layout phase"* — it never mentions the
  `Content::Flex` requirement. `Content::Flex`'s own doc does state the pairing.

**UPSTREAM STATUS — STILL PRESENT ON MAIN** (identical rustdoc in
`freya/crates/torin/src/values/size.rs`). No matching issue (searched `flex`; the closest
precedent is the accepted docs issue #1202 "docs: Improve documentation of
`width:fill`/`height:fill`", closed).

**FILE? YES (docs/diagnostics) — marc2332/freya.**

> **Title:** `Size::flex` silently behaves like `fill` when the parent isn't `Content::flex()`
>
> **Body:**
> torin 0.4.1 / freya 0.4.0.
>
> Flex distribution is gated on the *parent*'s `content` (`torin/src/measure.rs`, the
> `parent_node.content.is_flex()` branches). When the parent is `Content::Normal`,
> `Size::Flex` still resolves in the final phase to `Some(available_parent)`
> (`torin/src/values/size.rs`, `Size::eval`), i.e. it behaves like `Size::fill()`. Three
> siblings each with `Size::flex(1.)` therefore render as one full-width child plus two
> zero-width ones, with no warning.
>
> `Size::Flex`'s doc comment ("Flex grow factor, fills the available space proportionally
> in the final layout phase") does not mention that the parent must opt in with
> `.content(Content::flex())`; only `Content::Flex`'s own doc mentions the pairing, which
> is the wrong direction for discovery.
>
> **Ask:** either (a) document the requirement on `Size::Flex` / `ContainerSizeExt::width`,
> or (b) emit a debug-build warning when a node has a `Size::Flex` child and
> non-`Content::Flex` content — the layout pass already knows both facts at that point.

---

## I-1(d) freya — `Input` is single-line, and `on_pre_key_down` replaces the stock key filter

**CLAIM.** `apps/freya-board/FRICTION.md`: *"`Input` is single-line only … Escape needs
`Input::on_pre_key_down`, which replaces the widget's stock key filter, so the app has to
re-implement the default arms."*

**ROOT CAUSE — CONFIRMED (both halves), one half fixed upstream.**
- Single-line: `freya-components-0.4.1/src/input.rs:690` hard-codes `.max_lines(1)` on the
  paragraph; there is no `multiline` property on the 0.4.1 `Input`.
- Key filter: the default is a `Callback` constructed in `Input::new`
  (`input.rs:217-227`) with arms `Enter|Escape|Shift => true`, `Tab => false`,
  `_ => stop_propagation + prevent_default + true`. `on_pre_key_down`
  (`input.rs:330-335`) **overwrites** that field, and `on_key_down`
  (`input.rs:425-431`) calls whatever is in it. There is no `on_escape`/`on_cancel`
  callback: Escape is consumed internally by `request_unfocus()` (`input.rs:441-445`).

**UPSTREAM STATUS.**
- Multiline: **FIXED ON MAIN** — `Input::multiline(bool)` added by `3ae0607` (2026-08-23)
  *"feat(components): Multi line input (#2192)"*.
- Key filter: **STILL PRESENT ON MAIN** — `on_pre_key_down` is now `Option<Callback<..>>`
  and the stock filter is applied via `unwrap_or_else` at
  `crates/freya-components/src/input.rs:508`, so setting your own still replaces it
  wholesale. No `on_escape`/`on_cancel` on `Input` on main.

**FILE? YES (small feature request) — marc2332/freya.**

> **Title:** `Input`: no way to react to Escape without re-implementing the stock key filter
>
> **Body:**
> freya-components 0.4.1 and `main` @ b57fe8a.
>
> `Input` consumes Escape internally (`request_unfocus()`), and the only hook is
> `on_pre_key_down`, which *replaces* the widget's default filter rather than composing
> with it. An app that wants "Escape = cancel this inline edit" has to re-implement the
> stock arms (`Enter`/`Shift` pass, `Tab` skip, everything else
> `stop_propagation()` + `prevent_default()`) around its own Escape case, and to re-check
> them whenever the default changes upstream (it did in #2123).
>
> **Ask:** an `on_escape` / `on_cancel` callback alongside `on_submit` (vizia's `Textbox`
> has `on_submit` + `on_cancel`; masonry recently added `TextAction::Cancelled`, linebender/xilem#1826),
> or make `on_pre_key_down` additive by exposing the default filter as a public fn.

**Corpus note.** "`Input` is single-line only" should be annotated "fixed on freya main
(#2192), unreleased".

---

## I-1(e) freya — no timer/interval primitive

**CLAIM.** `apps/freya-dash/FRICTION.md`: *"Freya ships **no timer/interval primitive** —
`spawn` takes a bare future and the prelude exports nothing time-related."*

**ROOT CAUSE — CONFIRMED.** `freya-core-0.4.1/src/hooks/` contains only `use_id.rs` and
`previous_and_current.rs`; grepping `freya-core`, `freya-components`, `freya` 0.4.x for
`use_interval|use_timer|set_interval` returns nothing. Freya's own components use
`async_io::Timer::after` directly (`tooltip.rs:209`, `cache.rs:187`, `gif_viewer.rs:362`,
`freya-animation-0.4.1/src/hook.rs:308`), i.e. the dependency is already in the tree — it
is a discoverability/API gap, not a capability gap, exactly as the FRICTION file says.

**UPSTREAM STATUS — STILL ABSENT ON MAIN** (same grep over the clone). No issue found
(search `timer OR interval` returns 0 results in marc2332/freya).

**FILE? YES (low-priority feature request) — marc2332/freya.**

> **Title:** No first-party interval/timer hook
>
> **Body:** freya 0.4.0 / `main`. `spawn` takes a bare future and nothing time-related is
> exported from the prelude, so a "tick at N Hz" loop means adding `async-io` (or another
> timer crate) yourself and calling `Timer::after` — which is what `freya-animation`,
> `Tooltip`, `Cache` and `GifViewer` already do internally, so the dependency is in every
> freya app's tree already. A `use_interval(Duration, impl FnMut())` hook (or just
> re-exporting `Timer`) would make the common "poll/animate on a clock" case discoverable.
> Comparison points: vizia has `cx.add_timer`/`modify_timer`; slint has `slint::Timer`.

---

## I-1(f) freya — canvases never repaint unless they carry an event handler

**CLAIM.** `apps/freya-dash/FRICTION.md` ("The sharpest edge") and
`report/data/interactive-rows.md:357`: *"`RenderCallback`'s `PartialEq` returns `true`
unconditionally, and `CanvasElement::changed()` is `self != other`. A `canvas()` whose only
changing input is the data captured by its closure therefore diffs as unchanged … Observed
directly — the six sparklines were frozen at their seed values while the main chart
animated."*

**EVIDENCE CHECK.** Supported: the app ships the documented workaround
(`fn live_canvas(on_render) -> Canvas { canvas(on_render).on_wheel(|_| {}) }`).

**ROOT CAUSE — CONFIRMED, exactly as described.**
- `freya-components-0.4.1/src/canvas.rs:55-59`
  ```rust
  impl PartialEq for RenderCallback {
      fn eq(&self, _other: &Self) -> bool { true }
  }
  ```
- `canvas.rs:66-84` — `CanvasElement` derives `PartialEq` over
  `{layout, event_handlers, effect, on_render}`, and `ElementExt::changed` is `self != rect`.
- `freya-core-0.4.1/src/element.rs:441` — `Element::eq` is
  `key1 == key2 && !element1.changed(element2) && elements1 == elements2`, so an unchanged
  canvas element is kept in the tree together with its **old** `Rc<CanvasElement>`, i.e.
  the first closure keeps being invoked forever.
- The asymmetry the app observed is also confirmed:
  `freya-core-0.4.1/src/event_handler.rs:85-90` — `impl<T> PartialEq for EventHandler<T>`
  returns `false` (with a `// TODO: Decide whether event handlers should be captured or
  not.`), and `EventHandlerType` derives `PartialEq` over it, so *any* canvas carrying an
  event handler compares unequal every render and is replaced. Hence "add any handler and
  it works".

**UPSTREAM STATUS — STILL PRESENT ON MAIN** (`crates/freya-components/src/canvas.rs:55-59`
@ b57fe8a; last touches to that file are #2058 and #1959, unrelated). No matching issue
(GitHub search `canvas repaint` in marc2332/freya returns 0 results).

**FILE? YES — this is the strongest defect in my batch.**

> **Title:** `canvas()` never re-renders when only its closure's captured data changes
>
> **Repo:** marc2332/freya
>
> **Body:**
> freya 0.4.0 / freya-components 0.4.1, and `main` @ b57fe8a. macOS 26.5.2, M4 Pro.
>
> `RenderCallback`'s `PartialEq` is unconditionally `true`
> (`crates/freya-components/src/canvas.rs`):
>
> ```rust
> impl PartialEq for RenderCallback {
>     fn eq(&self, _other: &Self) -> bool { true }
> }
> ```
>
> `CanvasElement` derives `PartialEq` over `{layout, event_handlers, effect, on_render}`
> and `ElementExt::changed` is `self != other`, so a `canvas()` whose *only* changing input
> is data captured by its render closure diffs as **unchanged**. The old element — and with
> it the old closure — stays in the tree, and the render pipeline keeps invoking the first
> closure for the lifetime of the app.
>
> **Repro:**
> ```rust
> fn app() -> impl IntoElement {
>     let mut n = use_state(|| 0);
>     spawn(async move { loop { async_io::Timer::after(Duration::from_millis(200)).await; n += 1; } });
>     rect().child(
>         canvas(move |ctx: &mut CanvasContext| {
>             // draws `n` -> stays at 0 forever
>         })
>     )
> }
> ```
> Adding any event handler (`.on_wheel(|_| {})`) makes it update, because
> `EventHandler`'s `PartialEq` returns `false` unconditionally
> (`crates/freya-core/src/event_handler.rs`), which forces the element to be replaced.
>
> **Expected:** a canvas re-runs its render callback when the component re-renders, like
> every other element re-reads its props.
>
> **Observed in the wild:** six sparklines frozen at their seed values while the main chart
> (which carried `on_pointer_move`/`on_sized`) animated —
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/freya-dash/FRICTION.md
>
> **Possible fixes:** make `RenderCallback::eq` return `false` (matching `Callback`/
> `EventHandler`), or give `Canvas` an explicit invalidation key
> (`canvas(cb).depends_on(&data)`), or a `Cache::clear()`-style handle like
> `iced::canvas::Cache`.

---

## I-1(g) freya — release-only panic hook shows a modal with empty stderr

**CLAIM.** `apps/freya-board/FRICTION.md` sharp edge 1.

**Cross-reference only** (another agent owns this). Confirmed location for their benefit:
`freya-winit-0.4.1/src/lib.rs:61-73`, inside `freya::prelude::launch`:
```rust
#[cfg(all(not(debug_assertions), not(target_os = "android")))]
{
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        rfd::MessageDialog::new().set_title("Fatal Error")
            .set_description(&panic_info.to_string())
            .set_level(rfd::MessageLevel::Error).show();
        previous_hook(panic_info);
        std::process::exit(1);
    }));
}
```
Note the ordering: the **blocking** dialog is shown *before* `previous_hook` prints the
message, so a release build blocks on a modal with nothing yet on stderr. Same code on
main (`crates/freya/src/lib.rs` `launch` → `freya_winit::launch`).

**Related, in my scope (from `apps/freya-app/GAPS.md`):** `freya::prelude::launch`
also injects `freya_performance_plugin::PerformanceOverlayPlugin` under
`#[cfg(debug_assertions)]` with no opt-out (`freya-0.4.0/src/lib.rs:132-135`; same on main
at `crates/freya/src/lib.rs:137-140`). Escape hatch is calling `freya_winit::launch`
directly. Worth folding into the same issue as a one-line "please make both of these
opt-in / configurable via `LaunchConfig`" — **FILE? MAYBE (low value on its own).**

---

## I-2 vizia — actions only fire when the acted-on view *is* the hovered entity

**CLAIM.** `apps/vizia-board/FRICTION.md` trap 1 and `apps/vizia-dash/FRICTION.md` trap 1:
*"vizia only runs an action when the acted-on view is the hovered entity
(`cx.current == meta.target`). A click that lands on a card's `Label` never reaches the
card. Marking non-interactive children `.hoverable(false)` is the fix."*

**EVIDENCE CHECK.** Supported, and independently corroborated by
`apps/vizia-app/GAPS.md` (a sibling silent-failure: `cx.emit` from a `Model` propagates
*up* and never reaches a child view).

**ROOT CAUSE — CONFIRMED.**
- Every view is hoverable by default: `vizia_core-0.4.0/src/style/mod.rs:122-127`
  (`impl Default for Abilities { Abilities::HOVERABLE }`); hit-testing uses that flag
  (`systems/hover.rs:93`). So a `Label` child *is* the hovered entity.
- `vizia_core-0.4.0/src/events/event_manager.rs:414` — a press is dispatched with
  `mutate_direct_or_up(meta, cx.captured, cx.hovered, true)`; with no capture,
  `meta.target = cx.hovered` and `propagation = Up` (`event_manager.rs:742-750`).
- `vizia_core-0.4.0/src/modifiers/actions.rs:172-199` — the `Press` arm first allows
  descendants:
  ```rust
  let over = if *mouse { cx.hovered() } else { cx.focused() };
  if cx.current() != over && !over.is_descendant_of(cx.tree, cx.current()) { return; }
  if !cx.is_disabled() && cx.current == meta.target { ... (action)(cx) ... }
  ```
  …and then the second condition `cx.current == meta.target` throws that allowance away:
  during upward propagation `cx.current` is the ancestor while `meta.target` stays the
  hovered leaf, so the ancestor's `on_press` never runs. The `is_descendant_of` guard is
  effectively dead code.
- Same gate on `on_press_down` (`:207`), `on_double_click` (`:215`), `on_hover` (`:223`),
  `on_hover_out` (`:231`), `on_geo_changed` (`:304`). `on_drag_start` fails for the same
  reason via a different test — `cx.mouse.left.pressed == cx.current()`
  (`actions.rs:250-257`), and `mouse.left.pressed` is the hovered leaf.

**UPSTREAM STATUS — STILL PRESENT ON MAIN** (`vizia` @ 426d2e7, 2026-08-21:
`crates/vizia_core/src/modifiers/actions.rs`, byte-identical `Press` arm).
Prior art: **vizia/vizia#406** *"on_press event bug: event fails to trigger when pressing
children"* — closed as completed 2023-08-06. The `is_descendant_of` line is almost
certainly that fix; the `cx.current == meta.target` gate re-broke it.

**FILE? YES — vizia/vizia.**

> **Title:** `on_press` / `on_double_click` / `on_drag` never fire when the click lands on a child view (regression of #406)
>
> **Body:**
> vizia 0.4.0 (crates.io) and `main` @ 426d2e7. macOS 26.5.2.
>
> All views are `HOVERABLE` by default (`vizia_core/src/style/mod.rs`,
> `impl Default for Abilities`), so a press that lands on a container's `Label` makes the
> *label* the hovered entity, and press events are targeted at the hovered entity
> (`events/event_manager.rs`, `mutate_direct_or_up(meta, cx.captured, cx.hovered, true)`).
>
> In `modifiers/actions.rs` the `WindowEvent::Press` arm reads:
>
> ```rust
> let over = if *mouse { cx.hovered() } else { cx.focused() };
> if cx.current() != over && !over.is_descendant_of(cx.tree, cx.current()) {
>     return;
> }
> if !cx.is_disabled() && cx.current == meta.target {
>     ... (action)(cx) ...
> }
> ```
>
> The first check deliberately allows a press on a descendant, but the second one
> (`cx.current == meta.target`) is false for exactly that case during upward propagation,
> so the descendant allowance never has any effect. Same gate on `on_press_down`,
> `on_double_click`, `on_hover`, `on_hover_out`; `on_drag_start` fails equivalently via
> `cx.mouse.left.pressed == cx.current()`.
>
> **Repro:**
> ```rust
> VStack::new(cx, |cx| { Label::new(cx, "click me"); })
>     .on_press(|_| println!("pressed"));   // never prints when you click the text
> ```
> Adding `.hoverable(false)` to the `Label` fixes it — which is what vizia's own `list`
> example does — but nothing warns you, and every action modifier on a container with
> ordinary children is silently dead until you know this.
>
> **Expected:** either the action fires for presses on descendants (as `is_descendant_of`
> suggests was intended, and as #406 concluded), or non-interactive children are not
> hoverable by default, or `.on_press()` on a view with hoverable children emits a debug
> warning.
>
> **Cost data:** in a kanban built on vizia 0.4 this trap plus two siblings accounted for
> ~60 % of implementation time —
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/vizia-board/FRICTION.md
>
> Possibly a regression of #406.

---

## I-3 iced — `text_input` captures Escape so `event::listen()` never sees it

**CLAIM.** `report/data/interactive-rows.md:173,183`: *"SHARP EDGE: `text_input` CAPTURES
Escape so `keyboard::listen()` never fires — needs raw `event::listen_with`"*, and
*"required source-diving `~/.cargo` to diagnose"*.

**ROOT CAUSE — CONFIRMED, but the behaviour is documented and intended.**
- `iced_widget-0.14.2/src/text_input.rs:1235-1243` — a focused `text_input` handles
  `Escape` by clearing focus/drag/paste state and calls `shell.capture_event()`.
- `iced_futures-0.14.0/src/event.rs:7-16` — `listen()` is *"a `Subscription` to all the
  **ignored** runtime events … any `Event` that was not captured by any widget"*, i.e.
  `Status::Captured => None`. `listen_with` receives the `Status` and can act on captured
  events. Both behaviours are in the rustdoc.

**Assessment: NOT-A-BUG.** The subscription contract is documented; the app's fix
(`event::listen_with`) is the intended escape hatch. There is no `keyboard::listen()` in
iced 0.14 — the corpus is naming `iced::event::listen()`.

**Real residual gap:** `text_input` has no `on_escape`/cancel message, so "Esc cancels this
inline edit" cannot be expressed on the widget itself. Adjacent open issue:
**iced-rs/iced#2678** *"Input a11y issue on escape key press"* (open, bug, 2024-11-28).

**FILE? NO.** (Optionally comment on #2678 with the inline-edit use case.)

**Corpus correction.** Reword from "sharp edge / bug" to: *"`text_input` captures Escape
(documented: `event::listen()` only yields uncaptured events), so Esc-to-cancel needs
`event::listen_with`. What's missing is an `on_escape` hook on `text_input` itself."*
Also fix the API name `keyboard::listen()` → `iced::event::listen()`.

---

## I-4 egui — buttons inside a stock drag source are silently inert

**CLAIM.** `apps/egui-board/FRICTION.md` "The headline trap (cost: ~40 % of total time)".

**ROOT CAUSE — CONFIRMED, both halves, exactly as the FRICTION file states.**
1. `egui-0.35.0/src/ui.rs:2641-2683` — `Ui::dnd_drag_source` runs `add_contents` first and
   *then* `self.interact(response.rect, id, Sense::drag())`, so the drag-only interact
   registers on top of the children.
2. `egui-0.35.0/src/hit_test.rs:388-403` — when the topmost hit senses only drags, the
   click hit is discarded:
   ```rust
   // The top things senses only drags,
   // so we ignore the click-widget, because it would be confusing
   // if clicking a drag-widget would actually click something else below it.
   WidgetHits { click: None, drag: Some(hit_drag), ..Default::default() }
   ```
3. `egui-0.35.0/src/interaction.rs:195-203` — drag-only widgets have no movement
   threshold:
   ```rust
   } else {
       // This widget is just sensitive to drags, so we can mark it as dragged right away:
       widget.sense.senses_drag()
   };
   ```
   so a plain press enters ghost mode (the card is drawn on `Order::Tooltip`, whose widgets
   return empty `Response`s — noted in the `dnd_drag_source` comment itself).

**UPSTREAM STATUS — ALREADY REPORTED, still present.**
**emilk/egui#5822** *"dnd_drag_source zones take priority over interior input widgets"* —
**open**, filed 2025-03-19, labelled `bug`, no maintainer reply. The `interaction.rs`
block is unchanged on `main`. Latest release is egui 0.36.1 (2026-08-07); nothing in the
0.36 line addresses it. Related open issues: #4604, #5529, #2730.

**FILE? NO — comment on #5822 instead.** Suggested comment content: the two-line root-cause
pointer above, plus the working fix that our board uses — put the drag sense on the card's
*container* with `UiBuilder::sense(Sense::drag())` (registers *under* the children, the
ScrollArea-background pattern) and gate ghost mode + payload on
`pointer.is_decidedly_dragging()`; also gate drop handling on the same, or a plain click
counts as a zero-distance drop. Evidence:
`https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/egui-board/FRICTION.md`
plus the `tests/hit_experiment.rs` kittest cases.

---

## I-5(a) masonry — last-wins pointer capture breaks stock `Button`s inside draggable containers

**CLAIM.** `apps/xilem-board/FRICTION.md`: *"masonry pointer capture is last-wins and
`Button` does not mark its Down as handled, so a stock button inside the draggable card
frame loses its capture and never fires."*

**ROOT CAUSE — CONFIRMED.**
- `masonry_core-0.4.0/src/core/contexts.rs:388-397` — `capture_pointer()` overwrites
  unconditionally:
  ```rust
  pub fn capture_pointer(&mut self) {
      let id = self.widget_id();
      if !self.allow_pointer_capture { debug_panic!(...); return; }
      self.global_state.pointer_capture_target = Some(id);
      self.global_state.needs_pointer_pass = true;
  }
  ```
  No check for an existing capture, no "first wins", no per-pointer stack.
- `masonry_core-0.4.0/src/passes/event.rs:100-140` — `run_event_pass` walks from the hit
  target up to the root, invoking every ancestor until `is_handled`.
- `masonry-0.4.0/src/widgets/button.rs:116` — `Button` calls `ctx.capture_pointer()` on
  `Down` and **never** `ctx.set_handled()`. Same for `checkbox.rs`, `slider.rs`,
  `scroll_bar.rs`. The only widgets that set handled at all are `split.rs:402` and
  `text_area.rs:738,787`.
- Net effect: an ancestor that also captures on `Down` runs *after* the button and takes
  the capture, so the button never sees its `Up`.

**UPSTREAM STATUS — STILL PRESENT ON MAIN** (`xilem` @ b81d8d7:
`masonry_core/src/core/contexts.rs` `capture_pointer` unchanged;
`masonry/src/widgets/button.rs:112` still `ctx.capture_pointer()` with no `set_handled`).
Nearest existing issue: **linebender/xilem#1581** *"Split bar area doesn't get exclusive
pointer events"* (open, `masonry`) — a symptom of the same area but not this statement.
Also relevant context: #1345 (tracking: multiple pointers), #1620 (closed, "Refactor how
Button deals with pointer events").

**FILE? YES — linebender/xilem.**

> **Title:** masonry: `capture_pointer` is last-wins, and stock widgets don't `set_handled()` on Down — a `Button` inside any capturing container never fires
>
> **Body:**
> masonry / masonry_core 0.4.0 (crates.io) and `main` @ b81d8d7. macOS 26.5.
>
> `EventCtx::capture_pointer` overwrites `global_state.pointer_capture_target`
> unconditionally (`masonry_core/src/core/contexts.rs`), and `run_event_pass`
> (`masonry_core/src/passes/event.rs`) delivers `Down` to the hit widget and then to every
> ancestor until one calls `set_handled()`. `Button::on_pointer_event`
> (`masonry/src/widgets/button.rs`) calls `ctx.capture_pointer()` but never
> `ctx.set_handled()` — likewise `Checkbox`, `Slider`, `ScrollBar`.
>
> Consequence: put a stock `Button` inside *any* custom widget that captures the pointer on
> `Down` (a draggable card, a custom tap surface), and the ancestor's capture wins because
> it runs last. The button gets its `Down`, never gets the matching `Up`, and silently
> never fires. Nothing warns.
>
> **Repro sketch:** a custom `Widget` whose `on_pointer_event` calls `ctx.capture_pointer()`
> on `PointerEvent::Down`, wrapping `button("x", ..)`. Clicking the button does nothing.
> Making the inner widget a second custom frame that *does* call `ctx.set_handled()` on its
> own capture works.
>
> **Expected / possible fixes:** (a) `capture_pointer` refuses (or warns) when a descendant
> already captured this pointer during the same event; or (b) widgets that capture also mark
> the `Down` handled; or (c) the docs in `masonry_concepts.md#pointer-capture` state the
> last-wins rule and the `set_handled()` requirement explicitly.
>
> **Where this bit us:** a kanban board's delete "✕" had to be a hand-rolled mini widget
> instead of a stock `Button` —
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/xilem-board/FRICTION.md

---

## I-5(b) xilem/masonry — Esc-cancel has no API; autofocus has no API

**CLAIM.** `apps/xilem-board/FRICTION.md`: *"Esc-cancel: no API — masonry's `TextArea`
ignores Escape … Autofocus: no API — xilem 0.4 has no focus view/API and masonry only
allows `set_focus` from an `EventCtx`."*

**ROOT CAUSE — CONFIRMED for 0.4.0.**
- Escape: grepping `NamedKey::Escape` across `masonry-0.4.0/src` and
  `masonry_core-0.4.0/src` returns **zero hits** — `TextArea`/`TextInput` genuinely ignore
  Escape in 0.4.0.
- Focus: grepping `autofocus|auto_focus|request_focus|fn focus` across `xilem-0.4.0/src`
  returns **zero hits**; focus is only reachable via `EventCtx::request_focus`
  (`masonry_core-0.4.0/src/core/contexts.rs`), i.e. from inside a widget.

**UPSTREAM STATUS — split.**
- Escape: **FIXED ON MAIN.** `masonry/src/widgets/text_area.rs:726` now emits
  `TextAction::Cancelled` on Escape, and `xilem_masonry/src/view/text_input.rs:369-374`
  exposes an `on_cancel` callback. Commit `cbd97ce` (2026-07-28),
  *"feat: emit `TextAction::Cancelled` on Escape key press and expose `on_escape` callback
  for TextInput (#1826)"*. Not in xilem 0.4.0.
- Autofocus: **STILL ABSENT ON MAIN.** No `autofocus`/`auto_focus`/`request_focus` anywhere
  under `xilem/src` or `xilem_masonry/src` on main. No matching issue (searched `focus`;
  the tracker has #1151, #1175, #1274, #1478, #1562, #1679 — none is "let a view request
  focus").

**FILE? YES for autofocus only — linebender/xilem.**

> **Title:** No way to focus a widget from the view layer (autofocus)
>
> **Body:**
> xilem 0.4.0 and `main` @ b81d8d7.
>
> Focus can only be requested from inside a widget (`EventCtx::request_focus`); there is no
> view-level equivalent — no `text_input(..).autofocus(true)`, no focus-request message, no
> `ViewCtx` hook. The common pattern "a button reveals an inline editor, and the caret
> should already be in it" therefore needs a hand-rolled wrapper `Widget` that reaches into
> the `TextInput`'s inner `TextArea` (`TextInput::area_pod()`, `NewWidget.widget` being
> public) at view-build time and calls `set_focus` on the first pointer event that bubbles
> through afterwards. Related: `text_input` has no select-all / caret-placement API either,
> so the caret always lands at index 0.
>
> **Ask:** an `autofocus`/`request_focus` affordance on the view side (and ideally
> caret/select-all placement on `text_input`).
>
> **Where this bit us:** ~20 % of implementation time on a kanban went to AutoFocus +
> bubbled-Escape plumbing —
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/xilem-board/FRICTION.md
> (the Escape half is now fixed on main by #1826 — thank you.)

**Corpus correction.** "Esc-cancel: NO API" needs "fixed on masonry/xilem main by #1826
(2026-07-28), after xilem 0.4.0".

---

## I-6 dioxus — "Rust-side mouse events expose no element geometry"

**CLAIM.** `report/data/interactive-rows.md:82` and SURPRISES: *"MouseEvent has offset
coords but NOT target size — above/below-midpoint uncomputable from the event; no sync
`getBoundingClientRect`."*

**ROOT CAUSE — CONFIRMED but by design, and the headline phrasing is overstated.**
- `dioxus-html-0.7.9/src/point_interaction.rs:9-33` — `MouseData` **does** expose
  `client_coordinates()`, `screen_coordinates()`, `page_coordinates()` and
  `element_coordinates()` (offsetX/offsetY). So "no element geometry" is wrong; what is
  missing is the target's **size/rect**.
- Measurement path: `dioxus-html-0.7.9/src/events/mounted.rs:161` —
  `MountedData::get_client_rect()` is `async`, reached via `onmounted` (documented at
  `events/generated.rs:156-193`), because on desktop the DOM lives in an out-of-process
  webview and every measurement is an IPC round-trip. There is no synchronous alternative
  and there structurally cannot be one.
- Note the DOM has the same shape: a browser `MouseEvent` also carries no target size;
  `getBoundingClientRect()` is just synchronous there.

**Assessment: NOT-A-BUG** (architectural, and matches DOM semantics). Rating of the
*derived* friction (half-overlay workaround) stands.

**FILE? NO.**

**Corpus correction.** Change "Dioxus mouse events carry offset coordinates but NO
target-element geometry" to "…carry offset coordinates but not the target's *size*; the
rect is only reachable asynchronously through `onmounted` →
`MountedData::get_client_rect()`, because the desktop DOM is out-of-process. Same shape as
the DOM itself, where `getBoundingClientRect` is merely synchronous."

---

## I-7 wry/tauri — `dragDropEnabled` default swallows HTML5 DnD

**CLAIM.** `report/data/interactive-rows.md:19,32,46`: *"`\"dragDropEnabled\": false` is the
load-bearing trap for both apps: default true makes wry's native file-drop handler silently
eat HTML5 drag events."*

**ROOT CAUSE — CONFIRMED, and partially documented.**
- `tauri-utils-2.9.3/src/config.rs:1943-1947`:
  ```rust
  /// Whether the drag and drop is enabled or not on the webview. By default it is enabled.
  ///
  /// Disabling it is required to use HTML5 drag and drop on the frontend on Windows.
  #[serde(default = "default_true", alias = "drag-drop-enabled")]
  pub drag_drop_enabled: bool,
  ```
  Default `true`, and the doc scopes the caveat to **Windows only**.
- On macOS the mechanism is `WryWebView` implementing `NSDraggingDestination`
  (`wry-0.53.5/src/wkwebview/class/wry_web_view.rs:78-107` →
  `wry-0.53.5/src/wkwebview/drag_drop.rs`): `draggingEntered`/`draggingUpdated`/
  `performDragOperation` call the app's `drag_drop_handler` first and only fall through to
  `super` when the handler returns `false`; `dragging_updated` additionally overrides a
  `None` OS operation with `NSDragOperation::Copy`. With Tauri's default handler installed,
  the webview's own HTML5 drag machinery loses.

**UPSTREAM STATUS — ALREADY REPORTED (docs).**
**tauri-apps/tauri#14373** *"[docs] `dragDropEnabled` is confusingly named, the entire
drag-and-drop feature not documented enough"* — **open**, filed 2025-10-27, labels
`type: documentation` + `platform: Windows`, no maintainer reply. The reporter also notes
the macOS mismatch with the Windows-only doc wording.

**FILE? NO — comment on #14373** with our macOS 26.5 data point (two Tauri 2 apps,
tauri-utils 2.9.3 / wry 0.53.5, HTML5 `dragstart`/`dragover`/`drop` silently never fired
until `dragDropEnabled: false`), and suggest the config doc drop "on Windows".

---

## I-8(a) slint — `DragArea.drag-image` is bitmap-only

**CLAIM.** `report/data/interactive-rows.md:127`: *"`DragArea.drag-image` is bitmap-only, no
element→image rendering in the DSL; shipped pseudo-ghost overlay … Biggest gap in the new
API."*

**ROOT CAUSE — CONFIRMED.** `i-slint-compiler-1.17.1/builtins.slint:1234-1248`:
```slint
export component DragArea {
    ...
    in property <image> drag-image;
    in property <int> drag-image-offset-x;
    in property <int> drag-image-offset-y;
```
The property type is `image`. Slint 1.17 has no "render this element to an `image`"
primitive in the DSL, so a drag preview that looks like the dragged card has to be produced
outside the DSL or faked with an overlay.

**UPSTREAM STATUS — no matching issue.** Search of slint-ui/slint for `drag-image` surfaced
only #13066 (open, *"Api review for 1.18"*, milestone 1.18), #11790 (open, *"Expose the
source of a `data-transfer`"*), #11399 (closed) — nothing about rendering an element as the
drag image.

**FILE? YES (feature request) — slint-ui/slint.**

> **Title:** `DragArea.drag-image` only accepts an `image` — no way to use the dragged element as the drag preview
>
> **Body:**
> Slint 1.17.1. `DragArea` (`internal/compiler/builtins.slint`) exposes
> `in property <image> drag-image` plus offsets, but the DSL has no way to render an element
> subtree to an `image`, so a drag preview that looks like the thing being dragged cannot be
> expressed. Building a kanban on the new DragArea/DropArea API, everything else was
> declarative and the ghost was the one piece that had to be faked with a manually
> positioned overlay driven by `can-drop(DropEvent)` positions — which only tracks while the
> pointer is over a `DropArea`.
>
> **Ask:** either accept a component/element for `drag-image`, or add an
> element→image snapshot primitive usable from the DSL.
>
> Might be worth folding into the 1.18 API review (#13066).
>
> Context:
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/slint-board/FRICTION.md

---

## I-8(b) gpui — no general element-bounds query outside drag events

**CLAIM.** `report/data/interactive-rows.md:204,227`: *"gpui has no element-bounds query
API: geometry-aware interaction outside drags (chart hover, slider) needs a paint-time
`Rc<Cell<Bounds>>` canvas probe; `on_drag_move` is the only bounds-carrying event."*

**ROOT CAUSE — CONFIRMED as an API asymmetry.**
- `gpui-0.2.2/src/elements/div.rs:62-70` — `DragMoveEvent<T>` carries
  `pub bounds: Bounds<Pixels>`; ordinary `MouseMoveEvent`/`MouseDownEvent` handlers get
  position only.
- `gpui-0.2.2/src/window.rs:1688` `Window::bounds()` is the *window*'s bounds, not an
  element's. Element bounds are available only inside `prepaint`/`paint` — which is exactly
  what `gpui::canvas` (`elements/canvas.rs:10`) is for, and using it as a bounds probe is
  the idiom Zed itself uses.

**Assessment: by design, not a defect.** The workaround is the sanctioned mechanism.
No matching issue found in zed-industries/zed.

**FILE? NO** (a feature request would be reasonable but low-value; the tracker has no
precedent and `canvas` is the documented path). Keep as a report observation.

Also verified in passing and worth keeping in the report: the corpus's second gpui claim,
*"two-layer drop targeting leans on capture-phase `on_drag_move` dispatching
parent-before-child — load-bearing ordering behavior not in the docs"* — I did not verify
the ordering itself; treat as unverified.

---

## I-9 Todo-round `apps/*-app/GAPS.md` — upstream-attributable items

Skimmed all 10. Most content is spec-coverage and measurement; the framework-attributable
items are:

| # | Framework | Item | Assessment |
|---|---|---|---|
| 1 | **gpui 0.2.2** | Default build fails on Xcode 26: `build.rs` shells out to `xcrun metal` (`gpui-0.2.2/build.rs:196-240`), which Xcode 26 no longer bundles → `cannot execute tool 'metal' due to missing Metal Toolchain`. Workaround is the `runtime_shaders` feature (`build.rs:72-75, 177-195`) or a multi-GB `xcodebuild -downloadComponent MetalToolchain`. | **CONFIRMED**, upstream-attributable. Not fixed: the only related upstream work, zed-industries/zed **#53330** *"docs: add instructions to ensure Metal Toolchain is available for dev on macOS"*, was **closed/abandoned** (2026-04-07). **FILE? MAYBE** — a short docs/build issue on zed-industries/zed (area:gpui): "document `runtime_shaders`, or fall back to it automatically when `xcrun metal` is unavailable". Highest-value item in this table. |
| 2 | **floem** | Latest crates.io release is **0.2.0 (2024-11-14)** — 21 months stale; `main` is unpublishable (git deps on a forked `floem-winit` and `understory_*`), so every floem app in this study pins a git rev. Confirmed against crates.io (only 0.1.0, 0.1.1, 0.2.0 exist). | **CONFIRMED.** Ecosystem finding, not a code defect. **FILE? NO** (a "please cut a release / unfork winit" issue would be noise; the maintainers already direct users to `main`). Keep as a dashboard fact. |
| 3 | **vizia 0.4** | `cx.emit(TextEvent::Clear)` from a `Model` compiles, runs and silently does nothing: model-emitted events propagate **up** the tree and never reach a child view (`apps/vizia-app/GAPS.md`). | **LIKELY** (consistent with `Propagation::Up` in `vizia_core/src/events/event_manager.rs`; I did not build a repro). Same silent-failure family as I-2. **FILE? NO separately** — worth one sentence in the I-2 issue as corroboration that vizia's event targeting fails quietly. |
| 4 | **freya 0.4** | Debug builds inject `freya_performance_plugin::PerformanceOverlayPlugin` unconditionally (`freya-0.4.0/src/lib.rs:132-135`; unchanged on main at `crates/freya/src/lib.rs:137-140`), with no `LaunchConfig` switch; the only escape hatch is calling `freya_winit::launch` directly. | **CONFIRMED.** **FILE? MAYBE** — fold into the panic-hook issue (I-1g) as "please make both the debug FPS overlay and the release panic dialog opt-in". |
| 5 | **egui / egui_kittest 0.35** | `Node::type_text` emits `egui::Event::Text`, delivered to the *focused* widget, so you must call `node.focus()` first or the text silently goes nowhere. | **LIKELY** (app-level observation; matches egui's focused-text-delivery model). Docs nit at most. **FILE? NO.** |
| 6 | **freya 0.4** | `Input` has a fixed default `width` of 150 px; `Size::fill()` on a horizontal child eats the whole row (pushing siblings out of view) where `Size::flex(1.)` is wanted. | Same family as I-1(c) — the `fill` vs `flex` vocabulary. Covered by the I-1(c) docs issue. **FILE? NO separately.** |
| 7 | **xilem 0.4.0** | Version skew inside the release: pins vello 0.6.0 / parley 0.6.0 while standalone vello is 0.9.0 and parley 0.11.0; two `skrifa` versions in the tree. | Release-cadence fact, not a defect. **FILE? NO.** |
| 8 | **tauri 2** | `bundle.icon` PNGs must be RGBA; RGB-only PNGs make `tauri-build`/`tauri` error. | App-level setup note, error is explicit. **FILE? NO.** |
| 9 | **slint 1.16+** | Default widget style is `fluent` on every platform, so a macOS app looks like a Windows app by default. | Documented, intentional (slint.dev blog). **FILE? NO.** |
| 10 | **iced / floem / xilem** | `block v0.1.6` future-incompatibility warning via `copypasta`/`window_clipboard` → `clipboard_macos`. | Transitive-dependency hygiene, three frameworks share it. **FILE? NO** (would belong on `copypasta`/`window_clipboard`, and is cosmetic today). |

---

## Summary of dispositions

| Finding | Rating | Upstream status | File? |
|---|---|---|---|
| I-1(a) freya `DropZone` sizing | CONFIRMED | fixed on main, `2b21c88` / #2151 (issue #2147) | no |
| I-1(b) freya ancestor `on_mouse_up` in empty area | **UNVERIFIED** | n/a | no — soften corpus |
| I-1(b′) freya `DropZone` unconditional `stop_propagation` | CONFIRMED | present on main | **yes** (marc2332/freya) |
| I-1(c) freya `Size::flex` needs `Content::flex` | CONFIRMED | present on main | **yes** (docs/diagnostics) |
| I-1(d) freya `Input` Escape / single-line | CONFIRMED | multiline fixed on main #2192; key-filter gap remains | **yes** (feature request) |
| I-1(e) freya no timer primitive | CONFIRMED | absent on main; no issue | **yes** (low priority) |
| I-1(f) freya stale `canvas()` | CONFIRMED | present on main; no issue | **yes** (strongest) |
| I-1(g) freya panic hook / FPS overlay | CONFIRMED (cross-ref) | present on main | other agent + optional fold-in |
| I-2 vizia hovered-entity actions | CONFIRMED | present on main; #406 closed 2023 | **yes** (vizia/vizia) |
| I-3 iced Escape capture | NOT-A-BUG (documented) | n/a; adjacent #2678 open | no — correct corpus |
| I-4 egui drag-source click swallowing | CONFIRMED | already reported: **#5822 open** | no — comment on #5822 |
| I-5(a) masonry last-wins pointer capture | CONFIRMED | present on main; no issue | **yes** (linebender/xilem) |
| I-5(b) masonry Esc / xilem autofocus | CONFIRMED for 0.4.0 | Esc fixed on main #1826; autofocus still absent | **yes** (autofocus only) |
| I-6 dioxus mouse geometry | NOT-A-BUG (overstated) | n/a | no — correct corpus |
| I-7 wry/tauri `dragDropEnabled` | CONFIRMED | already reported: **#14373 open** | no — comment on #14373 |
| I-8(a) slint `drag-image` bitmap-only | CONFIRMED | no issue | **yes** (feature request) |
| I-8(b) gpui element bounds | CONFIRMED (design) | no issue | no |
| I-9 gpui Metal Toolchain build failure | CONFIRMED | PR #53330 abandoned | maybe (docs) |
| I-9 floem 21-month-stale release | CONFIRMED | n/a | no |

## Corpus statements I now believe are wrong or overstated

1. **freya-board FRICTION sharp edge 2** — "`on_mouse_up` on an ancestor rect never fired
   … Bubbling for released events looks unreliable outside the `DropZone` path." No
   mechanism supports an empty-area asymmetry between `MouseUp` and `PointerDown`; the
   verified mechanism (`DropZone`'s unconditional `stop_propagation`) applies to clicks
   *inside* a DropZone. Rewrite as unconfirmed + the real mechanism.
2. **freya-board FRICTION sharp edge 2** — the "no width/height setters" limitation should
   be annotated "fixed on freya main (#2151), unreleased".
3. **freya-board FRICTION sharp edge 3 / interactive-rows** — "`Size::flex(1.)` collapses"
   is imprecise: outside `Content::flex()` it evaluates to the full available parent extent
   (behaves like `fill`), which is why the first column ate the row.
4. **freya-board FRICTION + interactive-rows** — "`Input` is single-line" needs "fixed on
   freya main (#2192), unreleased".
5. **xilem-board FRICTION** — "Esc-cancel: no API" needs "fixed on masonry/xilem main by
   #1826 (2026-07-28), after 0.4.0".
6. **interactive-rows iced row 173/183** — "`text_input` CAPTURES Escape so
   `keyboard::listen()` never fires" is mechanically right but framed as a defect; it is
   documented behaviour of `iced::event::listen()` (uncaptured events only), and the API is
   `iced::event::listen()`, not `keyboard::listen()`. The real gap is the missing
   `on_escape` hook on `text_input`.
7. **interactive-rows dioxus SURPRISES** — "Dioxus mouse events carry … NO target-element
   geometry" overstates it: `element_coordinates()` (offsetX/offsetY) exists; the target's
   *size* is what is missing, and only asynchronously reachable via `onmounted` →
   `MountedData::get_client_rect()`.
8. **interactive-rows gpui line 228** — "capture-phase `on_drag_move` dispatching
   parent-before-child" was not verified by me; mark as unverified or verify separately.
9. **Already-tracked upstream** (worth saying so on the dashboard rather than presenting as
   novel): egui drag-source trap = emilk/egui#5822; tauri `dragDropEnabled` = tauri#14373;
   freya DropZone sizing = freya#2147/#2151.
