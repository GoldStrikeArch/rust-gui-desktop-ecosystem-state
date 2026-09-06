# Iteration 5 verification — wave 1 (dioxus, xilem, gpui, iced, egui, tauri × windows/ledger)

Verifier run 2026-08-30 18:35–19:35 on the shared M4 Pro desktop while five other build agents
(slint/freya/vizia/floem/dioxus-native) drove synthetic input. All my artefacts are under
`scratchpad/verify-iter5/` (`build-logs/`, `selftest/`, one directory per app with `run.out`,
screenshots taken with `screencapture -x -o -l <CGWindowID>`, and `axdump.txt`). Helper tools
written for this pass: `wins` (layer-agnostic CGWindowList lister with owner), `topat x y`
(frontmost window at a point, ignoring the layer-20 Dock/menubar windows), `axdump` (compiled
copy of `apps/egui-ledger/evidence/axdump.swift`), `lib.sh` (activate-then-act loops, AX path
finder, delta-based dismissal of out-of-process alerts).

Method notes that matter for reading the results:

- Every interactive check was re-run until the click/keystroke was *provably* delivered to the
  app under test (`topat` = my pid before every CGEvent click; AX read-back, stdout trace lines or
  key logs after every keystroke burst). Runs contaminated by sibling input were discarded and
  repeated; the contaminated attempts are kept in the per-app `run.out` files.
- The first CGEvent click on a freshly activated winit/gpui/egui window is consumed by window
  activation (egui's agent documented this; it holds for xilem, gpui, iced and the webviews too).
  All positive controls therefore use a double click.
- Two of my early "modal blocked" attempts were invalid for test-design reasons, not framework
  reasons: (a) xilem's always-on-top edit dialog opens centred over the parent and covered the
  parent's Delete button — I moved the dialog aside via AX before clicking; (b) iced's and tauri's
  sheets make AppKit shift the parent window up (iced 630→550, tauri 600→539), so button
  coordinates cached before the sheet are stale — re-read the frame after the sheet attaches.
- Ghost alerts: dioxus-windows, xilem-windows and iced-windows present their rfd alerts through
  macOS's out-of-process `CFUserNotification` fallback (unbundled binary + app-modal
  `NSAlert runModal`). These windows are owned by `UserNotificationCenter`, are invisible to
  per-process AX scripting, and *outlive the app*. I dismissed only alerts that appeared as a
  delta during my own step. Note for the next verifier: two ghost alerts from other agents' apps
  were on screen when I started, and my first dismissal sweep clicked "No"/"Yes" on them.

## Per-app results

Legend: **build** = `cargo build --release --locked` in the app dir (incremental, cached rustc
diagnostics replayed); **self-test** = my run of the hook with a 90 s alarm; **evidence** = log +
3–4 screenshots read; **claims** = FRICTION statements checked against `src/` and the vendored
crates in `~/.cargo/registry/src/index.crates.io-*`; **re-check** = my own synthetic-input run.

### dioxus-windows — build PASS · self-test PASS 13/13 · evidence PASS · claims PASS · re-check PASS

- Rules: only `apps/dioxus-windows/` touched; pin `dioxus = "=0.7.9"` + `desktop` identical to
  `dioxus-app`; helpers `rfd =0.17.2` (`xdg-portal`, no default features) and `tokio =1.52.3`
  pinned and listed; `Cargo.lock` present; zero JS (checked). Edition 2024 vs the baseline's 2021
  — the BRIEF mandated 2024, so consistent with the brief, inconsistent with the other dioxus apps
  (LOW, corpus-wide, see cross-cutting).
- Build: no-op; only the transitive `block v0.1.6` future-incompat note.
- Self-test: `SELFTEST DONE pass=13 fail=0`, exit 0 (matches report).
- Evidence: `08-close-prompt.png` (in-webview Save/Discard/Cancel card, honest), `10-native-confirm`
  (rfd alert), `03-shared-state` (Inspector edit mirrored in the list). `06b` and `03` are region
  captures that include a sibling xilem-windows NSAlert in the middle of the frame — interpretable
  but contaminated (LOW).
- Claims vs code: `VirtualDom::with_root_context` provides on the base scope (dioxus-core
  `virtual_dom.rs:362`); `ReactiveContext::new_for_scope` captures *that runtime's* sender
  (`reactive_context.rs:103-108`) — so cross-VirtualDom signal wake-up is real; `GlobalSignal`
  resolves through `get_global_context()` → `Runtime::current()` (`global/mod.rs:286-291`), i.e.
  per-runtime, as claimed; `App::handle_close_requested` at `dioxus-desktop/src/app.rs:195` is
  hide-or-close only, and `launch.rs:20-33` runs `app.tick` (user handlers) before the
  `CloseRequested` match — the hide-and-restore trick is sound; tao 0.34.8
  `platform_impl/macos/window.rs:315-317` maps `Parent::ChildOf` to `addChildWindow:ordered:`.
- Re-check (mine): modal open → two `topat`-confirmed CGEvent clicks on Delete → no stdout line,
  AX static-text count 32 → 32 (`v3-*`); positive control: double click on Delete with no modal →
  a new `UserNotificationCenter` alert window appeared (`v4-control-alert.png`). Close veto:
  dirty via the dialog → ⌘W → `close requested while dirty -> prompt` → Cancel →
  `close: Cancel -> vetoed`, process alive → ⌘W → Discard → `close: Discard -> exit`, exit 0.
- Severity: none. LOW: contaminated region shots; the rfd alert is out-of-process (note added).

### dioxus-ledger — build PASS · self-test PASS 26/26 · evidence PASS · claims PASS · re-check PASS

- Rules: pin identical; `rust_decimal =1.39.0`, `arboard =3.6.1`, `tokio =1.52.3` listed; lock
  present; zero JS.
- Evidence: `02`/`03` show `1,234.50` → `1 234,50`; `04` shows red border + `Save (1 errors)`
  disabled + summary; tabular digits visibly aligned.
- Claims: `font-variant-numeric: tabular-nums lining-nums` + `font-feature-settings` in the CSS
  (`app.rs:922-923`); `key: "{i}-{r.bump}"` remount trick present (`app.rs:497`); `aria-label`s
  present on every cell.
- Re-check: AX-focus `Unit price row 1`, keystrokes `1234.5` (read back exactly), Tab →
  `1,234.50`, click `Locale: en-US` → `1 234,50` (`02-after-tab.png`, `03-locale-toggled.png`).
  `axdump`: 323 nodes, 37 `AXTextField`, all named — a11y **built-in** confirmed.
- Severity: none.

### xilem-windows — build PASS · self-test PASS 23/23 · evidence PASS · claims PASS · re-check PASS

- Rules: pin `xilem = "=0.4.0"`; helpers `masonry_winit =0.4.0`, `rfd =0.15.4`, `serde =1.0.228`,
  `serde_json =1.0.150` pinned/listed; lock present. `verify/` scaffolding lives inside the app
  dir (allowed). Edition 2024 vs other xilem apps 2021 (same corpus note).
- Evidence: `07` (separate "Edit project" window), `09` (6 rows, status "modal open (main window
  input blocked)"), `15` (out-of-process alert), `16` (in-process parented alert with dimmed parent).
- Claims: `WindowOptions::with_owner_window` only under `#[cfg(windows)]` (`window_options.rs:269,
  308`), `build_initial_attrs` is `pub(crate)` (172), "attempted to change position attribute…"
  warning (250); `MasonryDriver::on_close_requested` = `on_close` + `run_logic` + exit-if-
  `!keep_running` (`xilem/src/driver.rs:329-338`) and `masonry_winit`'s `CloseRequested` arm only
  calls the driver (`event_loop_runner.rs:702-704`) — so "not forwarding is the veto" is exactly
  right; `WindowLevel::AlwaysOnTop` → CGWindowLayer 3 observed in my `wins` output.
- Re-check: run with `WINDOWS_LOG=1`: Edit… → `STATE … modal=true`; dialog moved aside;
  three `topat`-confirmed clicks on the parent's Delete → `BLOCKED` 1 → 17, STATE unchanged
  (rows=6, modal=true); Esc → modal closed; control: double click Delete → new
  `UserNotificationCenter` alert → No → `status="delete cancelled"`. Veto (`WINDOWS_SHEET=1`):
  budget edited + Return → `dirty=true`; ⌘W → sheet (AX: Save/Discard/Cancel) → Cancel →
  `VETOED #1 CloseRequested on main window (Cancel)`, alive → ⌘W → Discard → exit 0.
- Severity: none. LOW: `WINDOWS_SHEET=1` parents only the close prompt; the Delete confirm is
  still out-of-process (the FRICTION only says "pass the parent to rfd").

### xilem-ledger — build PASS · self-test PASS 13/13 · evidence PASS · claims PASS · re-check PASS

- Rules: `masonry_winit =0.4.0`, `rust_decimal =1.39.0`, `arboard =3.6.1` listed; lock present.
- Evidence: `03` `1,234.50`, `04b` `1 234,50`, `01` two-box decimal alignment + red `(34.75)`.
- Claims: `StyleProperty::FontFeatures` set in the re-implemented label view (`views.rs:95`) and
  `Padding::all(0.0)` inserted (119); `RenderRoot::focus_on`/`focused_widget` used in `shell.rs`;
  masonry `TextArea` handles ArrowUp/ArrowDown itself (`text_area.rs:633,641`), has no undo
  (0 matches), and `masonry_winit` turns ⌘V into `TextEvent::ClipboardPaste`
  (`event_loop_runner.rs:680`); the AccessKit tree is built on `InitialTreeRequested` (766).
- Re-check: `LEDGER_DEMO=0` focus → keystrokes read back via AX as `1234.5` → Tab → `1,234.50` →
  Locale click → `1 234,50`, button `Locale: fr-FR`. `axdump` (second pass): 237 nodes, 61
  `AXTextField`, **0 with a Title** — the agent's "partial (values yes, names no)" is exact.
- Severity: none. Rating normalized (see matrix footnote k).

### gpui-windows — build PASS · self-test PASS 16/16 · evidence PASS · claims PASS · re-check PASS

- Rules: `gpui = "=0.2.2"` + `runtime_shaders` identical; no helpers; lock present; edition 2024
  like `gpui-tray`/`gpui-babel`.
- Evidence: `07` (6 rows under the app-level gate), `08` (dialog is a normal window), `10` and `12`
  are genuine sheets (AX exposes `sheet 1 of window 1` buttons).
- Claims: `prompt` → `beginSheetModalForWindow:` at `platform/mac/window.rs:1202` (FRICTION says
  1198 — 4 lines off, LOW); `WindowKind::Normal | Floating` share one path and
  `NSNormalWindowLevel` (621, 779-780) while the doc comment promises "on top of its parent"
  (`platform.rs:1270`); `WindowOptions` has no parent/owner field (`platform.rs:1093-1118`);
  `bounds()` returns the NSWindow *frame* (515-527) and the self-test prints `FRAME_DELTA 32`;
  `grep -ril accesskit|NSAccessibility gpui-0.2.2` → nothing.
- Re-check: control: double click Delete → `No, Yes` sheet, dismissed; Edit… →
  `modal opened (app-level input gate ON)`; two `topat`-confirmed clicks on Delete → no sheet,
  window count unchanged, byte-identical before/after captures; Esc → `modal closed, focus
  returned to main`. Veto (`WINDOWS_START_DIRTY=1`): ⌘W → sheet → Cancel →
  `close vetoed (Cancel)`, alive → ⌘W → Discard → exit 0.
- Severity: none.

### gpui-ledger — build PASS (1 warning, undeclared) · self-test PASS 22/22 · evidence PASS · claims PASS · re-check PASS

- Rules: identical pin/feature; no helpers; lock present.
- Build: rustc replays `warning: unused variable: cx` at `src/main.rs:892`; the FRICTION says the
  build is clean. **MEDIUM-LOW** — factual error, fixed with a verifier note.
- Evidence: `03` `1,234.50`, `04` `1 234,50`, `07` red date cell + `1 error(s)` + greyed Save +
  summary.
- Claims: `FontFeatures(vec![("tnum",1),("lnum",1)])` (`main.rs:324`), `deferred(anchored())`
  popup (968-969), `tab_index/tab_stop/focus_next` (335-343, 748), `write_to_clipboard/
  read_from_clipboard` (625, 634); the "`src/tab_stop.rs`" it cites is gpui's own file
  (`gpui-0.2.2/src/tab_stop.rs` exists) — ambiguous wording, LOW.
- Re-check (`LEDGER_KEYLOG=1`): Tab walk to `FOCUS row0 Price`, key log shows exactly
  `1 2 3 4 . 5`, Tab → `FOCUS row0 Reimb` and the cell reads `1,234.50` (`v2-02-after-tab.png`);
  single click on the Locale button → `LOCALE fr-FR`. `axdump`: **7 nodes** (traffic lights +
  title) — a11y not-achievable confirmed.
- Severity: MEDIUM-LOW (warning claim), otherwise none.

### iced-windows — build PASS · self-test PASS 18/18 · evidence PASS · claims PASS (one overstatement) · re-check PASS

- Rules: `iced = "=0.14.0"` default features identical; helpers `rfd =0.17.2`, `serde =1.0.229`,
  `serde_json =1.0.151`, macOS-only `objc2 =0.6.4` pinned/listed; lock present.
- Evidence: `06`/`07` identical (sheet up, 6 rows, status `modality: OS window-modal (NSWindow
  beginSheet:)`), `10` three-button alert, `03` main + inspector.
- Claims: `beginSheet:`/`endSheet:`/`addChildWindow:`/`removeChildWindow:` via `objc2::msg_send!`
  in `src/main.rs:103-136`; `iced::window::run` hands `&dyn Window` inside a `Send + 'static`
  closure (`iced_runtime/src/window.rs:463-466`); widget operations run against every window
  (`iced_winit/src/lib.rs:1744-1748`); `window::Settings` has no parent/owner; `iced_core::Font`
  has no feature list. **Overstated**: the rfd dialogs are called "a genuine three-button
  NSAlert" — for this unbundled binary they are presented out-of-process by
  `UserNotificationCenter` (a new window owned by that process appeared on every Delete click and
  every dirty ⌘W in my runs); note added.
- Re-check (`WINDOWS_DIRTY=1`): Edit… → `modality: OS window-modal (NSWindow beginSheet:)`,
  parent shifted 630→550 (Delete re-located), three `topat`-confirmed clicks on Delete →
  `delete-requested` 0 → 0; Esc; control double click → `delete-requested` ×2 and two
  out-of-process alerts. Veto: ⌘W → `close-veto: asking (dirty)` → Cancel →
  `close-veto: cancel -> window survives`, alive → ⌘W → Discard → `discard -> exit`, exit 0.
- Severity: LOW (alert hosting overstated; FRICTION does not mention the AppKit parent shift that
  tauri's FRICTION records as a trap).

### iced-ledger — build PASS · self-test PASS 29/29 · evidence PASS · claims PASS (one wrong widget) · re-check PASS

- Rules: pin identical but **`features = ["advanced"]` added** to the framework crate (declared and
  justified: `find_focused`/`unfocus`). The BRIEF asks for the baseline's feature style; this is a
  declared deviation (LOW-MEDIUM, flag for the measure pass — it changes the compiled feature set
  vs `iced-app`). `rust_decimal =1.39.0` listed; lock present.
- Evidence: `02` `1,234.50` with the app-drawn focus ring on the checkbox, `03` `1 234,50`, `01`
  monospace alignment + red `(34.75)`.
- Claims: only `text_input.rs` and `text_editor.rs` call `operation.focusable(..)`; the FRICTION
  also listed `scrollable` — `scrollable.rs` only implements `operation.scrollable(..)`. Fixed with
  a note. `combo_box` has no `.id()` (only `text_input` does among the five widgets); no
  `accesskit` in any iced 0.14 manifest.
- Re-check: click row-1 Unit price (`focus: Cell(0, Price)`), ⌘A, key codes for `1234.5`, Tab →
  `focus: Cell(0, Reimb)` and `1,234.50` (`02-after-tab.png`); Locale click → `1 234,50`
  (`03-locale-toggled.png`). `axdump`: 97 nodes, all `AXMenuItem`/menubar + window chrome, zero
  content — not-achievable confirmed.
- Severity: LOW (scrollable claim), LOW-MEDIUM (feature deviation, declared).

### egui-windows — build PASS (0 warnings) · self-test PASS 13/13 · evidence PASS · claims PASS · re-check PASS

- Rules: `eframe = "=0.35.0"` default features identical; `rfd =0.17.2`, `serde =1.0.228`,
  `serde_json =1.0.150` listed; lock present.
- Evidence: `07b` overlay modal with 6 rows, `09`/`10` real NSAlert sheets, `14b` greyed
  Preferences (hand-rolled cross-window disable).
- Claims: `Memory.focus: ViewportIdMap<Focus>` (`memory/mod.rs:109`), `top_modal_layer` committed
  in `end_pass` (624), `ViewportBuilder` has no parent/owner (only `ViewportIdPair.parent`),
  rfd `set_parent` → `beginSheetModalForWindow_completionHandler` (`message_dialog.rs:97`),
  eframe `persistence` not in `default` (Cargo.toml:60-68).
- Re-check: control double click Delete → `TRACE toolbar Delete` + `No, Yes` sheet → No; Edit… →
  overlay; two `topat`-confirmed clicks on Delete → `toolbar Delete` count unchanged, identical
  captures. Veto: `TRACE modal OK` (dirty) → red close button → `TRACE root close_requested` +
  sheet `Save, Discard, Cancel` → Cancel → `TRACE close vetoed (Cancel)`, alive → close → Discard →
  `close allowed (Save/Discard)`, exit 0. Caveat: System Events ⌘W keystrokes never reached the
  root viewport in my runs while the persisted Inspector viewport was also open (the agent's log
  and self-test cover ⌘W); I verified `CloseRequested` through the window's close button instead.
- Severity: none (LOW: my ⌘W injection not reproduced — probably a key-window question with two
  viewports, not a contradiction).

### egui-ledger — build PASS (0 warnings) · self-test PASS 26/26 · evidence PASS · claims PASS · re-check PASS

- Rules: `egui_extras =0.35.0`, `rust_decimal =1.39.0` listed; lock present.
- Evidence: `02b` `1,234.50`, `03` `1 234,50`, `06` monospace alignment with `(129.99)`.
- Claims: `shaper.shape(buffer, &[])` (`text_layout.rs:1434`), U+2009/U+202F half-space override
  (272-279), `DragValue` `count_and_consume_key(Modifiers::NONE, …)` (496), `raw_input_hook`
  (`main.rs:742`), `labelled_by` (320, 395, 540, 596), `Event::Copy/Paste` matching (472-480).
- Re-check (`LEDGER_TRACE=1`): `TRACE commit id_D0EA = 1,234.50`, ⌘L → `TRACE locale -> fr-FR`,
  screenshot `1 234,50`. `axdump` pass 1: 98 nodes, zero `AXTextField`; passes 2–3: 224 nodes,
  48 titled `AXTextField` — the AccessKit tree is lazily built, so the *first* AXUIElement walk is
  a false negative too (note added to the FRICTION; same trap masonry's agent recorded).
- Severity: none.

### tauri-windows — build PASS (0 warnings) · self-test PASS 15/15 (after one contention flake) · evidence PASS · claims PASS · re-check PASS

- Rules: `tauri =2.11.5`/`tauri-build =2.6.3` identical to `tauri-app`; `tauri-plugin-dialog
  =2.7.1`, `tauri-plugin-window-state =2.4.1` listed; `serde = "1"` unpinned but identical to the
  baseline; edition 2021 like every tauri app (conflicts with the BRIEF's 2024, consistent with the
  framework's apps — the FRICTION says so explicitly); no npm/`package.json`, only local `ui/*.js`;
  lock present. Missing from `report/data/iter5-rows.md`: **there is no tauri section** (MEDIUM
  for the rows file, not the app).
- Self-test: my first run `pass=14 fail=1` — `FOCUS ["inspector=false","prefs=false","main=false"]`,
  i.e. no tauri window was key because a sibling held frontmost; re-run under an activation loop:
  `pass=15 fail=0`, exit 0. Contention flake, not a bug.
- Evidence: `10`/`11` region captures include sibling windows (xilem-windows, xilem-ledger) but
  show the dimmed parent + dialog; `12` sheet; `09` confirm.
- Claims: `tauri-runtime-wry 2.11.4 src/window/macos.rs:12-32` — `set_enabled(false)` allocates a
  titled NSWindow the size of the parent frame, `setAlphaValue(0.5)`, `beginSheet_completionHandler`;
  `set_enabled(true)` ends the attached sheet. `.parent(&main)` (`main.rs:162`), modal built with
  `always_on_top(true)` and no parent (218-222), `api.prevent_close()` (734),
  `PredefinedMenuItem::close_window` (482); tao 0.35.3 `addChildWindow:` (315-317).
- Re-check: control double click Delete → `[main] Delete pressed` + confirm sheet (No); Edit… →
  windows: dialog (layer 5) + untitled disabling sheet + main shifted 600→539; three
  `topat`-confirmed clicks on the Delete centre all hit the tauri-owned disabling-sheet window, no
  handler line, AX static-text count 28 → 28. Veto: `Aurora (dirty)` via the dialog → ⌘W → sheet
  `Save, Discard, Cancel` → Cancel → `close CANCELLED — window survives`, alive → ⌘W → Discard →
  exit 0 (wrapper-captured).
- Severity: LOW (contaminated region shots; self-test focus assertion is contention-sensitive).

### tauri-ledger — build PASS (0 warnings) · self-test PASS 29/29 · evidence PASS · claims PASS · re-check PASS

- Rules: pins identical; `tauri-plugin-clipboard-manager =2.3.2` listed; **`rust_decimal =1.42.1`**
  while the other five ledgers pin `=1.39.0` (allowed, inconsistent; LOW); no npm; lock present.
- Evidence: `02` `1,234.50`, `03` `1 234,50` (U+202F grouping), `04` alignment, `05` red Qty +
  `Save (1 error)` + summary.
- Claims: `beforeinput` filter (`ui/main.js:133`), `Intl.NumberFormat(loc).formatToParts`
  (14, 19), `font-variant-numeric: tabular-nums lining-nums` + `font-feature-settings`
  (`styles.css:49-50`) read back by the self-test.
- Re-check: AX-focus `Row 1 unit price`; the first typing attempt was hit by a sibling's Esc
  (`ESC reverted row=0 field=unit` in stdout), second attempt read back `1234.5` → Tab →
  `1,234.50` → `Toggle locale` → `1 234,50`. `axdump`: 741 nodes, 73 `AXTextField` — built-in.
- Severity: none.

## Cross-cutting findings

1. **Shared-desktop contamination is the dominant error source, in both directions.** Sibling
   ⌘W killed three of my runs (dioxus-windows ×2, tauri-windows ×1), stray Esc/letters landed in
   ledger cells (tauri, gpui), and always-on-top sibling windows (dioxus-native, freya, slint)
   covered click targets. Every agent's FRICTION says the same about *their* run. Nothing in the
   12 apps failed once delivery was proven.
2. **Out-of-process rfd alerts.** dioxus-windows, xilem-windows and iced-windows all show their
   rfd message boxes through macOS's `CFUserNotification` fallback (unbundled binary, app-modal
   `runModal`); egui (rfd `set_parent`), tauri (dialog plugin `.parent()`) and gpui
   (`window.prompt`) get real in-process sheets. The ghost alerts survive the app. Only xilem's
   FRICTION had recorded this; notes added to iced-windows and dioxus-windows.
3. **Accessibility probes lie twice.** `osascript … entire contents` is a false negative for
   AccessKit apps (egui agent's finding, confirmed) *and* the first AXUIElement walk is a false
   negative for both AccessKit-on-winit stacks (egui 98→224 nodes, masonry: agent's note). The
   webviews and gpui/iced give stable answers on the first pass (dioxus 323, tauri 741, gpui 7,
   iced 97-all-menubar).
4. **Sheets move the parent.** iced's `beginSheet:` and tauri's `set_enabled(false)` both make
   AppKit shift the parent window up to fit the sheet; only tauri's FRICTION records it.
5. **Edition split.** The BRIEF mandated `edition = "2024"`; dioxus/xilem/gpui followed it (their
   baselines are 2021), tauri kept 2021 to match its siblings. Not a correctness issue; note it
   before comparing LoC/build numbers across iterations.
6. **Feature/pin deviations to carry into the measure pass:** `iced-ledger` adds the `advanced`
   feature; `tauri-ledger` pins `rust_decimal =1.42.1` (others `=1.39.0`).
7. **`report/data/iter5-rows.md` has no tauri section**; the tauri agent's numbers exist only in
   the two FRICTION files (windows: 37.5 s clean build, 8.85 MiB, 446 packages, selftest 15/15;
   ledger: 57.0 s, 8.87 MiB, 505 packages, selftest 29/29).

## FRICTION.md edits made (each marked "verifier note (2026-08-30)")

1. `apps/gpui-ledger/FRICTION.md` — build is not warning-free (`unused variable: cx`,
   `src/main.rs:892`).
2. `apps/iced-ledger/FRICTION.md` — `scrollable` is not focusable (`scrollable.rs` implements
   `operation.scrollable`, never `focusable`); only `text_input`/`text_editor` are.
3. `apps/iced-windows/FRICTION.md` — the rfd alerts are hosted out-of-process by
   `UserNotificationCenter` for this unbundled binary (not an in-process NSAlert); they are
   invisible to per-process AX and survive the app.
4. `apps/dioxus-windows/FRICTION.md` — same nuance for its Delete confirm.
5. `apps/egui-ledger/FRICTION.md` — the direct AXUIElement walk also needs a second pass (lazy
   AccessKit tree: 98 nodes first, 224 afterwards).

No `src/`, Cargo, evidence or out-of-scope file was modified; nothing committed.

## Normalized capability matrix

Rubric: **built-in** = framework API does it; **assembled** = composed from framework pieces plus
≤ ~30 lines of glue; **hand-rolled** = the app implements the mechanism itself (incl. objc2/OS
calls, or owning the event loop); **not-achievable** = no path in the pinned framework
(approximation shipped). Footnote letters mark cells where I changed an agent's rating.

### SPEC-9 "Windows"

| Capability | dioxus 0.7.9 | xilem 0.4.0 | gpui 0.2.2 | iced 0.14.0 | egui 0.35.0 | tauri 2.11.5 |
|---|---|---|---|---|---|---|
| Second top-level window (open/close/singleton focus) | built-in | built-in [c] | built-in | built-in | built-in | built-in |
| Modal dialog — kind achieved | hand-rolled (child window + scrim) | hand-rolled (window + event filter) | hand-rolled (window + occlude scrim) [g] | hand-rolled (objc2 `beginSheet:` = real OS sheet) | built-in (`egui::Modal` overlay) [h] | assembled (`set_enabled(false)` sheet + own window) |
| Parent blocked while modal | hand-rolled | hand-rolled | hand-rolled | hand-rolled [a] | built-in (per viewport) [h] | built-in |
| Focus returns to parent after modal | assembled | assembled | assembled [b] | assembled [b] | built-in (never leaves the window) | assembled [b] |
| Shared state across windows | built-in | built-in | built-in | built-in | hand-rolled (`Arc<Mutex>`) [i] | assembled (emit/listen bus) [i] |
| Cross-window message (Ping/Pong) + wake | built-in | built-in [j] | built-in | built-in [j] | assembled | built-in |
| Theme/layout change to all windows | built-in (paint lags on unfocused windows) | built-in (light palette hand-applied per widget) | built-in | built-in | built-in | built-in |
| Window parenting (child/owner/transient) | built-in (tao `with_parent_window`, macOS ext) | not-achievable | not-achievable | hand-rolled (objc2 `addChildWindow:`) | not-achievable | built-in |
| Close veto (Save/Discard/Cancel) | hand-rolled (hide + restore) | hand-rolled [c] | built-in | built-in | built-in | built-in |
| Quit semantics | assembled | built-in | assembled | built-in | built-in | built-in |
| Native confirm dialog | assembled (rfd, out-of-process) [n] | assembled (rfd, out-of-process unless `set_parent`) [n] | built-in (sheet) | assembled (rfd, out-of-process) [n] | assembled (rfd sheet) | assembled (dialog plugin, sheet) |
| Position/size persistence | assembled | hand-rolled [c] | assembled | assembled | assembled [d] | assembled (window-state plugin) |
| Multi-monitor scale change | not-verified | not-verified | not-verified | not-verified | not-verified | not-verified |
| Per-window shortcuts (⌘W focused only, ⌘, , ⌘⇧I) | assembled [f] | hand-rolled | assembled | assembled [e] | built-in | built-in |

### SPEC-10 "Ledger"

| Capability | dioxus | xilem | gpui | iced | egui | tauri |
|---|---|---|---|---|---|---|
| Numeric field (typed, filtered, step keys) | hand-rolled | hand-rolled | hand-rolled | assembled (controlled `text_input`; stepping hand-rolled) | hand-rolled | hand-rolled |
| Locale-aware parse + format (live toggle) | hand-rolled | hand-rolled | hand-rolled | hand-rolled | hand-rolled | assembled (`Intl` format; parse hand-rolled) [m] |
| Decimal alignment / tabular figures (font-feature request) | built-in | hand-rolled | built-in | not-achievable (alignment via monospace) [l] | not-achievable (alignment via monospace) [l] | built-in |
| Inline validation + disabled Save + summary | assembled | assembled | hand-rolled | assembled | assembled | assembled [o] |
| Dropdown with type-ahead | built-in | hand-rolled (inline list approximation) | hand-rolled | built-in | assembled | built-in |
| Date input (masked or picker) | built-in (picker) | hand-rolled (mask; breaks on filled cell) | hand-rolled (mask) | hand-rolled (mask) | hand-rolled (mask) | built-in (segmented native) |
| Slider ↔ numeric field | assembled | assembled | assembled | assembled [r] | built-in | assembled |
| Tab order across mixed controls | built-in | built-in | built-in | hand-rolled | built-in | built-in |
| Enter-moves-down | assembled | hand-rolled | assembled | assembled [p] | assembled [p] | assembled [p] |
| Undo/redo (field / form) | built-in / hand-rolled | not-achievable / hand-rolled | not-achievable / hand-rolled | not-achievable / hand-rolled | built-in / hand-rolled | built-in / hand-rolled |
| Live computed columns and totals | built-in | built-in | built-in | built-in | built-in | built-in |
| Row copy/paste as TSV | assembled (⌘⇧C/V) | assembled (⌘⇧C/V) | assembled [q] | assembled (⌘⇧C/V) | assembled | assembled (clipboard plugin) |
| Accessibility labels (verified dump) | built-in (323 nodes, named) | not-achievable for names (61 values, 0 names) [k] | not-achievable (7 nodes) | not-achievable (97 nodes, no content) | built-in (224 nodes, 48 named; needs 2nd AX pass) | built-in (741 nodes, named) |
| IME composition (optional) | not-verified | not-verified | not-verified (not-achievable as built) | not-verified | not-verified | not-verified |

Footnotes (cells where the agent's word was changed):

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
- [n] rfd confirm dialogs are assembled in all five non-gpui apps, but only egui/tauri (parented)
  get an in-process sheet; dioxus/xilem/iced get the out-of-process `CFUserNotification` alert.
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

## Verified numbers (my runs)

| app | build | self-test (mine) | agent's N | interactive re-check |
|---|---|---|---|---|
| dioxus-windows | ok | 13/13 | 13 | modal-block ✓ (control ✓), veto Cancel/Discard ✓ |
| dioxus-ledger | ok | 26/26 | 26 | `1,234.50` → `1 234,50` ✓, AX 323 nodes |
| xilem-windows | ok | 23/23 | 23 | modal-block ✓ (BLOCKED 1→17, control ✓), veto ✓ |
| xilem-ledger | ok | 13/13 | 13 | ✓, AX 237 nodes / 61 unnamed fields |
| gpui-windows | ok | 16/16 | 16 | modal-block ✓ (control ✓), veto ✓ |
| gpui-ledger | ok (1 warning) | 22/22 | 22 | ✓ (keylog exact), AX 7 nodes |
| iced-windows | ok | 18/18 | 18 | modal-block ✓ (control ✓), veto ✓ |
| iced-ledger | ok | 29/29 | 29 | ✓, AX 97 nodes (menubar/chrome only) |
| egui-windows | ok | 13/13 | 13 | modal-block ✓ (control ✓), veto via close button ✓ |
| egui-ledger | ok | 26/26 | 26 | ✓ (trace), AX 98 → 224 nodes (lazy) |
| tauri-windows | ok | 14/15 then 15/15 | 15 | modal-block ✓ (control ✓), veto ✓ |
| tauri-ledger | ok | 29/29 | 29 | ✓ (2nd attempt; stray Esc on 1st), AX 741 nodes |
