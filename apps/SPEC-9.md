# SPEC-9: "Windows" — multi-window & modal test

Iteration 5 (proposed 2026-08-30, prompted by an r/rust comment: "most GUI
apps I developed contained at least 10 windows, mostly modal … have you
considered the problems of passing around data between windows?"). Every
earlier spec is a one-window app; SPEC-4 opened a second window but never
moved data through it. This app tests the **window model**: modality,
parenting, shared state across windows, cross-window messages, close
semantics, and focus. Build idiomatically; the effort profile is the output.

## Functional requirements

1. **Main window** titled `Windows (<framework>)`, ~720×480, resizable. A
   list of 6 seed "projects" (name, owner, budget as a number, status
   Open/Closed) with single selection, a status bar showing
   `selected: <name> · pings: <n>`, and a toolbar: **Edit…**, **Inspector**,
   **Preferences…**, **Delete**.
2. **Modal edit dialog** (Edit… or double-click a row): edits the selected
   project's name and budget. **OK** commits, **Cancel**/Esc discards, Enter
   commits. While the dialog is open the main window must not accept input
   (a click on the main window's Delete must do nothing). Ship the most
   native modality the framework offers and record which of these it is:
   OS window-modal (macOS sheet / Windows owner-disabled / Wayland
   `xdg_dialog` modal), OS app-modal, or an in-framework overlay that only
   *looks* modal. Focus must return to the main window when the dialog
   closes.
3. **Inspector window**: a second top-level window (~360×300) showing the
   selected project's fields as editable inputs. Selection changes in the
   main window update it live; edits typed into it update the main list
   live (bidirectional shared state — one source of truth, no copy).
   Inspector is a **singleton**: the button focuses the existing window
   rather than opening a second one. Closing it must not quit the app.
4. **Cross-window message**: a **Ping** button in the inspector increments
   the main window's `pings` counter; a **Pong** button in the main window
   flashes the inspector's background for 300 ms. Record the mechanism
   (shared signal, message bus, channel + wake, framework "window event").
5. **Preferences window**: singleton, non-modal, ~320×200, with a
   *Compact rows* toggle that re-lays-out the main list immediately and a
   *Theme* Light/Dark/System radio that restyles **every** open window at
   once.
6. **Window parenting**: the inspector should behave as a child of the
   main window where the OS has that notion (stays above its parent, is
   hidden/minimised with it, opens offset from it). Record which of
   `parent`/`owner`/`transient_for` the framework can express and what was
   observed.
7. **Close semantics**: the main window has an unsaved-changes flag (set by
   any edit). Closing it while dirty asks **Save changes?** with
   Save/Discard/Cancel — Cancel must veto the close (`CloseRequested`
   interception). Closing the main window with Save/Discard quits the app
   even if the inspector or preferences are still open. ⌘W/Ctrl+W closes
   the *focused* window only.
8. **Native confirm**: Delete asks **Delete "<name>"?** Yes/No using the most
   native message box available (rfd, framework dialog, or hand-rolled —
   record which).
9. **Persistence**: every window remembers its position and size across
   runs (a JSON file next to the binary is fine); on relaunch the main
   window reappears where it was and re-opens the inspector if it was open.
10. **Multi-monitor / DPI**: with two displays of different scale factor
    attached, drag the inspector to the second display. Text must re-render
    crisply at the new scale and the main window must be unaffected. If
    only one display is available, record `scale_factor` handling from the
    code and mark the cell *not-verified*.
11. **Keyboard**: ⌘,/Ctrl+, opens Preferences, ⌘⇧I/Ctrl+Shift+I toggles the
    inspector, Tab order inside the modal is sane, and Esc closes the
    topmost non-main window.

## Implementation rules

Same as SPEC-4: independent crate at `apps/<framework>-windows/` (package
`<framework>-windows`), same pinned framework version as
`apps/<framework>-app/`, Rust helper crates allowed and recorded (rfd,
winit extension traits, serde for persistence), no external JS libraries for
webviews, fallback rule applies. Budget guide: ~3 serious attempts per
capability, then document and move on.

Verification on macOS (retain the evidence):
- `scripts/window-count.swift <pid>` must report 1, 2, 3 windows at the
  right moments (main; +inspector; +preferences).
- Modality: with the modal open, post a synthetic click on the main window's
  Delete button (CGEvent) and screenshot — the row count must not change.
- Shared state: type in the inspector, screenshot the main window.
- Close veto: dirty the state, ⌘W the main window, choose Cancel, confirm
  the process and window survive; then Discard and confirm exit code 0.
- Persistence: move the main window, quit, relaunch, compare bounds.

## FRICTION.md (required, per app)

Rating (built-in / assembled / hand-rolled / not-achievable) + short note for
each capability:

| Capability |
|---|
| Second top-level window (open, close, singleton focus) |
| Modal dialog — kind achieved (OS window-modal / OS app-modal / overlay) |
| Parent blocked while modal (verified by synthetic click) |
| Focus returns to parent after modal |
| Shared state across windows (live, bidirectional) |
| Cross-window message (Ping/Pong) + wake mechanism |
| Theme/layout change applied to all windows |
| Window parenting (child/owner/transient) |
| Close veto (CloseRequested → Save/Discard/Cancel) |
| Quit semantics (main closes ⇒ app exits; child closes ⇒ app lives) |
| Native confirm dialog |
| Position/size persistence |
| Multi-monitor scale change |
| Per-window shortcuts (⌘W focused window only, ⌘, , ⌘⇧I) |

Also: helper crates used + why, total LoC, where the time went, surprises,
and — new for this spec — a **"window model" paragraph**: is a window a
value, an entity, a view, a component, or a handle in this framework, and
what does the code look like when two windows must see the same mutable
state?

## Why this spec

The corpus' sharpest shell findings came from the seams between a
framework's event loop and the OS. Modality and parenting are the same seam
one level up: winit exposes `with_parent_window`/`with_owner_window` and
per-window `CloseRequested`, but no modal API at all; macOS sheets, Windows
owner-disabling and Wayland `xdg_dialog` are three different shapes; and
"one source of truth across windows" is where reactive frameworks either
shine (one signal graph) or hurt (per-window `App` state). No comparable
public measurement exists for Rust frameworks.

## Reference machine

Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1 (same pins as
iteration 1); Linux/Windows reruns follow the round-5/round-6 harnesses.
