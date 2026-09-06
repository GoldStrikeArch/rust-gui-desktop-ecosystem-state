# Iteration 5 — final normalized capability matrices (all 11 entries)

Merged from the three verifier reports: wave 1 (`report/data/verification/iter5-wave1-verifier.md`,
columns dioxus/xilem/gpui/iced/egui/tauri, footnotes [a]–[r]), wave 2a
(`iter5-wave2a-verifier.md`, slint/dioxus-native, [s]–[ap]) and wave 2b
(`scratchpad/verify-iter5/REPORT-wave2b.md`, freya/vizia/floem, [aq]–[az]). Wave-1/2a columns
are copied verbatim from their reports; every rating was re-verified against the pinned
frameworks and the apps' behaviour on macOS 26.5.2 (M4 Pro).

Rubric: **built-in** = framework API does it; **assembled** = composed from framework pieces
plus ≤ ~30 lines of glue; **hand-rolled** = the app implements the mechanism itself (incl.
objc2/OS calls, or owning the event loop); **not-achievable** = no path in the pinned framework
(approximation shipped). Footnote letters mark cells where a verifier changed or qualified an
agent's rating.

## SPEC-9 "Windows"

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

## SPEC-10 "Ledger"

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

## Consolidated footnotes

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
