# FRICTION — Windows (Tauri =2.11.5)

SPEC-9. Tauri =2.11.5 / tauri-build =2.6.3 — the same pins and the same manual
no-Node setup as `../tauri-app` (hand-written `tauri.conf.json`, hand-written
capability, `withGlobalTauri`, static vanilla HTML/CSS/JS in `ui/`, copied
icons, `edition = "2021"` as in every other Tauri app in the corpus).
Four windows: `main` (from config), `inspector`, `prefs`, `edit` (all built at
runtime with `WebviewWindowBuilder`).

Built and verified on macOS 26.5.2 (M4 Pro, one 1512×982 display @2×).
Evidence: `evidence/log.txt` (command-by-command), `evidence/selftest-log.txt`
(`WINDOWS_SELFTEST=1` → `SELFTEST DONE pass=15 fail=0`, exit 0), and 11 PNGs.

## Design choice: one model, in Rust, broadcast to every window

There is exactly one copy of the state and it lives in `Mutex<Model>` in the
Rust core. No window owns anything. Every window (a) pulls a snapshot with
`get_model` on load and (b) subscribes to the `model` event, which Rust
re-emits to *all* windows after every mutation. Writes from any window go
through the same `#[tauri::command]`s. That is the whole "shared state across
windows" story — about 12 lines — and it is why main↔inspector is bidirectional
with no copy and no sync code.

## Capability ratings

Scale note (as in `../tauri-tray`): **built-in** = in the `tauri` crate itself;
**assembled** = an official first-party plugin (one Cargo line + one
`.plugin()` line).

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** | self-test + synthetic-input | `WebviewWindowBuilder::new(app, label, WebviewUrl::App("inspector.html"))`. Singleton is 3 lines of your own (`get_webview_window(label)` → `set_focus()`); the framework will happily build a second window with the same label and error. `window-count.swift` reported 1 → 2 → 3, and 3 again after re-clicking Inspector. |
| Modal dialog — kind achieved | **assembled → OS window-modal** | synthetic-input + by-construction | There is no `.modal()` flag. But `WebviewWindow::set_enabled(false)` **is** a real OS block: on macOS `tauri-runtime-wry/src/window/macos.rs` allocates a bare NSWindow the size of the parent frame, `setAlphaValue(0.5)`, and `beginSheet:completionHandler:` — AppKit's own window-modal mechanism (Electron's trick, cited in the source). Windows: `EnableWindow`; Linux: GTK `set_sensitive`. So: **OS window-modal**, assembled from `set_enabled` + a hand-built dialog window, not a modal API. |
| Parent blocked while modal (verified by synthetic click) | **built-in** | synthetic-input | Three CGEvent clicks at the Delete button's exact AX-reported centre (301,438) while the modal was open: each landed on the untitled disabling-sheet window (id 11629), never on main. Neither the button handler (`[main] Delete pressed`) nor the JS fallback veil (`click swallowed by modal veil`) fired, and the row count stayed 6. Control: the same click with no modal open printed `[main] Delete pressed` and raised the confirm sheet. `evidence/10-…png`, `evidence/11-…png`. |
| Focus returns to parent after modal | **built-in** | self-test | `end_modal()` → `main.set_enabled(true)` (ends the sheet) + `set_focus()`. Self-test asserts `main.is_focused() == true` afterwards and prints the per-window focus map. |
| Shared state across windows (live, bidirectional) | **built-in** | self-test + observed | `app.emit("model", snapshot)` after every mutation; every window has one `listen("model")`. `evidence/07-shared-state-live.png`: typing in the inspector changed main's row and status bar in the same frame. The event bus is the only permission the app needs (`core:default`). |
| Cross-window message (Ping/Pong) + wake mechanism | **built-in** | self-test | Same bus, addressed: `app.emit_to("inspector", "flash", 300)` for Pong, and Ping is just a command that mutates + re-broadcasts. No wake/channel plumbing: the webviews are always live, the IPC hop is the wake. |
| Theme/layout change applied to all windows | **built-in** | self-test + observed | Two halves. Native chrome: `for (_, w) in app.webview_windows() { w.set_theme(t) }` — self-test asserts `w.theme() == Dark` for all three. CSS: the same broadcast sets `data-theme` on `<html>`, so tokens swap with zero JS. Compact rows is one CSS custom property. `evidence/03-…png`. |
| Window parenting (child/owner/transient) | **built-in** | observed + by-construction | `WebviewWindowBuilder::parent(&main)` — tao maps this to `[parent addChildWindow:ordered:NSWindowAbove]` on macOS, `HWND` owner on Windows, `gtk_window_set_transient_for` on Linux; `parent_raw` also exists. Observed: the inspector stays above main, hides/minimises with it, and `screencapture -o -l <main-id>` returns *both* windows in one image — the OS treats them as one group. |
| Close veto (CloseRequested → Save/Discard/Cancel) | **built-in** | synthetic-input | `WindowEvent::CloseRequested { api, .. }` + `api.prevent_close()`. Cancel → `[win] close CANCELLED — window survives`, process and window alive. Discard → `app.exit(0)`, exit code **0**, verified with a wrapper. `evidence/12-close-veto-sheet.png`. |
| Quit semantics (main closes ⇒ app exits; child closes ⇒ app lives) | **built-in** | self-test + synthetic-input | Main always `prevent_close()`es first and then decides, so "main closes ⇒ exit" holds even with the inspector open (verified: exit 0 with the inspector still up). Child windows use the default path; ⌘W on the inspector closed only it. |
| Native confirm dialog | **assembled** | synthetic-input | `tauri-plugin-dialog` =2.7.1 (rfd 0.17 underneath). With `.parent(&main)` rfd calls `beginSheetModalForWindow_completionHandler`, so it is a real macOS **sheet**, not a floating alert. `evidence/09-native-confirm-sheet.png`. Callback style, not blocking: `blocking_show()` from a command would deadlock the main thread. |
| Position/size persistence | **assembled** | synthetic-input | `tauri-plugin-window-state` =2.4.1 restores on `on_window_ready` and saves on `RunEvent::Exit` — including runtime-built windows, for free. It does **not** remember which windows existed, so a 3-line sibling-of-the-binary `windows-session.json` carries `inspectorOpen`. Verified: quit at main 120,300 / inspector 900,620 → relaunch reproduced both exactly and re-opened the inspector. |
| Multi-monitor scale change | **not-verified** | not-verified | One display only. `WindowEvent::ScaleFactorChanged` is handled (logs label → factor) and `WebviewWindow::scale_factor()` is read per window in the `census` command (all reported 2.0). wry re-rasterises the WKWebView itself, so no app-side work is expected; unproven here. |
| Per-window shortcuts (⌘W focused window only, ⌘, , ⌘⇧I) | **built-in** | synthetic-input | `PredefinedMenuItem::close_window` carries ⌘W and targets the focused window natively (verified: ⌘W on the inspector closed only the inspector, ⌘W on main hit the veto). `MenuItem::with_id(..., Some("CmdOrCtrl+,"))` and `"CmdOrCtrl+Shift+I"` for the other two — both verified via `osascript` keystrokes. Esc-closes-topmost is 3 lines of JS in each non-main window (the focused window is the one receiving the keystroke; there is no framework "topmost"). |

## The two traps that cost the time

1. **`set_enabled(false)` is not a no-op on macOS, and its sheet outranks your
   dialog.** The disabling sheet is the size of the *parent's frame* and is
   ordered above it — so a dialog positioned over the parent is covered and
   becomes unclickable by mouse (AX clicks still work, which is how this was
   nearly missed). `always_on_top` lifts the dialog back out… but
   `always_on_top` and `parent` are **mutually exclusive on macOS**:
   `addChildWindow:` resets the child to the parent's window level, sinking it
   under the sheet again. Verified both ways with a CGWindowList probe. Final
   shape: the **inspector** keeps `.parent()` (that is what SPEC-9 §6 measures),
   the **modal** trades parenting for reachability (`always_on_top`, no parent).
2. **AppKit moves your window when the sheet does not fit.** Opening the modal
   shifted main from y=446 to y=384 so the 480-px-tall disabling sheet fit on
   screen. Anything that caches window coordinates across a `set_enabled` must
   re-read them.

Minor: `WebviewWindowBuilder` must run on the main thread (from worker threads,
`run_on_main_thread`); and the toolbar's Inspector button is relabelled while
the inspector is open, which is enough to break a name-based AX script.

## Helper crates

| Crate | Why | Underneath |
|---|---|---|
| `tauri-plugin-dialog` =2.7.1 | native Yes/No confirm and the Save/Discard/Cancel veto prompt; `.parent()` makes them macOS sheets | rfd 0.17 |
| `tauri-plugin-window-state` =2.4.1 | position/size persistence for all four windows | own JSON in `app_config_dir` |
| `serde` / `serde_json` | `#[tauri::command]` (de)serialisation across IPC; reading the two small JSON files | — |

Both plugins are driven **exclusively from Rust**, which bypasses the ACL
entirely: the capability file needs no plugin permissions, only `core:default`
(for the event bus) and the four window labels. Total ACL bill: **zero lines**
beyond listing the windows.

Tried and rejected: `rfd` directly (the dialog plugin already wraps it and
gives `.parent()` a `WebviewWindow` for free); an `objc2` dependency to call
`beginSheet:` on the dialog window itself (unnecessary — `set_enabled` already
does exactly that, and hosting a Tauri webview *inside* an AppKit sheet is not
expressible through the public API); a hand-rolled JSON geometry store (the
official plugin is two lines and handles runtime-built windows).

## LoC (1 007 source; 1 043 including config) & size

- Rust: **683** (677 `src/main.rs` code lines + 6 `build.rs`); of those ~150 are
  the `WINDOWS_SELFTEST` harness, which is verification, not app code. Counting
  comments and blanks, `src/main.rs` is 801 lines.
- Frontend: **324** (86 HTML across 4 pages, 147 JS across 5 files, 67 CSS,
  plus 24 blank/comment lines already counted in those files).
- Config: 36 (`tauri.conf.json` 29 + capability 7).
- Release binary **8.85 MiB**; **446 packages** in `Cargo.lock` vs the
  `tauri-app` baseline's 418 — the two plugins cost **28 packages**.
- Clean `cargo build --release`: **37.5 s**. `cargo build --release --locked`
  afterwards: clean, no lockfile churn.

## Where the time went

1. Discovering what `set_enabled` actually does. tao 0.35.3 has no
   `set_enabled` at all — the implementation lives in *tauri-runtime-wry*'s own
   per-platform `WindowExt`, which is easy to miss and is why the first version
   of this app shipped a JS overlay it did not need.
2. The parent/always-on-top/sheet z-order fight (trap 1), which is only
   observable with a CoreGraphics window-order probe; a screenshot looks fine
   either way.
3. Verification under contention: six sibling agents were driving their own GUI
   apps on the same display, several with always-on-top windows over the
   default spawn area, so every coordinate click had to be gated on "is this
   point mine right now" and most non-mouse steps were re-routed through the
   accessibility API.

## Surprises

- **Good:** Tauri has real OS window-modality and almost nobody knows, because
  it is spelled `set_enabled(false)` and documented as "Enable or disable the
  window." One line replaces the usual overlay hack, on all three platforms.
- **Good:** the whole cross-window data problem is one `emit` and one `listen`.
  Adding the third and fourth window cost zero state code.
- **Good:** WKWebView exposes the entire DOM to the macOS accessibility tree —
  `click button "Preferences…"` works from AppleScript with no AccessKit and no
  app-side work.
- **Bad:** the disabling sheet is invisible in screenshots and in
  `webview_windows()`, but it *is* a window to CoreGraphics (window counts are
  off by one whenever a modal or a dialog is up) and it silently outranks child
  windows.
- **Bad:** `window-state` remembers geometry but not which windows were open —
  the "re-open the inspector" half of §9 is yours to write.

## Window model

**A window is a handle to an OS resource, plus a URL.** `WebviewWindow` is a
cheap `Clone` handle (label + dispatcher) that you look up by string label from
any `AppHandle` — `app.get_webview_window("inspector")` — from any thread. It
is not a value you own, not a component in a tree, and not part of any
reactive graph: the framework keeps a `HashMap<String, WebviewWindow>` and you
address windows the way you address files. Content is a *separate* thing: an
HTML document loaded from `WebviewUrl::App("inspector.html")`, with its own
JS heap and no shared memory with any other window.

That split decides what "two windows see the same mutable state" looks like.
There is no way to hand a signal, a struct, or a reference across the boundary,
so state cannot live in a window; it has to live in the Rust core behind a
`Mutex` and travel as serialised snapshots:

```rust
fn broadcast(app: &AppHandle) {
    let s = { let st = app.state::<AppState>(); snap(app, &st.0.lock().unwrap()) };
    let _ = app.emit("model", s);          // -> every window
}
#[tauri::command]                          // the single write path, used by BOTH windows
fn set_field(app: AppHandle, id: u32, field: String, value: String) { …; broadcast(&app); }
```

and on the other side, in every window, one subscription:

```js
await listen("model", (e) => render(e.payload));
M = await invoke("get_model");
```

The cost is that every update is a JSON round trip and re-renders whole
windows (fine at 6 rows, a real design constraint at 100 000 — see
`../tauri-grid`), and that you must not clobber the field the user is typing
in, which is 3 lines of "skip the focused input" in the inspector. The benefit
is that the answer to "who owns the truth" is never ambiguous, adding a window
costs one HTML file and one `listen`, and the same code shape works whether
there are two windows or ten.

## Approximated or skipped

- **§2 "the most native modality":** shipped as OS window-modal via
  `set_enabled(false)`, but the dialog is a plain top-level window rather than a
  macOS *sheet* containing the form — Tauri cannot put a webview inside an
  NSWindow it did not create. The dialog is also not `parent`ed (trap 1).
- **§6 parenting:** verified for the inspector on macOS only ("stays above",
  "hidden with parent", "opens offset"); the Windows-owner and Wayland
  `transient_for` paths are read from tao's source, not exercised.
- **§10 multi-monitor:** not verified — a single display was attached.
- **§11 "Esc closes the topmost non-main window":** implemented as "Esc closes
  the *focused* window", which is the same thing in practice; there is no
  framework-level z-order query to do better.
- The JS `#veil` overlay is retained as a visible cue and as the fallback for
  any platform where `set_enabled` is not honoured, but on macOS it is
  redundant — the OS block happens first, which the modality evidence shows.
