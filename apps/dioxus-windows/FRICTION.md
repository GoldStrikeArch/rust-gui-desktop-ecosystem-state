# FRICTION — Windows (dioxus =0.7.9 desktop/webview)

Built as `apps/dioxus-windows` (package `dioxus-windows`, edition 2024), same pin
as iteration 1 (`dioxus = { version = "=0.7.9", features = ["desktop"] }`).
Four window kinds: main list, Inspector, Preferences, modal Edit dialog.

**Structure (deliberate, for the Dioxus-Native follow-up):**
`src/app.rs` (807 lines) is pure `dioxus::prelude` + `dioxus::html` — no
`dioxus::desktop`, no `document::eval`, **zero JavaScript in the whole app**.
`src/platform.rs` (250 lines) holds every OS call behind a plain-Rust signature.
`src/main.rs` is 8 lines: `fn main() { platform::launch(app::App) }`
(the brief's literal `dioxus::launch(app::App)` would give up the window title,
size and close behaviour, all of which are `Config`-time on desktop; putting the
launch in `platform.rs` keeps `main.rs` logic-free *and* makes the Native port a
one-file swap).

Verified on macOS 26.5.2 / M4 Pro, release binary, evidence in `evidence/`:
`log.txt` (every command + result), 8 screenshots, `selftest-log.txt`,
`run-a…run-e.txt` (raw app stdout per run).

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | **built-in** | synthetic-input | `dioxus::desktop::window().new_window(VirtualDom::new(Root), Config…)` returns a `PendingDesktopContext`; `spawn` + `.await` yields the `DesktopContext`. Singleton is mine: a `thread_local!` `HashMap<&str, WeakDesktopContext>` in platform.rs — `Weak`, because holding the `Rc` would keep the tao window alive forever (dioxus documents this). `window-count.swift`: 1 → 2 → 3 windows as the buttons were pressed; a second press on *Inspector* left the count at 2 and focused the existing one. Esc / the Close button / ⌘W each removed exactly one. |
| Modal dialog — kind achieved | **hand-rolled (app-modal)** | synthetic-input | **Dioxus and tao expose no modality at all** — no sheet, no `runModal`, no owner-disable. Shipped: a real child window (`Edit project`) plus a full-window scrim `div` in the parent that swallows pointer events. So the honest label is *neither* of the spec's three: an OS **child** window with a hand-rolled parent-side input block, not an in-webview overlay and not OS-modal. `NSWindow beginSheet:`/`NSApp runModalForWindow:` would need raw objc2 and a nested run loop inside dioxus' tao callback (see the RefCell re-entrancy trap below); not attempted beyond reading the source. |
| Parent blocked while modal (verified by synthetic click) | **hand-rolled** | synthetic-input | Control first: a CGEvent click on the main window's `Pong` at (474,115) printed `[windows] pong -> inspector`. Then with the dialog open, CGEvent clicks on `Delete` (382,115) and `Pong` (454,115) produced **no stdout line and no row change** (AX static-text count 31 → 31). 4 windows open at the time. The block is CSS (`position:fixed; inset:0; z-index:99`), so it is real for the mouse and for anything that hit-tests, but an AX `AXPress` still reaches the button underneath — AX presses the DOM element directly. |
| Focus returns to parent after modal | **assembled** | synthetic-input | `platform::focus("main")` → `window.set_focus()` after the dialog closes. `AXMain` of `Windows (dioxus)` = true, Inspector/Preferences = false. Note: the Inspector still *orders* above the main window because it is a real macOS child window (`addChildWindow`) — a parent can never be raised above its child. |
| Shared state across windows (live, bidirectional) | **built-in** | self-test + synthetic-input | **The headline finding, and it is a good one.** Each window is a separate `VirtualDom` with its own runtime and scheduler, but a `Signal` is a `generational-box` handle, not a runtime-local cell: `Readable::try_read_unchecked` subscribes whatever `ReactiveContext` is current (i.e. the *reading* window's scope) and `update_subscribers` marks each subscriber dirty through **its own** runtime's sender (`dioxus-core` `reactive_context.rs::new_for_scope`). So passing one `#[derive(Clone,Copy)] struct Shared { … Signal<T> … }` to `VirtualDom::with_root_context` gives genuine one-source-of-truth state across windows — no channel, no bus, no mutex, no wake plumbing, ~1 line per window. Self-test proved both directions (`pings` and a rename written by the Inspector's VDOM were seen by main; `sel`/`theme`/`compact` written by main were seen by the Inspector, which reported back through a third shared signal). Live: AX-setting the Inspector's Name field to "Harbor Migration LIVE" changed the main window's list row and dirty flag in the same frame (`evidence/03-shared-state.png`). **`GlobalSignal` does NOT work this way** — it resolves per-runtime, so every window would silently get its own copy; that is the trap. |
| Cross-window message (Ping/Pong) + wake mechanism | **built-in** | self-test + synthetic-input | Same mechanism, no separate message layer: `Ping` is `sh.pings += 1` in the Inspector's VDOM; the write marks main's reactive context dirty, which pushes `SchedulerMsg::Immediate` into *main's* runtime channel, which wakes main's VirtualDom future, which posts `UserWindowEvent::Poll` on the tao proxy. Verified: two AXPresses on `Ping` → main status bar `pings: 2`. `Pong` is `sh.pong += 1` in main; the Inspector renders its panel with `key: "{pong}"`, so the element is recreated and a 300 ms CSS keyframe replays — zero timers, zero JS (the flash itself was not caught in a screenshot; stdout `[windows] pong -> inspector` is the evidence). |
| Theme/layout change applied to all windows | **built-in (with a paint wart)** | synthetic-input | One `Signal<Theme>` + one `Signal<bool>` read by all three roots; the radio in Preferences restyles main and Inspector. **Wart, observed:** the DOM update reaches an unfocused window immediately but the *paint* can lag until that window gets a real event — after the Dark click, main's compact re-layout appeared at once while the dark palette only painted after an `AXRaise` (`05-theme-all-windows.png` vs `05a-theme-main-after-raise.png`). Same root cause as the dioxus-tray "idle event loop defers everything" note. |
| Window parenting (child/owner/transient) | **built-in via tao** | synthetic-input | dioxus re-exports tao (`pub use tao;`), so `dioxus::desktop::tao::platform::macos::WindowBuilderExtMacOS::with_parent_window(main.window.ns_window())` is reachable from `Config::with_window`. tao maps it to `-[NSWindow addChildWindow:ordered:NSWindowAbove]` (`platform_impl/macos/window.rs:315`). Observed: minimising **only** the main window removed `Inspector` from the process' window list and left the non-child `Preferences` alone; restoring brought it back. macOS expresses only *parent/child*; there is no `owner` or `transient_for` here (those are the Windows/Wayland spellings, both also in tao but not on this platform). |
| Close veto (CloseRequested → Save/Discard/Cancel) | **hand-rolled** | synthetic-input | **There is no veto in dioxus-desktop 0.7.9.** `App::handle_close_requested` (app.rs:195) either hides or closes, unconditionally, and it runs *after* user `use_wry_event_handler`s. The workaround shipped here: the main window is configured `WindowCloseBehaviour::WindowHides`, our handler decides on `CloseRequested`, and if the close is vetoed we re-show the window on the very next `MainEventsCleared`/`RedrawEventsCleared` — a one-iteration hide that is not perceptible. Verified: dirty + ⌘W → `[windows] close requested while dirty -> prompt`, window still on screen with the Save/Discard/Cancel card (`08-close-prompt.png`), process alive; **Cancel** → `close: Cancel -> vetoed`, still alive; **Save** → geometry written, exit; **Discard** → `close: Discard -> exit`, shell exit code **0**. |
| Quit semantics (main closes ⇒ app exits; child closes ⇒ app lives) | **assembled** | synthetic-input | Child windows keep the default `WindowCloses` and `exit_on_last_window_close = true`, so closing the Inspector or Preferences leaves the app running (observed repeatedly). Main quits the process explicitly (`std::process::exit(0)`) after persisting, so it exits even with children open. One Rust-level gotcha: a `-> !` helper makes a Dioxus event closure infer the never type and `SpawnIfAsync` rejects it, so `platform::quit()` is declared `-> ()`. |
| Native confirm dialog | **assembled (rfd)** | synthetic-input | `rfd::AsyncMessageDialog` awaited in `spawn` → real NSAlert "Delete project / Delete \"Orbit Failover\"? / No / Yes" (`10-native-confirm.png`) — *verifier note (2026-08-30): for this unbundled binary the alert is hosted out-of-process by `UserNotificationCenter` (macOS's `CFUserNotification` fallback for an app-modal `NSAlert` from an unbundled binary, not rfd code; a new window owned by that process appeared on each verified Delete click), which is why it is invisible to per-process AX scripting and outlives a killed app as a ghost dialog*; Yes → `[windows] deleted row 5, 5 left`. rfd 0.17 is the same version dioxus-desktop already vendors for `<input type=file>`, so it adds zero crates, but it is not re-exported. **Must be the async variant:** the sync `MessageDialog::show()` spins a nested NSAlert run loop, and if it is called from inside a wry event handler that loop re-enters `WindowEventHandlers::apply_event`, which is holding `RefCell::borrow_mut` on the handler slab — a guaranteed panic. |
| Position/size persistence | **assembled** | synthetic-input | `window.outer_position()/inner_size()/scale_factor()` → logical rect; hand-written JSON next to the binary (no serde: six numbers). Moved main to (180,300), ⌘W → Save → file `{"main":[180.0,300.0,720.0,480.0], … "inspector_open":true,"theme":1,"compact":true}`; plain relaunch printed `restored=Some((180.0,300.0,720.0,480.0))` and `synth bounds` reported `180 300 720 512` (512 = 480 content + 32 titlebar) with the Inspector re-opened and dark+compact restored. **Partial:** the *child* window's saved position is not honoured — macOS re-places a window added with `addChildWindow`, so the Inspector came back at (744,211) instead of (960,33). |
| Multi-monitor scale change | **not-verified** | by-construction | One display only. Every window logs `tao` `scale_factor` on creation (`scale_factor=2` for all four). The webview re-renders from `scale_factor` changes without app code, but nothing was moved between displays. |
| Per-window shortcuts (⌘W focused window only, ⌘, , ⌘⇧I) | **mixed: built-in / hand-rolled** | synthetic-input | ⌘W and ⌘Q come free from dioxus' default muda menubar and are genuinely app-global and per-key-window (⌘W with Preferences focused closed *only* Preferences and left main up). ⌘, and ⌘⇧I are DOM `onkeydown` on each window's root `div` (`tabindex="-1"` + `onmounted` → `set_focus`) — which means **they only fire while that window is the key window**: ⌘⇧I with Preferences focused did nothing; with main focused it opened the Inspector, and could not toggle it off because the freshly-opened child window immediately became key. Making them app-global would mean muda accelerators (native but menu-shaped) or `use_global_shortcut` (system-wide, wrong scope). Esc closes the topmost non-main window (verified on the Inspector) but only when focus is inside the root div — clicking a non-focusable background area moves focus to `body`, which is *outside* the handler's subtree, so the key is lost. |

## Helper crates

- **rfd `=0.17.2`** (`default-features = false`, `xdg-portal`) — the native message box. Same version dioxus-desktop already depends on internally, so 0 new crates in the graph; it simply is not re-exported.
- **tokio `=1.52.3`** (`time` only) — paces `WINDOWS_SELFTEST`. Production paths never touch it. The recurring corpus finding repeats: dioxus-desktop *runs* on tokio but re-exports no timer.
- **Not needed:** tao and muda are re-exported by dioxus-desktop (`pub use tao; pub use muda;`), which is what made native parenting and the default ⌘W menubar free.
- **Rejected:** serde/serde_json for persistence (six numbers; a 20-line hand-rolled JSON reader/writer was smaller than the dependency note would have been); objc2 for `beginSheet:`/`runModal` (nested run loop inside the tao callback re-enters dioxus' handler slab).

## Verification-only env knobs (no production path)

`WINDOWS_SELFTEST=1` (scripted run, always-on-top, exits with a pass/fail line),
`WINDOWS_PLACE=1` (fixed geometry), `WINDOWS_TOPMOST=1` (geometry + always-on-top).
Two exist because they conflict: always-on-top raises the CGWindow level above 0,
and both `scripts/window-count.swift` and `tools/synth/synth bounds` filter on
`layer == 0` — so the window-count evidence had to be taken *without* it, driving
buttons through AX instead (AX is per-process and immune to another app being on
top), while the CGEvent modality test needed it so a sibling agent's window could
not intercept the click. That is not a framework property; it is this shared
desktop, on which six other agents' apps were moving over, clicking on, and
sending ⌘W to whatever was frontmost throughout.

## Where the time went

- ~35% verification choreography on the shared desktop (windows stolen mid-test,
  ⌘W from other agents killing runs, CGEvent clicks landing in a sibling's app —
  two "the button does not respond" dead ends were both another app on top).
- ~20% the close-veto design: reading `app.rs`/`launch.rs` to establish that no
  veto exists, then the hide-and-restore trick and its ordering proof
  (`app.tick` runs user handlers *before* the `CloseRequested` match).
- ~20% pre-flight source reading in the vendored 0.7.9 crates (does a Signal
  cross a VirtualDom? does `Config` reach tao's macOS ext traits? is
  `with_root_context` on `VirtualDom`?) — all three answered yes before a line
  was written, which is why the app compiled with 12 trivial errors and no
  redesign.
- ~15% the self-test across three VirtualDoms (timing the cross-window
  assertions so a child's scripted edit lands before the parent checks it).
- ~10% CSS/layout for four windows sharing one stylesheet.

## Surprises

- (+) Cross-VirtualDom signals *just work*, including the wake path. This is the
  single nicest thing found in the Dioxus corpus so far: "two windows, one
  source of truth" costs one struct and one `with_root_context` call.
- (+) tao is re-exported, so the macOS-only `with_parent_window` is one `use`
  away and gives a genuine OS child window.
- (+) `key`-driven element recreation replays a CSS keyframe, so the 300 ms
  cross-window flash needs no timer and no JS.
- (−) No modality and no close veto, at all, at any level of the stack.
- (−) Keyboard shortcuts that are not menu accelerators are per-window *and*
  per-focused-subtree; a DOM keydown on the root div silently stops working the
  moment focus lands on `body`.
- (−) An unfocused window can hold a stale paint after a state change; an
  occluded one is worse (its whole VirtualDom parks — the iteration-4 finding
  bit again here, in RUN D, where an edit in an occluded Inspector never reached
  Rust at all until the window was raised).

## The window model

**A window in Dioxus desktop is a handle to an independent VirtualDom, not a
value in the UI tree.** You never "render" a window: you construct a
`VirtualDom` from a `fn() -> Element`, hand it to
`window().new_window(dom, Config)`, and get back a `PendingDesktopContext` that
resolves to an `Rc<DesktopService>` — a smart pointer that derefs to the tao
`Window`. Windows are therefore addressed imperatively (a registry of weak
handles is *your* job; dioxus keeps its own map keyed by `WindowId` and does not
expose it), and everything about them — title, size, position, close behaviour,
parenting — is set on the tao `WindowBuilder` at creation or via method calls
afterwards, never declaratively.

What makes this pleasant is the state layer, which does *not* follow the window
boundary. Signals are `generational-box` handles: identity and storage are
process-global, subscription is per-`ReactiveContext`, and notification is
routed back through whichever runtime owns each subscriber. So the code for
"two windows see the same mutable state" is:

```rust
#[derive(Clone, Copy)]                       // Signals are Copy
struct Shared { projects: Signal<Vec<Project>>, sel: Signal<usize>, /* … */ }

// parent
let sh = use_context_provider(|| Shared { projects: Signal::new(seed()), .. });
let dom = VirtualDom::new(Inspector).with_root_context(sh);
window().new_window(dom, cfg);

// child, in its own VirtualDom
let mut sh = use_context::<Shared>();
sh.projects.write()[i].name = e.value();     // the parent re-renders
```

That is the whole mechanism. The one landmine is `GlobalSignal` (and
`use_context` alone): both resolve per-runtime, so a "global" looks shared and
silently is not. And the imperative half stays imperative — closing, focusing,
parenting and modality are all method calls on handles you have to keep
yourself, and two of them (modality, close veto) are not there to call.

## Approximated or skipped

- **Modality** is app-level, not OS-level: a child window + a click-swallowing
  scrim in the parent. No sheet, no app-modal run loop. AX presses bypass it.
- **Close veto** is a one-event-loop-iteration hide-and-restore, not a true veto,
  because 0.7.9 offers only hide-or-close.
- **Child window position persistence** is overridden by macOS' child-window
  placement; only the main window round-trips exactly.
- **Multi-monitor scale change**: not verified (one display).
- **⌘⇧I is not app-global** (DOM keydown, key-window only); Esc needs focus
  inside the root element.
- The 300 ms Pong flash is verified by stdout + code path, not by a screenshot
  (300 ms is shorter than `screencapture`'s latency here).

## What a Dioxus-Native (Blitz) port must replace

`src/app.rs` and `src/main.rs` should compile unchanged (pure `dioxus::prelude`
+ `dioxus::html`, no `document::eval`, no JS anywhere in the crate). Everything
below lives in `src/platform.rs` and is what a Native backend has to supply:

| function | what it does on desktop | Native note |
|---|---|---|
| `launch(fn() -> Element)` | `LaunchBuilder::desktop()` + tao `WindowBuilder` (title, 720×480, `WindowCloseBehaviour::WindowHides`) | swap for `dioxus_native::launch`; window attributes come from Blitz/winit instead |
| `register(&'static str)` | records `Rc::downgrade(&window())` in a thread-local registry | needs whatever handle Blitz gives for "the window this VirtualDom renders into" |
| `open_or_focus<T>(WinSpec, fn() -> Element, ctx)` | `window().new_window(VirtualDom::new(root).with_root_context(ctx), Config)`; macOS `with_parent_window`; singleton/focus | **the hard one** — Blitz must be able to host a second VirtualDom in a second window and inject a root context |
| `is_open`, `open_count`, `close_key`, `close_self`, `focus` | registry lookups + `DesktopService::close/set_focus/set_visible` | |
| `bounds` / `set_bounds` | `outer_position`, `inner_size`, `scale_factor`, `set_outer_position`, `set_inner_size` | winit equivalents exist |
| `scale_factor()` | tao `Window::scale_factor` | |
| `confirm(title, msg, Callback<bool>)` | `rfd::AsyncMessageDialog` in `spawn` | rfd is backend-agnostic; should port as-is |
| `use_close_guard(Callback<(), bool>)` | `use_wry_event_handler` on tao `CloseRequested` + the hide/restore trick | needs the Native event-loop hook; winit *does* let you ignore a close request, so a Native port could implement a real veto here |
| `quit()` | `std::process::exit(0)` | portable |
| `selftest` / `place` / `on_top` | env-var knobs; `on_top` sets `with_always_on_top` | verification only |
