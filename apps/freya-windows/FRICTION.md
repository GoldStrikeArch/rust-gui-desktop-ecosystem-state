# FRICTION — Windows (freya =0.4.0)

Reference: `apps/SPEC-9.md`. Built and verified on macOS 26.5.2 (M4 Pro,
rustc 1.96.1). `cargo build --release` clean from an empty `target/` in
**33.7 s**; `cargo build --release --locked` reproduces with no lockfile
change. Binary **21.8 MB**. LoC **1251** in a single `src/main.rs` (1011
code / 99 comment / 141 blank), of which ~120 are the `WINDOWS_SELFTEST=1`
hook — Freya's element builders put one attribute per line, so this is
comparable to `freya-tray`'s 867 rather than to a 700-line egui file.

Transitive `freya-*` crates are pinned to **0.4.1** in `Cargo.lock` (the
version the rest of the Freya cohort locked); a fresh resolution today picks
0.4.3, which would have made this app's rows incomparable with
`freya-app`/`-board`/`-grid`/`-tray`.

Evidence: `evidence/log.txt` (every command), `evidence/selftest-log.txt`
(`SELFTEST DONE pass=16 fail=0`, exit 0) and eleven screenshots. Note that six
sibling agents were driving synthetic input on the same display; the log
explains the two helpers that made scripted clicks trustworthy anyway.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** | self-test + synthetic-input | `Platform::get().launch_window(WindowConfig::new(component)…).await -> WindowId`, `close_window(id)`, `focus_window(Some(id))` (`WinitPlatformExt`). Singleton is one `State<Option<WindowId>>`: if it is `Some`, focus instead of launching. `use_drop` in the window's root component clears the id when the window is closed from the title bar. Window count 1 → 2 → 3 verified with `scripts/window-count.swift`. |
| Modal dialog — kind achieved | **hand-rolled (in-framework overlay)** | synthetic-input, 05-/06- | Neither Freya nor winit 0.30 has any modal API. Shipped: a real child window plus an `interactive(false)` scrim over the parent. **Not OS modality** — the parent's title bar, resize and menu still work, only its content is inert. Freya *does* ship an in-window `Popup` component (full-window scrim + centred card, `on_close_request`), which is the "looks modal" option; it was not used because it cannot test the window model. macOS sheets (`-[NSWindow beginSheet:]`) would need `objc2` + a `NSWindow` reachable before the window is shown; rejected, see below. |
| Parent blocked while modal (verified by synthetic click) | **assembled** | synthetic-input, 06- | `rect().interactive(false)` on the main window's root sets `Interactive::No`, which freya-core propagates to the whole subtree, so no pointer event reaches any element. Verified with a control: the *same* CGEvent click on Delete does nothing with the dialog open and pops the native confirm with it closed. Keyboard is blocked because the dialog window owns the OS focus. |
| Focus returns to parent after modal | **built-in** | self-test + observed, 09- | `Platform::get().focus_window(Some(main_id))` in `close_dialog` and in the dialog's `use_drop`. 09- shows the focus ring back on the main window's `Edit…` button after the dialog committed. |
| Shared state across windows (live, bidirectional) | **built-in** | self-test + synthetic-input, 03-/04- | **The headline result.** `State::create_global(value)` in `main()` returns a `Copy` signal that is leaked into a process-wide `generational_box` owner; every window's root closure captures the same `Shared { … }` struct of those signals. Freya's reactive graph is per-process and single-threaded: a `write()` in window A notifies the subscribing `ReactiveContext`s of window B, each of which pushes `MarkScopeAsDirty` into *its own* runner channel, and the runner's waker posts `PollRunner` for that window through the shared winit event-loop proxy. No bus, no channel, no clone: the inspector's `Input` writes into the same `Vec<Project>` the main list renders. 12 shared signals here, and the app-side code for "two windows see the same mutable state" is literally `WindowConfig::new(move \|\| inspector_window(shared))`. |
| Cross-window message (Ping/Pong) + wake mechanism | **built-in** | self-test + synthetic-input, 03- | Same mechanism: `pings: State<u32>` incremented from the inspector, rendered in the main status bar; `pong: State<u32>` bumped from the main window, watched by a `use_side_effect` in the inspector that sets a flash flag and clears it after 300 ms (`async_io::Timer`, since Freya has no timer hook). The "wake" is the per-window `TreeHandle` waker (`freya-winit` window.rs:294-306) → `NativeWindowEventAction::PollRunner`. |
| Theme/layout change applied to all windows | **built-in** | self-test + synthetic-input, 10-/11- | Global `State<ThemeChoice>` + a per-window `use_side_effect` that pushes `light_theme()`/`dark_theme()` into that window's `use_init_theme` state, and a plain palette function for the app's own colours. 10- shows the main window and the inspector repainting light in the same frame while `Compact rows` shrinks the list rows from 34 px to 22 px. `ThemeChoice::System` additionally reads `Platform::get().preferred_theme`, which is itself reactive. |
| Window parenting (child/owner/transient) | **assembled (with a Freya-blocking bug)** | observed | See "the parenting crash" below. winit's `WindowAttributes::with_parent_window` **does** work on macOS (it calls `-[NSWindow addChildWindow:ordered:]`), but using it through `WindowConfig::with_window_attributes` **crashes Freya on window creation**. The relationship is therefore established *after* creation from a `Platform::post_callback`, with 20 lines of `objc_msgSend` FFI and no extra crate. Observed: the inspector and the dialog always paint above the main window, follow it, and are returned together with it by `screencapture -l` (one AppKit window group). Preferences is left unparented as a control. |
| Close veto (CloseRequested → Save/Discard/Cancel) | **assembled** | synthetic-input, 08- | `WindowConfig::with_on_close(hook) -> CloseDecision::{Close, KeepOpen}` is a genuine `WindowEvent::CloseRequested` interception. The hook must answer **synchronously** and is `Box<dyn FnMut(..) + Send>`, so (a) the prompt has to be a blocking native dialog — `rfd::MessageDialog` with `MessageButtons::YesNoCancelCustom("Save","Discard","Cancel")` — and (b) it cannot capture a `State`; the dirty flag is mirrored into an `AtomicBool` by a `use_side_effect`. Verified: Cancel → `KeepOpen`, process and both windows survive; Discard → quits, exit code 0. |
| Quit semantics (main closes ⇒ app exits; child closes ⇒ app lives) | **assembled** | self-test + synthetic-input | Freya only exits when the **last** window closes, so "main closes ⇒ quit" is an explicit `RendererContext::exit()` inside the close hook (verified with the inspector still open: `EXIT=0`, no windows left). Closing the inspector leaves the app running with no extra code. |
| Native confirm dialog | **assembled** | synthetic-input, 07- | `rfd` 0.17.2 `MessageDialog::…::YesNo`. Freya depends on rfd internally (its release-mode panic dialog) but does not re-export it and ships no message-box API. The synchronous API blocks the UI thread while the panel is up, which is the correct app-modal behaviour; macOS logs `CFUserNotificationDisplayAlert: called from main application thread, will block waiting for a response`. |
| Position/size persistence | **assembled** | synthetic-input | serde/serde_json to `freya-windows-state.json` next to the binary. Reading the geometry back is the nice part: the close hook's `RendererContext::windows()` hands you **every** live window's `winit::Window`, so one pass records main/inspector/prefs frames and whether the inspector was open. Restoring needs `WindowConfig::with_window_attributes(\|attrs, _\| attrs.with_position(..))` — `WindowConfig` has `with_size` but **no `with_position`**. Verified round-trip: AX-move to (240,260) → quit → relaunch at the identical frame. |
| Multi-monitor scale change | **not-verified** | not-verified | One display only. Code path read: `WindowEvent::ScaleFactorChanged` → `AppWindow::sync_scale_factor()` (updates that window's reactive `Platform::scale_factor`) → layout reset + text-cache reset + redraw, per window (`freya-winit` renderer.rs:653-659). Each window owns its own Skia surface and text cache, so one window rescaling cannot disturb another. |
| Per-window shortcuts (⌘W focused window only, ⌘, , ⌘⇧I) | **hand-rolled** | synthetic-input | `rect().on_global_key_down(..)` on each window's root; `Platform::get()` is per-window, so "the focused window" is implicit. ⌘W on a secondary window is `close_window(self_id)`; ⌘W on the main window **cannot** replay the OS path — there is no way to post a `CloseRequested`, and `Platform::close_window` bypasses `with_on_close` entirely — so it calls the same prompt+quit function the hook does. Verified: ⌘W with Preferences focused closed only Preferences (3 → 2 windows). See the `Input` trap below, which silently breaks all of these while a text field has focus. |

> **Verifier note (2026-08-30):** three corrections from the verification pass.
> (1) *Position persistence creeps upward by the title-bar height (~32 px logical) on every
> save/restore cycle* — measured over three clean quit/relaunch cycles: frame y 268 → 236 → 204.
> `save_geometry` records `window.outer_position()` (frame origin) but the restore goes through
> `WindowAttributes::with_position`, which winit 0.30 applies to the AppKit *content* rect at
> window creation. The evidence log's own numbers show it (AX-move to 240,260 → relaunch at
> 240,228); the "relaunch at the identical frame" reading above is wrong.
> (2) *The `input_keys` replacement filter leaks the plain character of a ⌘-shortcut into the
> focused `Input`*: with the inspector's Name field focused, ⌘, opens Preferences **and**
> prepends a "," to the name (reproduced twice; row 0 became ",Apollo" in all three windows and
> the document went dirty). Returning `true` for META/CTRL combos un-swallows the shortcut but
> lets the editor insert the character; a correct filter must also `prevent_default()` for
> non-editing chords.
> (3) For this unbundled binary the blocking rfd prompts are presented *out-of-process* by
> `UserNotificationCenter` (the `CFUserNotificationDisplayAlert` line quoted below is that
> fallback, not an in-process `NSAlert`); the alert windows are invisible to per-process AX
> scripting and survive the app if it dies while one is up.

## The parenting crash

`WindowAttributes::with_parent_window(Some(RawWindowHandle::AppKit(..)))` is
supported by winit 0.30 on macOS and is reachable from Freya through
`WindowConfig::with_window_attributes`. Doing it kills the app on the spot:

```
PANIC: panicked at accesskit_winit-0.33.2/src/lib.rs:198:13:
The AccessKit winit adapter must be created before the window is shown
(made visible) for the first time.
```

`AppWindow::new` deliberately creates every window with `.with_visible(false)`
and only calls `window.set_visible(true)` *after*
`Adapter::with_event_loop_proxy(..)`. But `addChildWindow:ordered:` orders the
child in immediately, so the window is already visible when the adapter is
built. Neither of Freya's two creation hooks helps: `with_window_attributes`
and `with_window_handle` both run *before* the adapter. There is no
"window created" hook that runs after it.

The workaround in this app is to attach the child afterwards, from a
`Platform::post_callback` that has the `RendererContext`, using the objc
runtime directly (`sel_registerName` + `objc_msgSend`, ~20 lines, no crate) —
`[[parent_view window] addChildWindow:[child_view window] ordered:NSWindowAbove]`.
It works and needs no dependency, but it is exactly the kind of thing a GUI
framework should not make you write. Set `WINDOWS_NO_PARENT=1` to skip it.

Upstream shape of a fix: either create windows hidden and let the app opt into
`set_visible` after the adapter exists, or expose the parent as a first-class
`WindowConfig::with_parent(WindowId)` that Freya applies at the right moment.

## The `Input` trap: a focused text field eats every app shortcut

`Input`'s default `on_pre_key_down` is

```rust
Key::Named(Enter | Escape | Shift) => true,
Key::Named(Tab)                    => false,
_ => { e.stop_propagation(); e.prevent_default(); true }
```

`stop_propagation()` also cancels the root's `on_global_key_down`, so while an
`Input` has focus **⌘, / ⌘⇧I / ⌘W silently stop working** — which is precisely
the state a user is in when they want them. It cost a debugging cycle here
(the shortcuts worked when a `Button` had focus and not when the inspector's
name field did). The fix is to replace the filter and return early for
`META`/`CONTROL` combos; `apps/freya-board` had to replace the same filter for
a different reason (Escape). A stock `Input` that does not swallow modifier
combinations would remove that whole class of bug.

## The close hook is `Send`, so it cannot see the app

`OnCloseHook = Box<dyn FnMut(RendererContext, WindowId) -> CloseDecision + Send>`.
`State<T>` is `!Send` (thread-local `generational_box`), so the veto hook —
the one place that has to ask "is the document dirty?" — cannot read the
document. This app mirrors the flag into a `static AtomicBool` from a
`use_side_effect`, the same bridge shape `apps/freya-tray` needed for its tray
handler. Since Freya is single-threaded by construction and the hook runs on
the UI thread, the `Send` bound looks unnecessary; dropping it would make the
hook usable directly.

The flip side is genuinely good: `RendererContext` is a complete renderer
view (`windows()`, `windows_mut()`, `launch_window`, `exit`, the
`ActiveEventLoop`), so "save every window's geometry, then quit" is six lines
inside the hook, and `Platform::post_callback` gives the same view to normal
app code asynchronously. That is the API that made the in-process self-test
(window counts asserted from inside the app) possible at all.

## Helper crates

- `rfd` **=0.17.2** — native `NSAlert`. Freya depends on rfd but does not
  re-export it, and has no message-box API. Used for the Delete confirm and
  for the Save/Discard/Cancel prompt inside the close hook (the only place a
  *synchronous* answer is possible).
- `serde` **=1.0.229** + `serde_json` **=1.0.151** — window-geometry file.
  Both are already in Freya's dependency tree, so they add no build time.
- `async-io` **=2.6.0** — Freya's executor has `spawn` but no timer; needed for
  the 300 ms Pong flash and the self-test's waits. Same crate
  freya-animation uses for its clock.

Tried and rejected: `objc2`/`objc2-app-kit` for macOS sheets and for
`addChildWindow:` — a sheet needs the `NSWindow` before Freya shows it (same
ordering problem as above) and would have added two large crates; 20 lines of
raw `objc_msgSend` cover the parenting case instead. No `winit` dependency of
our own: `freya::winit` re-exports it, including `raw_window_handle`.

## Where the time went

1. **Verification on a contended desktop, not the app.** Six agents driving
   CGEvents at the same screen meant scripted clicks landed in other apps'
   windows, another agent's ⌘H hid this app, and stray keystrokes typed
   `1234.5` into my focused inspector field. Two helpers fixed it: gate every
   click on "is my window the topmost window at this point?" and screenshot
   with `screencapture -l <windowid>` instead of `-R`. Easily half the wall
   clock.
2. The AccessKit parenting crash and the objc workaround.
3. The `Input` shortcut-swallowing bug.
4. Everything else — three windows, shared state, ping/pong, theme, close veto
   — was fast; the first version of the multi-window skeleton compiled and
   passed its self-test in one pass.

## Surprises

- **Good, and the finding of this spec:** `State::create_global` is documented
  *for* multi-window (`lifecycle/state.rs:107-121, 494-520`) and simply works.
  "Two windows see the same mutable state" is a `Copy` struct captured by two
  closures — no `Arc<Mutex<_>>`, no message enum, no per-window `App` state to
  reconcile. Bidirectional live editing between two OS windows cost zero
  lines of plumbing.
- Good: `Platform` + `RendererContext` + `post_callback` make the winit layer
  fully reachable from app code without a custom event loop.
- Bad: `WindowConfig` has `with_size` but no `with_position`; every window
  placement goes through the raw `WindowAttributes` hook.
- Bad: no modal, no way to post a close request, no window-parent API, and the
  one parenting route winit does offer crashes the framework.
- Bad: a newly launched window does not become key; a modal has to
  `focus_window` itself.
- Neutral: `Button` is keyboard-activatable (its `on_all_press` handles
  `PressEventData::Keyboard`), which is what made keyboard-only verification
  possible when the pointer was unusable.

## Skipped / approximated

- **OS window-modality**: not achievable. Shipped an in-framework overlay over
  a real child window and rated it as such (see the modal rows).
- **Multi-monitor scale change**: only one display available; rated
  *not-verified* with the code path recorded, as the spec allows.
- **Pong flash screenshot**: the 300 ms flash is exercised by the self-test and
  was seen by eye, but `screencapture` cannot round-trip inside 300 ms, so
  there is no image of it.
- **Preferences window parenting**: deliberately left unparented as a control
  against the parented inspector/dialog.
- Preferences lays the three theme radios out horizontally so the spec's
  ~320×200 window fits them; a vertical stack overflowed the 200 px height.

## "Window model" paragraph

**In Freya a window is a configuration value that owns a component tree, and
the state is not in the window at all.** `WindowConfig::new(app)` is a plain
struct — root component, size, title, hooks — that you either register on
`LaunchConfig` before `launch()` or hand to `Platform::launch_window(..).await`
later; the identity you keep afterwards is a `winit::WindowId`, and every
operation on a live window (`close_window`, `focus_window`, `with_window`,
`post_callback`) is a message posted to the renderer keyed by that id. Behind
each window the renderer builds a *complete independent stack* — its own
`Runner`, its own scope tree, its own `Platform` root context, its own Skia
surface, layout tree, text cache and AccessKit adapter — so a window is much
closer to an entity than to a value or a view: `Platform::get()` inside a
component means "the platform of *this* window", which is exactly what makes
per-window shortcuts and per-window scale factors fall out for free. What
crosses window boundaries is not the window abstraction but the reactive
graph, which is process-wide: `generational_box` storage is a thread-local, so
a `State` created with `create_global` in `main()` is visible to every runner,
and its subscriber set holds `ReactiveContext`s from several windows at once.
Writing to it marks scopes dirty in each of those runners and wakes each
window through the shared event-loop proxy. So the code for "two windows must
see the same mutable state" is:

```rust
let shared = Shared { projects: State::create_global(seed()), .. };   // Copy

launch(LaunchConfig::new().with_window(
    WindowConfig::new(move || main_window(shared)).with_title("Windows (freya)")));
// …and later, from anywhere:
Platform::get().launch_window(
    WindowConfig::new(move || inspector_window(shared)).with_title("Inspector")).await;
```

and nothing else — no `Arc`, no lock, no message type, no synchronisation of
two copies. That is the cheapest multi-window shared state in this corpus so
far. The cost is on the other side of the same seam: anything the *platform*
layer needs from the app (the close hook, a tray handler) runs outside every
component scope and behind a `Send` bound, so it cannot read a signal at all
and has to be fed through `static`s — the reactive graph is easy to share
between windows and impossible to share with the shell.
