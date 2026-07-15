# SPEC-4: "Tray Notes" — OS shell integration test

Iteration 3. This app tests the layer the RCN initiative calls the "hard
parts": everything AROUND the window. Helper crates are expected and are
themselves the finding — record every one and why in FRICTION.md.

## Functional requirements

1. **Window** titled `Tray Notes (<framework>)`, ~500×420, with a multiline
   plain-text editor filling most of it.
2. **System tray icon** (macOS: menu-bar extra) with a menu: Show/Hide
   window, New note, Quit. **Closing the window hides it to the tray** (app
   keeps running); Quit (tray or menubar) actually exits.
3. **Global hotkey** Cmd+Shift+9 toggles window visibility even when the app
   is unfocused/hidden.
4. **Native menubar** (macOS: the real top menu bar): File → New, Open…,
   Save…, Quit with standard accelerators (⌘N/⌘O/⌘S/⌘Q); Edit → standard
   clipboard roles where the framework allows.
5. **Native file dialogs**: Open…/Save… read/write plain `.txt`.
6. **Clipboard**: normal text paste into the editor must work; plus a
   "Paste image" button — if the clipboard holds an image, render a
   thumbnail below the editor (tests image clipboard, e.g. arboard).
7. **File drop**: dropping a `.txt` from Finder onto the window loads it.
8. **System notification** "Note saved" fires on save.
9. **Live dark-mode reaction**: UI follows the OS theme without restart.
10. **Second window**: an About window (or second note) can be opened and
    closed independently — multi-window test.

## Implementation rules

- Independent crate at `apps/<framework>-tray/`, package `<framework>-tray`,
  `cargo run --release` works. Same pinned framework version as iteration 1
  (crib setup from `apps/<framework>-app/`).
- Any Rust helper crate is allowed (rfd, arboard, notify-rust, muda,
  tray-icon, global-hotkey, framework plugins…) — record each + why. For
  webview frameworks, no external JS libraries.
- **Fallback rule** applies aggressively: if a capability cannot work with
  this framework's event loop / architecture, ship the closest approximation
  and record rating **not-achievable** with the precise technical reason —
  that is exactly the data this round exists to collect. Budget guide: ~3
  serious attempts per capability, then document and move on.
- Verify on macOS: build release, launch ~10 s, confirm alive, kill. Verify
  interactively whatever is scriptable (e.g. `osascript` for theme toggle:
  `tell app "System Events" to tell appearance preferences to set dark mode
  to not dark mode`); a window screenshot (`screencapture`) of the running
  app is encouraged. Note macOS-permission-gated items (e.g. notifications
  may need approval) instead of fighting them.

## FRICTION.md (required)

Rating (built-in / assembled / hand-rolled / not-achievable) + 1–3 sentence
note per capability: tray, global_hotkey, native_menubar, dialogs,
clipboard_text, clipboard_image, file_drop, notification, dark_mode_live,
multi_window, close_to_tray. Plus: helper crates used (and any that were
tried and REJECTED — with reason), total LoC, where the time went.

## Reference machine

Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
