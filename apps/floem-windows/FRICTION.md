# FRICTION — Windows (floem git @ 778bb5f2)

Reference: `apps/SPEC-9.md`. Built and verified on macOS 26.5.2 (M4 Pro,
rustc/cargo 1.96.1). `cargo build --release` clean from scratch in **48 s**;
`cargo build --release --locked` succeeds unchanged; binary
**18.4 MB** (`target/release/floem-windows`). LoC: **1162** total in one
`src/main.rs` (952 non-blank/non-comment) — of which ~130 are the
`WINDOWS_SELFTEST` hook and ~60 the shared-desktop verification hooks, so the
app proper is ~760.

Version note: same pinned git rev as `apps/floem-app` (crates.io 0.2.0 is
stale; `main` is unpublishable because it depends on the forked
`floem-winit`). See `apps/floem-app/GAPS.md`.

Evidence: `evidence/log.txt` (step-by-step, with the command for each step),
`evidence/selftest-log.txt` (`SELFTEST DONE pass=21 fail=0`),
`evidence/persistence.txt`, 12 screenshots, and the two Swift helpers the
shared desktop forced (`clickpid.swift`, `dragwin.swift`).

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** (focus: hand-rolled) | synthetic-input + self-test | `floem::new_window(\|id\| view, Some(WindowConfig…))` + `close_window(id)`. `new_window` is fire-and-forget: it returns `()`, and you only learn the `WindowId` **inside** the view closure, so every app that wants to close/measure/re-focus a window must stash the id itself (here: `RwSignal<Option<WindowId>>`, which doubles as the singleton flag). *Focusing* an existing window is the gap: `floem::action::focus_window()` routes through `get_current_view()` and so only ever focuses the window you are already in — raising a **different** window needed objc2 `makeKeyAndOrderFront:`. |
| Modal dialog — kind achieved | **OS window-modal (macOS sheet)** — assembled from objc2 | observed + synthetic-input | floem has **no modality API at all**: `WindowConfig` has `window_level`, `undecorated`, `resizable`, `theme_override`, per-OS sub-configs… and nothing for parent/owner/modal (read field by field in `window/mod.rs`). But `WindowIdExt::with_window_handle` (added in the very rev pinned here) hands out the `raw-window-handle`, so `-[NSWindow beginSheet:completionHandler:]` on the parent turns an ordinary `new_window` window into a real sheet. Crucially `beginSheet:` is **non-blocking** — it does not spin a nested modal run loop the way `runModalForWindow:` would — so it cooperates with winit's event loop and floem keeps painting. `evidence/04-sheet.png`: no title bar, rounded, centred on the parent, parent chrome drawn inactive. |
| Parent blocked while modal | **built-in to the sheet** (i.e. AppKit does it) | synthetic-input | With the sheet up, **6** clicks on the parent's Delete button (3 × `tools/synth/synth`, 3 × `clickpid` posted straight to the app's pid) produced `grep -c 'toolbar: Delete pressed'` = **0**, and a click on a list row produced 0 `row: selected`. Control: a click on the *sheet's* Cancel at the same moment worked, and after the sheet closed the identical Delete click at the identical screen point fired (count 0 → 1). No app-side input gate is compiled in — the blocking is entirely AppKit's. |
| Focus returns to parent after modal | **hand-rolled** | observed | Nothing returns focus by itself. `close_modal()` calls `endSheet:` then `close_window(id)` then `makeKeyAndOrderFront:` on the parent; the `WindowClosed` listener does the same again for the "user closed it some other way" path. |
| Shared state across windows (live, bidirectional) | **built-in — the headline** | self-test + observed | The whole app state is `RwSignal`s created on a detached `Scope::new()` in `main()`. floem's reactive runtime is process-wide (thread-local to the UI thread), **not** per window, so every window's view closure just captures the same `Copy` `App` struct. There is no message bus, no channel, no per-window `App`, no serialisation, and no "which window owns this" question. Better still, making the *model's own fields* signals (`Project { name: RwSignal<String>, … }`) lets the Inspector's `TextInput::new(p.name)` bind **directly to the model** — zero copies, zero sync code, and the main list updates per keystroke (`evidence/07-shared-state-and-ping.png`). The one wart: `TextInput::new` takes a concrete `RwSignal<String>`, so a numeric field still needs a `String` mirror plus one `Effect` to parse back (the Budget field here). |
| Cross-window message (Ping/Pong) + wake | **built-in** (there is no message; there is a signal) | self-test + observed | Ping = `app.pings.update(\|n\| *n += 1)` in the inspector; the main window's status `Label::derived` is subscribed and repaints. Pong = `app.flash.set(true)` in main + `exec_after(300 ms)` to clear; the inspector's *style closure* reads `app.flash.get()`. No wake mechanism is needed at all — both windows are driven by one event loop and one signal graph. (`ExtSendTrigger`/`create_ext_action` exist for *foreign threads*, and are used here only to bring the rfd dialog answers back.) |
| Theme/layout change applied to all windows | **built-in** | observed | `floem::action::set_global_theme(Theme::Light\|Dark)` → `AppUpdateEvent::ThemeChanged` → floem loops over **every** window handle and re-runs the style pass (`app/handle.rs:220`). One radio click restyles all three windows (`10-theme-light-all.png` + `10b-prefs-light.png`). Gap: there is no *global* "follow the OS again" — `set_theme(None)` is a per-window update message routed through `get_current_view()`, so the System option here resolves `NSApp.effectiveAppearance` once and pushes that globally. Compact rows is just another signal read by the row style closure (`11-compact-rows.png`). |
| Window parenting (child/owner/transient) | **not-achievable in floem; assembled with objc2** | observed | `WindowConfig` cannot express any of `parent`/`owner`/`transient_for` — even though the winit fork underneath has `with_parent_window`, floem's builder never surfaces it. `-[NSWindow addChildWindow:ordered:NSWindowAbove]` gives the real thing, and the behaviour is visible: the inspector stays above main, **moves with it** (dragging main's title bar from 200,300 to 360,493 carried the inspector along — `evidence/persistence.txt`), and `screencapture -l <main>` captures the *pair*, because they are one AppKit window group. That last property also bit the persistence restore (below). |
| Close veto (CloseRequested → Save/Discard/Cancel) | **built-in** | synthetic-input | `listener::WindowCloseRequested` + `cx.prevent_default()`; floem's default behaviour (`event/dispatch.rs:1204`) posts `CloseWindow` only if nothing prevented it, and `request_close_window(id)` re-enters the same path programmatically. Verified end to end: dirty → ⌘W → native Save/Discard/Cancel → **Cancel** leaves the process and both windows alive → **Discard** writes the layout and exits, `EXIT_CODE=0`. |
| Quit semantics (main closes ⇒ exit; child closes ⇒ app lives) | **assembled** | self-test + synthetic-input | macOS default is already `exit_on_close: false`, so closing the inspector or preferences leaves the app running for free. The other half is manual: the main window's `WindowClosed` listener calls `quit_app()`. There is no notion of a "main"/"primary" window in floem — every window is peer-equal. |
| Native confirm dialog | **assembled** (`rfd` =0.17.2) | observed | floem wraps rfd for **file** dialogs only (`open_file`/`save_as`), never for message boxes, so rfd is added directly — it unifies with floem's own rfd 0.17.1, no second dialog stack. Same threading pattern floem itself uses: blocking `MessageDialog::show()` on a `std::thread::spawn`, answer marshalled back with `create_ext_action`. macOS renders it via CFUserNotification (`06-native-confirm.png`, `12-close-veto-dialog.png`), which blocks the UI thread while up — native-normal, and *system-wide serialised*: while a sibling app's alert was open this app's alert went inactive and ignored input until theirs was dismissed. |
| Position/size persistence | **hand-rolled** (25 LoC, no serde) | synthetic-input | floem stores nothing. Two real traps: (a) `WindowIdExt::bounds_of_content_on_screen()` is **broken on macOS at this rev** — `window/tracking.rs:184` reports winit's `surface_position()`, which is window-relative (measured `0,32`), not screen-relative; only its *size* is usable; (b) `WindowConfig::position` places the **content** origin while `bounds_on_screen_including_frame()` reports the **frame**, so a naive save/restore creeps up by the 32 px title bar every launch. Fix: persist frame-origin + content-size, then correct with `WindowIdExt::set_outer_location` once the native window exists. A third trap follows from the parenting above: the correction on the parent **drags the child with it**, so the inspector's correction has to run *after* the parent's. Result is byte-exact (`evidence/persistence.txt`), and `inspector_open` is honoured on relaunch. |
| Multi-monitor scale change | **not-verified** | not-verified | Only one display attached. From the code path: `listener::WindowScaleChanged` is a first-class typed listener (main and inspector both log it), `WindowIdExt::scale()` returns the live factor (status bar reads "scale: 2.0x"), and `WindowIdExt::screen_layout()` / `monitor_bounds()` exist. Note `scale()` returns 1.0 if called while the view closure is still building — the native window does not exist yet. |
| Per-window shortcuts (⌘W focused only, ⌘, , ⌘⇧I) | **assembled** | synthetic-input | The menubar is a floem **built-in** (`floem::Menu` + `action::set_window_menu`, muda underneath, per-item action **closures** — no event channel). Accelerators fire app-wide, which is right for ⌘, and ⌘⇧I. ⌘W is the assembled bit: floem tracks no "key window" for the app, so every window registers `listener::WindowGainedFocus` into a `focused: RwSignal<Option<WindowId>>` and the menu item calls `request_close_window(focused)` — which then runs the veto path. All three verified with real `osascript` keystrokes; Esc closes the topmost non-main window via a per-window `KeyDown` handler. |

## Helper crates

- **rfd =0.17.2** — native message boxes (Delete confirm, Save/Discard/Cancel).
  floem already depends on rfd 0.17.1 for its file dialogs, so this unifies to
  one rfd in the tree; floem simply exposes no message-box wrapper.
- **objc2 =0.6.3 + raw-window-handle =0.6.2** — the only route to the three
  window relations floem/winit do not express: `beginSheet:completionHandler:`
  (window-modality), `addChildWindow:ordered:` (parent/owner), and
  `makeKeyAndOrderFront:` (focus a *different* window). Reached through
  `WindowIdExt::with_window_handle`. Same pair `floem-babel`/`floem-peek` use.
- **No serde**: window geometry is a 5-token-per-line text file (25 LoC).

Tried and rejected: `WindowConfig::window_level(WindowLevel::AlwaysOnTop)` as
a modality substitute — it neither blocks input nor keeps the window on
CGWindowLevel 0 (which is where `scripts/window-count.swift` and
`synth bounds` look); and `NSApp runModalForWindow:` was not attempted, because
it spins a nested modal run loop inside winit's own loop — `beginSheet:` gets
real window-modality without that risk.

## Where the time went

1. **Verifying on a contended desktop** — by far the largest slice, and not a
   floem problem: ~10 sibling GUI apps overlapped this window and drove their
   own synthetic input, so screen-point clicks kept landing on their windows.
   Two helpers (`clickpid.swift`, `dragwin.swift`) and one in-app
   raise-on-file-touch hook were needed before any of the SPEC-9 checks could
   be made repeatable, and one intermediate approach (continuously
   re-activating this app) had to be abandoned because it made *this* app
   swallow the siblings' keystrokes.
2. **The AppKit bridge** — establishing that `beginSheet:` is safe inside
   winit's loop, and that the parent really stops receiving events.
3. **Persistence arithmetic** — three separate coordinate-space traps
   (broken `bounds_of_content_on_screen`, content-vs-frame origin, and the
   child window moving with its parent).
4. The app itself was quick: the six-project list, the four windows and all
   the cross-window behaviour are ordinary floem code.

## Surprises

- **Good, and the story of this app**: *sharing mutable state between windows
  costs nothing*. Signals live in a process-wide runtime, so the two-window
  code is the one-window code. Making the model's fields signals means the
  Inspector's text input is literally bound to the main list's data — the
  spec's "one source of truth, no copy" is the path of least resistance rather
  than an achievement.
- **Good**: `set_global_theme` restyling every window from one call, and the
  menubar taking action closures.
- **Good**: `WindowIdExt::with_window_handle` is a genuinely well-judged escape
  hatch — it is what makes real sheets and real parenting reachable at all.
- **Bad**: `new_window` returns nothing. Every non-trivial multi-window app has
  to build its own window registry before it can do anything.
- **Bad**: `focus_window()` can only focus the window you are already in.
- **Bad**: `bounds_of_content_on_screen()` silently returns window-relative
  coordinates on macOS. Nothing warns; it just looks like a 32 px drift.
- **Bad**: an occluded floem window does not repaint, so a `screencapture -l`
  of a covered window returns a stale frame — worth knowing before trusting a
  screenshot as evidence.

## Skipped / approximated

- **Multi-monitor DPI (req 10)** — one display on the machine; rated
  *not-verified*, code path recorded above.
- **Theme "System" (req 5)** — floem has no *global* revert-to-OS; approximated
  by resolving `NSApp.effectiveAppearance` and pushing that with
  `set_global_theme`. Light/Dark are exact.
- **Tab order inside the modal (req 11)** — floem's default `Tab` behaviour
  (`element_tab_navigation` in `event/dispatch.rs`) walks the two text inputs
  and the two buttons; verified by construction only, not screenshotted.
- **The "Save" branch of the close prompt** does not write the project list
  anywhere — this spec has no document model, so Save and Discard differ only
  in the log line.

## Window model

**A window in floem is a handle, and specifically a `winit::WindowId` you have
to catch yourself.** `new_window(app_view_fn, Some(config))` is a request
posted to the app-update queue, not a value you get back; the identity arrives
as the argument to the view closure, and from then on the window is addressed
through free functions (`close_window(id)`, `request_close_window(id)`) and an
extension trait on the id (`WindowIdExt::set_outer_location`, `scale`,
`bounds_on_screen_including_frame`, `with_window_handle`). There is no `Window`
value, no window struct to hold state in, and no parent/child or primary/
secondary relation — every window is a peer, and "the main window" is an
application-level convention you enforce yourself (here: a `WindowClosed`
listener that calls `quit_app`).

The corollary is the good half. Because a window is only a handle to a *view
tree*, and because the reactive runtime is process-wide rather than per-window,
**two windows seeing the same mutable state is not a design problem in floem —
it is the absence of one.** The code that makes the Inspector edit the main
list is:

```rust
let app = App::new(Scope::new());              // in main(), outside any window
// … main window:      dyn_stack(move || app.projects.get(), …)
// … inspector window: TextInput::new(project.name)
```

That is the entire mechanism: no channel, no bus, no `Arc<Mutex<_>>`, no
"which window is authoritative", and no wake-up. Ping and Pong are one
`signal.update()` each. Where the effort actually goes in a floem multi-window
app is not state at all — it is the *OS* side of a window: modality, owner,
focus and geometry, none of which floem models, all of which it lets you reach
by handing you `raw-window-handle` and getting out of the way.
