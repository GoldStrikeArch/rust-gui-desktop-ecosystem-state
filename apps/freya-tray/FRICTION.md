# FRICTION — Tray Notes (freya =0.4.0), SPEC-4

Reference machine per spec. Verified on macOS 26.5.2 with the release binary
plus scripted interaction: `System Events` AX clicks on both menu bars,
synthetic CGEvent mouse input, synthetic keystrokes, `osascript` dark-mode
toggle, and window-scoped screenshots. Total LoC: **865** (single
`src/main.rs`; ~90 of those are the multi-line text editor that Freya does not
ship, and ~60 are verification-only self-test hooks).

## Capability ratings

| capability | rating | note |
|---|---|---|
| tray | **built-in** | `LaunchConfig::with_tray(builder, handler)` behind the `tray` feature. Freya owns the tray-icon/muda **global** `TrayIconEvent`/`MenuEvent` handlers and forwards both into one `TrayEvent` callback, so the app never links its own muda copy — the channel-splitting trap other frameworks hit cannot happen here. The builder closure is invoked on the main thread after the loop starts, which is exactly what tray-icon requires. Verified: AX click on `menu bar 2` → Show/Hide hid the window, New note re-showed it. |
| global_hotkey | **assembled** | `global-hotkey` 0.7 (Carbon `RegisterEventHotKey`, no accessibility permission). Freya has nothing here. Gotcha: the channel reports **both** `Pressed` and `Released`, so an unfiltered toggle nets out to no change — filter on `HotKeyState::Pressed`. Verified: ⌘⇧9 hid the window (it disappeared from `CGWindowListCopyWindowInfo`) and a second press restored it, from an unfocused state. |
| native_menubar | **assembled** | muda via `freya::tray::menu` (i.e. `tray_icon::menu`), `Menu::init_for_nsapp()` called from the root component's first render — a component render *is* main-thread-after-loop-start, so no boot-task dance is needed. Verified: `menu bar 1` reports `Apple, freya-tray, File, Edit`; AX clicks on File→New/About window and Edit→Select All/Copy all fired. **Two traps, see below.** |
| dialogs | **assembled** | `rfd` 0.17 `FileDialog::pick_file()/save_file()`. Verified: clicking Open… produced a real NSOpenPanel (window titled "Open" in the process's window list) and Escape dismissed it. The synchronous rfd API blocks the UI thread while the panel is up; that matches macOS app-modal behaviour, but `AsyncFileDialog` + `spawn` would be the non-blocking form. |
| clipboard_text | **built-in** | `use_editable` implements ⌘C/⌘X/⌘V/⌘A itself on top of `freya-clipboard`. Verified end-to-end: typed text → Edit▸Select All → Edit▸Copy → `pbpaste` returned `clipboard roundtrip test`. |
| clipboard_image | **assembled** | Freya's built-in `Clipboard` (freya-clipboard → copypasta) is **text-only**, so `arboard` 3.6 `get_image()` → `ImageHandle::from_rgba(w, h, Bytes, AlphaType::Unpremul)` → `image()` element. Verified via `TRAY_SELFTEST_IMAGE`: `paste-image: OK 700x632` and the thumbnail renders (see the self-test screenshot). Naming `AlphaType` requires the `engine` feature even though the `image()` element itself does not. |
| file_drop | **built-in** | `.on_file_drop(\|e: Event<FileEventData>\| …)` is a first-class element event with a `PathBuf` payload — no winit plumbing, and it can be attached to any sub-tree rather than the whole window. **Evidence: source-only** — a real Finder drag cannot be synthesised with CGEvents (no drag pasteboard session), so the handler was not exercised. |
| notification | **assembled** | `notify-rust` 4.18. `.show()` returns `Ok` from an unbundled binary (`notification: OK`). **Trap:** with no bundle identifier, mac-notification-sys asks the OS to choose one and pops a modal **"Choose Application"** panel that blocks the UI thread indefinitely (observed — it froze the self-test until dismissed). `notify_rust::set_application("com.apple.Terminal")` before `.show()` makes it non-interactive. |
| dark_mode_live | **built-in** | `Platform::get().preferred_theme` is a reactive `State<PreferredTheme>`; a `use_side_effect` swaps `light_theme()`/`dark_theme()` into the component theme context and repaints. Verified live with `osascript … set dark mode to not dark mode` while running: `theme-changed: Light` then `theme-changed: Dark`. Zero platform code. |
| multi_window | **built-in** | `Platform::get().launch_window(WindowConfig::new(about_app)…).await` returns the new `WindowId`; `close_window(id)` closes it. Verified: `About — Tray Notes (freya)` appeared alongside the main window, independently. |
| close_to_tray | **assembled** | `WindowConfig::with_on_close(…) -> CloseDecision::KeepOpen` plus `LaunchConfig::with_exit_on_close(false)` (Freya also refuses to exit while a tray handler is registered). The hook receives a `RendererContext`, but `AppWindow`'s `window` field is `pub(crate)`, so **the hook cannot hide the window itself**; it sets a flag that the UI poll loop turns into `Platform::with_window(None, \|w\| w.set_visible(false))`. Verified: clicking the red close button removed the window from the window list while the process stayed alive. |

## The headline trap: muda menu items are use-after-free by default

muda stores a **raw `*const MenuChild`** inside each `NSMenuItem` and does not
retain it (there is a `FIXME: Use Rc or something else` at that exact spot in
muda 0.17). The idiomatic Rust shape —

```rust
let menu = Menu::new();
menu.append_items(&[&MenuItem::with_id(ID, "New", true, accel), …]);
menu.init_for_nsapp();       // items dropped here
```

— leaves every one of those pointers dangling. The build is clean, the menu
*renders* correctly, and then the **first click on any item** reads freed
memory. Observed symptom: the freed `MenuChild` was reinterpreted as a
`PredefinedMenuItemType::About` carrying a zero-sized icon, panicking inside
muda's PNG encoder (`FormatError { inner: ZeroWidth }`) — a message that points
at an About panel this app doesn't have. The fix is to keep every `Menu`,
`Submenu` and item alive for the process lifetime (this app parks them in a
`thread_local` `Vec<Box<dyn Any>>`). The same applies to the tray menu built
inside `LaunchConfig::with_tray`.

This cost the single largest block of time in the app, and it was only
diagnosable because of the second trap:

## Second trap: release-mode panics become a modal dialog, not a backtrace

`freya_winit::launch` installs, **only in release builds**, a panic hook that
shows an `rfd::MessageDialog` titled "Fatal Error", *then* chains to the
previous hook, *then* `exit(1)`. So in release a panic produces a frozen window
and a modal alert with nothing on stderr — and if the alert is behind another
window, the app just looks hung. Diagnosis required a debug build (where the
hook is not installed) plus an app-level `set_hook`. This app keeps a
`#[cfg(debug_assertions)]` hook for that reason.

Related: `State::peek()`/`read()` return a `Ref`; holding one across a `write()`
panics — and therefore also freezes the app behind that dialog.

## Third trap: `PredefinedMenuItem` clipboard roles

Not used here, on purpose. `use_editable` implements ⌘X/⌘C/⌘V/⌘A itself, and a
menu key equivalent always wins over the focused view, so predefined Edit roles
would both no-op (the winit NSView implements none of those Cocoa selectors)
*and* shadow the editor's own bindings. The Edit menu is therefore four custom
items whose handlers replay the equivalent `EditableEvent::KeyDown` into the
editor — which is 4 lines and keeps both paths working (verified by the
`pbpaste` round-trip).

## The bridge problem

`with_tray`'s handler runs on the renderer thread **outside** any component
scope, so it cannot touch signals or call `Platform::get()`. It pushes menu ids
into a `static Mutex<VecDeque<String>>` that the UI drains on an 80 ms timer —
the same loop that polls `global-hotkey`'s crossbeam channel and the
close-to-tray flag. Freya has no first-party way to inject an event into the
reactive runtime from the platform layer, and no timer primitive either
(`async-io` supplies the interval).

## Helper crates (all recorded, none rejected)

- `global-hotkey` 0.7.5 — system-wide ⌘⇧9.
- `rfd` 0.17.2 — native open/save panels.
- `arboard` 3.6.1 — image clipboard (Freya's is text-only).
- `notify-rust` 4.18 — notifications.
- `async-io` 2.6.0 — the 80 ms poll timer (Freya's executor has none).

**Not needed here, unlike the other frameworks in this cohort:** `tray-icon` and
`muda` (re-exported by `freya::tray`), `bytes` (`Bytes` is in Freya's prelude),
`ropey` (`Rope` is re-exported by `freya::text_edit`), `image` (the tray icon is
generated as raw RGBA and `ImageHandle::from_rgba` takes raw RGBA), and a PNG
encoder for the self-test screenshot (Freya has no window-screenshot API at
all, so `TRAY_SELFTEST_SHOT` reads the window frame from winit and shells out
to `screencapture -R`).

## Where the time went

1. **The muda dangling-pointer bug** (~40 % of the app), amplified by the
   release-mode panic dialog hiding the message.
2. **Writing a multi-line text editor.** Freya's `Input` is hard-wired to
   `max_lines(1)`; a text area has to be assembled from the low-level
   `use_editable` hook — one `paragraph` element per line, a persistent
   `ParagraphHolder` per line for hit testing, per-line selection ranges from
   `get_visible_selection(EditorLine::Paragraph(i))`, and manual key/pointer
   plumbing. ~90 LoC. The holders must be non-reactive (a `Rc<RefCell<Vec<_>>>`
   via `use_hook`), otherwise growing them during render loops forever.
3. The notify-rust "Choose Application" modal.

## Surprises

- Good: `Platform` is a genuinely complete platform surface — `launch_window`,
  `close_window`, `focus_window`, `with_window` (raw winit `Window`),
  `post_callback`, plus reactive `preferred_theme`, `accent_color`,
  `is_app_focused`, `scale_factor`. Live dark mode and multi-window cost
  ~5 lines each; both are the easiest of the cohort so far.
- Good: `on_file_drop` as an element event rather than a window event.
- Bad: `AppWindow`'s `window` is `pub(crate)`, so the one hook that is *about*
  a window (`with_on_close`) cannot act on it.
- Bad: no window-screenshot API, no timer, no way to signal the reactive
  runtime from a platform callback.
