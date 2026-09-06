# FRICTION — Windows (vizia =0.4.0)

Reference: `apps/SPEC-9.md`. Built + verified on macOS 26.6.2 (M4 Pro, rustc
1.96.1): `cargo build --release` and `cargo build --release --locked` both
clean, no warnings; binary launched, all windows pixel-verified, self-test
`SELFTEST DONE pass=17 fail=0`, process exits 0. Evidence in `evidence/`
(`log.txt` describes every step with the command used, plus 22 screenshots,
`selftest-log.txt`, `title-leak.txt` and `run-evidence.sh` to reproduce).

Clean release build **50.4 s**; binary **23,199,984 B (22.1 MiB)** (Skia
statically linked). `src/main.rs` is **1021** lines: ~632 production Rust,
141 self-test/pose hook, 59 CSS, the rest blanks and comments.

Evidence labels: **observed**, **synthetic-input** (CGEvent clicks/keys, with
window-scoped `screencapture -l` shots retained), **self-test**
(`WINDOWS_SELFTEST=1`, output retained), **source-only** (code path read in
the vendored crates, not exercised), **not-verified**.

> Verification note: four sibling agents were driving synthetic input on the
> same display throughout. Windows are therefore parked at fixed coordinates
> by pre-seeding the app's own persistence file, and every click goes through
> a retry wrapper that AXRaises this app first and re-clicks until the
> expected window count appears. `evidence/log.txt` reports the retries.
> `evidence/postclick.swift` (a `CGEventPostToPid` helper) is used for the
> keyboard shortcuts, because posted key events reach the target process
> regardless of z-order; posted *mouse* events do not, which is why clicks
> still needed the retry loop.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** | synthetic-input + self-test | `Window::new(cx, \|cx\| ..)` is an ordinary *view* whose entity is flagged `tree.set_window(e, true)`; vizia_winit materialises the winit window in `about_to_wait` when `cx.windows` grows and destroys it when the entity leaves the tree. So a window's existence is `Binding::new(cx, flag, \|cx\| if flag.get() { Window::new(..) })` — no handles, no registry. Verified 1 → 2 → 3 with `scripts/window-count.swift`. The singleton is structural (one bool can only make one window); *raising* the existing one is the only part with no vizia API — `cx.get_view_with::<Window>(e).window.focus_window()` reaches the raw `winit::window::Window`. |
| Modal dialog — kind achieved | **assembled** (in-framework focus lock + `always_on_top`; **not** an OS window-modal on macOS) | synthetic-input + source-only | `Window::popup(cx, is_modal, ..)` really exists: it stores `WindowState { owner: Some(parent), is_modal: true }`, emits `WindowEvent::SetEnabled(false)` at the parent and calls `.lock_focus_to_within()`. Only the focus lock is real on macOS — `Window::event`'s `SetEnabled` arm is `#[cfg(target_os = "windows")] set_enable(flag)` and *unconditionally* calls `self.window().focus_window()`, so on macOS "disable the parent" actually **focuses the parent**. Two further 0.4 defects: `is_modal: true` is stored regardless of the `is_modal` argument, and the `SetEnabled(true)` restore is emitted from the `WindowEvent::Destroyed` arm of the *popup's own* `Window` view. `always_on_top(true)` was added to keep the dialog above its parent — which pushes it off CGWindowLevel 0, so `scripts/window-count.swift` and `tools/synth/synth bounds` (both filter `layer == 0`) cannot see it; the Accessibility API is used for that check instead. |
| Parent blocked while modal (verified by synthetic click) | **assembled** | synthetic-input + self-test | Hand-rolled: `.disabled(show_modal)` on the main window's content root. `disabled` is an *inherited* style property (`inline_inheritance_system`) and `hover_system` refuses to make a disabled view a hover target, which kills every press action in the subtree. Proven: with the dialog open, two synthetic clicks on **Delete** produced no rfd confirm and left the row count at 6 (`10-main-before-modal.png` vs `13-main-after-blocked-delete.png`, and `12-main-blocked.png` shows the greyed parent). Self-test asserts `cx.is_disabled()` on the panel. |
| Focus returns to parent after modal | **built-in** | synthetic-input | Esc closed the dialog; the AX window list shows the main window back in front (`log.txt` step 10). |
| Shared state across windows (live, bidirectional) | **built-in** | synthetic-input + self-test | The headline finding. There is **one** `Context`, **one** entity tree and **one** signal graph; a sub-window's content closure captures the same `Signal<Vec<Project>>` the main window uses. Typing in the Inspector's `Textbox` moves the main window's list row and status bar on every keystroke with zero synchronisation code (`04-inspector-edited.png` / `05-main-live-update.png`). No `Lens` gymnastics needed either — `Signal` is `Copy` and closure-captured. |
| Cross-window message (Ping/Pong) + wake mechanism | **built-in** | synthetic-input + self-test | Ping: `cx.emit(Msg::Ping)` from a button *inside the Inspector window* propagates `Propagation::Up` through the shared entity tree to the model on `Entity::root()` (`06-main-pings.png`, `pings: 2`). Pong: the root model sets a signal and `cx.schedule_emit(Msg::PongOff, now + 300 ms)`; the Inspector's root has `.toggle_class("flash", pong)` and a CSS `transition: background-color` (`07-inspector-pong-flash.png`). Wake: emits land in `cx.event_queue`, and `about_to_wait` pushes a winit user event through the event-loop proxy whenever that queue is non-empty, so the loop wakes itself. |
| Theme/layout change applied to all windows | **assembled** | self-test + observed | `EnvironmentEvent::SetThemeMode` only toggles the built-in `.dark` class on `Entity::root()`, and the theme's colour tokens live in a `:root { ... }` rule whose `is_root()` is `entity == Entity::root()` — it matches **no** sub-window. A freshly created `Window::new` therefore has no background at all until you give it one. The app toggles the class on **every** entity in `cx.windows` itself (self-test `theme_all_windows themed=3/3`) and paints its own `.panel { background-color: var(--background) }`. Result verified in both directions across three windows (`09x/09y/09z` dark, `09a/09b/09c` light). Compact rows: `09d`/`09e`. |
| Window parenting (child/owner/transient) | **not-achievable** on macOS (Windows-only) | source-only | `WinState::new` takes an `owner: Option<Arc<winit::window::Window>>` and uses it in exactly one place: `#[cfg(target_os = "windows")] window_attributes.with_owner_window(hwnd)`. On macOS and Linux the parameter is `#[allow(unused_variables)]` — there is no `parent`, no `transient_for`, no NSWindow `addChildWindow`. What *does* work everywhere is the geometry half: `.anchor(..)`, `.parent_anchor(..)`, `.offset(..)` with `AnchorTarget::Window` are computed by vizia_winit from the parent's `outer_position()`, so "opens offset from its parent" is one modifier chain. "Stays above the parent" is only reachable as `always_on_top` (above *everything*, not just the parent), and hide/minimise-with-parent is not expressible. |
| Close veto (CloseRequested → Save/Discard/Cancel) | **built-in** | synthetic-input + self-test | winit's `CloseRequested` arrives as `WindowEvent::WindowClose` targeted at the window entity, and `EventManager::visit_entity` runs the **models** on an entity before the **view** on the same entity. A `Model` on `Entity::root()` therefore sees it before the `Window` view and `meta.consume()` stops `cx.close_window()` dead. Proven live: dirty flag set, close button clicked, native Save changes? dialog (`14-close-veto-dialog.png`), **Cancel** → stdout `close: vetoed`, process and window survive (`15-alive-after-veto.png`); **No** → `close: discarded`, geometry saved, exit code 0. |
| Quit semantics (main closes ⇒ app exits; child closes ⇒ app lives) | **built-in** | synthetic-input + self-test | Sub-window entities are children of the root in the same tree, so removing the root removes them too, `self.windows` empties and `about_to_wait` calls `event_loop.exit()` → `run()` returns `Ok(())`, exit 0. Closing the Inspector removes only its entity: self-test `child_close_keeps_app count=2`, and ⌘W on the Inspector left the process alive (`log.txt` step 14). |
| Native confirm dialog | **assembled** (`rfd`) | synthetic-input | vizia ships no dialog API of any kind — not an alert, not a file picker. `rfd::MessageDialog` blocking API (vizia has no executor to await an async dialog on, and `Model::event` is already on the main thread). Worth recording: because the test binary is **not inside an .app bundle**, rfd falls back to `CFUserNotificationDisplayAlert`, which logs `called from main application thread, will block waiting for a response` and produces a *system* alert owned by `UserNotificationCenter`, not an NSAlert owned by the app — so it does not appear in the app's own AX window list. Buttons come back as `Yes / No / Cancel` (`MessageButtons::YesNoCancel`). |
| Position/size persistence | **hand-rolled** | synthetic-input + self-test | vizia surfaces `WindowEvent::WindowMoved(WindowPosition)` but has **no resize event and no geometry getter at all**. The app reaches the backing handle — `cx.get_view_with::<Window>(entity).window` → `outer_position()` / `inner_size()` / `scale_factor()` — and does the physical→logical division itself (`WindowModifiers::position` takes *logical* `(i32, i32)`, `outer_position()` returns *physical*). Stored as a five-line `key=value` file next to the binary. Verified end to end: window moved to (700,380) with the AX API, closed with Discard, relaunched → `bounds` reports `700 380 720 512`; and `inspector_open=1` re-opens the Inspector on the next launch (`log.txt` steps 12–13). |
| Multi-monitor scale change | **not-verified** — and wrong by construction | source-only | Only one display (built-in Liquid Retina XDR) was attached, so the drag could not be performed. The code path is unambiguous though: vizia_winit's `ScaleFactorChanged` arm calls `BackendContext::set_scale_factor`, which writes `self.0.style.dpi_factor` — a **single global**. `WindowState::scale_factor` exists per window and is set at creation, but no layout or text system reads it. Moving one window to a 1× display would therefore re-scale *every* window, which is the opposite of what SPEC-9 asks for. |
| Per-window shortcuts (⌘W focused window only, ⌘, , ⌘⇧I) | **assembled** | synthetic-input | All three work (`log.txt` step 14: ⌘, opened Preferences, ⌘⇧I closed and reopened the Inspector, ⌘W closed the focused Inspector and left the app running). But `cx.focused` is **one global `Entity` for the whole application**, not per window: `internal_state_updates` retargets every `KeyDown`/`CharInput` to that single entity. The only per-window information on a key event is `meta.origin`, which `emit_window_event` sets to the window entity — so "which window did this ⌘W mean" is `meta.origin`, and a hand-kept `focused_window` field is needed for anything more. |

## Traps found (all silent)

1. **`.title()` on a sub-window renames its parents.** `Handle<Window>::title`
   does `cx.emit(WindowEvent::SetTitle(..))` with `Propagation::Up`, and
   `Window::event`'s `SetTitle` arm does not `meta.consume()`. Opening the
   Inspector renamed the *main* window to "Inspector" —
   `evidence/title-leak.txt` has the `window-count.swift` output. Workaround:
   re-assert the main title from each sub-window's `on_create` with
   `cx.emit_to(Entity::root(), ..)` (Direct propagation cannot leak).
2. **`disabled` is inherited, so `:disabled { opacity: 0.45 }` compounds.** The
   rule matches every descendant of a disabled view and the opacities
   multiply; three levels down the list rows were invisible rather than dim.
   Scope the rule to the outermost container (`.panel:disabled`).
3. **The built-in layout sheet pins `list list-item { height: 30px }`**, which
   wins over any height set on your own row content — a "compact rows" toggle
   has to target `list-item`, not the view you built.
4. **`TreeProps::parent_window(e)` skips `e` itself** even when `e` *is* a
   window entity (the early return is commented out in `Tree::get_parent_window`),
   so `ModifyWindow::modify_window` called with a window entity as `current`
   operates on that window's *parent*. Use `get_view_with::<Window>(e)` instead.
5. **`always_on_top` moves a window off CGWindowLevel 0**, where
   `scripts/window-count.swift` and `tools/synth/synth bounds` stop seeing it.
   Not a vizia bug, but it silently breaks the SPEC-9 harness.
6. Inherited from `apps/vizia-board`: action modifiers only fire on the
   *hovered* entity, so non-interactive children need `.hoverable(false)`.
   Applied to every `Label` in the list rows and the preference rows here.

## Helper crates

* **`rfd = "=0.17.2"`** — the native confirm and the Save/Discard/Cancel close
  prompt. vizia has no dialog API. Same version as `apps/vizia-tray`.

Nothing else. Sub-windows, modality plumbing, `on_close`/`on_create`,
`WindowEvent::WindowClose` interception, anchoring/offsetting a window against
its parent, `Keymap`/`KeyChord`, CSS transitions and the raw
`winit::window::Window` escape hatch (`ModifyWindow`, `get_view_with::<Window>`)
are all in `vizia` + `vizia_winit`. Persistence is a hand-rolled five-line
`key=value` file rather than `serde_json` — five integers did not justify a
dependency.

Tried and rejected: `Window::popup(cx, true, ..)` *alone* for modality (it does
not block the parent on macOS, see the table); `EnvironmentEvent::SetThemeMode`
alone for the theme (it only reaches `Entity::root()`).

## Where the time went

1. **Roughly half the wall clock went to the multi-agent desktop, not to
   vizia.** Four other GUI apps were opening, moving and raising windows over
   the same coordinates; a `CGEvent` click posted to the HID tap goes to
   whatever is on top, so buttons "did nothing" for long stretches. The fix
   was the AXRaise + retry-until-observed wrapper plus a
   `CGEventPostToPid` helper for keys. This is a measurement-harness finding,
   not a framework finding, but it is why `WINDOWS_POSE=` exists.
2. The `.title()` leak (trap 1) — the symptom is a *correct* window count with
   the wrong titles, which looks like a screenshot mistake.
3. Deciding what "modal" actually means here: reading `Window::popup`,
   `WindowState`, `SetEnabled` and `WinState::new` end to end to establish
   that only the focus lock is real on macOS.
4. The features themselves were quick. Second window, singleton, shared state,
   Ping/Pong and the close veto together were well under an hour.

## Surprises

* **Good — "one source of truth across windows" is not a problem in vizia, it
  is the default.** Signals are plain values captured by the closure, the
  entity tree spans every window, and events emitted in a child window bubble
  to the root model. SPEC-9's requirement 3 cost zero lines of plumbing.
* **Good — a window is a view.** `Binding::new(cx, flag, |cx| if flag.get() {
  Window::new(..) })` opens and closes a real OS window declaratively, and
  `Context::remove` cleans `cx.windows` so the winit window disappears with the
  entity. Compared with a `HashMap<WindowId, State>` this is a different world.
* **Good — the close veto falls out of the event model** (models before views
  on the same entity) with no special API.
* **Bad — modality and parenting are Windows-only stubs.** The API surface
  (`Window::popup(is_modal)`, `WindowState::owner`, `WindowEvent::SetEnabled`)
  reads as cross-platform; on macOS `SetEnabled(false)` focuses the very window
  it is supposed to disable.
* **Bad — the built-in theme only styles `Entity::root()`.** Every sub-window
  starts with no background, and the framework's own theme-mode event does not
  reach it.
* **Bad — `style.dpi_factor` is global.** Per-window scale factors are stored
  and then never used.
* Neutral: geometry is write-only through vizia's own API; anything that reads
  back a window's position or size goes through `winit` directly.

## The "window model" paragraph

**In vizia a window is a *view* — an entity in the single application tree that
happens to be flagged as a window — and everything else follows from that.**
`Window::new`/`Window::popup` build a `View` like any other, insert a
`WindowState` into `cx.windows`, and vizia_winit reconciles that map against
its own `HashMap<WindowId, WinState>` once per `about_to_wait`: an entry that
appeared gets a winit window, an entity that was removed loses one. There is no
window handle to store, no id to route messages to, and no per-window `App`
state: sub-window entities are *children of the entity that built them*, so
they share the root `Context`, the root model, the style tree, the focus
pointer and the signal graph. When two windows must see the same mutable state,
the code is simply that both content closures captured the same `Signal<T>`:

```rust
let projects = Signal::new(seed());          // built once, before any window
VStack::new(cx, move |cx| { List::new(cx, projects, ..); });           // main
Binding::new(cx, show_inspector, move |cx| { if show_inspector.get() {
    Window::new(cx, move |cx| {                                        // child
        Textbox::new(cx, projects.map(|p| p[0].name.clone()))
            .on_edit(|cx, t| cx.emit(Msg::SetName(t)));   // -> root model
    });
}});
```

No lens, no channel, no `Arc<Mutex<_>>`, no copy to keep in sync — and an event
emitted in the child reaches the root model by ordinary upward propagation.
The price of that unification is paid at the edges: **focus is one global
`Entity` for the whole app**, **the DPI factor is one global f64**, and the
built-in theme's `:root` rule matches only `Entity::root()`, so per-window
identity has to be re-established by hand (`meta.origin` for "which window sent
this key", a class toggled on every window entity for "restyle everything").
And because the model is so thoroughly in-framework, the *OS* window
relationships that SPEC-9 asks about — modal, owner, transient-for — are the
part vizia has not built: they exist as fields and events, but outside Windows
they are stubs.

## Approximated or skipped

* **OS window-modality** (req. 2). Shipped: `Window::popup(cx, true, ..)`
  (focus lock) + `always_on_top` + a hand-rolled `disabled` mask on the
  parent. macOS sheets / `NSApp::runModalForWindow` are not reachable through
  vizia or vizia_winit without an `objc2` dependency, which would have been a
  second framework rather than a measurement of this one.
* **Window parenting** (req. 6): only the "opens offset from the parent" half.
  Owner/transient is `#[cfg(target_os = "windows")]` in vizia_winit.
* **Multi-monitor DPI** (req. 10): one display attached; marked *not-verified*
  with the source-level finding recorded above.
* **"⌘W closes the focused window"** is resolved from `meta.origin` plus a
  `WindowFocused(true)` tracker rather than a real per-window focus, because
  vizia has only one.
* The persistence file is `key=value` text, not JSON (SPEC-9 allows any
  format); it stores the main window and the Inspector, not Preferences, which
  is a fixed-size singleton.
