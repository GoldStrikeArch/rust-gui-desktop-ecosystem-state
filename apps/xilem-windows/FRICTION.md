# FRICTION — Windows (xilem =0.4.0)

SPEC-9 built as `apps/xilem-windows` (package `xilem-windows`, edition 2024,
same `xilem = "=0.4.0"` pin as `apps/xilem-app`). Verified on macOS 26.5.2 /
Apple M4 Pro / rustc 1.96.1 with (a) a built-in scripted self-test
(`WINDOWS_SELFTEST=1` → `SELFTEST DONE pass=23 fail=0`, exit 0), (b) synthetic
CGEvent/AppleScript input with per-window screenshots, and (c) the app's own
`WINDOWS_LOG=1` stdout, which prints one `STATE` line per observable state
change, one `BLOCKED` line per input event the modal filter drops, one
`VETOED` line per vetoed close request and one `KEY` line per raw winit
`KeyboardInput` **with the window it was addressed to**. All of it is under
`evidence/` (16 screenshots, `log.txt` with the exact command per step,
`run-log.txt`, `selftest-log.txt`, `window-count.txt`, `persisted-state.json`).

LoC **1518** (`src/main.rs` 1054 + `src/shell.rs` 464); ~160 of those are the
self-test harness and ~120 are doc comments, so the app proper is ~1240.
Over the ≤700 guide, and honestly so: ~460 lines are a winit event-loop
embedding that exists solely because xilem 0.4 has no modality, no close veto
and no window handle.

Clean `cargo build --release` **27.6 s** (a later repeat on a machine running
seven parallel research agents took 41.6 s); `cargo build --release --locked`
straight after succeeds with no lock change. Binary **11,522,512 B** raw /
**9,329,528 B (8.9 MiB)** stripped. `Cargo.lock`: 443 crate name entries.

## Architecture (the headline)

In xilem 0.4 **a window is a view**. `Xilem::new(state, logic)` takes one
`AppState` and a closure returning an *iterator of `WindowView`s*; a window
exists exactly while its `WindowId` is yielded, `MasonryDriver::run_logic`
diffs the returned ids against the live ones and creates/closes the
difference. So SPEC-9's hardest-sounding requirement — "one source of truth
across windows" — is *free*: there is one `&mut AppData`, and the inspector's
`text_input` callback writes `s.projects[i].name` while the main window's row
list reads it in the same pass. There is no per-window `App`, no context, no
handle to marshal anything through.

Everything the OS owns, however, is missing, and all of it lands in
`src/shell.rs`, which uses masonry_winit's **external-event-loop embedding**
(`Xilem::into_driver_and_windows` + `MasonryState::new` + our own
`ApplicationHandler`, the pattern `apps/xilem-tray` already needed) to get
three things `Xilem::run_in` cannot give:

1. **A wrapper `AppDriver`.** `AppDriver::on_close_requested` is the only
   place a close request can be intercepted, and `DriverCtx::window(id).handle()`
   is the only public path from app code to a winit `Window` — needed for
   `focus_window()` and for reading position/size.
2. **Raw winit events.** Needed for the modal input block and for per-window
   ⌘W / ⌘, / ⌘⇧I / Esc, since masonry only routes keys to the focused widget
   and xilem has no accelerator API.
3. **Ordering.** `on_action` → inner driver (runs app logic, creates/closes
   windows) → our `sync()`, which republishes the winit-id→window map,
   geometry, and applies pending focus/quit/close requests.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** (singleton focus **assembled**) | synthetic-input + self-test | `window(id, title, view)` yielded from app logic; `WindowOptions::on_close` flips the flag that stops yielding it. `window-count.swift` reports 1→2→3 (`evidence/window-count.txt`). Singleton is a state flag plus `DriverCtx::window(id).handle().focus_window()` — there is no view-layer "focus this window". |
| Modal dialog — kind achieved | **hand-rolled** (in-framework window + winit-layer block); **OS window-modal only for `rfd`** | synthetic-input | xilem/masonry/winit expose **no** modality: no sheet, no app-modal, no `set_enable`. The edit dialog is an ordinary `WindowView` with `WindowLevel::AlwaysOnTop` and `with_resizable(false)`; blocking is ours. The *one* OS-modal presentation reachable is `rfd::MessageDialog::set_parent(handle)`, which switches rfd from an out-of-process `CFUserNotification` to an in-process alert centred over the parent with the parent dimmed (`evidence/16-rfd-sheet-over-parent.png`) — but only for rfd's fixed button sets, never for an app-drawn dialog. |
| Parent blocked while modal (verified by synthetic click) | **hand-rolled** | synthetic-input | `shell.rs::window_event` drops `MouseInput`/`CursorMoved`/`KeyboardInput`/`Ime`/`ModifiersChanged`/… addressed to the main window while the modal exists — a hand-built Win32 "owner-disabled". Three synthetic clicks on the parent's **Delete** produced 40 `BLOCKED #n ... -> main window (modal open)` lines, no `STATE` line, no confirm dialog, and an unchanged 6-row list (`evidence/08-*` vs `09-*`). The clicks provably reached the process; they were dropped. |
| Focus returns to parent after modal | **assembled** | synthetic-input + self-test | App state sets a request; the driver calls `handle().focus_window()` in `sync()`. `evidence/11-main-after-modal-commit.png` shows the main window key again after Enter committed. The self-test asserts on a `focus_applied` flag the driver sets. |
| Shared state across windows (live, bidirectional) | **built-in** | synthetic-input + self-test | One `AppData`; the inspector's three `text_input`s write straight into `projects[sel]` and the main list re-renders in the same app-logic pass. Typing in the inspector changed the main list live (`evidence/log.txt` §6, and the earlier run where the row title followed each keystroke). Zero plumbing — this row is where xilem is furthest ahead of the spec. |
| Cross-window message (Ping/Pong) + wake mechanism | **built-in** for Ping, **assembled** for the timed Pong | observed | Ping is just `s.pings += 1` in the inspector's button callback — "cross-window" isn't a concept when there's one state. Pong needs a *timer*: a detached `std::thread` sleeps 300 ms and pushes `Ev::PongEnd` into the tokio channel drained by a stock `worker` view, whose `MessageProxy` wakes the loop (`evidence/13-inspector-flash.png` at ~80 ms vs `14-*` at ~700 ms). xilem has no per-window event/message API at all; the channel+worker is the sanctioned wake path. |
| Theme/layout change applied to all windows | **built-in** | synthetic-input | One click on *Dark* in the Preferences window restyles all three windows in one pass (`evidence/04..06`), because every view is rebuilt from the same `state.theme`. Caveat: masonry 0.4's `default_property_set()` is **dark-only** and cannot be swapped at runtime, so the light palette is hand-applied per widget (`.color()`, `.background_color()`, `window(..).with_base_color()`) — and widgets with no colour knob leak through: the `checkbox` view's label is unreadable on a light background (`evidence/03-preferences.png`) because the label lives *inside* the masonry widget with no exposed property. |
| Window parenting (child/owner/transient) | **not-achievable** (approximated with `AlwaysOnTop`) | by-construction + observed | `WindowOptions` has `with_owner_window`/`with_menu` **only under `#[cfg(windows)]`**; there is no `parent`, `owner` or `transient_for` on macOS/Wayland, and `build_initial_attrs()` is `pub(crate)` so winit's own `with_parent_window` cannot be reached either. The only expressible knob is `with_window_level(WindowLevel::AlwaysOnTop)`, which *is* reactive. It buys "stays above its parent" and nothing else: the child floats above every other app too, is not hidden or minimised with the parent, and the offset-from-parent position is computed by hand from geometry the driver reads back. Side effect worth recording: an always-on-top window lands at `CGWindowLayer 3`, so `scripts/window-count.swift` (which filters `layer == 0`) cannot see it — hence `verify/allwin.swift`. |
| Close veto (CloseRequested → Save/Discard/Cancel) | **assembled** | synthetic-input + self-test | The view layer *cannot* veto: `WindowOptions::on_close` is `Fn(&mut State)` with no return value. The driver can, and the mechanism is inaction — `MasonryDriver::on_close_requested` calls `on_close`, re-runs the logic and only closes windows the logic stopped yielding, so **not forwarding the request leaves the window open**. Our wrapper intercepts the main window, shows `rfd` Save/Discard/Cancel, and on Cancel simply returns: `VETOED #1 CloseRequested on main window (Cancel)`, process and both windows alive. Save round-trips through app state (which owns the projects) and then sets a quit flag; Discard persists and calls `ctx.exit()`. |
| Quit semantics (main closes ⇒ app exits; child closes ⇒ app lives) | **built-in** | synthetic-input | `AppState::keep_running()` is checked after every close request. Child windows go to the stock driver, `keep_running()` stays true, app survives (⌘W on the inspector closed only it). ⌘W on the main window with inspector **and** preferences still open exited with **code 0** (`wait $!`). Caveat in xilem itself: `keep_running` is "currently only checked after a close request" (upstream TODO), so a state-driven quit needs the driver's `ctx.exit()`. |
| Native confirm dialog | **assembled** | observed | `rfd::MessageDialog` (`YesNo` for Delete, `YesNoCancelCustom("Save","Discard","Cancel")` for the close prompt), called synchronously from the main thread inside winit's dispatch — the shape `apps/xilem-tray` already proved safe. Two findings: an **unbundled** binary gets macOS's out-of-process `CFUserNotification` fallback (stderr: "called from main application thread, will block waiting for a response"; the alert is owned by `UserNotificationCenter`, and it *outlives a SIGTERM'd app* as a ghost window); passing `set_parent(main_handle)` moves it back in-process and parents it to the window. |
| Position/size persistence | **assembled** | synthetic-input | serde/serde_json to a JSON file next to the binary. Nothing in xilem can *read* a window's geometry — `WindowOptions` position/size are initial-only and changing them just logs "attempted to change position attribute after window creation, this is not supported". The driver reads `handle().outer_position()/inner_size()/scale_factor()` in `sync()` and on quit. Restore uses `with_initial_position`/`with_initial_inner_size` plus the saved `inspector_open`/`prefs_open`/`compact`/`theme`. Verified: quit wrote `main [10,33,720,330]`, relaunch **without** `WINDOWS_POS` (default 40,400) reopened at 10,33 with the preferences window restored. macOS clamps a requested position to the visible frame, so a restored rect is not always byte-identical. |
| Multi-monitor scale change | **not-verified** | by-construction | One display attached. Code path read: masonry_winit forwards `ScaleFactorChanged` to each window's own `RenderRoot` as `WindowEvent::Rescale` independently, and our geometry maths divides by the per-window `handle().scale_factor()`. Nothing in the app is global-DPI. |
| Per-window shortcuts (⌘W focused window only, ⌘, , ⌘⇧I) | **hand-rolled** | synthetic-input | No menu, accelerator or app-level key API in xilem 0.4. Handled in our `ApplicationHandler::window_event`: modifiers tracked from `ModifiersChanged`, ⌘, → `Ev::OpenPrefs`, ⌘⇧I → toggle, **⌘W → we synthesise `WindowEvent::CloseRequested` for the winit id the key arrived on**, which masonry then resolves to the right logical window — so "focused window only" is structural. `KEY Character("w") ... window=Some(Inspector)` in `run-log.txt` is the receipt. Esc is tested *before* the modal input block so it cancels the dialog from any window. Tab order inside the modal is masonry's own focus chain (not exercised here; SPEC-10 covers it). |

## Helper crates

- `masonry_winit =0.4.0` — direct dependency because xilem does not re-export
  `MasonryState` / `AppDriver` / `DriverCtx`. Same version as the one xilem
  pulls, so no duplicate in the tree. Without it, five rows above collapse to
  *not-achievable*.
- `rfd =0.15.4` — native message boxes. xilem has no dialog API whatsoever.
- `serde =1.0.228` + `serde_json =1.0.150` — the geometry/preferences file.

Tried and rejected: nothing was rejected, but two things were deliberately
*not* attempted after reading the source. (1) `objc2` + `raw-window-handle`
to call `-[NSWindow addChildWindow:ordered:]` / `beginSheet:` on the winit
windows — reachable in principle, but it would make the app's own dialog a
sheet by side-stepping the framework entirely, which answers a different
question than "what can xilem express". `rfd`'s `set_parent` gets the same OS
behaviour for the alert with a supported API, so that is what is measured.
(2) A `zstack` overlay "modal" inside the main window: it would block clicks
by hit-testing but not keyboard focus, and it is not a window, so the
window-count/parenting rows would become meaningless.

## Test scaffolding (declared, because it shows up in the evidence)

`WINDOWS_SELFTEST=1`, `WINDOWS_LOG=1`, `WINDOWS_STATE=path`,
`WINDOWS_POS=x,y[,w,h]`, `WINDOWS_TOP=1` (secondary windows to `AlwaysOnTop`),
`WINDOWS_SHEET=1` (pass the parent to rfd). They exist because six sibling
SPEC-9 apps from parallel test agents shared this screen, several parked at
the same default position with always-on-top windows; scripted clicks landed
in the wrong process until this app could be positioned and raised
deterministically. None of them change the app's behaviour under test.

## Where the time went

1. **Verification against six other GUI apps on one screen (~45 %).** Not a
   xilem problem, but it dominated: clicks swallowed by another agent's
   floating window, keystrokes arriving in the wrong process, another app's
   ghost `NSAlert` sitting over the click target, and `window-count.swift`
   silently ignoring always-on-top windows. Fixed by capturing per-CGWindowID,
   re-activating before every click, retrying until the app's own log showed
   a state change, and adding `verify/allwin.swift`.
2. **Reading masonry_winit/xilem source to find the seams (~20 %)** — which
   of `on_close`, `on_close_requested`, `keep_running` and `run_logic` can say
   "no", and where a winit `Window` is reachable.
3. **The modality/veto machinery (~15 %)** — the event filter, the
   winit-id→window map, the ordering between `run_logic` and `sync`.
4. **Persistence + shortcuts (~10 %)**.
5. **The actual app — six rows, four windows, a palette (~10 %)**. Genuinely
   quick; xilem's declarative window list is pleasant.

## Surprises

- **Good:** the close veto is *free*. Because `run_logic` closes only windows
  the app logic stopped yielding, "do nothing" already means "don't close".
  Most toolkits make you return `false` from a handler; here the handler
  cannot say anything, and that turns out to be the safe default.
- **Good:** one `AppState` for N windows removes an entire class of bugs. The
  inspector/main round-trip that this spec exists to stress took no code.
- **Good:** `WindowOptions` is *reactive* for title, level, resizability,
  min/max size and cursor — a window's attributes are diffed like any view.
- **Bad:** the reactive/initial split is a trap. Position and size are
  initial-only and silently degrade to a `tracing::warn!`, so "restore the
  window where the user left it" is impossible without the driver escape
  hatch, and "move this window" is impossible at all from app code.
- **Bad:** every OS-modality primitive is absent, *and* winit itself only has
  `with_owner_window` on Windows. A cross-platform xilem app cannot express
  "this dialog owns that window" today.
- **Bad:** an unbundled binary's `NSAlert` is hosted by `UserNotificationCenter`
  and survives the app being killed — a ghost dialog blocking the screen. Any
  scripted test suite for a Rust GUI app should expect this.
- **Ugly:** masonry 0.4's theme is dark-only and `DefaultProperties` cannot be
  swapped at runtime, so a Light/Dark toggle is 40 lines of per-widget colour
  properties — and widgets that own their text (`checkbox`) cannot be recoloured
  at all.

## The window model

A window in xilem 0.4 is a **view** — a value of type `WindowView<State>`
produced fresh every pass and reconciled by identity (`WindowId`, which you
allocate and store in your own state). It is not an entity, not a component,
and emphatically not a handle: nothing you hold lets you move, resize, focus,
raise or close it. Existence is a *consequence* of the app-logic function
still yielding that id, which makes opening and closing windows pure state
mutation (`state.inspector_open = true`) and makes "is this window open?" a
question about your model rather than the OS.

The consequence for two windows sharing mutable state is the best answer in
this corpus: there is nothing to write. The code is

```rust
fn app_logic(state: &mut AppData) -> impl Iterator<Item = WindowView<AppData>> {
    let main = window(state.ids[0], "Windows (xilem)", main_view(state));
    let inspector = state.inspector_open
        .then(|| window(state.ids[1], "Inspector", inspector_view(state)));
    once(main).chain(inspector)
}
```

and `inspector_view`'s `text_input(p.name.clone(), |s: &mut AppData, t| {
s.projects[s.sel].name = t; })` mutates the same `Vec` the main window's rows
are built from. One pass, one borrow, no synchronisation, no copy. The cost is
paid at the other end: because a window is only a view, anything the *OS*
knows about it — geometry, focus, modality, parenting, close vetoes — is
outside the model, and the only door is `AppDriver`/`DriverCtx` in an
externally-owned event loop. In this app that door is 464 lines wide.

## Approximated or skipped

- **Item 2, native modality:** shipped as an in-framework top-level window
  plus a hand-rolled input block. `rfd`'s parented alert is the only OS
  window-modal presentation reachable and is used for the two message boxes.
  A xilem-drawn sheet is not expressible.
- **Item 6, parenting:** `AlwaysOnTop` only (env-gated for the inspector and
  preferences, unconditional for the dialog). No hide/minimise-with-parent, no
  `transient_for`. Reason: `WindowOptions` has no cross-platform owner setter
  and its winit `WindowAttributes` builder is private.
- **Item 10, multi-monitor:** not verified — a single display was attached.
- **Item 11, Tab order inside the modal:** masonry's built-in focus chain,
  not separately exercised here (SPEC-10's ledger measures tab order properly).
- **Row double-click to edit** is detected by timing two `Button` actions
  (`Instant`, 450 ms) because masonry's `button` action carries no click count.
- **Item 9, "re-opens the inspector if it was open":** implemented and
  persisted; the relaunch shown in the evidence restored the *preferences*
  window (that was the one open at quit), which exercises the same path.
