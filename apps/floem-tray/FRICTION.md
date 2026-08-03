# FRICTION — Tray Notes (floem git @ 778bb5f2), SPEC-4

Verified on macOS (M4 Pro, rustc 1.96.1): `cargo build --release` clean
(locked rebuild too), launched >10 s, killed cleanly. Scripted verification:
self-test hooks exercised image paste + save + notification + screenshot;
synthetic ⌘⇧9 keystrokes toggled the window both directions (log:
`hotkey: ⌘⇧9` / `toggle-window: hiding` / `toggle-window: showing`).
Total LoC: **610** (single `src/main.rs`; ~60 are self-test hooks +
PNG/screenshot helpers).

Version note: same pinned git rev as apps/floem-app (crates.io 0.2.0 stale;
`main` unpublishable — forked `floem-winit`). See apps/floem-app/GAPS.md.

## Capability ratings

| capability | rating | note |
|---|---|---|
| tray | **assembled** | floem has nothing; `tray-icon` 0.24 NSStatusItem created in an `exec_after` callback (main thread, run loop live — tray-icon #90). Verified: icon appears, menu opens, Show/Hide toggles. Event delivery is NICER than under iced: no polling — see the muda note below. |
| global_hotkey | **assembled** | `global-hotkey` 0.7 (Carbon `RegisterEventHotKey`). Its `set_event_handler` pushes into a queue and wakes the UI thread via floem's `ExtSendTrigger` + `register_ext_trigger` — floem is the only framework tested so far with a first-class "poke the reactive graph from a foreign thread" primitive, which eliminates the 100 ms polling loop the iced port needed. Verified with synthetic ⌘⇧9 both directions, including while hidden. |
| native_menubar | **BUILT-IN** (headline) | `floem::Menu` builds muda menus with per-item action **closures** and `set_window_menu()` installs them on NSApp — no MenuId bookkeeping, no event channel. File ⌘N/⌘O/⌘S work as accelerators. Same trap as iced for Edit roles: muda `PredefinedMenuItem::cut/copy/paste` go through Cocoa responder-chain selectors floem's winit-fork NSView doesn't implement, so custom items are routed by hand — BUT floem's routing target is much better than iced's: `Document::run_command(ClipboardCut/Copy/Paste/SelectAll)` drives the real Lapce editor-core clipboard behavior at the cursor/selection. |
| dialogs | **BUILT-IN** | rfd is a floem *dependency*: `floem::open_file/save_as(FileDialogOptions, callback)` — the callback is delivered back on the UI thread via `create_ext_action`. Zero integration code. |
| clipboard_text | **built-in** | `floem::Clipboard::get/set_contents` + the editor core's own ⌘C/⌘V bindings (which call the same Clipboard internally — verified in source `views/editor/text.rs`). Menu-routed Cut/Copy/Paste exercise the same path. Evidence: source-only for the in-editor keybindings (headless typing not scriptable without Accessibility). |
| clipboard_image | **assembled** | `arboard::Clipboard::get_image()` → RGBA. Papercut: floem's `img` view only accepts ENCODED bytes, so the RGBA must be PNG-encoded in memory first (`png` crate) — no raw-pixels image view (finding; iced has `Handle::from_rgba`). Verified: osascript-placed PNG → `paste-image: OK 64x48`, thumbnail rendered. |
| file_drop | **built-in** | Typed `listener::FileDragDrop` event with `paths: Rc<[PathBuf]>` + drop position. Handler loads `.txt`. Not interactively verified (a real Finder drag cannot be synthesized) — plumbing confirmed in floem's app handle (`DragDropped` → `file_drag_dropped`); evidence: source-only. |
| notification | **assembled** | `notify-rust` 4 — `.show()` returned Ok from the unbundled binary (`notification: OK`). Same macOS caveat as iced: banner display is gated on per-app notification approval. |
| dark_mode_live | **built-in** | floem's default theme has a `dark_mode()` style selector re-resolved on the OS ThemeChanged event; the whole UI restyled live. The typed `listener::ThemeChanged` (winit `Theme` payload) surfaces the mode in the status bar (`theme-changed: dark` fired at startup, observed). |
| multi_window | **built-in** | `new_window(view_fn, config)` / `close_window(id)`. About window opens/closes independently. |
| close_to_tray | **built-in** | `listener::WindowCloseRequested` + `cx.prevent_default()` swallows the close; `WindowIdExt::set_visible(false)` hides. floem's macOS `AppConfig` even defaults to `exit_on_close: false`. BONUS vs iced: `AppEvent::Reopen` exposes Dock-icon reopen (`applicationShouldHandleReopen`) — the hidden window comes back on Dock click, which iced structurally could not do. |

## Helper crates (all recorded; one rejected)

- `tray-icon` 0.24.1 — tray icon + menu (muda 0.19 inside).
- `global-hotkey` 0.7 — system-wide ⌘⇧9.
- `arboard` 3.6 — image clipboard (floem Clipboard is text + file-list only).
- `notify-rust` 4 — notifications.
- `png` 0.18 — RGBA→PNG for the thumbnail (floem img wants encoded bytes) and
  reused by the self-test screenshot hook.
- REJECTED: a direct `muda` dependency and `rfd` — floem already ships both
  (menubar and dialogs are built-in); adding muda 0.17.x directly would be
  actively dangerous (see below).

## The muda-version minefield (headline finding)

floem pins muda =0.17 and claims that instance's single global
`MenuEvent::set_event_handler` slot at `Application::new()` for its own menu
system. tray-icon 0.24 bundles muda 0.19 — a *separate* compiled instance
with a *free* handler slot, which this app hooks. The two coexist only
because the versions DIFFER: had tray-icon resolved to muda 0.17.x, cargo
would have unified the crates and floem's handler would silently swallow
every tray-menu click (no error, no event). An app author has no way to see
this except by reading floem's source. Price paid: two copies of muda in the
binary.

## Where the time went

1. Reading floem source to discover what is built-in (menubar! dialogs!
   reopen! prevent_default on close!) — none of it is documented anywhere
   outside the source at this rev.
2. The muda handler-slot analysis above.
3. Editor plumbing: getting text in/out of the Lapce editor core
   (`doc.edit_single(Selection::region(..))`, `rope.slice_to_cow`) and
   routing menu items via `run_command` — powerful but wholly undocumented.

## Surprises

- Good: floem is the first framework in this experiment where the menubar,
  dialogs, file-drop, dark-mode, multi-window AND dock-reopen cells are all
  built-in; only tray/hotkey/image-clipboard/notification needed crates.
- Good: `ExtSendTrigger` removes the poll-the-channels pattern entirely.
- Bad: the muda version coupling is a silent-failure trap.
- Bad: no window-screenshot API (iced has one); the self-test hook shells
  out to `screencapture -R` with `WindowIdExt` bounds instead.
- Observed: ⌘/⇧ symbols render as tofu in labels (parley fallback misses the
  symbol font) — logged for the Babel iteration.

## Totals

- LoC: 610 · helper crates: 5 (+2 floem built-ins that replaced planned ones)
