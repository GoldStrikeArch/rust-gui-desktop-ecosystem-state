# FRICTION — tauri-tray ("Tray Notes", SPEC-4)

Tauri =2.11.5 / tauri-build =2.6.3 (same pins as ../tauri-app), same manual
no-Node setup (hand-written `tauri.conf.json`, `withGlobalTauri`, static
vanilla HTML/CSS/JS in `ui/`, hand-written capability, copied icons).

Division of labor: Rust owns all OS integration (tray, menubar, hotkey,
notification, clipboard image, file drop, windows); JS owns the note text
and calls the dialog plugin — the one deliberately webview-routed plugin,
to measure the ACL wiring cost.

## Capability ratings

Scale note: **built-in** = in the `tauri` crate itself (possibly behind a
cargo feature); **assembled** = official first-party plugin (one Cargo line
+ one `.plugin()` line — far less assembly than hand-wiring the same
underlying crates elsewhere).

| Capability | Rating | Note |
|---|---|---|
| tray | **built-in** | `TrayIconBuilder` in core behind the `tray-icon` cargo feature; menu, tooltip, left-click-menu in ~10 LoC. Icon reuses the bundle icon `tauri-build` embeds (`app.default_window_icon()`), so no runtime PNG decoding feature needed. Verified: "tray icon built" on launch, menu items route to the same `on_menu_event` handler as the menubar. |
| global_hotkey | **assembled** | `tauri-plugin-global-shortcut` =2.3.2 (global-hotkey/Carbon underneath). `with_shortcuts(["super+shift+9"]) + with_handler` — registration confirmed via `is_registered`=true, and the handler FIRED from a synthetic System-Events Cmd+Shift+9 while verifying, toggling the hidden window. Rust-side only → zero ACL entries. |
| native_menubar | **built-in** | `tauri::menu` (muda) makes a real NSMenu. Verified by UI-scripting the running app: menu bar = Apple / tauri-tray / File / Edit; clicked File→Save… programmatically and the handler ran. Predefined items provide the native clipboard roles (without an Edit menu, ⌘C/⌘V would not reach WKWebView at all on macOS). ⌘Q lives on the app menu (`PredefinedMenuItem::quit`); File gets a plain Quit item — a duplicate ⌘Q accelerator would conflict. |
| dialogs | **assembled** | `tauri-plugin-dialog` =2.7.1 (rfd underneath), called from JS (`window.__TAURI__.dialog.open/save` — plugin JS is auto-injected under `withGlobalTauri`, still no npm). Verified end-to-end: menubar File→Save… → Rust menu event → JS `dialog.save` → a real NSSavePanel **sheet** appeared on the window (System Events counted 1 sheet) and Escape dismissed it. |
| clipboard_text | **built-in** | Plain text paste into the `<textarea>` is WebKit + the Edit-menu paste role; no plugin, no permission. (clipboard-manager also has text APIs; not needed for the editor.) |
| clipboard_image | **assembled** | `tauri-plugin-clipboard-manager` =2.3.2 — whose desktop backend IS arboard, so the spec's "arboard fallback" is what the official plugin already wraps; no direct arboard dependency needed. `read_image()` → raw RGBA returned to JS as `tauri::ipc::Response` raw bytes (`[w u32][h u32][rgba…]`, no JSON pixel arrays) → canvas `putImageData` thumbnail. Verified round-trip headlessly: Rust `write_image` 48×32 → real button click → "clipboard image read: 48x32" → thumbnail visible in screenshot.png. Command is `async` so the read never runs on the main thread (plugin documents a Linux deadlock risk). |
| file_drop | **built-in** | `WindowEvent::DragDrop(DragDropEvent::Drop)` with `dragDropEnabled: true` — the OPPOSITE of the iteration-2 apps (which needed it false for HTML5 DnD; the two models are mutually exclusive per window). Drops deliver real filesystem paths; Rust reads the .txt and pushes content down as an event (webview never needs fs access). Handler wired + reviewed; a physical Finder drag is not automatable headlessly. |
| notification | **assembled (display unverified)** | `tauri-plugin-notification` =2.3.3 (notify-rust/mac-notification-sys underneath), fired from Rust on save. `show()` returned `Ok(())` from the raw unbundled binary, but no banner was visually confirmed. Bundle identity and notification permission are plausible factors on this configuration; this run proves the API path, not a universal requirement that notifications need `.app` packaging. |
| dark_mode_live | **built-in** | Verified LIVE in both directions with `osascript` theme toggles: Rust `WindowEvent::ThemeChanged: light/dark` AND, inside the webview, `matchMedia('(prefers-color-scheme: dark)')` change events fired (reported over IPC to stdout); CSS variables restyle with zero JS. Screenshot shows the dark palette applied. |
| multi_window | **built-in** | `WebviewWindowBuilder` + a second static `about.html`. Selftest opened it, confirmed it exists, closed it independently; its CloseRequested is NOT intercepted (label check). Trap: window creation must happen on the **main thread** — from worker threads use `run_on_main_thread`. |
| close_to_tray | **built-in** | `CloseRequested` → `api.prevent_close()` + `hide()`, main window only. Verified: after a close request the window still exists with `visible=Some(false)` and the app stays alive; restored via the same toggle the hotkey uses. `RunEvent::Reopen` restores on dock-icon click (macOS nicety). |

## Helper crates (all four are official first-party plugins)

| Crate | Why | Underneath |
|---|---|---|
| tauri-plugin-global-shortcut =2.3.2 | Cmd+Shift+9 toggle | global-hotkey 0.8 |
| tauri-plugin-dialog =2.7.1 | native Open…/Save… | rfd |
| tauri-plugin-notification =2.3.3 | "Note saved" banner | notify-rust 4.18 |
| tauri-plugin-clipboard-manager =2.3.2 | image clipboard | arboard 3.6.1 |

Tried and REJECTED: a direct `arboard` dependency (the task's allowed
fallback) — unnecessary, the official plugin already wraps arboard and
exposes `read_image`/`write_image`. Nothing else was needed: tray + menus
are core-tauri features (tray-icon / muda vendored in).

## Permission wiring cost (the ACL bill)

- Total: **one line** — `"dialog:default"` in `capabilities/default.json`
  (plus adding `"about"` to the capability's window list). Dialog is the
  only plugin invoked *from the webview*.
- The other three plugins are used purely Rust-side, and **Rust-side plugin
  APIs bypass the ACL entirely** — no permissions, no capability edits.
  That is the real cost model: you pay per plugin-used-from-JS, not per
  plugin installed. (Had all four been JS-driven: 4 permission entries;
  each plugin's JS API is auto-injected under `withGlobalTauri`, so still
  no npm.)
- App-defined `#[tauri::command]`s (write_note/read_note/paste_image)
  need no permissions, and Rust commands doing `std::fs` mean no fs plugin
  and no fs scopes were ever involved.

## LoC (559 source; 602 including config) & size

- Rust: **338** (332 `src/main.rs` — of which ~70 is the TRAY_SELFTEST
  harness — + 6 `build.rs`)
- Frontend: **221** (29 + 21 HTML, 85 JS, 86 CSS)
- Config: 43 (`tauri.conf.json` 33 + capability 10)
- Release binary **10.0 MiB** (vs 8.0 MiB baseline); **235 unique crate
  names** — the four plugins added **31 names** over iteration 1's 204.
- Canonical serial clean `cargo build --release`: **52 s**.

## Where the time went

1. Verification, not implementation: the selftest harness + osascript
   driving (theme toggle, synthetic hotkey, UI-scripting the menubar and
   counting NSSavePanel sheets) is most of the Rust LoC delta.
2. Knowing macOS conventions up front (⌘Q placement, Edit-menu roles
   required for ⌘V, main-thread window creation) — the APIs themselves
   never fought back; the app compiled and every capability worked on the
   **first** `cargo build --release`.
3. Byte-packing the clipboard image over `tauri::ipc::Response` instead of
   JSON (worth it: no base64/no png crate, ArrayBuffer straight to canvas).

## Verification

Built release; plain launch alive at 10 s (RSS ~123 MiB main process, kill
clean). TRAY_SELFTEST run verified on stdout: tray built, shortcut
registered=true, clipboard image round-trip 48×32 rendered, About window
opened/existed/closed, close request intercepted (exists=true,
visible=false, app alive), toggle restored visibility, save wrote
/tmp/tray-notes-selftest.txt with notification Ok(()). Live osascript:
theme flip light↔dark seen by BOTH Rust ThemeChanged and in-webview
matchMedia; synthetic Cmd+Shift+9 fired the handler twice (hide + show);
File→Save… clicked in the real menubar opened a real save sheet, Escape
cancelled. screenshot.png (window-scoped `screencapture -l`) shows dark
theme + rendered clipboard thumbnail. Not automatable headlessly: physical
Finder file drag, notification banner visibility, dialog file pick-through.
