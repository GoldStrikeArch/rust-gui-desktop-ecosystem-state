# A winit shell-integration facade: requirements & traps from a 10-framework corpus

> Distilled 2026-08-08 from the SPEC-4 "Tray Notes" round: the same OS-shell
> app (tray, global hotkey, native menubar, dialogs, clipboard text+image,
> Finder file-drop, notifications, live dark mode, multi-window,
> close-to-tray) built in **ten** frameworks — iced, egui, gpui, tauri, xilem,
> slint, dioxus, freya, vizia, floem — on macOS, plus a headless-container
> Linux round with retained probes. Primary sources: `apps/<fw>-tray/FRICTION.md`
> ×10, `report/12-shell-integration-results.md`, `report/18-linux-reality-results.md`,
> `report/data/{shell-text-rows,linux-rows,load-bearing-crates}.md`.
>
> **Audience:** anyone building the "Tauri plugins, but for winit" layer —
> maintained, framework-neutral shell facades over portal/SNI/DBusMenu and the
> platform APIs. This document is the requirements input: what the facade must
> do, the traps it must design away (each one observed, not hypothesized), and
> the acceptance tests the corpus already defines.
>
> **Evidence labels.** macOS results are app-level tested on one Apple M4 Pro
> (macOS 26.5–26.6). Linux results come from an arm64 Debian 12 Xvfb container
> (X11, software GPU, **no tray host, no portals, no WM**) with four dedicated
> fault-line probes retained in `linux/probes/`. Windows is source-verified
> only — nothing here was executed on Windows. Finder drops were verified at
> the event/code level, not by synthesized drags.

## 1 · Why a facade, in one paragraph

Nine of ten frameworks assemble the same five crates — tray-icon, muda,
global-hotkey, rfd, arboard, plus notify-rust — and every integration
re-discovers the same traps. The layer is structurally concentrated: four of
the crates live in the tauri-apps org, whose historical author went from ~400
commits to near-zero in 12 months, leaving a single unsponsored volunteer on
top of all four (`report/data/load-bearing-crates.md:10`). On Linux the layer
is GTK-coupled (muda menubars *cannot* attach to a plain winit window — a
compile-time E0277, not a missing feature) and Wayland-unserved
(global-hotkey is X11-only). winit itself has no menu/tray API (winit#403,
open since 2018; #3108). Protocol implementations exist in Rust — ksni for
StatusNotifierItem, ashpd for the GlobalShortcuts portal, DBusMenu
implementations — but **no maintained framework-neutral facade integrates
them with the API and platform coverage of the tauri-apps crates**
(`report/00-ecosystem-map.md:134`).

## 2 · Acceptance checklist

A facade is done when a **plain winit application** (no framework) passes all
of these; a framework adapter is done when the framework's app does. The
corpus's 10 implementations are the reference answers — every cell below was
achieved by all ten frameworks on macOS (zero not-achievable cells,
`report/12-shell-integration-results.md:66`).

| # | Capability | Acceptance test (from SPEC-4) | macOS status in corpus | Linux status in corpus |
|---|---|---|---|---|
| 1 | System tray + menu | icon visible; menu clicks delivered; live menu-label update | 10/10 | panics (GTK) or silently invisible — §4 T6–T8 |
| 2 | Global hotkey | register chord; fires while app unfocused | 10/10 | **works end-to-end on X11**; Wayland unserved |
| 3 | Native menubar | real OS menubar; working Edit accelerators | 10/10 (via trap T1) | **impossible on plain winit (E0277)** |
| 4 | Native dialogs | open/save panels, non-blocking or safely blocking | 10/10 | untested (rfd's XDG-portal path is the candidate) |
| 5 | Clipboard text + image | read/write text; paste a screenshot into the window | 10/10 | untested |
| 6 | File drop from file manager | window receives real paths | 10/10 (event-level) | untested |
| 7 | Notification | banner attributed to the app | **weakest cell** — see T10–T12 | untested (notify-rust's home turf) |
| 8 | Live dark mode | palette follows OS theme change without restart | 10/10 | untested |
| 9 | Close-to-tray + reopen | close hides; tray/Dock brings it back; process survives 0 windows | 10/10 (Dock-reopen: only floem/tauri) | untested |

Cross-cutting acceptance criteria the corpus adds:

- **No silent failure.** Every Linux failure observed was a panic or a silent
  success (§4). The facade must return `Err` — with a reason — when the tray
  has no StatusNotifier host, when GTK/portal prerequisites are missing, and
  when a notification cannot be displayed.
- **Build-only CI must not be able to lie.** macOS-written tray apps compiled
  unchanged on Linux and then 100% of tested launches died
  (`report/18-linux-reality-results.md:69`). The facade needs a
  runtime-checkable capability report (`facade::probe() -> CapabilityReport`)
  so CI can assert more than compilation.
- **Focus-aware Edit-menu dispatch.** Only gpui achieved menubar
  Cut/Copy/Paste routed to the *focused* widget; all nine others hard-wire the
  Edit menu to one widget. The facade's menu API should make the
  focused-target route the default shape (see §6.1).

## 3 · The API surface, derived from what ten frameworks actually needed

Minimum viable facade (phase 1 = replace the assembled crates on macOS +
Linux with honest errors; phase 2 = de-GTK the Linux paths):

1. **Tray**: create-after-loop-start contract enforced by the API (builder
   handed to a main-thread callback — freya's `LaunchConfig::with_tray` shape,
   §6.3); menu with action *closures*, not a global event channel (floem's
   `Menu` shape); live icon/label update; explicit `TrayError::NoHost`.
2. **Menus**: one menu type shared by tray and menubar; **owns its items**
   (muda's dangling `*const MenuChild` — T3); predefined OS roles only where a
   responder exists, otherwise synthesized-event routing with an explicit
   target (T1); Linux backend = DBusMenu/SNI, not GTK windows.
3. **Global hotkey**: registration + **callback/waker delivery** (no polling —
   nine of ten frameworks run a 50–100 ms poll today, T15); Linux = X11 grab
   *and* ashpd GlobalShortcuts portal behind one API with a capability flag.
4. **Dialogs**: rfd already is the neutral answer (its XDG-portal Linux default
   is the cleanest Linux story in the layer) — the facade's job is only
   executor-neutral delivery (callback on the caller's thread, floem's
   `open_file` shape).
5. **Clipboard**: text + image with *decoded-image* delivery (gpui's
   `ClipboardEntry::Image` is the reference; every arboard user hand-rolled
   RGBA→framework-image conversion, four of them via a PNG round-trip — T14).
6. **Notifications**: must be identity-aware: detect unbundled binaries and
   fail loud (T10–T12); never pump the runloop from inside an event callback
   (T11); macOS backend should target the modern UserNotifications framework, not deprecated NSUserNotification via a "small subset" backend — note that notify-rust 4.18, the exact version pinned here, already ships such a backend behind `features = ["preview-macos-un"]` (mac-usernotifications 0.3.1, with `check_bundle`/`request_auth` and real errors); none of the ten apps enabled it (checked 2026-08-30).
7. **App lifecycle**: `ActivationPolicy` control (none of the ten apps drops the Dock icon; 6 of 10 frameworks expose a first-class route to `Accessory` — tauri, eframe, slint, xilem, freya, dioxus — and 4 do not — iced, vizia, floem, gpui — though even those can call `-[NSApplication setActivationPolicy:]` directly; an earlier draft's "impossible in 8 of 10" was wrong, corrected 2026-08-30), Dock/taskbar reopen events
   (floem's `AppEvent::Reopen`), run-with-zero-windows.
8. **Capability probe**: `probe()` reporting per-capability
   available/degraded/absent with reasons (SNI host present? portal present?
   X11 or Wayland? bundled or bare binary?).

## 4 · Trap catalog — what the facade must design away

Each trap was **observed** in the corpus; citations point at the FRICTION
files, which carry reproduction context. Ratings: every trap below cost real
implementation time in at least one framework, several in three or more.

**T1 — muda predefined Edit roles: dead selectors + stolen key equivalents (macOS).**
`PredefinedMenuItem::{cut,copy,paste,select_all}` dispatch Cocoa
responder-chain selectors no winit view implements — the items no-op *and*
their ⌘X/⌘C/⌘V/⌘A key equivalents are consumed by NSMenu before the focused
widget sees them. Paste breaks app-wide, silently. Hit independently by iced,
egui, xilem; avoided-by-prior-knowledge in vizia/freya/floem/slint; only
webviews (real NSResponders) and gpui (`os_action` → focused-widget actions)
are structurally immune. Every fix re-injects synthetic clipboard events into
one hard-coded widget. (`apps/iced-tray/FRICTION.md:14`,
`report/12-shell-integration-results.md:37`)

**T2 — One global `MenuEvent` handler slot, three failure shapes.**
(a) A separately-resolved muda instance owns a *different* static channel —
menu clicks silently vanish (pre-empted by five frameworks via the
`tray_icon::menu` re-export); (b) **first** registration wins — the slot is a `OnceCell` whose `set` result is discarded (muda 0.19.3 `src/lib.rs:491,517`); dioxus-desktop 0.7.9 registers its menubar handler (`app.rs:449`) before its tray-menu handler (`app.rs:470`), so tray-menu clicks arrive on the muda hook and `use_tray_menu_event_handler` never fires — already tracked as DioxusLabs/dioxus#4495 (an earlier draft of this catalog had the direction inverted; corrected 2026-08-30, `apps/dioxus-tray/FRICTION.md`); (c) floem works **only because** its muda 0.17.x (floem requires `"0.17.1"`) and tray-icon 0.24's muda 0.19 are *different* compiled instances with two separate slots — had the app pulled tray-icon 0.21.x (muda 0.17) instead, cargo would have unified them and floem's handler would eat every tray click. Version divergence is currently load-bearing for correctness (`apps/floem-tray/FRICTION.md:44`). Facade requirement: per-menu closures or
an owned event route; **no global static channel**.

**T3 — muda use-after-free on first menu click (macOS, every published muda: 0.17.2 and 0.19.3 have identical code).** muda stores a raw `*const MenuChild` in each NSMenuItem (`#[ivars = Cell<*const MenuChild>]`, marked `FIXME: Use Rc`) without retaining it. Precisely: dropping the *item handles* is safe (the `Menu` keeps `Rc` clones), but dropping the owning `Menu`/`Submenu` after `init_for_nsapp` leaves AppKit's items dangling, and the first click reads freed memory (freya observed it as a panic inside muda's PNG encoder about an About panel the app doesn't have — most likely this bug, not a separate `about()` defect). Reported upstream as muda #202/#233 (both closed as user error) and **fixed on `main` by a1550bd / PR #361 on 2026-07-30, unreleased as of 2026-08-30**. Workaround until then: park every menu object for process lifetime. (`apps/freya-tray/FRICTION.md:26`) Facade: menus own their items.

**T4 — Creation ordering: main thread, *after* the run loop starts.**
Every framework paid this differently — boot task (iced), creator closure
(egui), `StartCause::Init` via external-loop embedding (xilem — impossible
with the stock runner), first timer tick (vizia), `exec_after` (floem);
gpui and freya got it free by owning the moment. The `TrayIcon` is `!Send`,
so frameworks with off-thread state need `Rc`-gymnastics. Facade: take a
main-thread callback; never expose a constructor that works "sometimes".
(`report/12-shell-integration-results.md`, per-app FRICTION)

**T5 — Menubar on plain winit is impossible on Linux at the type level.**
`Menu::init_for_gtk_window` requires `W: IsA<gtk::Window>` — the probe fails
to compile (E0277 retained, muda 0.19.3 `menu.rs:195`). This is the hard
boundary that forces either GTK-hosted windows (tao's reason to exist) or a
DBusMenu backend. (`report/data/linux-rows.md:74`)

**T6 — Linux tray: muda lets gtk-rs panic instead of returning `Err` when GTK is uninitialized.** macOS-written tray apps compile unchanged, then die at launch in the first `gtk::Menu::new` ("GTK has not been initialized"). The panic itself is gtk-rs's `assert_initialized_main_thread!` (gtk 0.18.2 `src/rt.rs:25`); muda's defect is the missing guard — `gtk::is_initialized()` is public and muda 0.19.3 (`src/platform_impl/gtk/mod.rs:377`) never consults it — so graceful degradation code never runs; build-only CI passes while every tested launch died. (`report/18-linux-reality-results.md:69`, verbatim panic in
`report/data/linux-rows.md:69`)

**T7 — Linux tray silent successes.** Bare `TrayIconBuilder::build` without
`gtk::init` returns `Ok` while the icon can never function; with GTK but no
StatusNotifier host on the bus (DBus `NameHasOwner` → false, captured),
everything still reports success. Zero feedback either way.
(`report/data/linux-rows.md:75-76`)

**T8 — tray-icon's winit path needs a parallel GTK event loop** (documented
in tray-icon's own winit example / lib.rs). A facade backed by ksni removes
the GTK loop entirely. (`report/00-ecosystem-map.md:125`)

**T9 — global-hotkey: X11-only Linux backend; polling delivery.**
On X11 it works end-to-end (registration + delivery of an xdotool-fired chord
— probe retained). No Wayland portal support, and no waker integration
anywhere: iced/slint/gpui/freya/vizia poll at 50–100 ms; only floem wired the
callback into a real wake primitive (`ExtSendTrigger`). Also: events carry
both `Pressed` and `Released` (unfiltered toggles net to zero — freya), and
contention behavior across processes was not characterised (egui and dioxus saw every registrant fire; slint's log mentions contention without detail — the "single owner" reading in an earlier draft had no source basis and is withdrawn) — the facade must define and test the contention contract. (`report/data/linux-rows.md:77`,
`apps/freya-tray/FRICTION.md:15`, `apps/egui-tray/FRICTION.md:71`,
`apps/slint-tray/FRICTION.md:30`)

**T10 — notify-rust on macOS: identity-dependent, and the failure is a modal.**
From an unbundled binary, mac-notification-sys resolves the default bundle id `"use_default"` by running the AppleScript `get id of application "use_default"` (0.6.15 `objc/notify.m:9-11` — not LaunchServices, as an earlier draft said); macOS 26 answers with **AppleScript's blocking "Where is use_default?" application chooser while `.show()` returns `Ok`** (already reported upstream as mac-notification-sys #81) (freya, vizia,
xilem all hit it). Attribution/permission belongs to the *host* terminal, not
the app. (`apps/vizia-tray/FRICTION.md:42`, `apps/freya-tray/FRICTION.md:21`)

**T11 — Runloop re-entrancy from notification helpers aborts the process.**
mac-notification-sys pumps the main runloop; called from inside a winit event
callback, winit 0.30 panic-aborts ("tried to handle event while another event
is currently being handled" — xilem, vizia, independently); called from a
`slint::Timer` callback it aborts with "Recursion in timer code". Safe shape: detached thread — which avoids the abort but does nothing about the identity problem, so unbundled binaries still get no banner (xilem's third failure), and `show()`'s `Ok` cannot tell you (T12). egui additionally observed eframe stop scheduling frames after a notify-rust call in 2 of 3 runs (cause unproven; the "delegate replacement" hypothesis in earlier drafts is refuted — mac-notification-sys sets only `NSUserNotificationCenter.delegate`, never `NSApp.delegate` (`objc/notify.m:308`) — and the same synchronous `[NSRunLoop runUntilDate:]` pumping (`notify.m:167-179`) is the likely cause; notify-rust rejected). Three frameworks
shipped `osascript` subprocesses instead. (`apps/xilem-tray/FRICTION.md:41`,
`apps/vizia-tray/FRICTION.md:42`, `apps/slint-tray/FRICTION.md:36`,
`apps/egui-tray/FRICTION.md:21`)

**T12 — `Ok(())` proves nothing about a banner.** Across the cohort, notification success returns were routinely not accompanied by an observable banner from bare binaries. Source-level cause (2026-08-30): in the pinned notify-rust 4.18 the macOS `show()` returns `Ok(NotificationHandle)` before anything is sent; the send happens in `Drop`, whose error is discarded with `.ok()` (`src/macos/nsusernotifications.rs:252`, `:142-161`). Acceptance tests must observe the banner (or the
facade must report display state), and must run from a bundled, identified
`.app`. (`report/12-shell-integration-results.md:86`)

**T13 — Hide/reopen is where frameworks leak.** eframe never runs `App::ui` for a hidden viewport, so reopen logic in `ui` is dead (first attempt: a window that could never come back; the fix — `App::logic` — is documented as running while hidden (eframe 0.35 `epi.rs:152-161`), but neither `App::ui` nor the viewport docs cross-reference it). gpui has no per-window hide, and app-level `cx.hide()` is
silently ignored while the NSMenu is still dismissing (fixed with a 300 ms
settle delay). (An earlier draft added "freya's close hook can't touch the window it's about (`pub(crate)`)" — retracted 2026-08-30: `AppWindow::window_mut()` and `RendererContext::windows_mut()` are public in freya-winit 0.4.1.) None of the ten apps drops the Dock icon, but 6 of 10 frameworks expose a first-class route to `ActivationPolicy::Accessory` (tauri, eframe, slint, xilem, freya, dioxus) and 4 do not (iced, vizia, floem, gpui) — "8 of 10 can't" in earlier drafts was wrong; only floem (`AppEvent::Reopen`) and tauri (`RunEvent::Reopen`) expose Dock-icon reopen.
(`apps/egui-tray/FRICTION.md:24`, `apps/gpui-tray/FRICTION.md:32`,
`apps/freya-tray/FRICTION.md:24`)

**T14 — Clipboard images: everyone re-rolls decode/convert.** arboard hands
back raw RGBA; four frameworks PNG-encode it again just to display it
(dioxus, vizia, floem, + base64 data-URI for the webview); one hit arboard's TIFF-only image read (3.6.1 `osx.rs:231-237`) failing on AppleScript's `as TIFF picture` flavor — real screenshots work (dioxus). gpui's native
multi-entry clipboard with decoded `ClipboardEntry::Image` is the reference
shape. (`apps/floem-tray/FRICTION.md:23`, `apps/dioxus-tray/FRICTION.md:25`)

**T15 — The polling tax (corrected 2026-08-30).** Nine of ten integrations drain the crates' crossbeam receivers from a 50–100 ms timer pump (iced additionally needs a non-default executor feature just to have a timer). That is a choice, not a necessity: muda, tray-icon and global-hotkey all ship `set_event_handler`, a callback invoked from the platform thread, and muda/tray-icon's docs recommend pairing it with an `EventLoopProxy` for winit/tao. The real constraint is T2 — each callback slot is a process-global `OnceCell`, so a framework that claims it (dioxus, floem) leaves the app with the receiver. Facade: per-menu callbacks with an explicit "call me on the main thread" bridge (floem's `ExtSendTrigger`/`create_ext_action` is the tested reference).
(`apps/iced-tray/FRICTION.md:48`, `apps/floem-tray/FRICTION.md:19`)

Secondary observations worth a footnote in any facade design doc: freya's release-only panic hook converts panics into a modal with stderr deferred behind it (diagnosability; `freya-winit/src/lib.rs:61-72`, still present in 0.5.0-rc); every winit-0.30 app in the cohort (seven) carries two objc2 generations — winit's 0.5 next to the tauri-apps shell crates' 0.6 — and xilem additionally two `keyboard-types` (type-identity conflicts across the shell crates; an earlier draft pinned this on xilem alone); slint's
tray icon handle is created once from a never-refiring change tracker (silent
stderr-only failure); wry's `dragDropEnabled` makes native drops and HTML5
DnD mutually exclusive per window.

## 5 · Linux target architecture

The corpus's probes bound the problem precisely:

- **Replace, don't wrap, the GTK paths.** ksni (StatusNotifierItem) removes
  both the parallel-GTK-loop requirement (T8) and the GTK-uninitialized panic
  (T6). A DBusMenu implementation replaces `init_for_gtk_window` (T5). ashpd's
  GlobalShortcuts portal adds the Wayland half that global-hotkey lacks (T9);
  keep the X11 grab as the fallback — it is the one path proven end-to-end.
- **Make absence loud.** The no-SNI-host case must be `Err(NoHost)` (T7); the
  container proved the current stack reports success. Ship the DBus
  `NameHasOwner` check the probe used.
- **Named gap:** the corpus never names a specific Rust DBusMenu crate —
  "Rust DBusMenu implementations exist" is as far as the sourced material
  goes. Selecting/auditing one is an early facade task.
- **Adjacent but out of scope:** renderer failures gate everything (a tray is
  useless if the window never paints — see the Linux round's iced/xilem/gpui
  render findings), and winit-gtk4 (merged 2026-07-16, on winit 0.31's
  beta-only pluggable-backend architecture) may eventually change where the
  GTK boundary sits for webview hosts.

## 6 · What good looks like — tested reference shapes

**6.1 gpui (menubar/dialogs/clipboard, zero tauri-apps crates for those):**
`cx.set_menus` + `actions!` + keymap-derived accelerators;
`MenuItem::os_action` gives the cohort's **only focus-aware Edit menu**;
`prompt_for_paths` native panels over a oneshot channel; multi-entry clipboard
with native image decode. Caveats: surface ≠ implementation off-macOS
(`register_url_scheme` unimplemented on Win/Linux in 0.2.2), no tray/toast in
core, thin re-entrancy guards (a scripted menu click aborted on a RefCell
borrow), and action handlers must `cx.defer` or silently no-op.
(`apps/gpui-tray/FRICTION.md`, `report/03-gpui.md` §4)

**6.2 floem (most complete built-in surface + the wake primitive):** menu
items carry action closures (no MenuId bookkeeping, no channel); `open_file`/
`save_as` deliver callbacks on the UI thread; typed `FileDragDrop`,
`ThemeChanged`, `WindowCloseRequested` listeners; `AppEvent::Reopen`; and
`ExtSendTrigger` — the only first-class foreign-thread wake primitive in the
cohort, which is exactly the delivery mechanism T15 asks for. Cost observed:
none of it is documented outside the source. (`apps/floem-tray/FRICTION.md`)

**6.3 freya (`tray` feature owns the global handlers):** the framework
registers muda/tray-icon's global handlers itself and forwards both into one
typed callback — the app never links its own muda copy, so T2 *cannot happen*,
and the builder runs on the main thread at the right moment, so T4 is gone.
This is the structural template for a facade's framework adapters. Remaining
cost: the callback runs outside the reactive scope (freya apps still poll to
get back in — the facade's main-thread bridge closes exactly this gap).
(`apps/freya-tray/FRICTION.md:14`, `report/data/shell-text-rows.md:486`)

**6.4 slint (declarative shell):** `SystemTrayIcon` + `MenuBar` as DSL
elements make the shell ~80% declarative with reactive menu labels — the
ceiling for ergonomics, though its standard Edit roles don't exist and its
tray handle creation is single-shot (T catalog). (`apps/slint-tray/FRICTION.md`)

**6.5 Aggregate baselines to beat** (same spec, LoC incl. markup): dioxus 394 ·
slint 447 · egui 514 · tauri 559 · floem 610 · iced 682 · vizia 720 · xilem
774 · freya 865 · gpui 1,575. Shell integration grew dependency trees by 8–112
crates per framework. A facade should collapse the assembled-crate boilerplate
toward the dioxus/slint end while keeping native rendering.

## 7 · Crate-health context (why "maintained" is part of the spec)

From the dated 2026-07-07 audit (`report/data/load-bearing-crates.md`): muda,
tray-icon, global-hotkey are bus-factor ≤1 with the historical author near
zero commits in 12 months and no visible funding for the volunteer now on top
of all of them; "de-GTK work has no owner." rfd: one maintainer, 0 sponsors,
479 reverse deps — but the best Linux story in the layer (XDG portal default).
arboard: ~10.5-month release gap against 1,052 reverse deps. notify-rust: one
volunteer across three platforms; the macOS backend self-describes as a
"small subset" of a deprecated API. A facade that *wraps* these crates
inherits these numbers; a facade that *replaces* the Linux paths (§5) and the
notification backend (T10–T12) reduces the exposure to the macOS/Windows
tray/menu cores.

## 8 · Open questions for the effort

1. **Where does it live?** rust-windowing (winit has wanted to shed this scope
   since #403), tauri-apps (owns the incumbent crates, concentration risk), or
   a neutral org. This corpus has no data to answer it — but funding
   recommendation #2 of the study notes no dedicated public funding source
   exists for this work today.
2. **Facade vs upstream fixes?** Several traps are one-line-ish upstream fixes
   (muda: return `Err` instead of panicking without GTK; retain menu items;
   document the Edit-role constraint). Filing those has value independent of
   any facade, and the trap catalog above is effectively the issue list.
3. **The contention/permission contracts** (hotkey multi-registration, TCC
   inheritance, notification identity) need explicit specification — the
   corpus observed inconsistent behavior and can only flag it.
4. **Windows.** Everything here is macOS-tested/Linux-probed; the Windows
   column of the incumbent crates is source-verified only. The acceptance
   checklist needs a Windows run before it can claim three platforms.
