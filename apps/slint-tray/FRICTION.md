# FRICTION — slint-tray (Tray Notes, Slint =1.17.1)

Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
Default slint features (winit backend, femtovg renderer, `system-tray` is a
default feature since 1.17) **plus `unstable-winit-030`** (needed only for
external file drop — see below).

Headline: Slint is genuinely strong here — tray, native menubar, dark mode,
multi-window and close-to-tray are first-party. The gaps are exactly the
classic ones (dialogs, notifications, global hotkey, image clipboard), all
fillable with the standard helper crates, plus one honest hole: external file
drop needs the unstable raw-winit escape hatch.

Verification caveat: this machine ran 7 parallel framework agents popping
windows, registering the *same* Cmd+Shift+9 hotkey, and opening identically
titled dialogs. Interactive tests were repeated until attribution was clean;
stray hide/show toggles in the logs are cross-app hotkey contention, which is
itself a real-world observation about global hotkeys.

Evidence status: the detailed interaction outcomes below are contemporaneous
narrative observations; no reusable interaction harness, raw session log or
tray screenshot was retained. The canonical build/binary/dependency figures
come from the serial measurement artifacts.

## Capability ratings

| Capability | Rating | Notes |
|---|---|---|
| tray | **built-in** | `SystemTrayIcon` element (new in 1.17, default feature): top-level component, `Menu`/`MenuItem` children, real `NSStatusItem`. Verified: icon in menu bar, menu items fire callbacks (Show/Hide toggled the window, Quit exited). One sharp edge: the platform handle is created ONCE from a change tracker whose eval closure is `\|_\| true` (i-slint-core `items/system_tray.rs`) — it never re-fires, so an icon bound to a property set from Rust after `new()` arrives too late and the tray fails with "Failed to create a rgba8 buffer from an icon image". The icon must be non-empty at creation: use `@image-url(...)`. |
| global_hotkey | **assembled** | `global-hotkey` crate (Carbon `RegisterEventHotKey`). Its event channel has no waker integration with Slint's loop → poll `GlobalHotKeyEvent::receiver().try_recv()` in a 50 ms `slint::Timer`. Verified: Cmd+Shift+9 toggles visibility while unfocused/hidden. Only one process on the machine can own the combo (contention observed with sibling test apps). |
| native_menubar | **built-in** | `MenuBar` element in the DSL → real macOS menu bar via muda. `@keys(Control + N)` maps Control→Cmd on macOS (verified in backend `muda.rs`: control→`SUPER`). muda auto-adds the standard app menu (About/Services/Hide/Quit ⌘Q) and macOS auto-injects Dictation/Emoji into a menu named "Edit". No standard *roles* though: Edit→Cut/Copy/Paste must be hand-wired — done by calling the `TextEdit`'s public `cut()/copy()/paste()/select-all()` functions from the menu callbacks, which targets that one editor, not "the focused widget". The bound ⌘V/⌘C accelerators are intercepted by the menu and re-routed into the same editor, so they still work — but only because we wired them back. Verified: menus present with ⌘N/⌘O/⌘S shown, Edit→Paste round-trip works. |
| dialogs | **assembled** | No Slint dialogs. `rfd::AsyncFileDialog` awaited inside `slint::spawn_local` — no event-loop freeze, no footgun hit as long as everything stays on the main thread. Verified: NSOpenPanel appeared over the app, loop stayed live, Open/Save flows work. Testing note: the panel is hosted out-of-process (`openAndSavePanelService`), so it is invisible to the app's accessibility tree and the menubar is blocked while it's open (app-modal). |
| clipboard_text | **built-in** | TextInput/TextEdit paste is native (backend uses copypasta internally). Verified via Edit→Paste round-trip: clipboard string landed in the editor. ⌘V works when the menu re-routes it (see menubar note). |
| clipboard_image | **assembled** | `arboard::Clipboard::get_image()` → `SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice` → `slint::Image::from_rgba8`. ~15 lines, zero drama. Verified: PNG placed on the clipboard via osascript rendered as a thumbnail ("Pasted image 64x64"). |
| file_drop | **assembled** (with unstable feature) | The specific iteration-2 open question is now answered: **`DropArea` does NOT receive external drops in 1.17.1.** The winit event loop never translates `WindowEvent::DroppedFile`/`HoveredFile` (verified: no such arms in `i-slint-backend-winit/event_loop.rs`), and `DataTransfer` has no file-path representation at all (only plain text + image; tracking issue slint#1967). Workaround: `unstable-winit-030` feature + `WinitWindowAccessor::on_winit_window_event` raw filter catching `DroppedFile(PathBuf)` — winit's macOS view registers as an `NSDraggingDestination`, so the event does arrive. Implemented + compiles + filter confirmed installed; a real Finder drag could not be scripted (AppleScript cannot synthesize drags), so end-to-end was not machine-verified. |
| notification | **assembled** | `notify-rust` (mac-notification-sys). Verified: `show()` returned Ok on save ("Note saved"); banner visibility is Focus/permission-gated (the machine was in a call; non-bundled binaries notify via the host terminal's identity). **Trap:** calling `show()` from inside a `slint::Timer` callback panics with "Recursion in timer code" and aborts the process (mac-notification-sys re-enters the main run loop). Fired from a `std::thread::spawn` instead. |
| dark_mode_live | **built-in** | Toggled OS appearance via osascript: the window was observed repainting light live without a restart. Fluent style follows the winit `ThemeChanged` event; the screenshots from that run were not retained. |
| multi_window | **built-in** | Second `Window` component (About), shown from Help menu; opened/closed independently of the main window (verified via accessibility window list). Gotcha: `preferred-width/height` on a Window whose root child is a layout are overridden by the layout's preferred size (About opened 261x100); use `min-width/height`. |
| close_to_tray | **built-in** | `Window::on_close_requested` returning `CloseRequestResponse::HideWindow` (which is even the default response). App stays alive via `run_event_loop_until_quit()`; a visible `SystemTrayIcon` also keeps a plain `run_event_loop()` alive by design. Verified: red close button → window gone, process alive, restored via hotkey and tray menu; tray Quit → clean exit. |

## Helper crates

| Crate | Version | Why |
|---|---|---|
| rfd | 0.15.4 | Native open/save dialogs (Slint has none). |
| arboard | 3.6.1 | Image clipboard (Slint clipboard is text-only). |
| notify-rust | 4.18.0 | System notifications (Slint has none). |
| global-hotkey | 0.7.0 | System-wide hotkey (Slint has none). |

Rejected: none. tray-icon and muda were NOT needed directly — Slint 1.17's
built-in `SystemTrayIcon` and `MenuBar` cover them (muda is an internal
dependency of the backend already).

## LoC

- Rust: 279 (`src/main.rs`) + 3 (`build.rs`)
- Slint DSL: 165 (`ui/main.slint`)
- Assets: 237-byte generated PNG tray icon (needed because the tray icon
  cannot be fed from Rust at runtime — see tray note)
- Total: 447 lines

## Measurements

- Canonical clean release build **58 s**; no-op incremental build **5 s**.
- Dependency graph: **317 unique crate names / 326 name-version entries
  including the app**.
- Binary: **16,654,032 bytes raw / 14,870,136 bytes (14.2 MiB) stripped**.

## Where the time went

1. ~30% tray-icon-from-Rust dead end: the silent single-shot handle creation
   (error only on stderr) + reading i-slint-core/system_tray.rs to understand
   why `set_app_icon()` after `new()` can never work.
2. ~25% verification choreography on a machine shared with 6 sibling agents
   (hotkey contention, focus stealing, identically-titled dialogs) — not
   Slint's fault, but the flakiness cost real time.
3. ~20% file-drop research: confirming from backend sources that DropArea is
   in-app only, then wiring the unstable winit filter.
4. ~10% the notify-rust "Recursion in timer code" abort + threading fix.
5. Rest: menubar/Edit-role wiring and dialogs (near-zero friction).

## Surprises

- Good: `SystemTrayIcon` + `MenuBar` being first-class DSL elements makes the
  "shell" 80% declarative; the tray menu is even reactive to property changes.
- Good: menu `activated` callbacks can call functions on window elements
  (`editor.paste()`) despite menus being lowered into separate item trees.
- Bad: external file drop is the one SPEC-4 capability that requires an
  *unstable* feature; `DataTransfer` cannot even represent a file path yet.
- Bad: two hard-abort-class integration traps in one app: tray icon set from
  Rust fails silently forever, and notify-rust inside a Timer callback
  panics the runtime ("Recursion in timer code").
