# FRICTION — Windows (Slint =1.17.1)

SPEC-9. Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2, rustc 1.96.1.
Default slint features + `raw-window-handle-06`; cupertino style (Slint's
macOS default), winit backend, femtovg renderer.

Built: `apps/slint-windows/` — 4 real top-level windows (main, inspector,
preferences, unsaved-changes prompt) plus a `Dialog` presented as a **real
macOS sheet**. Verified by a self-test hook (`WINDOWS_SELFTEST=1`,
19 checks, `evidence/selftest-log.txt`, all pass) and by external synthetic
input (`evidence/log.txt`: `scripts/window-count.swift`, `tools/synth/synth`,
and a 25-line CGEvent helper for real modifier flags — see "surprises").

LoC: app 985 (`src/main.rs` 494, `ui/main.slint` 404, `src/mac.rs` 84,
`build.rs` 3) + 474 lines of self-test harness (`src/selftest.rs`).
Binary 16,572,496 bytes. Clean release build 50 s.

## Headline

**Slint has no window model beyond "a component that happens to inherit
`Window`".** There is no parent, no owner, no modality, no application-level
shortcut table, and — the sharp edge — *no shared globals between two
windows*: `export global` is instantiated once per root component, so two
exported windows get two independent copies of every global, `Palette`
included. Everything cross-window is therefore Rust glue. What Slint *does*
give you is a `Model` that any number of windows can bind to, and that turns
out to be a perfectly good cross-window signal: this app keeps its entire
shared scalar state in a **one-row `VecModel<Shared>`** handed to all four
windows as the same `ModelRc`. `set_row_data` fires one `ModelNotify` and
every window re-renders. Real modality and real parenting were reachable
only by going down to `NSWindow` through `Window::window_handle()`.

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** | synthetic-input | Any `export component X inherits Window` is a window; `X::new()?` + `show()`/`hide()`. Singleton is your own bookkeeping (a flag in the shared row) — there is no window registry. `window-count.swift` 1 → 2 → 3 verified. |
| Modal dialog — kind achieved | **hand-rolled (OS window-modal)** | synthetic-input | Slint offers **no modality at all**. `Dialog` is only a `WindowItem` with an auto-laid-out button row. Real modality came from `-[NSWindow beginSheet:completionHandler:]` via `Window::window_handle()` → `RawWindowHandle::AppKit` → `[ns_view window]` (objc2). Result is a genuine macOS **OS window-modal sheet** (no titlebar, attached to the parent). A fallback in-window overlay (`if blocked: Rectangle { TouchArea {} }`) is kept for non-macOS and can be disabled with `WINDOWS_NO_OVERLAY=1`. |
| Parent blocked while modal | **built-in (once the sheet exists)** | synthetic-input | With `WINDOWS_NO_OVERLAY=1` (so only AppKit is under test) a CGEvent click on the main window's Delete button while the sheet is up produced **no** `DELETED` line; the identical click with no sheet deleted a row. `evidence/log.txt`, `evidence/modal-delete-blocked.png`. |
| Focus returns to parent after modal | **built-in** | self-test | `endSheet:` restores key status; we additionally `show()` the main window. `modal-closes` check. |
| Shared state across windows (live, bidirectional) | **assembled** | self-test + synthetic-input | One `Rc<VecModel<Project>>` + one `Rc<VecModel<Shared>>` as the same `ModelRc` in every window. Reads are reactive everywhere. Text inputs need an explicit down-sync (`changed src-name => if (e.text != src-name) e.text = src-name`) because a Slint `LineEdit` owns its `text` and the first keystroke would destroy a plain binding. `evidence/shared-state.png` shows typing in the inspector renaming the row in the main list live. |
| Cross-window message (Ping/Pong) + wake | **assembled** | self-test | No message bus, no channel, no wake needed: all windows are on one event loop, so `sh.set_row_data(0, …)` is the message. Ping = `+1` on the shared row; Pong = `flash=true` + a `slint::Timer` clearing it after 300 ms. |
| Theme/layout change applied to all windows | **hand-rolled** | self-test | `Palette` is a *per-component-instance* global, so `Palette.color-scheme = …` must be re-executed inside **every** window; each one carries `property <int> theme: sh[0].theme; changed theme => apply-theme()`. Objective evidence: mean window luminance light→dark, main 233→44, inspector 246→38, prefs 240→38. |
| Window parenting (child/owner/transient) | **hand-rolled** | by-construction + observed | winit 0.30's `with_parent_window` is Windows/X11 only and Slint exposes no attributes hook per window anyway. Used `-[NSWindow addChildWindow:ordered:]` (`addChildWindow: true` in the run log): the inspector then stays above the main window and is ordered out with it. Opens offset from the parent via `set_position`. |
| Close veto (CloseRequested → Save/Discard/Cancel) | **built-in** | self-test + synthetic-input | `window().on_close_requested(|| CloseRequestResponse::KeepWindowShown)` is exactly the hook. Because the handler must return synchronously, the prompt is a fourth Slint window rather than a blocking `rfd` call. ⌘W with unsaved changes → `close vetoed: unsaved changes`, "Save changes?" window appears, process alive. |
| Quit semantics | **built-in** | self-test | `run_event_loop_until_quit()` keeps the app alive when a child window hides; the main window's Save/Discard path calls `slint::quit_event_loop()` and the process exits 0 with the inspector still open. |
| Native confirm dialog | **assembled (rfd)** | by-construction | Slint has no message box. `rfd::AsyncMessageDialog` + `slint::spawn_local` (sync `rfd` inside an event-loop callback risks the same re-entrancy abort that `apps/slint-tray` hit with notify-rust). `WINDOWS_NO_CONFIRM=1` bypasses it for scripted tests. **Verifier note (2026-08-30):** exercised by the verifier — for this unbundled binary the rfd alert is presented *out-of-process* by `UserNotificationCenter` (a 260×202 window owned by that process, invisible to per-process AX, and it outlives the app), not an in-process NSAlert or sheet; the same `CFUserNotification` fallback the dioxus/xilem/iced apps hit. |
| Position/size persistence | **assembled** | synthetic-input | `Window::position()/set_position()/size()/set_size()` exist. **Trap:** they are *physical* and `scale_factor()` still reads `1.0` until the window is mapped, so a physical save/restore round-trip reopened the window at 2× size on a Retina display. Fixed by storing **logical** units and restoring from a 120 ms one-shot timer. Verified: `650 120 720 512` before quit and after relaunch. |
| Multi-monitor scale change | **not-verified** | — | Only one display available. From the code: `scale_factor()` is per-`slint::Window` and Slint re-renders on winit's `ScaleFactorChanged`; the two windows have independent adapters, so moving one cannot affect the other. The startup value being `1.0` before mapping (above) is the one real DPI hazard found. |
| Per-window shortcuts (⌘W focused only, ⌘, , ⌘⇧I) | **hand-rolled** | synthetic-input | No accelerator table: each window carries a root `FocusScope` with a `key-pressed` handler, and any window that must answer ⌘, / ⌘⇧I repeats the handler. `Window::close()` (DSL) routes through `on_close_requested`, so ⌘W gets the veto for free. Verified: ⌘⇧I toggles 1↔2 windows, ⌘, opens Preferences *from the inspector*, ⌘W with the inspector focused hides only the inspector. |

## Helper crates

| Crate | Pin | Why |
|---|---|---|
| `rfd` | `=0.15.4` | Native message box for the Delete confirmation. Slint ships none. |
| `raw-window-handle` | `=0.6.2` | rwh-0.6 types, to read `slint::Window::window_handle()` (needs slint feature `raw-window-handle-06`, not on by default). |
| `objc2` | `=0.5.2` (macOS only) | `beginSheet:`/`endSheet:`/`addChildWindow:` — the two window-model concepts Slint doesn't model. Version matched to winit 0.30's objc2. |

Tried and rejected: `serde`/`serde_json` for the geometry file (the whole
state is 13 numbers — a 4-line `k v1 v2 v3 v4` text file is 20 lines of Rust
and keeps the dependency graph honest); `slint`'s `unstable-winit-030`
(`WinitWindowAccessor::with_winit_window` would give `set_window_level`, but
winit still has no modality and no macOS parenting, so it buys nothing that
`window_handle()` doesn't).

## Where the time went

1. ~30% modality: establishing that Slint has none, then getting a sheet out
   of an already-visible winit window (`orderOut:` first, then `beginSheet:`).
2. ~20% the shortcut dead end (see surprises) — the modifier never arrived.
3. ~20% the "one source of truth" plumbing: discovering globals are
   per-instance, choosing the one-row model, and the `changed`-handler
   down-sync for text inputs.
4. ~15% geometry persistence (the scale-factor-1.0-before-mapping trap).
5. ~15% the self-test harness and the external scripts.

## Surprises

- **Good:** a shared `ModelRc` is a genuinely pleasant cross-window signal.
  One `set_row_data` repaints four windows with no channels, no `Weak`
  upgrades, no wake calls. `Window::close()` from the DSL routing through
  the Rust `on_close_requested` is also exactly right.
- **Good:** `Dialog` + `StandardButton { kind: ok/cancel; }` auto-generates
  `ok-clicked` / `cancel-clicked` on the component's public API and lays the
  buttons out in macOS order (Cancel left, OK right) without being told.
- **Bad:** `export global` is *not* process-wide. Two exported windows =
  two `Palette`s, two of everything. Anyone porting a Qt/GTK app will write
  a global singleton first and then discover their preferences window is
  styling itself only.
- **Bad (and expensive):** on macOS **Slint remaps modifiers —
  `modifiers.control` is the Command key and `modifiers.meta` is Control**
  (`i-slint-core/input.rs:836`). `e.modifiers.meta && e.text == "w"` silently
  never fires. Undocumented in the element reference.
- **Test-harness trap worth recording for the corpus:** `osascript … keystroke
  "w" using command down` posts a key event whose modifier flags winit never
  sees — the Slint handler observed `ctrl=0 meta=0 alt=0 shift=0`. Modifier
  shortcuts can only be exercised with a `CGEvent` that also posts
  `flagsChanged` (25-line Swift helper in the scratchpad, invocation recorded
  in `evidence/log.txt`). Every earlier "shortcut doesn't work" result taken
  with `osascript keystroke … using` should be re-read with this in mind.
- **Bad:** the first click into an unfocused Slint window is swallowed as an
  activation click (`acceptsFirstMouse` is not set), so scripted clicks need
  a warm-up click.
- **Bad:** `always-on-top: true` raises the NSWindow level above 0, which
  makes `scripts/window-count.swift` (layer == 0) stop seeing the window.
- **Neutral:** `changed <property>` handlers are deferred to the next
  event-loop iteration; a self-test that sets a property and samples the
  rendered result in the same tick sees the old frame.

## Window model

**A window in Slint is a component type, and a "window handle" is a component
instance.** `MainWindow::new()` builds an item tree that happens to have a
`WindowItem` at the root; `ComponentHandle::window()` hands you a
`slint::Window` (a thin `WindowInner` wrapper) for the OS-facing bits —
position, size, `on_close_requested`, `window_handle()`. Windows are values
in the ownership sense (drop the last handle and the window goes away) but
they are *not* values in the data sense: there is no window tree, no
`Vec<Window>` the framework maintains, and no way to ask "which windows are
open" — the app keeps that list. Nothing is shared between two instances,
including globals, so **two windows seeing the same mutable state means
handing both the same `ModelRc`**. Concretely, the whole cross-window story
in this app is:

```rust
let sh = Rc::new(VecModel::from(vec![Shared { … }]));   // one row
let sm = ModelRc::from(sh.clone());
main.set_sh(sm.clone()); insp.set_sh(sm.clone()); prefs.set_sh(sm);
// …and every write is
sh.set_row_data(0, s);   // one ModelNotify -> all windows repaint
```

with `property <Shared> s: root.sh[0];` on the DSL side. Writes cannot be
done from the DSL (a model row is only assignable through a repeater
variable), so every mutation is a `callback` into Rust. That is the shape of
a Slint multi-window app: reactive reads through models, imperative writes
through Rust, and one Rust struct playing the role that a global signal graph
plays in a reactive framework.

## Approximated / skipped

- **Multi-monitor scale change (req. 10)** — not-verified, single display.
- **Sheet placement:** AppKit put the sheet centred in the parent rather
  than sliding from its titlebar (winit had already positioned and shown the
  window before `beginSheet:`). Behaviourally it is a sheet — it blocks the
  parent and has no titlebar — but it doesn't animate down from the top.
- **Verifier note (2026-08-30) — windows first shown from a callback do not
  paint until something forces a render.** In the verifier's runs the sheet
  was a blank dark rounded rectangle for as long as it stayed untouched; it
  painted fully (and took typing) after a click into it. The `Save changes?`
  prompt was worse: its body stayed *transparent* (the main window's rows
  visible through it) even after a click into it, and only painted after an
  AX-driven resize (`set size of window "Save changes?" to {340, 120}`).
  `evidence/modal-open.png` shows the sheet *after* it had been clicked into.
  The "Esc needs a prior click" item below is the same defect. Also: AppKit
  shifts the parent up when the sheet attaches (frame y 630 → 555 here), so
  button coordinates cached before `beginSheet:` are stale — re-read the
  frame (the same trap the iced/tauri sheets showed).
- **Verifier note (2026-08-30) — secondary window sizes.** The Inspector,
  Preferences and prompt windows open at their layout minimum, not their
  `preferred-width/height`: Inspector 244×236, Preferences 123×202 and
  prompt 211×100 logical (the `bounds` lines in `evidence/log.txt` show the
  same 244/123 widths), versus the spec's ~360×300 / ~320×200. The
  Preferences window clips its own controls (`shared-state.png`).
- **Esc inside the sheet** only works after the sheet has been clicked into:
  `forward-focus` on the dialog is applied at instantiation, and the sheet
  does not receive Slint focus purely from being made key. The Cancel button
  works immediately.
- **Save** in the unsaved-changes prompt logs instead of writing (there is no
  document to save in this spec).

## Measurements

- Clean release build **50.1 s**; incremental no-op **0.4 s**.
- Binary **16,572,496 bytes** (unstripped, `debug = false`).
- `cargo build --release --locked` succeeds unchanged; `Cargo.lock` committed.
