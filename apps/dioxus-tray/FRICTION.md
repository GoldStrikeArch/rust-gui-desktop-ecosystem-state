# FRICTION — dioxus-tray ("Tray Notes", Dioxus 0.7.9 desktop/webview)

The headline: **dioxus-desktop ships most of the Tauri shell layer built in.**
It depends on and re-exports the tauri-apps crates (tray-icon 0.21, muda 0.17,
global-hotkey 0.7, rfd 0.17 internally) and pumps all their event loops through
its own tao loop, exposing hooks (`use_tray_menu_event_handler`,
`use_muda_event_handler`, `use_global_shortcut`, `use_wry_event_handler`).
What Tauri's plugin layer would add on top here is mainly: dialogs-as-API,
clipboard-image, and notifications — those three needed direct helper crates.

Evidence status: the detailed interaction outcomes below are contemporaneous
narrative observations; no reusable interaction harness, raw session log or
tray screenshot was retained. The canonical build/binary/dependency figures
come from the serial measurement artifacts.

## Capability ratings

| Capability | Rating | Notes |
|---|---|---|
| tray | **built-in** | `dioxus::desktop::trayicon::init_tray_icon(menu, icon)` (re-exported tray-icon crate) called once in a `use_hook`; `None` icon gives the Dioxus logo. Left-click shows/focuses the window (framework behaviour, configurable via `Config::with_tray_icon_show_window_on_click`), right-click opens the menu. Verified live: icon visible in menu bar, left-click restored the hidden window, menu items fire through `use_tray_menu_event_handler`, tray "Quit" (muda `PredefinedMenuItem::quit`) really exits. |
| global_hotkey | **built-in** | `use_global_shortcut("super+shift+9", handler)` — global-hotkey crate wired into the loop; handler gets Pressed/Released state. Verified live many times over (including presses sent while other apps had focus, and — accidentally — presses sent by sibling test agents testing THEIR SPEC-4 apps: the same combo can be registered by several processes at once and all get events). |
| native_menubar | **built-in** | muda `Menu` passed via `Config::with_menu`; first submenu becomes the macOS application menu; predefined items give ⌘Q quit, ⌘W close, full Edit clipboard roles; custom items take `Accelerator` (`"CmdOrCtrl+N".parse()`). Verified via AX: menus "Apple, dioxus-tray, File, Edit" exist; clicking File→New fired our id. **Trap:** tray-icon re-exports the *same* muda, and dioxus installs the tray receiver after the menubar receiver on muda's single global `MenuEvent` handler slot — so all menubar events actually arrive as `TrayMenuEvent`, and `use_muda_event_handler` alone would go silent the moment a tray icon exists. Register BOTH hooks with one shared callback. Unbundled binaries show the process name ("dioxus-tray"), not a display name, as the app menu title. |
| dialogs | **assembled** | rfd `AsyncFileDialog` awaited inside `spawn(...)` — native NSOpen/NSSavePanel, filters, default filename. Dioxus-desktop already *contains* rfd (it backs `<input type=file>`), so pinning the same 0.17 adds zero new crates; it just is not re-exported. A Save panel was observed from the File menu, but the screenshot was not retained. The panels are XPC-hosted on macOS 26 (invisible to per-process AX scripting — relevant to test automation only). |
| clipboard_text | **built-in** | The WKWebView textarea has the full native editing stack; muda's predefined Edit items provide the standard roles. Verified in the sibling Babel app (same widget): ⌘A/⌘C put the exact multiline Unicode content on the NSPasteboard, ⌘V pasted a string containing Arabic/Hebrew/CJK/ZWJ-emoji intact. |
| clipboard_image | **assembled** | arboard `get_image()` → RGBA → `image` crate PNG-encode → base64 data URI → `<img>` thumbnail. ~30 LoC. Verified live with a real `screencapture -c` clipboard: 80 KB data URI rendered as thumbnail. Quirks found: arboard fails with "could not be converted" on AppleScript's legacy `TIFF picture` flavor (fine with PNG flavor and real screenshots), and `image` was already in the dep tree (dioxus-desktop and arboard both depend on it). |
| file_drop | **built-in*** | wry's drag-drop handler is installed by default and dioxus splices the native paths into the HTML drop event: `ondragover: prevent_default` + `ondrop: evt.files()` yields `FileData` with real `path()`s (`NativeFileHover` in dioxus-desktop). *Rated from source + code path only — a genuine Finder drag could not be synthesized in this environment (CGEvent drags don't carry a file promise), so this is verified by construction, not interactively. |
| notification | **assembled** | notify-rust: `Notification::new().summary(...).show()` returned Ok from the **unbundled** release binary on macOS 26 (posts via the osascript backend attributed to the script runner — fine for the spec, would need a proper bundle id for production polish). An osascript `display notification` fallback is coded but was not needed. |
| dark_mode_live | **built-in** | CSS `prefers-color-scheme` + `:root { color-scheme: light dark }`. Toggled the OS theme via osascript with the app running: chrome, native controls and the media-query CSS all switched without restart. Caveat (same root cause as the repaint note below): while the window sat idle & unfocused the new CSS only *painted* when the next real event arrived; with the window focused the flip is immediate. |
| multi_window | **built-in** | `dioxus::desktop::window().new_window(VirtualDom::new(About), Config...)` awaited in `spawn`. The About window is a full independent webview; closes independently (default `WindowCloses`) while the app stays alive. **Trap:** give the child window `.with_menu(None)` — a default `Config` installs the *default* menubar, which on macOS is app-global and would clobber your custom menu. |
| close_to_tray | **built-in** | `Config::with_close_behaviour(WindowCloseBehaviour::WindowHides)` + `.with_exits_when_last_window_closes(false)`. Verified live: red close button → window gone from the on-screen list, process alive; tray click brings it back; Quit menu items really exit. Zero hand-written code. |

## Helper crates

- `rfd 0.17` (dialogs) — same version dioxus-desktop uses internally; 0 new transitive deps.
- `arboard 3.6` (image clipboard).
- `image 0.25` + `base64 0.22` (PNG-encode clipboard RGBA into a data URI); `image` was already in the tree via dioxus-desktop/arboard.
- `notify-rust 4.18` (notification).
- `tokio 1` (time feature) — only for the TRAY_SELFTEST timer; the iteration-2 finding repeats: dioxus runs on tokio but re-exports no timer.
- **Not needed directly** (built into dioxus-desktop): tray-icon, muda, global-hotkey — all re-exported under `dioxus::desktop::{trayicon, muda}`.
- Rejected: nothing — every candidate worked; no plugin ecosystem was needed at all.

## The one real framework wart found

**Idle event loop defers everything.** With the window unfocused and no input
arriving, work scheduled from background tasks (a `use_future` that slept on a
tokio timer, then wrote signals / called `new_window`) did not visibly execute
until the next real windowing event arrived — stdout appeared, but VDOM edits
were not flushed to the webview and pending futures resumed late. All
event-driven interactions (the entire spec surface) behave normally; it only
bites autonomous background updates into an idle unfocused window. Same
mechanism delayed the *paint* of a live theme switch. Not investigated to root
cause (tao `ControlFlow::Wait` wake-up path is the suspect); worth a proper
upstream repro.

## Where the time went

Honest split: writing the app was fast (first `cargo check`: **0 errors,
0 warnings**; every capability compiled against dioxus' own APIs first try).
- ~55% *verification* in a hostile shared environment: six parallel sibling
  agents constantly stole focus, toggled the OS theme, pressed the same global
  hotkey, and overwrote the clipboard mid-test. Deterministic AX scripting +
  an in-app TRAY_SELFTEST env hook were the fix.
- ~15% the muda-event routing discovery (menubar events arriving as tray
  events) — found by reading dioxus-desktop source, cheap to work around.
- ~15% clipboard-image forensics (sibling clipboard races + the AppleScript
  TIFF flavor arboard rejects — standalone repro exonerated the app code).
- ~15% design/CSS/menu construction.

## Measurements

- `src/main.rs`: 394 lines (incl. ~45 lines CSS-in-Rust, ~35 lines selftest instrumentation, comments).
- Canonical serial clean release build: **37 s**; no-op incremental build:
  **4 s**. The 217-second parallel-load run is retained only as a noncanonical
  observation. Dependency graph: **287 unique crate names / 295 name-version
  entries including the app**. Binary **9,324,656 bytes raw / 8,124,152 bytes
  (7.7 MiB) stripped**; the increase over the base app is mostly the image,
  arboard and notification stack.
- First `cargo check` after writing: 0 errors, 0 warnings, 2m10s clean.
- Launch: release binary ran >30 min through the interactive verification. A
  contemporaneous ≈93 MiB reading covered only the main process and its raw
  sample was not retained; stdout contained only the app's instrumentation.
