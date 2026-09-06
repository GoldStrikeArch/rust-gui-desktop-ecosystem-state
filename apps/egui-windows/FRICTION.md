# FRICTION — Windows (egui/eframe =0.35.0)

App: `apps/egui-windows/` · package `egui-windows` · `cargo run --release`.
Built and verified on macOS 26.6.2 (Apple M4 Pro, single 3024×1964 Retina
display, scale 2.00), rustc/cargo 1.96.1. Release build clean, no warnings;
`cargo build --release --locked` succeeds unchanged.

**LoC**: 732 app (`src/main.rs`) + 165 verification-only self-test driver
(`src/selftest.rs`) = 897.

**Evidence**: `evidence/` — `selftest-log.txt` (`WINDOWS_SELFTEST=1`,
`SELFTEST DONE pass=13 fail=0`), `log.txt` (every scripted step with the
command used), `trace.log` (the app's own `WINDOWS_TRACE=1` action log from
the three verification runs), and 14 screenshots.

Two verification notes that shaped the method (both in `log.txt`):
the desktop is shared with ~8 sibling agents, so screenshots use
`screencapture -o -l <windowid>` and every action is retried until the app
logs its `TRACE` line; and **macOS gives the first synthetic click to window
activation** — egui-winit never sets `acceptsFirstMouse:`, so one CGEvent
click on an inactive egui window only produces a hover. Two clicks inside one
activation are needed. That is also true for real users' first click.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** | synthetic-input | `ctx.show_viewport_deferred(ViewportId::from_hash_of("inspector"), builder, cb)`. `window-count.swift` went 1 → 2 → 3; clicking Inspector again sends `ViewportCommand::Focus` to the existing id instead of making a 4th window (a viewport is keyed by `ViewportId`, so "singleton" is the default and *multiple* instances would be the extra work). |
| Modal dialog — kind achieved | **built-in overlay** (`egui::Modal`); OS window-modal only via `rfd` | observed + synthetic-input | Custom-content modal = **in-framework overlay**: `egui::Modal::new(id).show(ctx, …)` paints an `Order::Foreground` `Area` with a backdrop over the *same* viewport. There is **no OS modality API in egui/eframe/winit** (`ViewportBuilder` has `with_window_level`/`with_active` but no `parent`/`owner`/`transient_for` — checked field by field in `egui-0.35.0/src/viewport.rs`). Real `NSAlert` **sheets** are reachable for message boxes only, because `eframe::Frame: HasWindowHandle` and `rfd::MessageDialog::set_parent(frame)` calls `beginSheetModalForWindow:` (`rfd-0.17.2/src/backend/macos/message_dialog.rs:96`). Used for Delete and the close-veto dialog; `sheet 1 of window 1` is visible in the AX tree. rfd exposes no accessory view, so the two-field Edit dialog cannot be a sheet. |
| Parent blocked while modal | **built-in** (within the viewport) | synthetic-input | 5 CGEvent clicks on Delete while the modal was up → 0 `TRACE toolbar Delete` lines, row count unchanged (`07b`). Mechanism read in source: `Modal::show` → `Memory::set_modal_layer` → every lower layer fails `Memory::allows_interaction`. The self-test asserts `!ctx.memory(|m| m.allows_interaction(LayerId::background()))` while open and `true` after. **Trap**: `top_modal_layer` is committed in `end_pass`, so blocking engages one frame late; and it lives in `Memory::focus: ViewportIdMap<Focus>` — **per viewport**, so it does not block the other windows (see next row). |
| Focus returns to parent after modal | **built-in / n/a** | observed | The modal is not an OS window, so the parent never loses OS focus. egui keyboard focus is requested on the first frame (`response.request_focus()` behind a "just opened" flag) and released with the modal. |
| Cross-window modality (blocking the other windows) | **hand-rolled** | observed | Because the modal layer is per-viewport, the app passes `modal_open` into each child viewport's closure and calls `ui.disable()`. `evidence/14b` shows the Preferences window greyed while the main window's modal is up. Their title bars stay live — no way to disable OS chrome. |
| Shared state across windows (live, bidirectional) | **hand-rolled** (`Arc<Mutex<_>>`) | observed | `show_viewport_deferred` takes `impl Fn(&mut Ui, ViewportClass) + Send + Sync + 'static`, so the child UI *cannot* borrow `&mut self`. One `Arc<Mutex<Shared>>` is the whole answer: the inspector mutates `s.projects[i]` in place and the main list reads the same `Vec`. Typing in the Inspector updated the main row and status bar in the same frame (`03`). No signals, no diffing, no copies. |
| Cross-window message (Ping/Pong) + wake | **assembled** | synthetic-input | There is no message bus. Ping = `s.pings += 1; ctx.request_repaint_of(ViewportId::ROOT)`; Pong = set a deadline in the shared state + `request_repaint_of(inspector_id())`, and the inspector self-schedules while flashing. `request_repaint_of` is the only "wake another window" primitive and it is enough. |
| Theme/layout change applied to all windows | **built-in** | synthetic-input | `Context::set_theme(ThemePreference)` — one `Context` behind all viewports, so all three restyled from one call (`06a`/`06b`). Same for the *Compact rows* relayout, which is just shared state (`13a`→`13b`). The one wrinkle: a preference edited in a child window must wake the root, because only the root pass owns `set_theme`. |
| Window parenting (child/owner/transient) | **not-achievable** (position offset only) | by-construction | egui records a parent *id* (`ctx.viewport_parents`, `ViewportInfo::parent`) but never passes it to winit: `ViewportBuilder` has no `parent`/`owner`/`transient_for` field, and `winit::WindowAttributes::with_parent_window` is unreachable through eframe (the `window_builder` hook receives egui's `ViewportBuilder`, not winit's attributes). Observed consequences: the Inspector does not stay above the main window, is not minimised with it, and appears in the window list as a peer. All this app can express is "opens offset from the parent" (`main.x + main.w + 12`, verified: 60+720+12 = 792). |
| Close veto (CloseRequested → Save/Discard/Cancel) | **built-in** + assembled dialog | synthetic-input | `ctx.input(\|i\| i.viewport().close_requested())` → `ViewportCommand::CancelClose`. Verified end to end: dirty → ⌘W → real sheet → Cancel → process alive with both windows; → Discard → **exit code 0** (`10`). |
| Quit semantics (main closes ⇒ exit; child closes ⇒ app lives) | **built-in** | synthetic-input | Root viewport close ends `run_native` even with the Inspector open (exit 0). ⌘W on the Inspector closed only the Inspector; the app kept running with main + Preferences. |
| Native confirm dialog | **assembled** (`rfd` =0.17.2) | observed | `MessageDialog::set_parent(frame).set_buttons(YesNo)` → NSAlert sheet, blocking `.show()` on the main thread inside the frame (AppKit runs its own nested modal loop; the UI freezes, which is native-normal). Downside of an unbundled binary: the sheet shows the generic folder icon, not an app icon. |
| Position/size persistence | **hand-rolled** (serde_json) | synthetic-input | eframe's `NativeOptions::persist_window` is unusable twice over: the `persistence` feature is **not** in eframe's default set (so it is inert with the pin this corpus uses), and it stores exactly one `WindowSettings` under a single `STORAGE_WINDOW_KEY` — one window, not three. ~40 LoC of JSON next to the binary instead. **Trap found by the round-trip**: `ViewportInfo` gives you `outer_rect` and `inner_rect`, but `ViewportBuilder` only takes an *inner* size; storing the outer size grew every window by the 32 pt title bar on each launch (512 → 544). Store outer *position* + inner *size*. |
| Multi-monitor scale change | **not-verified** | not-verified | Only one display attached. Code path: each viewport reads its own `i.viewport().native_pixels_per_point` (both windows render it; "scale 2.00"), and eframe recreates the surface per viewport, so per-window DPI is at least *representable*. Not exercised. |
| Per-window shortcuts (⌘W focused only, ⌘, , ⌘⇧I) | **built-in** | synthetic-input | Each viewport has its own `InputState`, so `ctx.input_mut(\|i\| i.consume_shortcut(..))` inside a viewport's pass is automatically scoped to that window — ⌘W in the Inspector's pass closes the Inspector, ⌘W in the root's pass starts the quit flow. ⌘, and ⌘⇧I verified from the trace log. Corollary the spec cares about: ⌘, only works when the *main* window is focused, because that handler only exists in the root pass. There is no app-level (menu-bar) shortcut without adding `muda` (see `apps/egui-tray`). |

## Helper crates

- `rfd = 0.17.2` — native message boxes. Chosen specifically because
  `set_parent(&eframe::Frame)` is the only route to real OS window modality
  from eframe.
- `serde = 1.0.228` + `serde_json = 1.0.150` — window-geometry persistence
  (see the row above for why eframe's own is not usable here).
- **Not used**: `eframe`'s `persistence` feature (would change the feature set
  vs `apps/egui-app`, and stores one window); `egui_extras` (the 6-row list
  needs no table); `arboard`/`tray-icon`/`global-hotkey` (out of scope here).

## Where the time went

~15% the app itself — the UI is small and the shared-state model is one
`Arc<Mutex<_>>`. ~15% reading `egui-0.35.0`/`eframe-0.35.0`/`rfd-0.17.2`
sources to establish what is *not* there (no owner/parent, no modal API, per-
viewport modal layer, single-window persistence). ~60% scripted macOS
verification on a desktop shared with eight sibling agents: windows of other
apps sitting exactly on top of mine, the keyboard layout switched to Russian
under me, another agent's ⌘W arriving while my app was frontmost and quitting
it mid-run. The fixes (per-window `screencapture -l`, a `WINDOWS_TRACE` action
log to retry against, two clicks per action) are written up in `log.txt`.
~10% the two real bugs the round-trip found: the outer/inner size drift and
the backdrop-click that made the modality screenshot ambiguous.

## Surprises

- **Good**: `show_viewport_deferred` really does give an independent window —
  the Inspector keeps rendering and repainting while the main window is not
  running its `ui`. The immediate variant (`show_viewport_immediate`, used by
  `apps/egui-tray`) only paints while the parent's `ui` runs; that is the
  documented trap and deferred avoids it entirely.
- **Good**: one `Context` for all windows means theme, zoom and style are
  automatically global — "restyle every open window at once" was zero code.
- **Good**: `request_repaint_of(ViewportId)` is exactly the cross-window wake
  primitive, and per-viewport `InputState` makes "⌘W closes the focused
  window" fall out for free.
- **Bad**: `Send + Sync + 'static` on the deferred callback is a real design
  tax. A single-window eframe app is `&mut self` all the way down; the moment
  a second window appears, the model must move behind a lock and every child
  window becomes a free function taking `&Arc<Mutex<Shared>>`. There is no
  egui-blessed alternative (the immediate variant lets you borrow `&mut self`
  but is not an independent window).
- **Bad**: the modal layer being per-viewport is invisible until you look at
  `Memory::focus: ViewportIdMap<Focus>`. "Modal" in egui means "modal for this
  window", and nothing warns you.
- **Bad**: no parent/owner at all. The single most-requested thing in the
  r/rust comment that prompted this spec (10 windows, mostly modal) is the one
  thing this stack cannot express.
- **Neutral**: egui's bundled font has no Cyrillic/Greek and no `→`; both show
  as tofu. Irrelevant for a Latin demo, fatal for a localized business app
  unless you ship your own font.

## Window model

**A window is a value plus an id, re-declared every frame — never an object
you hold.** `ctx.show_viewport_deferred(id, ViewportBuilder, ui_cb)` is the
whole API: the `ViewportId` is the identity, the `ViewportBuilder` is a
*desired state* that eframe diffs against the live window each frame (so
passing the same position every frame is a no-op, and changing it moves the
window), and the closure is the content. You stop showing a window by not
calling the function. There is no handle, no `Window` struct, no way to reach
the underlying `winit::Window` for anything but the root (`Frame::winit_window`).

When two windows must see the same mutable state, the code looks like this:
the model goes into `Arc<Mutex<Shared>>`, the root `App` keeps one clone, each
viewport closure `move`-captures another, and every child window is a free
function `fn inspector_ui(ui: &mut Ui, shared: &SharedState)` that locks,
mutates the real data in place, and calls `ctx.request_repaint_of(ROOT)`. That
is forced, not chosen: the deferred callback is `Fn + Send + Sync + 'static`.
The payoff is that "one source of truth, no copy" is trivially true —
bidirectional live editing between the list and the inspector needed zero
synchronisation code, only a lock. The cost is that a second window converts an
ordinary `&mut self` immediate-mode app into a lock-passing one, and any
per-window state (the modal draft, the "which window opened this" flags) has to
be sorted by hand into "root-only" vs "shared".

## Approximated or skipped

- **OS window-modal Edit dialog** — approximated by `egui::Modal` (overlay).
  rfd sheets are real OS modality but take no custom fields; there is no egui/
  eframe/winit API for a modal window, and reaching `NSApp.runModalForWindow:`
  would need an `objc2` dependency plus a nested AppKit loop inside winit's
  event loop (rejected: 3 attempts budget spent establishing the API gap).
- **Window parenting** — not achievable (see the table); approximated by
  opening the Inspector at a parent-relative offset.
- **Multi-monitor scale change** — not verified, single display available.
- **⌘W closing the focused window** relies on egui's per-viewport input rather
  than an OS menu item; without a menu bar (`muda`) macOS itself does not
  provide ⌘W, so the app implements it. Verified for both root and child.
- LoC is 732 for the app, slightly over the ~700 guide; the self-test driver
  is separate and counted as verification.
