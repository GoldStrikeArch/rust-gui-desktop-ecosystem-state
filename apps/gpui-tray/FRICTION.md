# FRICTION — Tray Notes (gpui =0.2.2)

Reference: SPEC-4.md. Built + verified on macOS (M4 Pro, rustc 1.96.1),
crates.io gpui with `runtime_shaders` (see apps/gpui-app/GAPS.md). Release
build clean (only the known transitive `block v0.1.6` future-incompat note),
binary 8.4 MiB unstripped, **412 unique crate names**. Launched, alive after
10 s at ~0.0–0.1 % CPU / ~95 MiB RSS, killed cleanly.

Verification was unusually deep this round because macOS automation
permissions were granted in this environment: System Events (AX) menu
clicks/keystrokes, real CGEvent mouse clicks via a tiny compiled Swift
helper, `screencapture` window captures, and an osascript dark-mode toggle
were all used against the *running release binary*. Every capability below
says how it was verified. (Caveat: 6 sibling agents were injecting UI events
on the same desktop, so status-item coordinates had to be re-read before
every click.)

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| tray | **assembled** | `tray-icon` crate (NSStatusItem). gpui runs the real AppKit main runloop, so building the status item on the main thread inside `Application::run` *just works* — no winit-style event-loop fight. Events arrive on tray-icon/muda's global crossbeam channels with no waker, so an 80 ms gpui timer task drains them. Verified: icon appears/disappears with the process (screenshot diff), menu opens, Show/Hide toggled the window, Quit exited — via real CGEvent clicks. Two costs: (1) menu-bar-only mode is impossible — gpui doesn't expose `NSApp.setActivationPolicy(.accessory)`, so a Dock icon always shows; (2) muda events need polling. |
| global_hotkey | **assembled** | `global-hotkey` crate (Carbon `RegisterEventHotKey`), registered on the main thread, drained in the same 80 ms pump. ⌘⇧9 verified end-to-end with scripted System Events keystrokes while another app was frontmost: window hides, fires again → window returns. No permissions needed. |
| native_menubar | **built-in** | The headline gpui strength: `cx.set_menus(vec![Menu{…}])` + `actions!` + `cx.bind_keys` gives the real macOS menu bar; accelerators come from the keymap (⌘N/⌘O/⌘S/⌘Q verified via the AX attribute `AXMenuItemCmdChar`). `MenuItem::os_action` wires Edit→Cut/Copy/Paste/Select All to the focused editor's actions, and macOS auto-appends Dictation/Emoji. Verified by AX menu clicks (File→New/Open…/Save… all dispatch). Two gotchas below (defer + app-menu name). |
| dialogs | **built-in** | `cx.prompt_for_paths` / `cx.prompt_for_new_path` → native NSOpen/NSSavePanel over a oneshot channel. Verified end-to-end by scripting the panels (⌘⇧G, type path, Return): Open loaded a .txt into the editor (status shows the path), Save wrote `/tmp/note.txt` with the editor content. |
| clipboard_text | **built-in** | `cx.read_from_clipboard().text()` / `write_to_clipboard`. Verified: ⌘V pasted a two-line pbcopy string into the editor (newlines preserved); select-all + ⌘C round-tripped the buffer back out to `pbpaste`. |
| clipboard_image | **built-in (surprise)** | gpui's ClipboardItem is multi-entry and macOS `read_from_clipboard` decodes pasteboard images natively (`ClipboardEntry::Image`, PNG/TIFF/…) — no arboard needed. "Paste image" renders the entry via `img(Arc<Image>)`. Verified: `screencapture -c` → probe logged `Image(Png, 32774 bytes)` → thumbnail visible in a window capture. |
| file_drop | **built-in (not interactively verified)** | `.on_drop::<ExternalPaths>()` + `.on_drag_move::<ExternalPaths>()` for hover highlight — the same typed-drag API as internal DnD, identical to Zed's file-drop path. A real Finder drag can't be scripted (CGEvent clicks don't carry an NSDragging session), so this is code-review + API-contract verified only. |
| notification | **assembled (display unverified)** | `notify-rust` → mac-notification-sys (deprecated NSUserNotification API). Save returned `Ok`, but **no banner was observed** from this unbundled process on this machine. A bundled identity/permission state is a plausible factor, not a proven universal requirement; the tested path establishes API return only. The fallback `osascript display notification` runs only on `Err` and therefore did not improve this case. |
| dark_mode_live | **built-in** | `window.observe_window_appearance(…)` (subscription must be stored) + `window.appearance()` per render drives a two-palette theme. Verified live: osascript-toggled OS appearance flipped the running window light↔dark without restart (before/after window captures). |
| multi_window | **built-in** | Second `cx.open_window` for About. Verified: opens from the app menu, closes via its close button while the main window and process continue, reopens after being closed (stale-`WindowHandle` detected via `update(..).is_err()` → recreate). |
| close_to_tray | **assembled** | `window.on_window_should_close(cx, |…| { …; false })` vetoes the close — verified by AX-clicking the red close button: window vanishes, process stays. The catch: gpui has **no per-window hide** (no `orderOut` exposure), so "hide" is app-level `cx.hide()` — it hides About too, and it turned out to be *silently ignored by AppKit while the status-item menu is still dismissing* (fixed with a 300 ms settle delay before acting on tray-menu events). |

## Helper crates

- `tray-icon 0.24.1` — NSStatusItem + menu (pulls muda). Required; nothing in gpui.
- `global-hotkey 0.7.0` — Carbon hotkey. Required; nothing in gpui.
- `notify-rust 4.18` — notifications. Kept despite the permission caveat; the
  alternative (`osascript`) is strictly worse.
- `unicode-segmentation 1` — grapheme boundaries for the hand-rolled editor
  (same dependency gpui's own input.rs example uses).
- Considered and REJECTED: `arboard` (image clipboard) — unnecessary, gpui's
  clipboard already decodes images on macOS; `rfd` — unnecessary, gpui has
  native dialogs.

## Where the time went

1. **The action-dispatch re-entrancy bug** (~1 h of confusion): menubar/keymap
   actions dispatch *through the focused window*, so a global `cx.on_action`
   handler that calls `WindowHandle::update` on that same window fails — and
   `.ok()` made it silent. Every handler that touches a window must go through
   `cx.defer(…)`. Nothing in the docs says this; cmd-q "worked" while cmd-n
   "did nothing", which is maximally misleading.
2. **Tray/hotkey event plumbing**: tray-icon and global-hotkey speak
   crossbeam channels; gpui timers + `try_recv` polling is easy but you must
   *know* to do it, plus the 300 ms NSMenu-dismiss settle before `cx.hide()`.
3. **A hard crash under menu automation**: gpui 0.2.2's
   `platform::mac::platform::menu_will_open` does a non-defensive RefCell
   borrow; one AX-scripted "set frontmost + click menu" sequence hit it while
   the app borrow was held → "RefCell already borrowed" → non-unwinding panic
   → abort. Not reproducible on the normal user path, but it shows the mac
   platform layer's re-entrancy guards are thin.
4. The multiline editor itself (shared with gpui-babel; see that FRICTION for
   the text-editing story).

## Totals

- LoC: 1575 (main.rs 739 + editor.rs 836, heavily commented)
- Binary: 8.4 MiB release (unstripped; 8,764,400 bytes); canonical serial
  clean build **67 s**.
- Idle CPU with the 80 ms pump: ~0.0–0.1 %.
