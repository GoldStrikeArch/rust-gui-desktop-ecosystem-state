# FRICTION — Tray Notes (iced 0.14.0), SPEC-4

Reference machine per spec. Verified on macOS with `cargo run --release`
plus scripted interaction (System Events menu clicks, synthetic keystrokes,
window captures). Total LoC: **682** (single `src/main.rs`; ~60 of those are
verification-only self-test hooks and PNG encoding).

## Capability ratings

| capability | rating | note |
|---|---|---|
| tray | **assembled** | iced has nothing; `tray-icon` 0.24 NSStatusItem created lazily via a boot `Task` (must happen on the main thread *after* the run loop starts — tray-icon #90). Works cleanly with iced's winit loop because `boot`/`update`/`view` all run on the main thread and `State` needs no `Send`, so the `!Send` `TrayIcon` lives in app state. Menu events polled off a global crossbeam channel every 100 ms. Verified: tray menu clicked via AX (`menu bar 2`), Show/Hide + New Note + Quit all fired. |
| global_hotkey | **assembled** | `global-hotkey` 0.7 (Carbon `RegisterEventHotKey`, no accessibility permission). Same pattern: create in `update`, poll the channel. Verified: synthetic ⌘⇧9 toggled the window both directions, including while hidden/unfocused. |
| native_menubar | **assembled** | `muda` (via the `tray_icon::menu` re-export — a separately-resolved muda would have a *different* global `MenuEvent` channel and silently eat events). `init_for_nsapp()` after loop start. File ⌘N/⌘O/⌘S verified via AX menu clicks and accelerators. **Gotcha (headline):** `PredefinedMenuItem::cut/copy/paste/select_all` use Cocoa responder-chain selectors that iced's winit NSView doesn't implement — the items no-op AND their key equivalents swallow ⌘X/⌘C/⌘V/⌘A before iced's own text_editor bindings see them (verified: ⌘V pasted nothing). Fixed with custom items routed by hand to the editor + `iced::clipboard` tasks. Undo/redo omitted — iced text_editor has no undo stack. Consequence: menu Edit actions are hard-wired to ONE widget; there is no focus-based dispatch. |
| dialogs | **assembled** | `rfd` 0.17 `AsyncFileDialog` composes directly with `Task::perform`; real NSOpenPanel/NSSavePanel (verified visually; Escape cancels, app unaffected). Zero friction — best-integrated helper of the lot. |
| clipboard_text | **built-in** | `iced::clipboard::read/write` tasks + text_editor's internal ⌘C/⌘V bindings. Verified end-to-end: paste inserted clipboard text, Select All + Copy round-tripped back to `pbpaste` (after the muda key-equivalent fix above; with predefined Edit items installed the built-in path is unreachable). |
| clipboard_image | **assembled** | `arboard` 3.6 `get_image()` → RGBA → `image::Handle::from_rgba` thumbnail (needs the non-default `image` feature). Verified: PNG placed on clipboard via osascript was read and rendered ("paste-image: OK"). No main-thread issues calling it inside `update`. |
| file_drop | **built-in** | `window::Event::FileDropped(PathBuf)` arrives via `event::listen_with` (winit-backed). Handler loads `.txt` into the editor. NOT interactively verified — a real Finder drag can't be synthesized (CGEvents can't fabricate a drag pasteboard session); event plumbing confirmed in iced_winit source. |
| notification | **assembled** | `notify-rust` 4.11 (mac-notification-sys). `.show()` returned Ok from the unbundled binary ("notification: OK" after save). Caveat: macOS attributes/gates notifications by bundle — from a bare cargo binary the banner may be suppressed until the user approves the host app in System Settings; not treated as a framework failure. |
| dark_mode_live | **built-in** | Genuinely zero code in 0.14 (dark-mode PR #3051): with no `.theme()` set, the shell resolves `Theme::default(system mode)` per window and re-resolves on the OS `ThemeChanged` event. Verified live via `osascript … set dark mode to not dark mode` while running; `iced::system::theme_changes()` subscription used only to display the mode in the status bar. |
| multi_window | **built-in** | `iced::daemon` + `window::open/close`, per-window `view`/`title` closures. About window opened next to the main one, captured, closed with ⌘W; main window unaffected. Straightforward. |
| close_to_tray | **built-in** (semantics) / assembled (the tray part) | `exit_on_close_request: false` delivers `window::close_requests()`; we answer with `window::set_mode(id, Mode::Hidden)`. The daemon keeps the process alive with zero visible windows; tray Quit → `iced::exit()`, app-menu ⌘Q → native `terminate:`. Verified: ⌘W hid ("close-to-tray: hidden"), tray/hotkey re-showed, tray Quit exited the process. |

## Helper crates (all recorded, none rejected)

- `tray-icon` 0.24.1 — tray icon + menu; also the source of the muda re-export.
- (`muda` 0.19.3 — pulled *through* tray-icon on purpose, see channel-sharing note.)
- `global-hotkey` 0.7.0 — system-wide ⌘⇧9.
- `rfd` 0.17.2 — native open/save panels.
- `arboard` 3.6.1 — image clipboard (iced clipboard is text-only).
- `notify-rust` 4.18 — notifications.
- `png` 0.18 + `smol` 2 — verification only (self-test screenshot encoding + async delay; smol already in tree via iced's executor feature).

Nothing tried-and-rejected; the first-choice stack worked. The one *pattern*
rejected: `PredefinedMenuItem` clipboard roles (see native_menubar).

## Where the time went

1. **The muda predefined-Edit-menu trap** (~1/4 of the effort): ⌘V silently
   dead, no error anywhere — the menu eats the key equivalent, the selector
   goes nowhere, iced never sees the keystroke. Diagnosis required a
   process-of-elimination interactive test; fix required hand-routing four
   menu items to one hard-coded widget.
2. **Ordering constraints**: everything shell-related must be created on the
   main thread after the run loop starts; the trick is a `Task::done(SetupShell)`
   from `boot` (iced conveniently runs update on the main thread — this whole
   app is impossible if a framework runs update off-thread).
3. **Event injection**: three separate crossbeam channels (menu, tray, hotkey)
   with no waker integration → a 100 ms polling subscription, which needs the
   `smol`/`tokio` feature because `time::every` doesn't exist on the default
   thread-pool executor.
4. Cosmetic gap, not fixed: the app keeps a Dock icon and doesn't respond to
   Dock-icon reopen when hidden (winit exposes no `applicationShouldHandleReopen`;
   ActivationPolicy::Accessory isn't reachable through iced).
