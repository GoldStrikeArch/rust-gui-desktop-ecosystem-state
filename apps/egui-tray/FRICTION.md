# FRICTION.md — Tray Notes (egui 0.35 + eframe 0.35, macOS)

App: `apps/egui-tray/` · package `egui-tray` · `cargo run --release`.
Verified on macOS: release build clean, launched, alive after 10 s, killed.
Everything below was exercised on the live app (scripted via
osascript/System Events AX and a small CGEvent injector; a parallel-agent
shared desktop made raw keystroke automation racy, so menu items were
clicked through the accessibility tree of this process directly).

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| tray | **assembled** (`tray-icon`) | Works well: create the `TrayIcon` in the eframe creator closure (main thread, after winit has initialized NSApplication — the only legal point on macOS). Menu events verified end-to-end via AX (`menu bar 2` of the process = the NSStatusItem): Show/Hide, New Note, Quit all fire. `with_icon_as_template(true)` gives automatic light/dark menu-bar tinting; icon drawn in code as RGBA. |
| global_hotkey | **assembled** (`global-hotkey`) | Cmd+Shift+9 via Carbon `RegisterEventHotKey`; no Accessibility permission needed. Handler fires even when app hidden/unfocused (verified repeatedly, incl. while another process was frontmost). Caveat: events must be *processed* somewhere that runs while the window is hidden — see close_to_tray. |
| native_menubar | **assembled** (`muda` via `tray_icon::menu` re-export) | Real NSMenu menubar via `Menu::init_for_nsapp()` from the creator closure. File → New/Open…/Save…/Quit with ⌘N/⌘O/⌘S/⌘Q accelerators work (⌘S keystroke verified: muda swallows the key equivalent and emits a MenuEvent). Use the tray-icon re-export for BOTH menubar and tray menu so a single muda instance owns the one global `MenuEvent` channel. |
| dialogs | **built-in-ish / assembled** (`rfd`) | Genuine NSOpenPanel/NSSavePanel (screenshot-verified, Cancel round-trip OK). Blocking `FileDialog` on the main thread inside the frame works with eframe (AppKit runs a nested modal loop; UI frozen meanwhile, which is native-normal). |
| clipboard_text | **built-in** (egui/eframe) + assembled Edit-menu roles | Plain ⌘V paste into `TextEdit` is core egui (its winit backend bundles arboard for text). BUT native Edit-menu roles can't use `PredefinedMenuItem::cut/copy/paste`: those dispatch `cut:`/`copy:`/`paste:` down the responder chain and egui's winit NSView implements none of them (egui draws its own widgets). Bridged instead with custom muda items whose events inject `egui::Event::{Cut,Copy,Paste(text)}` at the top of the pass — paste round-trip verified (pbcopy → Edit▸Paste ⌘V → editor → Save → file contents match). |
| clipboard_image | **assembled** (`arboard`) | egui's own clipboard path is text-only (confirmed: `egui::Event::Paste(String)`, `Context::copy_text` — no image variant). `arboard::Clipboard::get_image()` → `ColorImage::from_rgba_unmultiplied` → `ctx.load_texture` renders the thumbnail; verified with a PNG placed on the pasteboard via osascript (`Pasted image 400x280`). |
| file_drop | **built-in** | `ctx.input(\|i\| i.raw.dropped_files)` with `DroppedFile::path`. Code path exercised in earlier iterations of this research; a Finder drag is not scriptable without heavier automation, so this app's handler was verified by code review + the API contract only. |
| notification | **hand-rolled** (`osascript` subprocess); notify-rust rejected for this app | In two of three full-app runs, the first `notify-rust` notification appeared and eframe then stopped scheduling frames while tray/hotkey callbacks remained alive. `mac-notification-sys` replacing the NSApplication delegate is a plausible explanation, but no minimized reproduction was retained, so the root cause is **not proven**. The shipped experiment uses `osascript -e 'display notification …'`, avoiding in-process notification/AppKit state (and inheriting the attribution/permission limitations of an unbundled subprocess). |
| dark_mode_live | **built-in** | `ThemePreference::System` is the default; the same process rendered dark and light across OS appearance flips without restart (screenshot pairs; the shared test desktop had several agents toggling appearance, which made for a free soak test). |
| multi_window | **built-in** (viewports) | About window via `ctx.show_viewport_immediate` — note the 0.35 signature passes the closure `&mut egui::Ui` (not `&Context`). Opens/closes independently of the main window (AX-verified both windows listed; closed via its titlebar button). One nuance: immediate viewports only render while the parent's `ui` runs, and eframe keeps running the parent's `ui` when the parent is hidden *only if* a child viewport is visible. |
| close_to_tray | **assembled, with a landmine** | The mechanics are built-in: `close_requested()` → `CancelClose` + `ViewportCommand::Visible(false)`. The landmine: **eframe 0.35 never calls `App::ui` for a hidden viewport** (`run_ui = is_visible \|\| …` in wgpu_integration.rs), so any "reopen" logic living in `ui` is dead once you hide — first attempt produced a window that could hide but never come back. The fix is `App::logic` (new-ish in eframe), which runs on every pass *including hidden ones*; viewport commands sent from it (`Visible(true)`, `Focus`) are applied by the UI-less passes. Hide→show→hide toggled reliably via the global hotkey afterwards. |

## Helper crates

- `tray-icon = 0.24.1` — tray/menu-bar extra; also supplies muda (`tray_icon::menu`) used for the native menubar. Chosen so tray + menubar share one MenuEvent channel.
- `global-hotkey = 0.7.0` — system-wide Cmd+Shift+9.
- `rfd = 0.17.2` — native open/save panels.
- `arboard = 3.6.1` — image clipboard (egui text clipboard is built-in).
- **REJECTED in this experiment: `notify-rust 4.18`** — the full app stopped
  receiving frames after its first notification in two of three runs. Delegate
  replacement is a hypothesis pending a minimized reproduction. Replaced by
  an `osascript` subprocess.
- (debug-only, removed) `env_logger` — used with `RUST_LOG=eframe=trace` to diagnose the hidden-window stall; removed from the final build.

## Integration model (the actual finding)

eframe owns the winit loop, but all three shell crates integrate cleanly
*without* an external event loop: create them in the `run_native` creator
closure (main thread, NSApp already initialized), keep them alive as fields
on the `App` (they are `!Send`, which is fine — the App lives on the main
thread), and have their `set_event_handler` callbacks (invoked on the main
thread by AppKit/Carbon) push semantic actions onto an `Arc<Mutex<Vec<_>>>`
and call `egui_ctx.request_repaint()`. Drain the queue in `App::logic`, not
`App::ui` (hidden viewports never run `ui`). One robustness addition: a 2 Hz
`request_repaint_after` watchdog in `logic`, because a `request_repaint`
from a native callback was once observed to be dropped by eframe's
"outdated RequestRepaint" lost-wakeup guard, permanently stalling the queue;
the watchdog makes any lost wake self-heal in ≤500 ms for ~zero cost.

## LoC / time

514 LoC, one file. Time split: ~40% diagnosing the two integration failures
(hidden-viewport `ui` skip → `App::logic`; the notify-rust-associated freeze —
both needed `RUST_LOG=eframe=trace` spelunking into eframe internals), ~25%
scripted macOS verification (AX menu clicking, CGEvent injector, dodging six
parallel agents fighting over the same desktop/theme/hotkey), ~35% the
actual app + shell wiring, which was straightforward.

## Surprises

- eframe 0.35's `App::logic` + invisible-pass machinery exists precisely for
  tray-style apps — but nothing in the tray-icon/eframe docs points at it;
  without reading eframe source the close-to-tray dead-end looks like a
  winit bug.
- muda accelerators really do swallow ⌘X/⌘C/⌘V before winit sees them; if
  you give Edit-menu items those accelerators you MUST bridge the events
  back into egui or you break copy/paste everywhere in the app.
- The same Cmd+Shift+9 hotkey can be registered by multiple processes at
  once on macOS (all of them fire) — discovered because parallel test agents
  were registering it simultaneously.
