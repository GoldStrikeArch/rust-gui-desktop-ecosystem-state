# FRICTION — Windows (dioxus-native, Blitz main 64eb278 / 0.3.0-beta.2)

SPEC-9 built at `apps/dioxus-native-windows`, package `dioxus-native-windows`,
edition 2024, plain cargo. The renderer is Blitz: Stylo 0.20 + Taffy 0.14 +
Parley 0.11.1 + AccessKit, painted through anyrender / **Vello Hybrid** (the
default `vello-hybrid` feature), windowed by winit 0.31.0-beta.2. No webview,
no JS engine, one OS process.

**The UI is shared, not copied.** `src/main.rs` is

```rust
#[path = "../../dioxus-windows/src/app.rs"]
mod app;
mod platform;
fn main() { platform::launch(app::App) }
```

`apps/dioxus-windows/src/app.rs` (807 lines) compiles and runs **unchanged, with
zero diff hunks**, on Blitz. Only `platform.rs` was re-implemented: 446 lines
here vs 250 in the webview build. Own LoC: 12 (main) + 446 (platform) = **458**;
plus 807 shared = 1265 total. `apps/dioxus-windows` was not touched.

Verified on macOS 26.5.2 / M4 Pro. Evidence in `evidence/`: `selftest-log.txt`
(`SELFTEST DONE pass=13 fail=0`, exit 0), eleven screenshots, and `log.txt`
with the exact command for every step.

## Capabilities

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Second top-level window (open, close, singleton focus) | hand-rolled | self-test + synthetic-input | `dioxus_native::launch_cfg` can only ever create ONE window. `platform.rs` replaces it with its own `ApplicationHandler` over `BlitzApplication`, building a `DioxusDocument` + `View` per window and injecting the dioxus contexts by hand. `window-count.swift`: 1 → 2 → 3; re-clicking Inspector keeps 3. |
| Modal dialog — kind achieved | hand-rolled (in-app overlay + separate window) | observed | Same shape as the webview build: a real second window plus a `.scrim` div in the parent. winit has **no** modality API at all — no sheet, no app-modal, no owner disabling. |
| Parent blocked while modal (synthetic click) | hand-rolled | synthetic-input | `synth click 355 115` on the main window's Delete with the modal open: before/after PNGs byte-identical (`cmp -s`), 6 rows, no confirm dialog. The scrim swallowed it — the *window* is not blocked, the *content* is. |
| Focus returns to parent after modal | assembled | observed | `platform::focus("main")` → `Window::set_visible` + `focus_window()`. Verified after Cancel (07). |
| Shared state across windows (live, bidirectional) | built-in | self-test + synthetic-input | Unchanged from the webview build: `VirtualDom::new(root).with_root_context(Shared{…Signals})`. Selecting a row in main updates the Inspector; typing in the Inspector updates the main list live (03, 04). |
| Cross-window message (Ping/Pong) + wake | built-in | self-test + synthetic-input | Ping = `sh.pings += 1` on a shared Signal. The signal write wakes the *other* window's `View` through the `BlitzShellProxy` waker (`BlitzShellEvent::Poll`), no channel or bus. Main shows "pings: 1" (04). |
| Theme/layout change applied to all windows | built-in (via signals) — but unreachable from the UI here | self-test + observed | Driven programmatically all four windows restyle at once (10, 11) — ancestor class changes (`.t-dark`, `.compact .row`) DO invalidate descendants in Stylo. Driven by the actual radio/checkbox it does nothing, because Blitz never fires `change` (see below). |
| Window parenting (child/owner/transient) | **not-achievable** | by-construction | winit 0.31-beta's `WindowAttributes` has no `with_parent_window`/`with_owner_window`, and `winit-appkit`'s `WindowAttributesMacOS` exposes titlebar/panel/tabbing options only — no `addChildWindow:`. `platform.rs` logs `"OS parenting unavailable on winit 0.31"` for every `child_of_main: true` request. |
| Close veto (CloseRequested → Save/Discard/Cancel) | hand-rolled — and a **real** veto | synthetic-input | Better than the webview build. `BlitzApplication::window_event` destroys the window on `CloseRequested` unconditionally, so the veto lives in our handler: it calls the guard first and simply does not forward the event. Red close button while dirty → prompt, window survives, Cancel → still alive (08, 09). |
| Quit semantics (main closes ⇒ exit; child closes ⇒ live) | assembled | synthetic-input | Inspector Close → `[windows] closed inspector`, process alive with 1 window. Main close with Preferences still open → `close requested, clean -> exit`, process gone. |
| Native confirm dialog | built-in (rfd) | observed | `rfd::AsyncMessageDialog` in a `spawn`, identical to the webview build; the NSAlert renders and answers correctly (05). **Verifier note (2026-08-30):** for this unbundled binary the alert is presented *out-of-process* by `UserNotificationCenter` (a 260×202 window owned by that process, invisible to per-process AX; it outlives the app — two such ghosts were left by the verifier's own control clicks and had to be dismissed by hand), the same `CFUserNotification` fallback as `apps/dioxus-windows`. Under the corpus rubric an rfd message box is *assembled*, not built-in. |
| Position/size persistence | assembled | synthetic-input | `Window::outer_position` / `surface_size` / `set_outer_position` / `request_surface_size`. Moved to {420,140}, closed, relaunched → `restored=Some((420.0,140.0,720.0,480.0))` and the Inspector re-opened at its saved spot; `window-count` = 2. |
| Multi-monitor scale change | not-verified | by-construction | One display. `scale_factor()` is `winit::Window::scale_factor` and is printed for every window at open time (`scale_factor=2`); winit delivers `ScaleFactorChanged` to `View::handle_winit_event`, which we forward untouched. |
| Per-window shortcuts (⌘W focused window, ⌘, , ⌘⇧I) | **not-achievable** | synthetic-input + by-construction | Three independent blockers. (1) No menubar, so macOS never turns ⌘W into a `CloseRequested`. (2) The shared app.rs hangs its shortcuts on a root `div { tabindex: "-1", onmounted: set_focus }`; blitz-dom's `flush_is_focussable` treats **tabindex < 0 as not focusable**, so that div never holds focus and never sees a keydown. (3) Even with a focused element, ⌘-chords never arrive as keydown: AppKit delivers them as *standard key bindings*, which dioxus-native-dom explicitly drops ("AppleStandardKeybinding events are not exposed to script"). Verified separately in `apps/dioxus-native-ledger`, where a confirmed-focused input still never saw ⌘⇧C. ⌘⇧I, ⌘, and Esc all did nothing here, with the app forced frontmost. |

## Helper crates

| crate | pin | why |
|---|---|---|
| `dioxus-native` | git `64eb2785`, `features = ["prelude"]` | the renderer. crates.io `dioxus-native 0.7.x` is Blitz 0.2 from Oct 2025 and was deliberately not used. Default features kept: accessibility, hot-reload, net, html, svg, system-fonts, clipboard, file-dialog, **vello-hybrid**, woff, apple-font-embolden. |
| `blitz-shell` | git, same rev | **required, not a convenience.** `BlitzApplication`, `View`, `WindowConfig`, `BlitzShellProxy`, `BlitzShellEvent` and `create_default_event_loop` are what a second window needs, and `dioxus-native` re-exports none of them (they are private `use`s in its `lib.rs`). Same repo+rev, so no version skew. |
| `dioxus` | `=0.7.9`, `default-features = false`, `["macro","html","hooks","signals"]` | keeps `use dioxus::prelude::*;` working in the shared app.rs. Option (a) of the plan worked first try: everything unifies onto **one** dioxus-core/-html/-signals/-hooks 0.7.10, the same one dioxus-native uses. |
| `rfd` | `=0.17.2` | native confirm. blitz-shell already depends on rfd 0.17 for `<input type=file>`, so this adds zero crates. |
| `tokio` | `=1.52.3`, `["rt-multi-thread","time"]` | `rt-multi-thread` is new versus the webview build: `launch_cfg` builds and enters the runtime itself, and this crate does not call `launch_cfg`, so `platform::launch` must build its own or `tokio::time::sleep` in the self-test never fires. |

Tried and rejected: `dioxus_native::use_window_event` (the obvious close hook —
it cannot cancel, and its `WindowEventHandlers` context is crate-private, so it
is unusable once you host your own application handler);
`DioxusNativeApplication::add_window` (public, but unreachable from a component
and its windows get no dioxus contexts and no `initial_build()`, so they render
blank); `objc2`/`objc2-app-kit` for `NSWindow addChildWindow:` (two more heavy
crates to buy one spec row — recorded as not-achievable instead).

## Where the time went

Roughly half of it went into one question: *can Blitz open a second window at
all?* Reading `packages/dioxus-native/src/{lib,dioxus_application}.rs` and
`packages/blitz-shell/src/{application,window}.rs` answered it — no, not through
the public launcher, but yes if you assemble the shell yourself. Writing that
assembly took ~120 lines and compiled with **two** errors (`Runtime` and
`current_scope_id` are in `dioxus::core`, not `dioxus::prelude`). The self-test
passed 13/13 on its first run, including the four-window case.

The other half was verification on a desktop shared with six sibling agents:
windows constantly steal key focus, and macOS does not deliver the first click
to an inactive window, so most synthetic-input steps need an activating click
first. `WINDOWS_TOPMOST=1` was split out from `WINDOWS_PLACE=1` because raising
the window level pushes the CGWindow layer above 0 and hides the window from
`scripts/window-count.swift`.

## Surprises

**Good.** `app.rs` compiled with zero changes — no `#[cfg]`, no shim, no
copy — and the whole Signal-across-VirtualDoms model works identically on
Blitz, because it never depended on the renderer. The close veto is *better*
than on the webview: dioxus-desktop 0.7.9 can only hide-and-restore, while here
we simply do not forward the winit event. Stylo handles everything this app's
CSS asks for: CSS grid, `position: fixed` scrims with alpha, sticky headers,
border-radius chips, `font-variant-numeric: tabular-nums`, and ancestor-class
restyling. Text rendering is crisp and the app is visually indistinguishable
from the webview build side by side.

**Bad — the Blitz gaps this spec hit.**
- `<input type=checkbox|radio>` dispatch only `input`, never `change`
  (`blitz-dom/src/events/pointer.rs:645-695`). Both the Theme radios and the
  Compact-rows checkbox toggle *visually* and do nothing to the model. Any
  Dioxus code written against `onchange` (the idiomatic choice, and what the
  webview build uses) is silently dead. `FormData::checked()` is also always
  false — `BlitzInputEvent` carries only a `value: String`.
- Two radios in one `name` group can both paint as checked (11).
- `tabindex="-1"` is treated as non-focusable, which kills the standard
  "focus the root and listen for keydown" pattern for app-level shortcuts.
- `<select>` paints nothing at all — the Inspector's Status dropdown is an
  empty gap (the label "Status" with no control under it). It *is* in the
  accessibility tree as a pop-up button, so it is a paint/interaction gap,
  not a DOM gap.
- No menubar, therefore no ⌘W and no application menu of any kind. And every
  ⌘-chord is consumed as an AppKit standard key binding, which
  dioxus-native-dom deliberately does not expose to the VirtualDom — so
  application shortcuts have no route at all on macOS today.
- `@media (prefers-color-scheme: dark)` follows the *document's* colour
  scheme, not the OS: this app (which never declares `color-scheme`) renders
  light on a Mac in dark mode, while `apps/dioxus-native-ledger`, whose CSS has
  `:root { color-scheme: light dark; }`, renders dark. Explicit `.t-dark` works
  fine.
- CSS animations (the 300 ms Pong flash keyframe) were not verified either
  way; the `key`-driven remount that triggers them does work.

## The window model

On Blitz a window is exactly what it is on dioxus-desktop — **an independent
`VirtualDom` with its own runtime, scheduler and surface** — except that the
framework does not give you a way to make a second one. `dioxus_native::launch`
hard-codes a single `pending_window`; `DioxusNativeApplication` is what owns
windows, and it is created *inside* `launch_cfg` and moved into
`EventLoop::run_app`, so no component ever sees it. There is no `Window`
component, no `use_window_builder`, no `window().new_window(...)`. So the honest
answer for this framework today is: **a window is a private field of a struct
you are not allowed to hold.** To get two, you stop calling `launch` and become
the application yourself:

```rust
let doc = DioxusDocument::new(vdom_with_root_context, DocumentConfig::default());
let cfg = WindowConfig::with_attributes(Box::new(doc), DioxusNativeWindowRenderer::new(), attrs);
let mut view = View::init(cfg, event_loop, &proxy);      // needs &dyn ActiveEventLoop
view.downcast_doc_mut::<DioxusDocument>()
    .vdom.in_scope(ScopeId::ROOT, || { provide_context(shell); provide_context(renderer); provide_context(window); });
view.downcast_doc_mut::<DioxusDocument>().initial_build();
view.resume();
self.inner.windows.insert(view.window_id(), view);
```

Every piece of that is public, but you have to know that
`can_create_surfaces` is where `&dyn ActiveEventLoop` lives, that `initial_build`
is not called for you, and that the five root contexts dioxus-native injects are
what make `use_window`, redraws and the shell work. A component asks for a window
by pushing a fully-built `VirtualDom` onto a thread-local queue and calling
`proxy.wake_up()`; the handler drains that queue on the next loop turn.

The state half needs no work at all. `Signal` is a `generational-box` handle:
identity is process-global, subscription is per-`ReactiveContext`, notification
is routed through each subscriber's own runtime. Handing the same `Copy` struct
of Signals to a second `VirtualDom` via `with_root_context` gives genuine
bidirectional shared state across windows with no channel, bus or mutex — and it
behaves identically under Blitz, because that machinery is in dioxus-core, not
in the renderer. The one thing Blitz adds is that a signal write in window A has
to *wake* window B's event source; `View`'s waker sends
`BlitzShellEvent::Poll { window_id }` for exactly that, and it works even when
the window is occluded (unlike the webview build, whose occluded windows park
their tokio loop).

## Approximated or skipped

- **OS parenting** — not expressible in winit 0.31-beta at all (not-achievable).
- **Modality** — in-app scrim, as on the webview. No OS sheet or app-modal.
- **⌘W / ⌘, / ⌘⇧I / Esc** — not-achievable here: no menubar; `tabindex=-1` is
  non-focusable so the root keydown handler never runs; and ⌘-chords are
  swallowed as AppKit standard key bindings before they can become keydowns.
- **Theme / Compact from the Preferences controls** — dead because Blitz emits
  no `change` event for radios and checkboxes. Fixing it would mean editing the
  shared `app.rs` (`onchange` → `oninput`), which the sharing rule forbids, so
  it is reported as a Blitz gap and demonstrated programmatically instead.
- **Multi-monitor scale change** — not verified (one display).
- **Pong 300 ms flash** — code path only; 300 ms is shorter than
  `screencapture` latency here, same as the webview build.

## Side-by-side with the webview build

Same machine, same day, same commands.

| | `dioxus-native-windows` (Blitz) | `dioxus-windows` (wry/tao) |
|---|---|---|
| clean `cargo build --release --locked` | **76.5 s** | 35.0 s |
| binary | **24,533,888 B** | 6,477,664 B |
| unique crates (`cargo tree -e normal --prefix none \| sort -u \| wc -l`) | **520** | 377 |
| idle RSS, 1 window | **107 MB** | 94 MB (+ WebKit helpers) |
| idle RSS, 3 windows | 113 MB | not measured |
| processes | **1** | 1 + 3 `com.apple.WebKit` XPC helpers (GPU / Networking / WebContent) = 4 |
| own LoC (main + platform) | 458 | 258 |
| shared `app.rs` | 807, identical file | 807 |

(The 291-crate figure quoted for `dioxus-windows` in the brief comes from a
different counting command; 377 vs 520 above are both measured the same way.)
