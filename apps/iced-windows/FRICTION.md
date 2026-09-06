# FRICTION — Windows (iced =0.14.0)

Reference: `apps/SPEC-9.md`. Built and verified on macOS 26.5.2 (M4 Pro,
rustc/cargo 1.96.1). `cargo build --release` clean from scratch in **32.4 s**;
`cargo build --release --locked` afterwards succeeds unchanged. Binary
**10.8 MB**. Source **1279 lines** in one `src/main.rs` (1087 non-comment) —
over the ~700-line guide; roughly 130 of those lines are the scripted
self-test, ~90 the macOS `objc2` module and ~60 persistence, none of which
exist in the other iced apps in this corpus.

Verification lives in `evidence/`: `selftest-log.txt` (`WINDOWS_SELFTEST=1`,
**pass=18 fail=0**, exit 0), `log.txt` (every synthetic-input step with the
command used), and seven screenshots. **Read the header of `evidence/log.txt`
first**: this run shared one desktop with several other agents driving their
own GUI apps, which stole front-most status, occluded windows and twice
delivered a stray ⌘W into this process. Steps that could not be driven through
that contention are marked *self-test* rather than *synthetic-input*.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** | synthetic-input | `iced::daemon` + `window::open/close` + `window::gain_focus` for the singleton case. `view`/`title`/`theme` are `Fn(&State, window::Id)`, so a second window is a `match` in `view`, not a new object. 1 → 2 → 3 windows confirmed with `window-count.swift`. |
| Modal dialog — kind achieved | **hand-rolled (OS window-modal via objc2)** | synthetic-input | **iced 0.14 has no modality API at all** — nothing in `window::Settings`, `window::Action` or the widget tree says "modal". What it *does* have is `iced::window::run(id, f)`, which hands a `&dyn Window` (`HasWindowHandle`) to a closure that iced_winit runs on the main thread. 30 lines of `objc2` turn that into `NSWindow beginSheet:completionHandler:` and the dialog becomes a **real macOS sheet** (AppKit even strips the winit title bar). Status bar reads `modality: OS window-modal (NSWindow beginSheet:)`. |
| Parent blocked while modal | **built-in (OS, once the sheet exists)** | synthetic-input | Six CGEvent clicks on parts of the parent *not* occluded by the sheet (the Delete button above it, a row to its left) changed nothing: `07-modal-blocks-parent.png` is identical to `06-modal-sheet.png` and no `delete-requested` line was ever logged. Two weaker fallbacks were implemented and are selectable with `WINDOWS_NO_SHEET=1`: `window::enable_mouse_passthrough` on the parent (= winit `set_cursor_hittest(false)` = `setIgnoresMouseEvents:`) + `Level::AlwaysOnTop` on the dialog — that blocks the mouse at OS level but leaves the parent taking keystrokes; and `opaque(..)` inside a `stack` (iced's own documented modal pattern) which blocks mouse events *inside iced only* and never blocks keyboard. The dim scrim this app draws over the parent is deliberately **not** `opaque`, so the click test measures the OS and not the scrim. |
| Focus returns to parent after modal | **built-in** | observed | `window::gain_focus(main)` in the dismiss path; with the sheet, AppKit does it anyway. |
| Shared state across windows (live, bidirectional) | **built-in (free)** | self-test + observed | The headline: a daemon has exactly **one** `State` and **one** `update`. Two windows cannot hold different copies because there is nowhere to put a second copy. The inspector's `text_input` writes `self.projects[i].name` and the main list reads it — zero plumbing, no signals, no channels. Selection → inspector confirmed by screenshot, inspector → main by self-test. |
| Cross-window message (Ping/Pong) + wake mechanism | **built-in (free)** | self-test | `Message::Ping` from the inspector's button lands in the same `update` as everything else and increments the counter the main window renders. There is no wake mechanism to describe *because there is no second event loop*: iced's runtime is per-application, not per-window. Pong (300 ms inspector flash) needed a timer, and iced's default executor has none — `iced::time::every` requires the `smol`/`tokio` feature — so it is `Task::future(async { thread::sleep(300ms) })`, which the thread-pool executor handles. |
| Theme/layout change applied to all windows | **built-in** | synthetic-input | `.theme(|state, _id| …)` returning `Option<Theme>` — one closure for every window, and returning `None` hands the window back to iced's system light/dark resolution. Clicking "Light" in Preferences re-themed the main window *and* the inspector in the same frame (`09-theme-and-compact.png`). "Compact rows" is the same mechanism but is *self-test*-verified only: its `toggler` never responded to CGEvent clicks (six y positions across its bounds) while `radio` and `button` in the same window did — unexplained, and the one thing in this app I could not account for. |
| Window parenting (child/owner/transient) | **hand-rolled (objc2)** | observed | `window::Settings` has **no** `parent`, `owner` or `transient_for` field (checked in `iced_core-0.14.0/src/window/settings.rs`); `platform_specific` on macOS offers only `title_hidden`, `titlebar_transparent`, `fullsize_content_view`. winit itself has `with_parent_window`/`with_owner_window`, but iced never surfaces them. `NSWindow addChildWindow:ordered:` through `window::run` gives the real thing: moving the parent with AX moved the inspector by exactly the same delta, and `screencapture -l <parent>` captures parent+child as one group. |
| Close veto (CloseRequested → Save/Discard/Cancel) | **built-in** | synthetic-input | `exit_on_close_request: false` + `window::close_requests()`. Cancel → "window survives", process and window still there; Save/Discard → `iced::exit()`, `exit=0` captured in `evidence/exitcode.txt`. |
| Quit semantics (main closes ⇒ app exits; child closes ⇒ app lives) | **built-in** | synthetic-input | A daemon stays alive with zero windows, so "child closes ⇒ app lives" is the default and "main closes ⇒ exit" is the part you write (`iced::exit()`). |
| Native confirm dialog | **assembled (rfd)** | synthetic-input | iced has no dialog API. `rfd::AsyncMessageDialog` + `Task::perform` composes with no friction (same finding as `apps/iced-tray`), and `MessageButtons::YesNoCancelCustom("Save","Discard","Cancel")` gives a genuine three-button `NSAlert` (`10-close-veto-dialog.png`). *Verifier note (2026-08-30): for this unbundled binary both rfd alerts (Delete confirm, Save/Discard/Cancel) are presented **out-of-process** by `UserNotificationCenter` (macOS's `CFUserNotification` fallback for an app-modal `NSAlert runModal` from an unbundled binary — rfd itself has no such path — the same `CFUserNotificationDisplayAlert: called from main application thread…` stderr line `apps/xilem-windows` records) — a new window owned by that process appears on every Delete click and every dirty ⌘W, it is invisible to per-process AX scripting, and it survives the app being killed as a ghost dialog. Only `set_parent` (not wired here) gives an in-process sheet.* Note: rfd's macOS backend will present the alert as a *sheet* if given a parent through `set_parent(&impl HasWindowHandle)` — reachable from iced via `window::run`, but only inside a `Send + 'static` closure, so it was not wired up here. |
| Position/size persistence | **assembled** | synthetic-input | No iced API for it. Positions/sizes are collected from `window::Event::Moved`/`Resized` through `event::listen_with`, written to `windows-state.json` next to the binary with serde_json, and replayed into `window::Settings { position: Position::Specific(..), size }`. Round-trip is exact for both windows and the inspector re-opens by itself. (Two earlier round-trips looked broken — a saved y of 430/500 always came back as 386 — until it turned out macOS was clamping the window above the Dock.) |
| Multi-monitor scale change | **not-verified** | not-verified | One display attached. `window::Event::Rescaled(f32)` is subscribed and logged (every window prints `scale_factor=2` at open); `window::scale_factor(id)` exists as a `Task`. Nothing was dragged between displays. |
| Per-window shortcuts (⌘W focused window only, ⌘, , ⌘⇧I) | **hand-rolled** | synthetic-input | `event::listen_with(|event, _status, window| …)` reports the `window::Id` each event landed in — that id *is* iced's only per-window key routing. ⌘W turned out to be handled by AppKit's own close-window key equivalent before iced sees it (the window arrives as a `close_requests()` id and the app's own `cmd-w` trace does not fire) — the same class of trap as the muda predefined-menu-item finding in `apps/iced-tray`, except here the default behaviour is the one you want. ⌘, and ⌘⇧I are matched by hand. |

## Helper crates

- `rfd =0.17.2` — native `NSAlert` message boxes (Delete confirm, Save/Discard/Cancel). iced has no dialogs.
- `serde =1.0.229` + `serde_json =1.0.151` — the window-geometry file.
- `objc2 =0.6.4` (macOS only) — `addChildWindow:ordered:`, `beginSheet:completionHandler:`, `endSheet:`. The *only* way to reach OS parenting/modality from iced 0.14.
- Tried and rejected: `iced::time::every` for the Pong flash (would have forced a `smol`/`tokio` executor feature for a single 300 ms delay); `opaque(..)` as the primary modality (kept as a documented fallback because it blocks mouse but never keyboard).

## Where the time went

1. **Deciding what "modal" can even mean in iced** and then proving the strongest option works. Reading `iced_core`'s `window::Settings` to establish that no parent/modal field exists, finding `window::run`'s `&dyn Window` escape hatch, then getting `beginSheet:` to accept a live winit window.
2. **`addChildWindow:` and `beginSheet:` do not compose.** Presenting a sheet on a parent that already owns a child window *destroys the child* when the sheet is dismissed — iced reported a `Closed` event for a window the app never closed, and the singleton inspector silently vanished. Fix: detach the child (`removeChildWindow:`) for the lifetime of the sheet and re-attach afterwards. This cost more time than writing the whole Preferences window.
3. **Verifying anything on a contended desktop.** Half the elapsed time went into activation loops, per-window-id screenshots and re-running steps that other agents' apps had hijacked. The in-app self-test (`WINDOWS_SELFTEST=1`) ended up being the load-bearing evidence.
4. Coordinate archaeology for synthetic clicks (logical vs. captured-image scale, and macOS clamping windows above the Dock).

## Surprises

- **Good:** shared state and cross-window messages are *free*, to the point where the spec's questions ("wake mechanism?", "one source of truth?") do not apply. There is one loop, one state, one message enum.
- **Good:** `window::run` exists at all. It is the difference between "iced cannot do OS modality" and "iced can, with 30 lines of objc2".
- **Good:** `.theme()` returning `Option<Theme>` — `None` means "let the shell decide", so Light/Dark/System is three lines.
- **Bad:** widget operations are applied to **every** open window's interface in one pass (`iced_winit-0.14.0/src/lib.rs:1742-1750`). `operation::focus_next()` therefore walks the concatenation of all windows and will happily tab out of a dialog into the main list. Tab order inside the dialog had to be hand-rolled from explicit `operation::focus(id)` calls.
- **Bad:** `window::Event::Moved`/`Resized` are the only way to learn a window's geometry without an async round-trip, and `Position::Specific` is the only way to set it — persistence is entirely on the application.
- **Neutral but sharp:** iced's `text_input` swallows Escape (already known from `apps/iced-board`), so the modal's Esc handling has to come from `event::listen_with`, which ignores capture status.

## Window model

**A window in iced 0.14 is a key, not an entity.** `window::Id` is an opaque
handle; there is no `Window` struct you own, no per-window state, no per-window
event loop and no per-window widget tree that outlives a frame. `iced::daemon`
takes one `State`, one `update(&mut State, Message)` and closures
`view(&State, Id) -> Element`, `title(&State, Id)`, `theme(&State, Id)`. Every
window is a *projection* of the same state, chosen by matching on the id.

That makes the hard half of this spec trivial and the easy half awkward. Two
windows seeing the same mutable state is not a problem to solve — it is the
only thing that can happen, because there is nowhere to put a second copy:

```rust
Message::InspectorName(value) => {            // typed in the inspector window
    self.projects[self.selected].name = value; // read by the main window's view
    self.dirty = true;
    Task::none()
}
```

No signal, no channel, no proxy, no wake. Conversely, anything the OS thinks
of as a property *of a window* — modality, ownership, position memory, "which
window has focus", "which window did this key go to" — has to be reconstructed
by the application from `window::Id`s, `event::listen_with`'s third argument,
and (for parenting and modality) a raw `NSWindow` pointer fished out of
`window::run`. iced models windows as *views of an application*; macOS models
them as *objects with relationships*, and every capability in this spec that
failed or needed objc2 sits exactly on that mismatch.

## Approximated or skipped

- **Multi-monitor / DPI (spec 10)** — one display available; `Rescaled` handling is in the code and logged, but nothing was dragged to a second screen. Marked *not-verified*.
- **Compact-rows toggle** — the behaviour works (self-test) but could not be driven with synthetic clicks; recorded honestly rather than claimed.
- **Pong flash screenshot** — the 300 ms inspector flash is implemented and fires, but a screenshot within the 300 ms window was not captured on the contended desktop; the Ping direction is self-test-verified.
- **`WINDOWS_DIRTY=1`** is a verification hook (starts the app with the unsaved-changes flag set) so the close-veto path can be driven without keyboard input; it is not part of the spec'd behaviour.
