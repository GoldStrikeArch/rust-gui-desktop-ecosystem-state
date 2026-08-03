# FRICTION — Tray Notes (vizia =0.4.0), SPEC-4

Reference machine per spec. Built + verified on macOS 26.5.2 (M4 Pro, rustc
1.96.1): `cargo build --release` and `cargo build --locked --release` clean,
binary launched, alive past the 10 s bar, killed cleanly. Verification used
the env-var hooks below plus CGEvent synthetic input scoped to this app's own
window and menu-bar extra, with `CGWindowListCopyWindowInfo` used to assert
window state. Total LoC **720** (single `src/main.rs`; ~55 of those are the
opt-in `TRAY_SELFTEST*` hooks and their evidence prints). RSS ≈ **108 MiB**.

Self-test hooks (all opt-in, all off by default):

```
TRAY_SELFTEST=1            evidence lines on stderr
TRAY_SELFTEST_IMAGE=1      run the clipboard-image paste at startup
TRAY_SELFTEST_SAVE=<path>  write the note to <path> and fire the notification
TRAY_SELFTEST_SHOT=<path>  park the window at 120,120 and screencapture -R it
```

Canonical run (`TRAY_SELFTEST=1 TRAY_SELFTEST_IMAGE=1 TRAY_SELFTEST_SAVE=… TRAY_SELFTEST_SHOT=…`):

```
theme-changed: DarkMode
shell: tray + menubar + hotkey OK
paste-image: OK 600x300
file-saved: …/note2.txt
notification: OK
screenshot: saved …/tray-shot.png
```

## Capability ratings

| capability | rating | evidence | note |
|---|---|---|---|
| tray | **assembled** | observed + synthetic-input | vizia has nothing; `tray-icon 0.24` NSStatusItem, built on the **first tick of a 100 ms timer** so it happens after the winit run loop is up. vizia makes this unusually painless: `Model` has **no `Send` bound** and `Model::event` runs on the main thread, so the `!Send` `TrayIcon` just lives in app state — no boot-task dance, no `Option<Rc<…>>` gymnastics. Verified: the hand-drawn 22×22 icon is visible in the menu bar (menu-bar strip capture), the menu shows "Show/Hide window / New note / — / Quit", and clicking **New note** produced `new-note` in the log. |
| global_hotkey | **assembled** | synthetic-input | `global-hotkey 0.7` (Carbon `RegisterEventHotKey`, no accessibility permission). Verified twice in one run: ⌘⇧9 while the *About* window had focus removed the main window from the on-screen window list (`toggle-window: hiding`), and ⌘⇧9 again brought it back at the same position (`toggle-window: showing`). |
| native_menubar | **assembled** | observed + synthetic-input | vizia ships a `MenuBar` **view** (in-window, drawn by Skia) — not the macOS menu bar, so SPEC-4 needs `muda`, taken through the `tray_icon::menu` re-export on purpose: a separately resolved `muda` would own a *different* static `MenuEvent` channel and silently eat every click. Verified: menu-bar strip capture shows "vizia-tray  File  Edit", and ⌘N fired `new-note`. Cosmetic gap: the app-menu title comes from the process name (`vizia-tray`), not the submenu label, because the binary is unbundled. |
| dialogs | **assembled** | source-only | `rfd 0.17`, **blocking** API (`FileDialog::pick_file/save_file`) rather than the async one — vizia has no async executor to await on, and `Model::event` is already on the main thread, so a modal NSOpenPanel is the natural fit and needs no plumbing. Not driven under synthetic input (a modal panel on a shared desktop steals global focus); the non-interactive save path is exercised instead via `TRAY_SELFTEST_SAVE`. |
| clipboard_text | **built-in** | synthetic-input | vizia's default `clipboard` feature (copypasta) plus `TextEvent::{Cut, Copy, Paste, SelectAll}`, which the app routes from the Edit menu with `cx.emit_to(editor_entity, …)`. This is *better* than the iced equivalent: because vizia exposes clipboard actions as ordinary events addressed to a widget, the Edit menu is one `emit_to` per item instead of a hand-rolled clipboard task. Verified: ⌘A then ⌘C logged `edit-menu: select-all` / `edit-menu: copy`. The same reason as iced applies for **not** using `PredefinedMenuItem::cut/copy/paste`: their Cocoa responder-chain selectors are unimplemented by winit's NSView, and they swallow the key equivalents. |
| clipboard_image | **assembled** | self-test | `arboard 3.6` `get_image()` → `image 0.25` RGBA→PNG → `ContextProxy::load_image` → `Image` view thumbnail. Verified end-to-end: a 300×150-point (600×300-pixel) PNG placed on the clipboard was decoded and rendered as a thumbnail in the window capture (`paste-image: OK 600x300`). Trap: `Context::load_image` takes `&'static [u8]` and is **not reachable from an `EventContext`**; the reachable route at event time is `ContextProxy::load_image(String, &[u8], policy)`, which decodes to a Skia image and queues an internal load event. |
| file_drop | **built-in** | source-only | Genuinely zero helper code: winit's `DroppedFile` is surfaced by vizia as `WindowEvent::Drop(DropData::File(PathBuf))`, and the same `.on_drop(..)` modifier used for in-app card dragging receives it. Not interactively verified — a real Finder drag cannot be synthesized with CGEvents (no pasteboard drag session); the handler and the vizia_winit plumbing were read in source. |
| notification | **assembled** | self-test | `notify-rust 4`. **Two real macOS traps, both found by crashing:** (1) `mac-notification-sys` defaults to the bundle identifier `"use_default"`; from an unbundled cargo binary macOS 26 answers with a modal *"Choose Application — Where is use_default?"* panel that appears in front of the app **while `.show()` still returns `Ok`** (captured). Fixed with `notify_rust::set_application("com.apple.Terminal")`. (2) Sending from inside vizia's event dispatch **aborts the process**: `NotificationHandle::drop` calls NSUserNotification, which spins the Cocoa run loop and re-enters winit's event handler — winit panics with *"tried to handle event while another event is currently being handled"* inside a non-unwinding block. Fixed by sending from a plain background thread. |
| dark_mode_live | **built-in** | observed | Zero code: vizia resolves its light/dark theme from the OS and re-resolves on `WindowEvent::ThemeChanged(ThemeMode)`, which the app only *listens* to in order to display the mode. `theme-changed: DarkMode` is logged at startup and the window renders in the dark palette. The live OS toggle (`osascript … set dark mode to not dark mode`) was **not** performed: this is a shared desktop with other agents' apps and screenshot runs in flight, and SPEC-6's shared-desktop rule forbids toggling system-wide state. Labelled honestly as observed-at-startup, not observed-across-a-toggle. |
| multi_window | **built-in** | synthetic-input | `Window::new(cx, content)` inside a `Binding`, with `.on_close(..)`, `.title(..)`, `.inner_size(..)`. Verified: pressing "About" created a second 360×212 window alongside the 500×452 main one (both listed by `CGWindowListCopyWindowInfo`, `about-window: opened`), and it closes independently. |
| close_to_tray | **assembled** | synthetic-input | The nicest structural result of this app. vizia's event manager visits **models on an entity before the view on that entity**, and the app model sits on the window entity — so the model sees `WindowEvent::WindowClose` first, calls `meta.consume()`, and answers with `WindowEvent::SetVisible(false)`. vizia's own `Window` view never runs its close path, so `should_close` is never set and the run loop is never asked to exit. Verified: clicking the red traffic light removed the window from the on-screen list (`close-to-tray: hidden`) while the process kept running, and ⌘⇧9 brought it back. vizia exposes no *getter* for window visibility, so the app tracks the flag itself. |

## Helper crates

- `tray-icon 0.24` — NSStatusItem tray icon + menu; also the deliberate source of the `muda` re-export.
- (`muda` — pulled *through* tray-icon on purpose; see native_menubar.)
- `global-hotkey 0.7` — system-wide ⌘⇧9 via Carbon.
- `rfd 0.17` — native NSOpenPanel/NSSavePanel (blocking API).
- `arboard 3.6` — image clipboard (vizia's clipboard integration is text-only).
- `image 0.25` (`default-features = false`, `png`) — clipboard RGBA → PNG for `load_image`; the 22×22 tray icon is generated in code, no asset file.
- `notify-rust 4` — system notification (with the two workarounds above).

**Tried and rejected:** `PredefinedMenuItem::{cut, copy, paste, select_all}`
(dead selectors + swallowed key equivalents under winit — same finding as the
iced cohort); `Context::load_image` (not reachable from `EventContext`);
`cx.schedule_emit` for the screenshot delay (never delivered in this app; a
plain `std::thread::spawn` + `sleep` is used instead and is also what the
notification needs).

## Where the time went

1. **The two notify-rust traps** (~40 %): a modal "Choose Application" panel
   with a successful return value, then a non-unwinding abort deep inside
   winit's event handler. Neither is discoverable from the crate docs.
2. **Getting the clipboard image onto a Skia surface**: `Context::load_image`
   looks like the API, is `&'static [u8]`, and is unreachable at event time.
3. Everything vizia itself provides — close interception, file drop, second
   window, theme following, clipboard events addressed to a widget — was
   fast. Roughly 30 minutes for four of SPEC-4's eleven capabilities.

## Surprises

- Good: `Model` has no `Send` bound and models are visited before views on
  the same entity. Those two facts alone give a clean home for `!Send` OS
  handles and make close-to-tray a three-line intercept rather than a
  framework-level opt-out flag.
- Good: file drop and in-app drag & drop share one `on_drop` modifier
  (`DropData::File` vs `DropData::Id`).
- Good: clipboard actions are addressable events (`cx.emit_to(editor,
  TextEvent::Paste)`), which makes an Edit menu trivial to wire — no
  focus-dispatch machinery, though it does mean the menu targets one
  hard-coded widget.
- Bad: no window-visibility getter, no window-id/screenshot API (the
  screenshot hook has to park the window at a known position and shell out
  to `screencapture -R`).
- Bad: like the rest of the cohort, an unbundled cargo binary keeps a Dock
  icon and there is no reachable `ActivationPolicy::Accessory`, so "hidden
  to the tray" still shows in the Dock.
