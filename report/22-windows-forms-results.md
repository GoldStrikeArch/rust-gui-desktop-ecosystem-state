# Iteration 5: "Windows" (multi-window + modal) and "Ledger" (forms + numeric input) in eleven entries (macOS)

Two new specs, `apps/SPEC-9.md` and `apps/SPEC-10.md`, implemented as 22
independent crates on 2026-08-30: the ten frameworks of the corpus plus
**Dioxus Native** (Blitz `main` @ `64eb278`, workspace 0.3.0-beta.2) as an
eleventh entry. Every app was built `--release --locked` on the pinned
toolchain, drives its own scripted checks through a `WINDOWS_SELFTEST=1` /
`LEDGER_SELFTEST=1` hook (all 22 report `fail=0`), and carries retained
evidence in `apps/<fw>-<spec>/evidence/`. Three independent verifier passes
rebuilt every crate, re-ran every self-test, re-checked the key interactive
behaviours serially with synthetic input, spot-checked FRICTION claims
against the vendored framework sources, and normalized the ratings
(`report/data/verification/iter5-*-verifier.md`). Raw per-framework rows:
`report/data/iter5-rows.md`.

Reference machine: Apple M4 Pro, macOS 26.5.2, rustc 1.96.1. Build times in
the rows were measured while up to six sibling builds ran; the serial
`./measure.sh --round iter5` pass is canonical for build time, size and
dependency counts.

## What the two specs test

**Windows** asks the question every earlier round dodged: what *is* a window
in this framework, and can two of them see the same mutable state? Around
that it measures modality (which kind the framework can actually reach — OS
window-modal sheet, OS app-modal, or an in-framework overlay), parenting,
`CloseRequested` veto, focus return, persistence, per-window shortcuts.

**Ledger** asks whether the framework's text input can be a *numeric* input
— filter while typing, locale-aware parse/format on blur, arrow stepping,
digits that line up on the decimal point — and whether the things Qt/WinForms
people take for granted (tab order across mixed controls, Enter-moves-down,
validation state, undo) exist. It also puts the corpus' first accessibility
*verification* (an OS accessibility-tree dump per framework) on record.

## Headline findings

1. **The window model splits the cohort three ways, and it decides the
   shared-state question before any code is written.** *Window-as-view*
   frameworks (xilem, vizia, floem, gpui, iced's `daemon`, freya, slint via a
   shared `ModelRc`) give one state tree to N windows for free: the
   bidirectional inspector↔list requirement cost zero lines. *Window-as-
   VirtualDom* (dioxus) is the interesting middle: each window is its own
   `VirtualDom`, but a `Signal` injected through `with_root_context` is a
   generational-box handle whose reads subscribe the reading window and
   whose writes wake every subscriber through its own scheduler — genuine
   one-source-of-truth with no channel; `GlobalSignal`, by contrast,
   resolves per runtime and silently forks per window. *Window-as-
   deferred-closure* (eframe viewports) forces `Arc<Mutex<_>>` because the
   deferred callback is `Fn + Send + Sync + 'static`. Slint's `export
   global` is per-component-instance, so two windows get two globals.
2. **OS window-modality exists in exactly two frameworks' APIs, and one of
   them spells it strangely.** gpui's `window.prompt` and Tauri's
   `set_enabled(false)` (which tauri-runtime-wry implements on macOS as an
   invisible alpha-0.5 NSWindow attached with `beginSheet:`, `EnableWindow`
   on Windows, `set_sensitive` on Linux) are the only framework paths to a
   real sheet. iced, slint and floem reached the same sheet through ~30 lines
   of objc2 on the raw window handle; dioxus, xilem, freya, vizia, egui and
   dioxus-native ship an in-framework overlay that blocks input but is not
   OS modality (every one verified with a synthetic click on the parent's
   Delete button and a control click). winit exposes no modal API at all,
   which is why the hand-rolled solutions all look alike.
3. **Parenting (child/owner/transient) is built in only for the tao-based
   webviews** (dioxus, tauri via `with_parent_window`); winit's
   `with_owner_window` is `#[cfg(windows)]`-only, so xilem, egui, vizia and
   dioxus-native rate not-achievable, and iced/slint/floem/freya assembled
   it from `addChildWindow:` — which **crashes Freya** ("AccessKit adapter
   must be created before the window is shown", because the child is ordered
   in before Freya builds the adapter) and, in Tauri, is **mutually exclusive
   with `always_on_top`** (`addChildWindow:` resets the child's level).
4. **Close veto is where the winit seam shows.** iced, egui, gpui, tauri,
   slint, vizia and floem expose it; dioxus-desktop 0.7.9 has none (the app
   hides and restores); xilem's exists only if you own the event loop
   ("not forwarding `on_close_requested` is the veto"); and Dioxus Native —
   which had to re-implement the launcher to open a second window at all —
   got a genuine `CloseRequested` veto that its webview sibling lacks.
5. **Numeric input is hand-rolled almost everywhere; the framework-level
   differences are in what a text input will let you refuse.** Only slint
   (`TextInput.key-pressed` returning `accept`), freya (`on_validate` undoes
   rejected input) and iced (controlled `text_input`) offer a true
   pre-insertion filter; vizia's `validate` is post-validation; Dioxus
   drops characters when typed fast into a controlled input (every keystroke
   round-trips DOM→IPC→VDOM→diff→DOM) and needs an element remount to reject
   one. Locale parsing was hand-rolled in ten of eleven; only the webview
   (Tauri) got `Intl.NumberFormat` and `formatToParts()` for free.
6. **Tabular figures (`tnum`) are reachable in the CSS engines and in gpui
   only.** gpui's `FontFeatures` is real and measurable (`'1'×10` = 63.44 px
   vs `'0'×10` = 86.68 px on the system font; 86.68 for both with
   `tnum`+`lnum`); Tauri, Dioxus and Dioxus Native accept one CSS line. iced
   has no OpenType feature API, epaint hard-codes `shaper.shape(buf, &[])`,
   slint/vizia/freya/floem expose nothing; xilem's `FontFeatures` is only
   reachable by re-implementing the label view. Everyone else aligned on the
   decimal point with a monospaced face.
7. **Accessibility, verified rather than claimed.** Named *and* valued
   trees: tauri (741 nodes), dioxus (323), egui (224/48 named), slint (209,
   49 fields named), vizia (160), freya (331, but AccessKit is off by
   default). Values without names: xilem (61 fields, 0 names). Names and
   roles without values: dioxus-native (252 nodes with correct roles, empty
   `AXTitle`/`AXValue`). Nothing: gpui (4–7 nodes; no `accesskit` or
   `NSAccessibility` anywhere in 0.2.2), iced (97 nodes, all chrome), floem.
   Two harness traps matter for anyone repeating this: AccessKit trees
   activate lazily, so the spec's `System Events → entire contents` recipe
   returns only window chrome on the first query (egui, masonry, slint,
   freya, vizia all needed a second probe or `AXManualAccessibility`), and
   `osascript keystroke … using command down` delivers no modifier flags to
   winit — earlier corpus observations of "shortcut doesn't work" driven that
   way are suspect.
8. **Dioxus Native shares the desktop source unchanged and costs 4× the
   binary.** Both desktop `app.rs` files compile and run on Blitz `main` with
   zero diff hunks (`#[path]` include + a per-backend `platform.rs`). The
   price: 24.5/27.9 MB binaries vs 6.5/6.4 MB, 520/524 unique crates vs
   377/380, ~76 s clean builds vs 35–54 s; the gain: one process instead of
   four (no WebKit XPC helpers), similar idle RSS (105–108 MB vs 100 MB plus
   ~100 MB in helpers). Blitz gaps found: `<select>`, `<input type=date>`
   and `<input type=range>` are in the DOM and the AccessKit tree but paint
   nothing; checkbox/radio dispatch `input` but never `change`; Tab moves
   blitz-dom's internal focus without emitting focus/blur (so the next
   pointer blur misses the edited input and the edit is lost); ⌘-chords never
   reach the VirtualDom; IME is `None` upstream; ZWJ family emoji renders
   blank while CJK and Arabic shape correctly. Stylo handled every selector
   the corpus uses (grid, sticky, fixed scrims, `color-mix`, `tabular-nums`).

## Capability matrices

The ratings below are the three verifiers' *normalized* matrices (rubric:
**built-in** = framework API does it; **assembled** = framework pieces plus
≤ ~30 lines of glue; **hand-rolled** = the app implements the mechanism, OS
calls and owned event loops included; **not-achievable** = no path in the
pinned framework, approximation shipped). Footnote letters mark cells where
a verifier changed or qualified a build agent's rating; the same tables with
provenance live in `report/data/verification/iter5-matrix.md`.

### SPEC-9 "Windows"

| Capability | dioxus 0.7.9 | xilem 0.4.0 | gpui 0.2.2 | iced 0.14.0 | egui 0.35.0 | tauri 2.11.5 | slint 1.17.1 | dioxus-native (Blitz 64eb278) | freya 0.4.0 (freya-* 0.4.1) | vizia 0.4.0 | floem git 778bb5f |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Second top-level window (open/close/singleton focus) | built-in | built-in [c] | built-in | built-in | built-in | built-in | built-in [w] | hand-rolled (own `ApplicationHandler` over blitz-shell) [y] | built-in | built-in (a window is a view) | built-in (singleton focus hand-rolled) |
| Modal dialog — kind achieved | hand-rolled (child window + scrim) | hand-rolled (window + event filter) | hand-rolled (window + occlude scrim) [g] | hand-rolled (objc2 `beginSheet:` = real OS sheet) | built-in (`egui::Modal` overlay) [h] | assembled (`set_enabled(false)` sheet + own window) | hand-rolled (objc2 `beginSheet:` = real OS sheet) | hand-rolled (window + scrim) | hand-rolled (child window + overlay scrim) | assembled (focus lock + always-on-top; not OS-modal) | hand-rolled (objc2 `beginSheet:` = real OS sheet) [av] |
| Parent blocked while modal | hand-rolled | hand-rolled | hand-rolled | hand-rolled [a] | built-in (per viewport) [h] | built-in | hand-rolled [s] | hand-rolled | assembled (`interactive(false)` subtree) | assembled (`.disabled()` inheritance) | hand-rolled [av] |
| Focus returns to parent after modal | assembled | assembled | assembled [b] | assembled [b] | built-in (never leaves the window) | assembled [b] | assembled [t] | assembled | assembled [aq] | built-in | assembled [aw] |
| Shared state across windows | built-in | built-in | built-in | built-in | hand-rolled (`Arc<Mutex>`) [i] | assembled (emit/listen bus) [i] | assembled (one `ModelRc` + `changed` down-sync) | built-in | built-in (`State::create_global`) | built-in (one Context/tree) | built-in (process-global signals) |
| Cross-window message (Ping/Pong) + wake | built-in | built-in [j] | built-in | built-in [j] | assembled | built-in | assembled (`set_row_data` + `Timer`) | built-in (signal write → `BlitzShellEvent::Poll`) | built-in | built-in | built-in |
| Theme/layout change to all windows | built-in (paint lags on unfocused windows) | built-in (light palette hand-applied per widget) | built-in | built-in | built-in | built-in | assembled (per-window `Palette.color-scheme`) [u] | built-in mechanism, dead controls (no `change` event) [aa] | assembled [ar] | assembled | built-in (`set_global_theme`) |
| Window parenting (child/owner/transient) | built-in (tao `with_parent_window`, macOS ext) | not-achievable | not-achievable | hand-rolled (objc2 `addChildWindow:`) | not-achievable | built-in | hand-rolled (objc2 `addChildWindow:`) | not-achievable | hand-rolled (objc after the AccessKit crash) [as] | not-achievable (owner is `#[cfg(windows)]`) | hand-rolled (objc `addChildWindow:`) [av] |
| Close veto (Save/Discard/Cancel) | hand-rolled (hide + restore) | hand-rolled [c] | built-in | built-in | built-in | built-in | built-in (`on_close_requested`) [x] | hand-rolled (own `window_event`, real veto) | assembled (`with_on_close`, `Send`-bound) | built-in | built-in (`prevent_default`) |
| Quit semantics | assembled | built-in | assembled | built-in | built-in | built-in | built-in | assembled | assembled | built-in | assembled |
| Native confirm dialog | assembled (rfd, out-of-process) [n] | assembled (rfd, out-of-process unless `set_parent`) [n] | built-in (sheet) | assembled (rfd, out-of-process) [n] | assembled (rfd sheet) | assembled (dialog plugin, sheet) | assembled (rfd, out-of-process) [n] | assembled (rfd, out-of-process) [ab] | assembled (rfd, out-of-process) [n] | assembled (rfd, out-of-process) [n] | assembled (rfd, out-of-process) [n] |
| Position/size persistence | assembled | hand-rolled [c] | assembled | assembled | assembled [d] | assembled (window-state plugin) | assembled (logical units, 120 ms timer) | assembled | assembled (round-trip creeps −32 px/cycle) [at] | hand-rolled | hand-rolled (compensates window-relative geometry) |
| Multi-monitor scale change | not-verified | not-verified | not-verified | not-verified | not-verified | not-verified | not-verified | not-verified | not-verified | not-verified (broken by construction: one global `dpi_factor`) | not-verified |
| Per-window shortcuts (⌘W focused only, ⌘, , ⌘⇧I) | assembled [f] | hand-rolled | assembled | assembled [e] | built-in | built-in | assembled [v] | not-achievable [ac] | hand-rolled (⌘, leaks "," into a focused Input) [at] | assembled | assembled |

### SPEC-10 "Ledger"

| Capability | dioxus | xilem | gpui | iced | egui | tauri | slint | dioxus-native | freya | vizia | floem |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Numeric field (typed, filtered, step keys) | hand-rolled | hand-rolled | hand-rolled | assembled (controlled `text_input`; stepping hand-rolled) | hand-rolled | hand-rolled | assembled (`key-pressed` pre-insertion filter) [ah] | hand-rolled | assembled (`on_validate` keystroke filter) | hand-rolled (`validate` is post-only) | hand-rolled |
| Locale-aware parse + format (live toggle) | hand-rolled | hand-rolled | hand-rolled | hand-rolled | hand-rolled | assembled (`Intl` format; parse hand-rolled) [m] | hand-rolled | hand-rolled | hand-rolled | hand-rolled | hand-rolled |
| Decimal alignment / tabular figures (font-feature request) | built-in | hand-rolled | built-in | not-achievable (alignment via monospace) [l] | not-achievable (alignment via monospace) [l] | built-in | not-achievable (alignment via Menlo) [ad] | built-in (CSS accepted; glyph application not isolated) [ai] | not-achievable (monospace + padding; `text_align(Right)` broken) [ax] | not-achievable (monospace) [ax] | not-achievable (separator-split labels + monospace) [ax] |
| Inline validation + disabled Save + summary | assembled | assembled | hand-rolled | assembled | assembled | assembled [o] | assembled | assembled [aj] | assembled | assembled (`:invalid` pseudo-class built-in) | assembled |
| Dropdown with type-ahead | built-in | hand-rolled (inline list approximation) | hand-rolled | built-in | assembled | built-in | assembled (`ComboBox` + `FocusScope`) | not-achievable (`<select>` paints nothing) | hand-rolled (`Select` has none) | built-in (`ComboBox`) | built-in dropdown, type-ahead hand-rolled |
| Date input (masked or picker) | built-in (picker) | hand-rolled (mask; breaks on filled cell) | hand-rolled (mask) | hand-rolled (mask) | hand-rolled (mask) | built-in (segmented native) | assembled (char filter + calendar check; picker not per-cell) [ag] | not-achievable (`<input type=date>` paints nothing) | assembled (masked `Input`) | assembled (mask + built-in `Calendar`) | hand-rolled (mask) |
| Slider ↔ numeric field | assembled | assembled | assembled | assembled [r] | built-in | assembled | assembled [ae] | not-achievable (`<input type=range>` paints nothing) [ak] | assembled (Slider is percentage-only) | assembled | built-in (shared signal) |
| Tab order across mixed controls | built-in | built-in | built-in | hand-rolled | built-in | built-in | built-in | not-achievable [an] | built-in | built-in | built-in (defect: Dropdown visited at the row boundary) |
| Enter-moves-down | assembled | hand-rolled | assembled | assembled [p] | assembled [p] | assembled [p] | assembled | not-achievable [an] | assembled | hand-rolled | hand-rolled |
| Undo/redo (field / form) | built-in / hand-rolled | not-achievable / hand-rolled | not-achievable / hand-rolled | not-achievable / hand-rolled | built-in / hand-rolled | built-in / hand-rolled | built-in / hand-rolled [af] | not-verified / hand-rolled [am] | built-in (⌘Z/⌘Y) / hand-rolled [ay] | not-achievable / hand-rolled [ay] | not-achievable (`TextInput`; `text_editor` has it) / hand-rolled |
| Live computed columns and totals | built-in | built-in | built-in | built-in | built-in | built-in | built-in | built-in | built-in | built-in (`Memo`) | built-in |
| Row copy/paste as TSV | assembled (⌘⇧C/V) | assembled (⌘⇧C/V) | assembled [q] | assembled (⌘⇧C/V) | assembled | assembled (clipboard plugin) | assembled (⌘⇧C/V + buttons) | assembled (buttons only) [ap] | assembled [az] | assembled [az] | assembled [az] |
| Accessibility labels (verified dump) | built-in (323 nodes, named) | not-achievable for names (61 values, 0 names) [k] | not-achievable (7 nodes) | not-achievable (97 nodes, no content) | built-in (224 nodes, 48 named; needs 2nd AX pass) | built-in (741 nodes, named) | built-in (209 nodes, 49 fields named + valued) | not-achievable for names/values (337 nodes, 37 fields, 0 named) [al] | built-in, off by default + frozen (422 nodes, 49 titled fields, 0 values, titles never update) [au] | built-in (248 nodes, names + live values; 2nd AX pass) | not-achievable (98 nodes, chrome only) |
| IME composition (optional) | not-verified | not-verified | not-verified (not-achievable as built) | not-verified | not-verified | not-verified | not-verified | not-achievable [ao] | not-verified | not-verified | not-verified |

### Consolidated footnotes

Wave 1 ([a]–[r]):

- [a] iced "parent blocked": agent said **built-in (OS, once the sheet exists)**. The OS enforces it
  only because the app called `beginSheet:` through objc2; under the rubric OS calls made by the
  app are hand-rolled. The blocking itself is verified (0 `delete-requested` under the sheet).
- [b] "focus returns": gpui (agent: hand-rolled) is a single `window.activate_window()` call; iced
  (agent: built-in) and tauri (agent: built-in) both make explicit `gain_focus`/`set_focus` calls
  in their dismiss paths. All three are one-line compositions → assembled. egui is the only one
  where nothing is needed (overlay never leaves the window).
- [c] xilem close veto (agent: assembled), persistence (agent: assembled) and singleton focus
  (agent: assembled, folded into the built-in "second window" cell): all three exist only because
  the app replaces `Xilem::run_in` with its own winit `ApplicationHandler` and wraps `AppDriver`
  (`shell.rs`, 464 lines). "Not forwarding `on_close_requested`" is a genuine framework seam, but
  reaching it means owning the event loop → hand-rolled under the rubric.
- [d] egui persistence (agent: hand-rolled): `ViewportInfo.outer_rect/inner_rect` +
  `ViewportBuilder::with_position/with_inner_size` + ~40 lines of serde → assembled. The agent's
  reason (eframe's own `persist_window` is inert without the non-default `persistence` feature and
  single-window) stands as a finding.
- [e] iced shortcuts (agent: hand-rolled): `event::listen_with` already reports the window id per
  event; the app's part is a ~20-line key match → assembled. ⌘W itself is AppKit's key equivalent.
- [f] dioxus shortcuts (agent: "mixed"): ⌘W/⌘Q are built-in via the default muda menubar; ⌘, and
  ⌘⇧I are DOM `onkeydown` on the root element (framework piece + glue) but only fire while that
  window is key and Esc is lost when focus sits on `body` → assembled, with that defect.
- [g] gpui modal kind: the custom-content dialog is a plain window + `.occlude()` scrim
  (hand-rolled); `window.prompt` is a built-in OS sheet but button-only, so it cannot host the
  edit form. Rated on the edit dialog.
- [h] egui modal: `egui::Modal` is a framework API → built-in, but it is an in-framework overlay,
  not OS modality, and it is per-viewport (other windows need the app's `ui.disable()`); real
  sheets exist only for rfd message boxes.
- [i] shared state: egui's `Arc<Mutex<Shared>>` is std, not a framework piece (forced by the
  `Send + Sync + 'static` deferred-viewport closure) → hand-rolled kept. tauri's event bus is a
  framework piece plus ~12 lines of broadcast/listen glue → assembled (the tauri agent's private
  scale "built-in = in the tauri crate" is not the corpus rubric).
- [j] xilem (agent split: Ping built-in / timed Pong assembled) and iced: the cross-window message
  is free in both; only the 300 ms flash timer needed a workaround (xilem: std thread + channel +
  `worker` view; iced: `Task::future` + `thread::sleep` because timers need the smol/tokio feature).
- [k] xilem a11y (agent: "partial"): the AccessKit tree and every *value* are built-in, but
  `text_input` offers no accessible name and none of the 61 fields has one, and the split
  decimal renders as two `static text` nodes. The spec row asks for name *and* value → rated on
  the missing half.
- [l] iced/egui decimal alignment (agents: "assembled; font features not-achievable"): rated on
  the row's font-feature request (no OpenType feature API in `iced::Font`; epaint hard-codes
  `shaper.shape(buffer, &[])`). Alignment itself was achieved with a monospaced face + fixed two
  decimals in both, verified.
- [m] tauri locale (agent: built-in format / hand-rolled parse): `Intl.NumberFormat` is a real
  platform piece and the parser is ~55 lines of Rust → assembled.
- [n] rfd confirm dialogs are assembled in the non-gpui apps, but only egui/tauri (parented)
  get an in-process sheet; dioxus/xilem/iced — and slint, dioxus-native, freya, vizia, floem
  (waves 2a/2b) — get the out-of-process `CFUserNotification` alert (unbundled binaries).
- [o] tauri validation (agent: hand-rolled): DOM `disabled`, a CSS class and an `aria-live` strip
  plus ~45 lines across Rust/JS → assembled. gpui stays hand-rolled: there is no widget layer at
  all, the cell border, count and button are all app-drawn.
- [p] Enter-moves-down: iced (`listen_with` + `operation::focus(id)`), egui (`lost_focus()` +
  `request_focus(cell_id)`) and tauri (`focus()` on the next `[data-f]`) are each ≤ ~10 lines on
  framework focus APIs → assembled; xilem stays hand-rolled because `RenderRoot::focus_on` is only
  reachable from the app-owned driver.
- [q] gpui TSV (agent: built-in clipboard + hand-rolled format): framework clipboard + ~20 lines
  → assembled.
- [r] iced slider↔field (agent: built-in one way / hand-rolled the other): the value linkage is
  free, the keyboard half needs the app's own `Focus::Slider` because `slider` is not focusable →
  assembled overall.

Wave 2a ([s]–[ap]):

- [s] slint "parent blocked": agent said **built-in (once the sheet exists)**. As for iced [a],
  the OS enforces it only because the app called `beginSheet:` through objc2 → hand-rolled.
  Blocking verified (0 DELETED under the sheet, post-control deleted).
- [t] slint "focus returns": agent built-in; `endSheet:` restores key status but the app also
  calls `show()` on the main window in `close_dialog` (`main.rs:178`) — one-line composition →
  assembled, as [b].
- [u] slint theme: agent hand-rolled. `Palette.color-scheme` is the framework's theme API; the
  app repeats a 3-line `apply-theme()` in each window because globals are per instance (the
  finding stands) → framework piece + glue = assembled.
- [v] slint shortcuts: agent hand-rolled. `FocusScope.key-pressed` is a framework piece and the
  match is ~8 lines per window; ⌘W routes into the built-in `on_close_requested` → assembled,
  as iced [e]. The `modifiers.control` = Command remap is a documented trap, not extra code.
- [w] slint second window: built-in kept, qualified: secondary windows open at layout-minimum
  size (244×236 / 123×202) and windows shown from a callback do not paint until an event.
- [x] slint close veto: built-in kept (`CloseRequestResponse::KeepWindowShown` verified via
  ⌘W → Cancel → alive → Discard → exit 0); the in-app prompt window it relies on renders blank
  until a resize/click forces a paint.
- [y] dioxus-native second window: hand-rolled (agent's word) kept — the app replaces
  `launch_cfg` with its own `ApplicationHandler` (446 lines), the same reasoning as xilem [c].
- [aa] dioxus-native theme: agent "built-in (via signals) — unreachable from the UI". The
  restyle mechanism (shared Signals + Stylo ancestor-class restyle) is built-in and verified
  programmatically (self-test, shot 11), but the Theme radios and Compact checkbox in the
  shared `app.rs` use `onchange`, which Blitz never dispatches, so as shipped a user cannot
  change the theme. Cell reads "built-in mechanism, dead controls".
- [ab] dioxus-native confirm: agent built-in (rfd) → assembled per [n]; the alert is an
  out-of-process `UserNotificationCenter` window (verified twice).
- [ac] dioxus-native shortcuts: not-achievable confirmed from source (`AppleStandardKeybinding
  (_) => None`, `tabindex=-1` non-focusable, no menubar) and by the agent's frontmost retest.
- [ad] slint decimal alignment: agent "hand-rolled; feature request not-achievable" → rated on
  the row's font-feature request as [l]: no font-feature property exists anywhere in
  `builtins.slint`; alignment via Menlo + fixed two decimals verified.
- [ae] slint slider↔field: agent built-in. `Slider.changed` and `LineEdit.edited` each call a
  Rust callback that writes the other widget's property (`main.rs:344-363`) — the same
  two-callback glue rated assembled for iced/gpui/dioxus/tauri/xilem.
- [af] slint form-level undo: agent assembled → hand-rolled: a `Vec<(usize, Item)>` snapshot
  stack (`main.rs:42,131-135,376-384`), identical in kind to every other framework's
  hand-rolled form undo. Field-level built-in (TextInput's own undo/redo) kept.
- [ag] slint date: agent "assembled (mask) + built-in (picker)". The cell is a digit/`-` filter
  plus a calendar check on commit, not a positional mask; `DatePickerPopup` is real but
  window-level and fills the focused row → assembled overall.
- [ah] slint numeric field: assembled kept, with the verifier's caveat that mouse focus
  defeats select-on-focus (keyboard focus works).
- [ai] dioxus-native decimal alignment: agent assembled → built-in for consistency with the
  identical CSS (`font-variant-numeric: tabular-nums` + `font-feature-settings`) rated built-in
  for dioxus-desktop and tauri in wave 1; Stylo accepts it, whether Parley applies `tnum` was
  not isolated (the system UI face has uniform digit advances).
- [aj] dioxus-native validation: agent hand-rolled → assembled: it is the same shared `app.rs`
  code wave 1 rated assembled for dioxus-desktop.
- [ak] dioxus-native slider: agent "partially not-achievable" → not-achievable: the
  `<input type=range>` paints nothing (verified in `01-initial`), only the linked numeric field
  works.
- [al] dioxus-native a11y: agent "partial" → rated on the missing half as [k]: roles and
  geometry are exposed, `aria-label` and values are not (accessibility.rs never reads
  `aria-label`); 37 fields, 0 titled, 0 valued on the verifier's dump; first AXUIElement pass
  98 nodes.
- [am] dioxus-native undo: field-level "not verified" by the agent and unreachable in
  practice (⌘Z is an AppKit standard key binding that dioxus-native-dom drops) →
  not-verified; form-level hand-rolled (shared code).
- [an] dioxus-native Tab / Enter-moves-down: not-achievable confirmed (Tab: no FOCUS line,
  identical captures; `set_focus_to` dispatches no events), and Tab additionally desynchronises
  the next pointer blur (edit lost).
- [ao] dioxus-native IME: `DomEventData::Ime(_) => None` upstream → not-achievable by
  construction (agent's word kept; wave-1 columns say not-verified).
- [ap] dioxus-native TSV: assembled kept; the ⌘⇧C/⌘⇧V chords never reach the VirtualDom, so
  only the toolbar buttons (verified by the agent with the real pasteboard) and the self-test
  exercise it.

Wave 2b ([aq]–[az]):

- [aq] freya "focus returns": agent built-in → assembled as [b]: an explicit
  `Platform::get().focus_window(Some(main_id))` in `close_dialog` and the dialog's `use_drop`
  (one-line composition; a newly launched freya window does not become key by itself).
- [ar] freya theme: agent built-in → assembled as slint [u]: `use_init_theme` is the framework
  piece, but each window repeats a `use_side_effect` pushing `light_theme()`/`dark_theme()`
  into its own theme state; the change itself was verified in the agent's captures.
- [as] freya parenting: agent "assembled (with a Freya-blocking bug)" → hand-rolled: the
  shipped mechanism is 20 lines of raw `sel_registerName`/`objc_msgSend` FFI, required because
  winit's `with_parent_window` crashes freya's AccessKit adapter (panic verified in
  accesskit_winit 0.33.2 source, ordering in freya-winit 0.4.1). OS calls by the app are
  hand-rolled per [a]/[s].
- [at] freya defects found by the verifier, ratings kept: persistence (assembled) creeps
  −32 px logical per save/restore cycle (save `outer_position()`, restore
  `WindowAttributes::with_position` = AppKit content rect; 268 → 236 → 204 over three cycles,
  and the agent's own evidence shows 260 → 228); shortcuts (hand-rolled) leak the plain
  character of a ⌘-chord into a focused `Input` (⌘, prepended "," to the shared project name
  and dirtied the document — reproduced twice).
- [au] freya a11y: agent "built-in but off by default" → second qualifier added: after
  `AXManualAccessibility` activation the tree is complete and cell-named (79 `AXCell`s), but
  field AXTitles are creation-time snapshots that never update, no `AXValue` is exposed (0/49),
  and checkboxes are unnamable — a screen reader sees the launch-time ledger.
- [av] floem modal kind (agent: "OS window-modal assembled from objc2"), parent-blocked
  (agent: "built-in to the sheet") and parenting (agent: "assembled via addChildWindow:"):
  all three exist only through the app's own objc2 `msg_send!` calls → hand-rolled per [a]/[s];
  the sheet and its blocking are real and verified (0 presses under the sheet, byte-identical
  captures — the sheet even keeps the parent non-key).
- [aw] floem "focus returns": agent hand-rolled → assembled as [b]/[t]: `endSheet:` +
  `close_window` + one `focus_window(parent)` line in `close_modal` (`main.rs:275-285`).
- [ax] decimal alignment (freya/vizia/floem): rated on the row's font-feature request per
  [l]/[ad] — no OpenType-feature API exists in any of the three (freya's
  `Input::text_align(Right)` is additionally broken); the achieved alignments (freya monospace
  + space padding, vizia monospace, floem split-at-separator labels) were verified on screen.
- [ay] form-level undo: freya (agent: assembled, ⌘⇧Z) and vizia (agent: unrated "form-level
  only") are app-side snapshot stacks — `Vec<(row, field, prev)>` / `Vec<Vec<Row>>` — identical
  in kind to every other framework's form undo → hand-rolled per [af]. freya's field-level
  built-in (editor ⌘Z/⌘Y, verified in freya-edit 0.4.1 source) kept.
- [az] TSV (freya/vizia/floem): agents said built-in (framework clipboard in all three); the
  row⟷TSV formatting and the re-bound ⌘⇧ chords (freya) are app glue on the clipboard piece →
  assembled per [q].


## Defects and traps found in this round (upstream candidates)

Confirmed in source by the build or verifier agents; each is in the
corresponding FRICTION.md with the file:line.

- **freya**: `with_parent_window` crashes (AccessKit adapter ordering); position persistence creeps −32 px per save/restore cycle (`outer_position()` vs a content-rect `with_position`); ⌘, leaks a "," into the focused `Input` and dirties the document; AccessKit `AXTitle`s are frozen snapshots (a live locale toggle is invisible to AX);
  `Input::text_align(TextAlign::Right)` pushes text out of the widget's
  ScrollView (a `1` renders empty); `Input` swallows every key it doesn't use
  (third Freya app to hit this); no `on_blur`; `with_on_close` is `Send` so
  it cannot read a `State`; `Checkbox` cannot be given an accessible name;
  `WindowConfig` has no `with_position`.
- **vizia**: `.title()` on a sub-window renames its ancestors (`SetTitle`
  propagates Up un-consumed); `WindowEvent::SetEnabled` is a Windows-only
  stub that *focuses* the parent on macOS; `Window::popup` stores
  `is_modal: true` regardless of the argument; `FocusOut` never emits `Submit` (verifier correction: `TextEvent::Blur` *is* emitted by the Textbox's click-away listener — the real gap is that Tab away neither commits nor blurs); `validate` cannot refuse a keystroke; `style.dpi_factor` is one global (multi-monitor broken by
  construction); the built-in theme's `:root` matches only `Entity::root()`
  so sub-windows get no background; `Handle::class("a b")` does not split.
- **gpui**: `WindowOptions.window_bounds` is a content rect but
  `window.bounds()` is a frame rect (windows grow 32 px per launch when
  round-tripped); `WindowKind::Floating` documents parenting the macOS
  backend does not implement; `on_key_down` fires on both a cell and the
  root in bubble phase; no accessibility implementation.
- **floem**: `bounds_of_content_on_screen()` returns window-relative
  coordinates on macOS (persistence creeps by the title-bar height); Tab
  visits a row's `Dropdown` one row late; `TextInput` has no undo while the
  Lapce `text_editor` does.
- **slint**: windows first shown from a callback do not paint until a render
  is forced (blank sheet, transparent prompt); secondary windows open at
  layout-minimum size; `modifiers.control` is Command on macOS
  (undocumented); `scale_factor()` reads 1.0 until mapped (naive persistence
  reopens at 2× size); `input-type: decimal` rejects grouping separators;
  select-on-focus defeated by mouse focus.
- **iced**: `beginSheet:` on a parent that owns a child window destroys the
  child on dismissal; widget operations run against every window
  (`focus_next()` tabs out of a dialog); only `text_input`/`text_editor`
  are focusable (`button`/`checkbox`/`slider` are not, `combo_box` has no
  `.id()`), so mixed-control Tab order is impossible; `operation::focus` on
  a missing id silently no-ops; `find_focused`/`unfocus` need the `advanced`
  feature.
- **egui/eframe**: `Response::has_focus()` is false whenever the OS window
  isn't frontmost; ⌘C/⌘V arrive as `Event::Copy`/`Paste`, never key presses;
  `persist_window` needs a non-default feature and stores one window;
  `DragValue` cannot do ⇧×10 or reject a keystroke; no parent/owner in
  `ViewportBuilder`.
- **xilem/masonry**: window geometry is initial-only; `masonry_winit`'s
  driver types are not re-exported; `text_input` has no caret/selection API
  (a date mask corrupts a filled cell) and no accessible name; the
  AccessKit tree is built lazily.
- **dioxus (desktop)**: no close veto in 0.7.9; controlled inputs drop fast
  keystrokes; rejecting a keystroke needs a remount; `GlobalSignal` forks
  per window; child window position is overridden by macOS.
- **tauri**: `always_on_top` and `parent` are mutually exclusive on macOS;
  the `set_enabled(false)` sheet is parent-sized and outranks a dialog placed
  over the parent; AppKit shifts the parent so the sheet fits; the native
  date field displays in the system locale regardless of the app's toggle.
- **dioxus-native / Blitz**: see finding 8; plus `set_focus_to` never calls
  `generate_focus_events`, `tabindex="-1"` means non-focusable, and
  `prefers-color-scheme` follows the document's `color-scheme`, not the OS.
- **notification of a harness-level finding**: rfd message boxes from an
  unbundled binary are out-of-process `CFUserNotification` alerts that
  outlive the app unless `set_parent` gives them a host window (dioxus,
  xilem, iced, slint, dioxus-native); only parented rfd calls (egui, tauri)
  get an in-process sheet.

## Measured (serial pass, 2026-08-30)

`./measure.sh --round iter5`, canonical CSV
`measurements/reruns/20260830T193713Z/results-iter5.csv`. Clean build with
warm registries and an empty target dir, serial, one app at a time; every
binary survived the 8-second launch check. `deps_unique` uses the corpus'
cross-round counting (it differs from the `cargo tree` counts quoted in the
rows). `loc_rust` counts only the crate's own `src/` — dioxus-native-ledger's
**78** lines are the whole point: its UI lives in the shared
`apps/dioxus-ledger/src/app.rs`.

| app | clean s | bin MB | stripped MB | deps | LoC rust (+UI) |
|---|---:|---:|---:|---:|---:|
| iced-windows / -ledger | 23 / 23 | 10.8 / 10.8 | 9.2 / 9.1 | 148 / 141 | 1279 / 1435 |
| egui-windows / -ledger | 26 / 26 | 12.9 / 12.9 | 11.4 / 11.3 | 165 / 165 | 897 / 1359 |
| gpui-windows / -ledger | 54 / 55 | 5.7 / 5.5 | 4.8 / 4.7 | 397 / 397 | 1422 / 1521 |
| tauri-windows / -ledger | 35 / 38 | 9.3 / 9.3 | 7.4 / 7.5 | 208 / 224 | 807+300 / 542+519 |
| xilem-windows / -ledger | 26 / 29 | 11.5 / 11.6 | 9.8 / 9.8 | 151 / 156 | 1518 / 1821 |
| slint-windows / -ledger | 49 / 53 | 16.6 / 18.0 | 14.7 / 15.7 | 307 / 307 | 1055+404 / 1085+363 |
| dioxus-windows / -ledger | 31 / 31 | 6.5 / 6.4 | 5.5 / 5.5 | 284 / 286 | 1065 / 1000 |
| **dioxus-native-windows / -ledger** | 69 / 60 | 24.5 / 27.9 | 21.0 / 23.9 | 367 / 370 | 458 / **78** |
| freya-windows / -ledger | 31 / 32 | 21.8 / 23.3 | 19.6 / 20.9 | 198 / 193 | 1251 / 1490 |
| vizia-windows / -ledger | 19 / 20 | 23.2 / 24.7 | 20.8 / 22.0 | 129 / 130 | 1021 / 1280 |
| floem-windows / -ledger | 45 / 46 | 18.4 / 18.3 | 15.5 / 15.4 | 226 / 227 | 1162 / 1108 |

The spread is familiar from iteration 1, with the new entry at both extremes:
Dioxus Native is the largest binary and slowest build of the cohort (Stylo +
Vello + Taffy statically linked) while sharing the smallest app source; vizia
is the fastest clean build and smallest dependency count with one of the
largest binaries (static Skia); gpui stays the smallest binary with the
biggest dependency graph.

## Caveats

- One display: every "multi-monitor scale change" row is *not-verified*.
- No CJK input source on the reference machine: IME rows are *not-verified*
  (Dioxus Native's is *not-achievable* — `Ime` events are dropped upstream).
- The apps were verified while up to eleven sibling apps drove synthetic
  input on one desktop. Stray clicks and ⌘W reached several processes;
  every agent documented its gating (window-ownership probes, per-window
  `screencapture -l`, AX-driven input) and the verifiers re-ran the key
  behaviours serially. A handful of agent screenshots are contaminated by a
  sibling's window and are flagged as such in the verifier reports.
- LoC per app is 1.0–1.8k, above the ~700 guide, in every framework: roughly
  a quarter of each app is the self-test harness and the shared-desktop
  positioning hooks.
- Dioxus Native is a beta engine measured at a git revision; its numbers
  describe that revision only.
