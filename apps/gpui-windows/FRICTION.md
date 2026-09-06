# FRICTION — Windows (gpui =0.2.2)

Reference: `apps/SPEC-9.md`. Built and verified on macOS 26.5.2 (M4 Pro,
rustc/cargo 1.96.1) against the same pin as `apps/gpui-app`
(`gpui = "=0.2.2"`, feature `runtime_shaders` — see `apps/gpui-app/GAPS.md`).

- `cargo build --release`: clean, **1 m 31 s** cold (all deps); the only noise
  is the known transitive `block v0.1.6` future-incompat note.
  `cargo build --release --locked` afterwards: succeeds unchanged (0.72 s).
- Binary: **5.72 MiB** unstripped (5,720,240 B).
- LoC (`wc -l src/main.rs`): **1422** total = **1194 production** +
  **228 self-test**. That is well over the brief's ~700 guide and the number is
  reported as-is: four window types, each ~90–165 lines, are what SPEC-9 costs
  in gpui, and gpui's fluent builder style (`div().flex().px_2()…`) spends
  roughly twice the lines of a markup-shaped framework for the same UI. For
  scale inside the same corpus: gpui-board 515, gpui-grid 655, gpui-tray
  739 + 836.
- Helper crates: **none**. `gpui` alone (see below).
- Evidence: `evidence/log.txt` (every command), `evidence/selftest-log.txt`
  (`SELFTEST DONE pass=16 fail=0`, exit 0), 15 screenshots.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** | synthetic-input + self-test | `cx.open_window(WindowOptions{titlebar, window_bounds, ..}, \|window, cx\| cx.new(..))` returns a `Copy` `WindowHandle<V>`; `handle.update(..)` returns `Err` once the window is gone, which is also how you detect a stale handle. Singleton = keep the handle in a `Global`, and on a second request call `window.activate_window()` instead of opening. Closing is `window.remove_window()`. Verified: window-count 1→2→3, re-click logs "inspector already open — focused it". |
| Modal dialog — kind achieved | **hand-rolled overlay (custom content) / OS window-modal sheet (buttons only)** | observed + synthetic-input | Two different answers, and the split *is* the finding. `window.prompt(PromptLevel, msg, detail, &["Save","Discard","Cancel"], cx)` is a **real OS window-modal sheet** — the mac backend builds an `NSAlert` and calls `beginSheetModalForWindow:` (`src/platform/mac/window.rs:1198`), returning a `oneshot::Receiver<usize>`; screenshots 10 and 12. But it is *button-only*: no accessory view, no custom content, no way to make an arbitrary gpui window modal. So the Edit dialog is a plain second window plus an app-level gate: a `Store.modal` flag, a full-window `.occlude()` scrim on the parent, and a click on that scrim re-raises the dialog (the macOS modal "bounce"). Honest label per the spec's list: **in-framework overlay that only looks modal**. |
| Parent blocked while modal | **hand-rolled** | synthetic-input | A synthetic CGEvent click on the main window's Delete button while the dialog is open produced byte-identical before/after screenshots (`md5` equal, log §4) and no confirm sheet. The block is ours, not the OS's: the main window can still be focused, moved and closed — nothing is disabled at the AppKit level. |
| Focus returns to parent after modal | **hand-rolled** | synthetic-input | Nothing automatic: `close_modal` explicitly calls `main.update(.., \|_, window, _\| window.activate_window())`. Verified by Esc-ing the dialog (log §5). |
| Shared state across windows (live, bidirectional) | **built-in** | synthetic-input + self-test | The strongest part of gpui for this spec. One `Entity<Store>` is passed to every root view; each view's constructor does `cx.observe(&store, \|_, _, cx\| cx.notify())` — one line — and every window repaints when any window calls `store.update(..)`. The Inspector's Name field writes *directly* into `store.projects[sel].name`; there is no second copy of the value anywhere in the program. Typing " v2" into the Inspector changed the main window's list in the same frame (04/05). |
| Cross-window message (Ping/Pong) + wake mechanism | **built-in** | synthetic-input + self-test | Two mechanisms, both first-party. Ping needs no message at all: it mutates the shared entity and observation does the rest (main counter went 0→2 from the Inspector's button, 03). Pong is genuinely transient so it is an event: `impl EventEmitter<StoreEvent> for Store` + `cx.emit(Pong)` in the main window, `cx.subscribe(&store, ..)` in the Inspector, which sets a flash flag and clears it after a `cx.background_executor().timer(300 ms)`. No channel, no waker, no manual repaint request. |
| Theme/layout change applied to all windows | **built-in** | synthetic-input + self-test | Same one-field/three-observers mechanism: `Store.mode` and `Store.compact` are read inside each window's `render`. Clicking "Light" in Preferences repainted main, inspector and preferences together (13a/13b/13c); compact rows relayout the main list immediately. `window.observe_window_appearance(\|_, cx\| cx.refresh_windows())` covers the System option. |
| Window parenting (child/owner/transient) | **not-achievable** | observed | `WindowOptions` (`src/platform.rs:1089`) has **no** `parent`, `owner` or `transient_for` field — nothing to express. `WindowKind::Floating` is documented as "a floating window that appears on top of its parent window", but on macOS `Normal` and `Floating` take the identical code path and the same `NSNormalWindowLevel` (`platform/mac/window.rs:621,779`); only `PopUp` differs (a non-activating `NSPanel` at `NSPopUpWindowLevel`, which floats above *every* app and is invisible to `CGWindowListCopyWindowInfo`'s layer-0 filter, so it is wrong for a dialog). Observed consequences: raising the main window puts it *in front of* the inspector, and minimising the main window leaves the inspector on screen (log §9). All that was achievable is opening the child at an offset from `main.bounds().origin`. |
| Close veto (CloseRequested → Save/Discard/Cancel) | **built-in** | synthetic-input | `window.on_window_should_close(cx, \|window, cx\| -> bool)` is the exact hook, and the prompt it raises is the native sheet. Cancel → process alive, window intact; Discard → exit code 0 (log §7). One trap: the same decision function called from ⌘W only *decides* — ⌘W has to call `window.remove_window()` itself, because there `AppKit` is not the one doing the closing. |
| Quit semantics (main closes ⇒ app exits; child closes ⇒ app lives) | **assembled** | self-test + synthetic-input | gpui does **not** quit when the last window closes (same finding as iteration 1). `cx.on_window_closed(\|cx\| …)` fires for every window; we compare `cx.windows()` against the stored main handle and `cx.quit()` only if the main window is gone, and prune stale child handles in the same callback. Closing the inspector leaves 2 windows and a live process. |
| Native confirm dialog | **built-in** | synthetic-input | `window.prompt(PromptLevel::Warning, "Delete “X”?", Some(detail), &["Yes","No"], cx)`. No `rfd` needed. Note the mac backend deliberately re-orders the buttons so the last non-"Cancel" answer is added last (it keeps focus and the Space key) and binds Escape to any button titled "Cancel" — the returned index still maps to your original array. Its buttons are the only part of the app that appears in the accessibility tree. |
| Position/size persistence | **assembled (with a real trap)** | synthetic-input | `window.bounds()` + `WindowBounds::Windowed(bounds)` + a 4-line text file next to the binary. Two things gpui does not give you: (1) **there is no window-moved/resized event**, so bounds are polled on a 1 s `background_executor().timer`; (2) **`WindowOptions.window_bounds` is a *content* rect but `window.bounds()` returns the *frame* rect** (NSWindow frame, titlebar included), so feeding one back into the other grows every window by 32 px per launch — reproduced 480 → 512 → 544 before the fix. The app now measures the delta once at startup and subtracts it when saving. Verified end-to-end by AX-moving the window, quitting and relaunching (log §8). |
| Multi-monitor scale change | **not-verified** | not-verified | Only one display attached. The code path exists and is read out by the self-test: `cx.displays()` (→ `DISPLAYS 1`), `window.scale_factor()` (→ `2`), and `WindowOptions.display_id: Option<DisplayId>` to choose a display at open time. gpui re-shapes text per window from the window's own scale factor, so a drag to a second display should re-render crisply, but that was not exercised. |
| Per-window shortcuts (⌘W focused window only, ⌘, , ⌘⇧I) | **assembled** | synthetic-input | `cx.bind_keys([KeyBinding::new("cmd-,", OpenPrefs, None), …])` + `actions!`. The trap: **element-level `.on_action()` on a window's root div never fired for these bindings**, while global `cx.on_action` did — three attempts (root div with `track_focus` + `key_context`, verified focused, keystroke confirmed arriving via an `on_key_down` log). So ⌘W is a global handler that resolves the target with `cx.active_window()` and compares it against the stored handles — which is arguably the most direct expression of "the focused window only" anyway. Every handler that re-enters a window must be wrapped in `cx.defer(..)` (the re-entrancy trap already recorded in `apps/gpui-tray/FRICTION.md`). |

Tab order inside the modal is a bonus **built-in**: `cx.focus_handle().tab_index(n).tab_stop(true)` plus `window.focus_next()/focus_prev()` (the bundled `examples/tab_stop.rs`), verified in screenshot 09.

## Helper crates

**None.** `gpui = "=0.2.2"` with `runtime_shaders`, nothing else — the same
dependency story as `gpui-board` and `gpui-grid`.

- `rfd` — **rejected**: `window.prompt` is a native NSAlert sheet and
  `cx.prompt_for_paths` covers file dialogs, so rfd would have been strictly
  worse (app-modal, extra crate).
- `serde` / `serde_json` — **rejected**: the persisted state is four rects; a
  whitespace-separated text file parses in 15 lines and keeps the gpui apps'
  dependency count comparable across the corpus.
- `gpui-component` (a published third-party widget kit with inputs, dialogs
  and a table) exists on crates.io and was again **not** evaluated, per the
  core-only rule used by every other gpui app here. The hand-rolled text field
  therefore measures core gpui, not the ecosystem.

## Where the time went

1. **Proving modality on a shared desktop** (by far the largest slice). Six
   sibling agents were spawning windows on the same display, so region
   screenshots were unusable and CGEvent clicks landed on other agents' apps.
   The workable recipe — `screencapture -o -l<CGWindowID>` for evidence,
   activate-verify-retry before every click — took several rounds to find.
   gpui contributes its own hazard here: an occluded gpui window stops
   painting, so a stale frame can be captured (already noted in
   `apps/gpui-grid/FRICTION.md`).
2. **The frame-vs-content bounds asymmetry.** Persistence "worked" and the
   window silently grew 32 px per launch. Nothing in the API names hints at
   it: `WindowOptions.window_bounds` and `Window::bounds()` have the same type
   and mean different rectangles.
3. **⌘W not reaching element action handlers.** ⌘⇧I and ⌘, worked immediately
   (global handlers); the identical binding pattern on a root `div().on_action`
   did nothing, with no error. Diagnosed by adding a keystroke log to the root
   element and watching it stay silent while the action still dispatched.
4. **Deciding what "modal" honestly means here.** Three attempts: (a) a
   `WindowKind::PopUp` panel — rejected, non-activating and above every other
   app; (b) `window.prompt` for the edit dialog — impossible, buttons only;
   (c) a second normal window + an `.occlude()` scrim + focus bounce, shipped.

## Surprises

- **Good:** "one source of truth across two windows" is a *one-line* problem in
  gpui. `Entity<T>` + `cx.observe` needs no store, no reducer, no channel, and
  no manual invalidation, and it works identically for 2 or 4 windows. The
  Ping/Pong distinction (state → observe, transient → `EventEmitter`) falls out
  naturally.
- **Good:** `window.prompt` being a genuine `beginSheetModalForWindow:` sheet —
  in a framework with no widget library at all, the *most native* dialog in
  this whole app came from the framework itself, and it is the only part of the
  UI macOS accessibility can see.
- **Bad:** no parenting whatsoever, and a `WindowKind::Floating` variant whose
  documentation promises parenting that the macOS backend does not implement.
- **Bad:** no window-moved/resized event, so "remember my window position" — a
  thing every desktop app does — has to be a polling loop.
- **Bad:** the fourth repetition of the corpus' oldest gpui finding: no text
  input widget. Every editable field in this app is the ~40-line
  `key_char`/backspace field from `gpui-grid` (no selection, no caret movement,
  no IME, no clipboard). The sanctioned alternative is the 746-line
  `examples/input.rs` per field.

## Spec items approximated or skipped

- **§2 modality kind** — approximated (see the table). No OS modality is
  reachable for custom window content in gpui 0.2.2.
- **§6 parenting** — not achievable; only the "opens offset from the parent"
  half was implemented.
- **§10 multi-monitor** — not verified (one display); code path documented.
- **§11 "Esc closes the topmost non-main window"** — implemented per-window
  (`on_key_down` on the inspector/preferences/dialog roots closes *that*
  window). There is no notion of a window stack to consult, so "topmost" is
  approximated by "focused".
- The main list is six plain `div()` rows rather than `uniform_list`; with six
  items virtualization would be noise.
- `WINDOWS_START_DIRTY=1` exists purely so the close-veto path can be reached
  with keystrokes alone on a desktop where clicks were unreliable; it seeds
  `Store.dirty = true` at launch and changes nothing else.

## Window model

**A window in gpui is a handle, and the state is an entity that does not
belong to it.** `cx.open_window` gives back a `WindowHandle<V>`: a `Copy`,
`'static`, comparable token you keep in a global, whose `update()` fails once
the window is gone. Behind it sits a root `Entity<V>` (a view) plus a `Window`
struct that owns only genuinely per-window things — focus, appearance, bounds,
the tab-stop map, the dispatch tree. Crucially, application state is *not* any
of those: an `Entity<T>` is owned by the `App`, not by a window, so handing the
same `Entity<Store>` to four root views is the normal thing to do rather than a
workaround.

The code for "two windows must see the same mutable state" is therefore
three lines in each view's constructor and none anywhere else:

```rust
let subs = vec![cx.observe(&store, |_, _, cx| cx.notify())];
```

…and every mutation, from any window, is `store.update(cx, |s, cx| { s.pings += 1; cx.notify() })`.
Reads are `self.store.read(cx)` inside `render`. There is no "which window owns
the model" question to answer, no copying into a dialog and back, and no
difference between a same-window and a cross-window update — the Inspector's
Name field mutates `store.projects[i].name` in place and the main window's list
is correct on the next frame. Transient signals that have no state (the Pong
flash) use the sibling mechanism, `EventEmitter` + `cx.subscribe`, with the
same shape.

What the model does *not* give you is anything about windows as **OS objects**:
no parent/child relation, no modality, no per-window hide, no move/resize
events, and a content-vs-frame bounds asymmetry you have to discover. gpui's
window is a first-class *rendering + focus* context and a deliberately thin
wrapper over AppKit; everything a window manager knows about windows is either
absent or has to be reconstructed by the application.
