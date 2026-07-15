# FRICTION — xilem-tray ("Tray Notes", xilem 0.4.0 from crates.io)

Observed on macOS 26.5 (M4 Pro): release build, 10 s launch checks, and a
contemporaneous scripted interaction run (AppleScript menu/tray clicks plus
synthetic CGEvent input) covering menus, tray/hotkey visibility, clipboard,
dialogs, notification, live theme change, multi-window and quit paths. The
harness, raw output and screenshots were not retained, so the detailed results
below are narrative evidence rather than a reproducible test artifact.

## Architecture (the headline)

xilem 0.4 has **zero** shell-integration surface: no tray, no menubar, no
dialogs, no notifications, no theme events, no file-drop events, no way to
reach the winit `Window` from app code. Everything below rides on one
saving grace: masonry_winit 0.4's **external-event-loop embedding**
(upstream `external_event_loop.rs` example) is public API. We own the winit
`ApplicationHandler`, forward everything to `MasonryState`, and splice in:

1. a **wrapper `AppDriver`** around xilem's `MasonryDriver` — the only
   public path to window handles is `DriverCtx::window(id).handle()` inside
   driver callbacks, so hide/show/quit and TextEvent injection happen here;
2. a **tokio unbounded channel → stock `worker` view** — tray/menu/hotkey
   callbacks and intercepted winit events are `send()`-ed from any thread;
   the worker's `MessageProxy` wakes the loop and mutates xilem state
   (the iteration-2 "MessageProxy from tasks" pattern, now app-wide);
3. **winit-layer interception** in `window_event` for `DroppedFile` /
   `ThemeChanged`, which masonry_winit 0.4 silently discards (`_ => ()` in
   `event_loop_runner.rs`).

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| tray | **assembled** | `tray-icon` crate; zero xilem involvement. macOS constraint: must be created on the main thread *after* the loop starts — only possible because we own `ApplicationHandler::new_events(StartCause::Init)` via the external-loop embedding; with stock `Xilem::run_in` there is **no hook** to create it (would be not-achievable without the embedding). Menu events arrive on a muda callback thread → channel → worker. |
| global_hotkey | **assembled** | `global-hotkey` (Carbon `RegisterEventHotKey`); registered at Init on the main thread; ⌘⇧9 toggles visibility even when unfocused/hidden (verified — repeatedly, since parallel test agents' hotkey presses toggled this app too). No permission prompts. |
| native_menubar | **assembled** | muda (re-exported by tray-icon) `Menu::init_for_nsapp()` at Init. File ⌘N/⌘O/⌘S/⌘Q accelerators work (menu consumes key, event routed via channel). **Trap:** `PredefinedMenuItem::{cut,copy,paste,select_all}` are NSResponder-selector items that do nothing against masonry (not an NSText responder) *but still swallow ⌘X/⌘C/⌘V/⌘A key equivalents*, silently breaking masonry's own built-in clipboard keys (cost ~1 h to diagnose). Fix: custom items with **no** accelerators that inject synthetic `TextEvent`s (`ClipboardPaste`, fake ⌘X/⌘C/⌘A `KeyboardEvent`s) into the `RenderRoot` from the wrapper driver. No Undo role: masonry TextArea has no undo stack at all. |
| dialogs | **assembled** | rfd sync `FileDialog` (NSOpen/NSSavePanel) called directly from the state handler — which runs on the main thread inside winit's event dispatch; the panel's modal runloop coexists with winit fine (3 open + 3 save runs, no re-entrancy issue). Open/read and save/write `.txt` verified. |
| clipboard_text | **built-in** | masonry_winit handles ⌘V itself (copypasta → `TextEvent::ClipboardPaste`) and TextArea implements ⌘X/⌘C/⌘A; round-trip verified with pbpaste. Only broken while the muda predefined items were stealing the key equivalents (see above). |
| clipboard_image | **assembled** | `arboard::Clipboard::get_image()` → peniko `ImageData` (RGBA8) → stock `image()` view thumbnail. Straight-line code, verified with a PNG placed on the pasteboard ("pasted image 200x110" + visible thumbnail). |
| file_drop | **assembled (unverified interactively)** | masonry_winit 0.4 drops `WindowEvent::DroppedFile` on the floor (`event_loop_runner.rs` match arm `_ => ()`); we catch it in our own `ApplicationHandler::window_event` before forwarding and channel it into state. winit macOS delivers drops out of the box; a real Finder drag cannot be synthesized, so this is code-path-verified only. Not achievable at all without the external-loop embedding. |
| notification | **hand-rolled** | Three notify-rust attempts, all instructive: (1) naive `.show()` → mac-notification-sys resolves magic app name "use_default" via LaunchServices, which on macOS 26 pops a *blocking* "Where is use_default?" chooser; (2) `set_application("com.apple.Terminal")` first → send *pumps the main runloop from inside the winit callback* → winit 0.30 **panic-abort** ("tried to handle event while another event is currently being handled"); (3) `.show()` from a detached thread → returns Ok, app survives, but **no banner ever appears** (macOS 26 silently drops NSUserNotifications for the borrowed bundle id). Shipped: out-of-process `osascript display notification` from a thread — banner verified on-screen ("Note saved / Saved to /private/tmp/note.txt"). |
| dark_mode_live | **hand-rolled** | The window was observed restyling live in both directions after an `osascript` appearance toggle, but those screenshots were not retained. Every piece is manual: Masonry 0.4's `default_property_set()` is dark-only with no light theme and no way to swap `DefaultProperties` at runtime; winit `ThemeChanged` is intercepted at the winit layer (Masonry ignores it); initial theme is read by shelling out to `defaults read -g AppleInterfaceStyle`; restyle = reactive `window(...).with_base_color(..)` plus per-view palette properties on every label/button/text_input. |
| multi_window | **built-in** | Genuinely first-class in 0.4: app logic returns a window iterator; About window = `state.about_open.then(|| window(...))` with `on_close` flipping the flag. Open/close via button and red-button both verified. (Caveat: initial position is settable, but there is no API to reposition/hide a window after creation from xilem itself.) |
| close_to_tray | **assembled** | Wrapper driver intercepts `on_close_requested` for the main window and calls `handle().set_visible(false)` instead of forwarding; xilem never learns the window "closed", app keeps running (verified: red button → window gone, process alive, tray/hotkey restore + `focus_window()`). Pure xilem could only approximate this by destroying/recreating the window (loses position, flashes). |

## Helper crates

- `masonry_winit =0.4.0` (direct dep): xilem doesn't re-export
  `MasonryState`/`AppDriver`/`DriverCtx`, which the embedding needs.
- `tray-icon 0.21` — tray + (re-exported) muda menus. Chosen over a direct
  muda dep so tray menu and menubar share one `MenuEvent` handler/static.
- `global-hotkey 0.7` — system-wide ⌘⇧9.
- `rfd 0.15` — native file dialogs; sync API on the main thread just works.
- `arboard 3` — image clipboard (text clipboard needed nothing).
- `notify-rust 4` — **effectively rejected on macOS** after three attempts
  (blocking LaunchServices chooser / winit re-entrancy abort / silently
  dropped banners); retained for non-macOS, osascript shipped for macOS.

Duplicate-tree cost: winit 0.30 (objc2 0.5 family) + tray-icon/muda/
global-hotkey/arboard (objc2 0.6 family) coexist fine; 2 `keyboard-types`
versions forced a `Modifiers as KbModifiers` rename (compile error).

## Totals

- LoC: **774** (`main.rs` 403 + `shell.rs` 371). Roughly half is shell
  plumbing that a framework with these features built-in would not need.
- Canonical clean release build **35 s**; no-op incremental build **2 s**.
  Dependency graph: **169 unique crate names / 183 name-version entries
  including the app**. Binary **12,234,880 bytes raw / 10,459,512 bytes
  (10.0 MiB) stripped**. The earlier ~105-second observation was not the
  controlled serial result.
- ~10 s launch check: clean, no stdout/stderr noise.

## Where the time went

1. **Parallel-agent interference, not xilem** (~30 %): 6 sibling tray apps
   from other test agents fought over frontmost, screen space, the OS theme
   and the *same global hotkey*; clicks landed in the wrong app's window
   until this app got a `TRAY_POS` env var for deterministic placement.
2. **muda-eats-masonry-clipboard diagnosis** (~20 %): predefined Edit items
   silently consuming ⌘C/⌘V while doing nothing.
3. **notify-rust odyssey** (~20 %): three failure modes incl. a
   panic-abort from *inside* a dependency.
4. **Architecture** (~20 %): wrapper-driver + channel/worker + interception
   design (reading vendored masonry_winit source to find what's public).
5. Everything else (~10 %): the actual note app.

## Surprises

- Good: the external-event-loop embedding is *the* unlock — everything
  SPEC-4 asks for becomes possible (if manual) once you own the
  `ApplicationHandler`; with stock `Xilem::run_in` roughly half these rows
  would be not-achievable.
- Good: `DriverCtx::window(id).handle()` hands you the raw winit window —
  and `window().with_base_color()` is reactive.
- Bad: winit 0.30 panic-aborts on runloop re-entrancy, so any helper crate
  that pumps the macOS runloop inside an event callback (mac-notification-sys)
  kills the process; rfd's modal panels are fine.
- Bad: NSMenu key equivalents are a shared global namespace — a menu item
  nobody handles still steals the shortcut from the focused widget.
