# Iteration 5 verification — wave 2b (freya, vizia, floem × windows/ledger) — final pass

Verifier run 2026-08-30 20:25–21:30 on the M4 Pro desktop with **no** sibling build agents
running (every `target/release/*-(windows|ledger)` process was killed first). A first attempt
of this pass was interrupted at ~20:43; its artefacts (`build-logs-2b/`, `selftest-2b/`,
`{freya,vizia,floem}-{windows,ledger}/recheck*.log`, `v-*.png`, `axdump-pass*.txt`) were
re-read and are cited where they are conclusive; everything that could not be confirmed from
them was re-run (the `recheck*.log` files written by this pass are named per section). Method,
rubric and footnote style are those of `report/data/verification/iter5-wave1-verifier.md` and
`iter5-wave2a-verifier.md`; footnotes continue after [ap] → [aq]….

Harness (all under `scratchpad/verify-iter5/`): `lib3.sh` (= `lib2.sh` + a scratch launcher
that strips every persisted state file, `gated_click` = click only when `topat` says the point
belongs to my pid *and* the window title matches, `unc_*` helpers for the out-of-process
`UserNotificationCenter` alerts), `modkey` (CGEvent `flagsChanged` + key for ⌘/⇧ chords),
`axdump` (direct AXUIElement walker, always run twice for lazily activated AccessKit trees),
`wins`/`topat`, per-window `screencapture -x -o -l <CGWindowID>`.

Method notes that matter for reading the results:

- Every re-check used a scratch copy of the binary under `scratchpad/verify-iter5/<app>/target/
  release/` with the agents' persisted window state removed, so a "fresh launch" is fresh.
- Delivery was proven before every click (`topat` = my pid + expected title) and after every
  keystroke burst (the apps' own `FOCUS`/`TRACE`/`LOCALE` lines, AX value read-back where the
  framework exposes values, per-window captures otherwise).
- All three frameworks present their rfd alerts through the out-of-process `CFUserNotification`
  fallback (unbundled binary + blocking `MessageDialog::show`). Every alert I raised was
  dismissed through `UserNotificationCenter`'s AX tree and `unc_count` was 0 at the end of
  every script. The apps' `run.out` files carry macOS's `CFUserNotificationDisplayAlert: called
  from main application thread, will block` line, which doubles as proof that the prompt fired.
- Before/after PNG pairs for "modal blocks Delete" differ only when a gated click re-activated
  the parent (traffic-light colours flip between key/non-key); the row count, status bar and
  the absence of a confirm alert are the criteria and were read from the images, not from the
  hash. floem's pair is byte-identical because the sheet keeps the parent inactive.

## Per-app results

Legend as in waves 1/2a: **build** = `cargo build --release --locked` in the app dir
(incremental; cargo replays cached rustc diagnostics, so "0 warnings" is meaningful);
**self-test** = my run of the hook with a 120 s alarm from the scratch copy; **evidence** =
log + 3–4 screenshots read; **claims** = FRICTION statements checked against `src/` and the
vendored crates (freya-* 0.4.1, vizia* 0.4.0, winit 0.30.13, rfd 0.17.2 in the registry; the
floem checkout `~/.cargo/git/checkouts/floem-ab9be4e01bb293da/778bb5f`); **re-check** = my own
synthetic-input run.

Rules check (all six): only `apps/<fw>-windows/` and `apps/<fw>-ledger/` carry files from the
freya/vizia/floem agents' windows (18:40–19:54; every other file touched after 18:55 belongs
to the slint/dioxus-native agents, the two earlier verifiers or the orchestrator's measure pass
— checked by mtime, since all iteration-5 app dirs are untracked); pins identical to the
baselines: `freya = "=0.4.0"` default features (lock resolves the transitive `freya-*` crates
to 0.4.1 exactly as `apps/freya-app/Cargo.lock` does — the "pinned to match the cohort" note is
a description of that lock, not an extra constraint; a fresh `cargo update` would move them to
0.4.3), `vizia = "=0.4.0"` default features, `floem` git rev `778bb5f2aa08…` (lock:
`git+https://github.com/lapce/floem?rev=778bb5f2…#778bb5f2…`, same as `floem-app`); all
helpers `=`-pinned and listed in each FRICTION (freya-windows: rfd =0.17.2, serde =1.0.229,
serde_json =1.0.151, async-io =2.6.0; freya-ledger: rust_decimal =1.39.0, async-io =2.6.0;
vizia-windows: rfd =0.17.2; vizia-ledger: rust_decimal **=1.42.1** (like tauri-ledger; the
other five ledgers pin 1.39.0 — LOW, declared), chrono =0.4.45; floem-windows: rfd =0.17.2,
raw-window-handle =0.6.2, objc2 =0.6.3; floem-ledger: rust_decimal =1.39.0); edition 2024
everywhere (baselines are 2024 for these three frameworks, so no edition split here); LoC as
reported (1251 / 1490 / 1021 / 1280 / 1162 / 1108, one `src/main.rs` each).

Builds (`build-logs-2b/`, re-run in `build-logs-2b-rerun/` at 20:5x, identical): all six
`Finished … 0.2–0.5 s`, rc=0, zero app diagnostics; floem-windows/floem-ledger print only the
transitive `block v0.1.6` future-incompat note (same as wave-1's dioxus apps). Binary sizes
exactly as reported (21,799,264 / 23,322,704 / 23,199,984 / 24,686,096 / 18,413,248 /
18,262,464 B).

Self-tests (`selftest-2b/*.out`, run from the scratch copies with state files removed):
freya-windows `pass=16 fail=0`, freya-ledger `pass=22 fail=0`, vizia-windows `pass=17 fail=0`,
vizia-ledger `pass=30 fail=0`, floem-windows `pass=21 fail=0`, floem-ledger `pass=37 fail=0`
— all exit 0, empty stderr, N identical to the agents' reports and to the retained
`evidence/selftest-log.txt` files.

### freya-windows — build PASS (0 warnings) · self-test PASS 16/16 · evidence PASS · claims PASS · re-check PASS (2 new defects)

- Evidence (`apps/freya-windows/evidence/`): `05-`/`06-` modal + scrim with unchanged rows,
  `07-` rfd confirm, `08-` Save/Discard/Cancel prompt, `10-`/`11-` light theme across windows;
  `log.txt` records every command; `selftest-log.txt` = `pass=16 fail=0` (matches my run).
- Claims vs code (vendored 0.4.1): accesskit_winit 0.33.2 panics in `with_direct_handlers` when
  `window.is_visible() == Some(true)` (`lib.rs:198-201`, the exact message quoted in the
  FRICTION); `freya-winit` creates every window `.with_visible(false)` (`window.rs:138`), builds
  the adapter (`Adapter::with_event_loop_proxy`, :290) and only then `set_visible(true)` (:292)
  — and `addChildWindow:` orders the child in immediately, so `with_parent_window` through
  `WindowConfig::with_window_attributes` fires the panic exactly as claimed; the app's
  workaround is 20 lines of raw `sel_registerName`/`objc_msgSend` FFI (`main.rs`,
  `child_window::attach`) and my run.out shows `parenting: addChildWindow ordered:above -> true`
  for the inspector and the dialog; `OnCloseHook = Box<dyn FnMut(RendererContext, WindowId) ->
  CloseDecision + Send>` (`freya-winit/src/config.rs:48-49`) — the `Send` bound that forces the
  `AtomicBool` dirty mirror is real; `WindowConfig` has `with_size`/`with_min_size`/… and **no
  `with_position`** (full setter list read); `State::create_global` is documented for
  multi-window sharing (`freya-core/src/lifecycle/state.rs:107-121, 519`); `Input`'s stock
  `on_pre_key_down` is verbatim as quoted (Enter/Escape/Shift → true, Tab → false, `_ =>
  stop_propagation + prevent_default + true`, `freya-components/src/input.rs:217-227`).
- Re-check (mine, scratch binary, `WINDOWS_ORIGIN=300,300`): window count 1 → ⌘⇧I → 2 → ⌘, → 3;
  Inspector button again → still 3 + `singleton` log line. Modal: Edit… → `modal: opened …
  (parent scrim armed)`; three `topat`-gated clicks on Delete → delete log lines 0 → 0, UNC
  alerts 0 → 0, rows 6 → 6 (before/after PNGs differ only in traffic-light colours); Esc →
  `esc: closed the modal`; control click → out-of-process `UserNotificationCenter` alert
  (`Delete ",Apollo"?`, No/Yes) → No → `delete: ",Apollo" cancelled`. Veto: dirty via the modal
  (OK committed) → AX close → UNC `Save/Discard/Cancel` → Cancel → `close: vetoed (Cancel)`,
  alive; second pass (recheck2): ⌘W with main key → Cancel → alive → AX close → Discard →
  `close: Custom("Discard") — quitting`, exit 0, state written with `inspector_open: true`;
  relaunch restores the Inspector. ⌘W with Preferences focused closed only Preferences.
- **New defect 1 — persistence creeps by the title-bar height per cycle.** Three clean cycles
  (recheck3): frame y 268 → 236 → 204, −32 px logical per save/restore. Cause read from
  `main.rs:192-220,254-256`: save records `window.outer_position()` (frame origin) but restore
  goes through `WindowAttributes::with_position`, which winit 0.30 applies to the AppKit
  *content* rect at creation. The agent's own `log.txt:133-139` shows the same numbers
  (AX-move to 240,260 → relaunch at 240,**228**) mis-annotated as "identical frame"; the
  FRICTION's "relaunch at the identical frame" is wrong. MEDIUM-LOW (32 px/launch, compounds).
- **New defect 2 — ⌘, leaks a "," into the focused Input and dirties the document.** Twice
  reproduced: with the Inspector's Name field autofocused, ⌘, opened Preferences *and* prepended
  a comma (row 0 became ",Apollo" in all three windows, status bar `edited`). The app's
  replacement `input_keys` filter returns `true` for META/CTRL combos to un-swallow the
  shortcuts (`main.rs:635-651`), and the editor then inserts the plain character — the flip side
  of the FRICTION's `Input` trap. The subsequent Delete confirm read `Delete ",Apollo"?`.
  MEDIUM-LOW (silent data edit from a global shortcut).
- Also: both rfd prompts are hosted out-of-process by `UserNotificationCenter` (unbundled
  binary, blocking `MessageDialog::show`; run.out carries `CFUserNotificationDisplayAlert: …
  will block`) — the FRICTION quotes the log line but calls the alert a "native NSAlert"; same
  nuance as wave-1 iced/dioxus. Note added.
- Severity: MEDIUM-LOW ×2 (above), LOW (alert hosting wording). FRICTION notes added.

### freya-ledger — build PASS (0 warnings) · self-test PASS 22/22 · evidence PASS · claims PASS · re-check PASS (AX finding sharpened)

- Evidence: `02-typing-1234.5` / `03-blur-formats-en-US` / `04-locale-fr-FR` show the whole
  pipeline; `05-validation-error` is genuine (red border on the empty Description, red summary
  `row 1 · Description: description is required`, `Save (1)`) — not a stale frame; `ax-dump.txt`
  = 331 elements with `AXCell "Unit price row 1"` naming as claimed.
- Claims vs code (freya-components/freya-edit 0.4.1): `on_validate` really is a
  keystroke-granular filter — on `!validator.is_valid()` the Input calls `editor.undo()` +
  `clear_redos()` before the value is committed (`input.rs:455-465`); editor redo is `"y" if
  meta_or_ctrl`, undo `"z"` (`text_editor.rs:669-688`) — "redo is ⌘Y not ⌘⇧Z" exact; `Input`
  has **no `on_blur`** prop (0 matches in `input.rs`); the stock `on_pre_key_down` swallow
  filter verbatim at `input.rs:217-227`; `text_align` is forwarded into a paragraph inside a
  `ScrollView` (`input.rs:672,689`), consistent with the push-out defect (worked around with
  monospace + space padding, visible in every capture); the editor's catch-all `_ if
  allow_changes` arm inserts characters without checking modifiers (`text_editor.rs:690+`) —
  the same mechanism behind the freya-windows ⌘, comma leak.
- Re-check (mine, scratch binary): Tab walk from the window reached row-1 Unit price in 12 Tabs
  in reading order (Locale → Slider → VAT → Copy/Paste/Undo/Save → Date → Description →
  ComboBox → Qty → Unit price), FOCUS-trace-confirmed — tab order **built-in** ✓. Typed
  `1234.5` → Tab → screen `1,234.50`, Amount `1,234.50`, subtotal 4,303.97 (`v-02`); Locale
  click (topat-gated, `LOCALE fr-FR` logged) → `1 234,50`, VAT `19,00`, total `5 121,72`
  (`v-03`) — the SPEC-10 headline pipeline ✓.
- **AX finding sharpened.** The lazy activation is confirmed (cold walk 98 nodes; after
  `AXManualAccessibility` 422 — the agent's 331 was a different UI state) and the tree shape is
  as claimed: 79 named `AXCell`s ("Unit price row 1", …), 49 `AXTextField`s all titled, 12
  pop-ups, 12 checkboxes with **no name** (`cb_titled=0` — the `Checkbox` gap, confirmed),
  1 slider. But the **titles are creation-time snapshots that never update**: after the typed
  edit no field ever carried `1,234.50` in its AXTitle, and after a screen-verified locale
  toggle (`v-05-fr`: fully fr-FR) the same instant's AX dump still said `Title=Locale: en-US`
  with 0 static texts mentioning fr-FR; no `AXTextField` exposes an `AXValue` (0/49). Current
  text is only discoverable through nested static-text children (System Events' `entire
  contents` did find the edited `5,555` — 58 path mentions). So "built-in, but off by default"
  needs a second qualifier: labels yes, live values no. MEDIUM-LOW for real AT users; note
  added, matrix footnote [au].
- Severity: MEDIUM-LOW (stale AX titles), none else.

### vizia-windows — build PASS (0 warnings) · self-test PASS 17/17 · evidence PASS · claims PASS · re-check PASS

- Evidence: 20 screenshots incl. `11-modal`/`12-main-blocked`/`13-after-blocked-delete`,
  theme triptychs in both palettes, `14-`/`15-` veto pair; `title-leak.txt` is a full
  minimal repro of the retitle bug; selftest-log `pass=17 fail=0` (matches mine). The FRICTION
  already records the rfd/UNC out-of-process nuance correctly (unlike most wave-1 apps).
- Claims vs code (vendored vizia* 0.4.0 — every defect claim reproduced in source):
  `Window::event`'s `SetTitle` arm sets the title and does **not** `meta.consume()`
  (`vizia_winit/src/window.rs:466-474`), so the Up-propagating emit renames ancestor windows —
  title-leak claim exact; `WindowEvent::SetEnabled` handler is `#[cfg(target_os = "windows")]
  set_enable(flag)` + an unconditional `focus_window()` (`window.rs:546-551`) — "Windows-only
  stub that focuses the parent on macOS" exact; `Window::popup` inserts `WindowState { is_modal:
  true, … }` regardless of its `is_modal` argument (the argument only gates the
  `SetEnabled(false)` emit, `window.rs:395-423`); owner parenting exists only inside
  `#[cfg(target_os = "windows")]` (`WinState::new`, `window.rs:72-79`,
  `with_owner_window(hwnd)`) — not-achievable on macOS confirmed; `Style.dpi_factor` is a single
  `pub(crate) f64` on the process-wide `Style` (`vizia_core/src/style/mod.rs:423`) — the
  multi-monitor "broken by construction" note is structural fact; `Tree::get_parent_window`
  has the "entity is itself a window" branch literally commented out
  (`vizia_storage/src/tree/tree.rs:139-151`).
- Re-check (mine, scratch binary): count 1 → 2 → 3 (Inspector, Preferences), Inspector again → 3
  (singleton). Modal: Edit project opens as a separate always-on-top window (CG layer 3,
  340×242); three topat-gated clicks on the parent's Delete → UNC alerts 0 → 0, status texts
  unchanged, rows 6 → 6, toolbar visibly disabled in the pair (only traffic lights differ);
  delivery into the dialog proven (topat = dialog for its Name field and OK). Control click
  after OK-commit → out-of-process UNC `Delete "Apollo migrationZ"?` → No. Veto: AX close →
  UNC `Save changes?` Yes/No/Cancel → Cancel → alive + `close: vetoed` → close → No (Discard) →
  `close: discarded`, exit 0, state `main=400,300,720,480 inspector_open=1`. Persistence:
  relaunch → frame 400,300 identical (no creep — vizia saves and restores the same rect) and the
  Inspector reopens.
- Severity: none.

### vizia-ledger — build PASS (0 warnings) · self-test PASS 30/30 · evidence PASS · claims PASS (Blur mechanism corrected) · re-check PASS

- Evidence: `02-typing`/`03-blur-formatted`/`04-locale-fr-fr` pipeline, `05-validation-errors`
  (genuine: `Save (2 errors)` greyed, red per-row summary), `10-filter-rejects-letter`;
  ax-dump retained; selftest `pass=30 fail=0`.
- Claims vs code (vizia_core 0.4.0 `views/textbox.rs`): `WindowEvent::FocusOut` emits only
  `TextEvent::EndEdit` (:1095-1097) — "Tab does not commit" exact; `validate` is called
  post-change (`cx.set_valid(validate(value))` after the edit, :1398 etc.) — cannot refuse a
  keystroke, exact; **one mechanism claim corrected**: `TextEvent::Blur` is *not* un-emitted —
  the Textbox's own build-time listener emits it on a mouse-down outside the editing textbox
  (:188-196), and the `Blur` arm then runs `on_blur` **or else** `Submit(false)` (:1538-1544).
  So click-away commits *via Blur*, Tab commits not at all, and `on_blur` is dead only until
  someone sets it (setting it actually *suppresses* the click-away commit). The FRICTION's
  user-visible behaviour stands; its "nothing in the framework emits TextEvent::Blur / on_blur
  is dead code" wording is wrong. Note added.
- Re-check (mine): Tab walk with FOCUS trace (row 1 Date → Desc → unregistered ComboBox inner →
  Qty → Unit at FOCUS 10); typed → AX live read-back `AXIncrementor "row 1 unit price"
  Value=1234.5` → Tab → `Value=1,234.50`, focus `row 1 Reimb` → locale click →
  `Value=1 234,50` (fr screen verified: `v-03`, subtotal `5 313,87`, VAT `1 062,77`). AX dump:
  pass 1 = 98 nodes (lazy first pass, the agent's "probe twice" trap confirmed); passes 2-3 =
  248 nodes, 25 text fields **titled and valued**, 24 incrementors, 12 pop-ups, 12 checkboxes
  all titled, 1 slider — the only iteration-5 non-webview stack whose AX values update live.
  a11y built-in confirmed with names *and* values.
- LOW observation (app, both my run and the agent's `04-locale-fr-fr.png`): the fr-FR **Total**
  renders ungrouped (`6376,64`) while Subtotal/VAT are grouped (`5 313,87`, `1 062,77`); the
  en-US total is grouped. Cosmetic inconsistency in the app's format path.
- Severity: LOW (Blur wording, corrected; fr total cosmetic).

### floem-windows — build PASS (only the transitive `block` note) · self-test PASS 21/21 · evidence PASS · claims PASS · re-check PASS (veto + persistence re-run to completion)

- Evidence: `04-sheet`/`05-sheet-delete-clicks` (real sheet, unchanged rows), `06-native-confirm`,
  `12-close-veto-dialog`, `persistence.txt`, `drive.sh`/`drive-run.log` (the documented raise
  hook), selftest `pass=21 fail=0`. The FRICTION already records the CFUserNotification hosting
  of the rfd alerts — no correction needed.
- Claims vs code (floem checkout 778bb5f): `bounds_of_content_on_screen()` →
  `window_inner_screen_bounds` → **`window.surface_position()`** (`src/window/tracking.rs:
  184-191`) — winit's surface position is window-relative, so the "on_screen" name lies and
  naive persistence would creep by the title-bar height, exactly the upstream-worthy trap
  claimed (the app compensates; see re-check); close veto is genuinely built-in:
  `RouteCx::finish` runs `handle_default_behaviors()` only `if !self.prevent_default`
  (`event/dispatch.rs:1046-1048`) and the `CloseRequested` default posts
  `AppUpdateEvent::CloseWindow` (:1208-1213), while `window/mod.rs:667,678` documents the
  close-vs-request-close split; `set_global_theme` is a framework API (`action.rs:87`,
  `app/mod.rs:82`); `TextInput` contains zero undo code (0 matches) while the Lapce editor
  stack has it (`views/text_editor.rs`, `views/editor/movement.rs`, `keypress/mod.rs`); Tab
  traversal is the framework's `element_tab_navigation` (`event/dispatch.rs:1174,1717`); the
  app's sheet/parenting objc2 calls at `main.rs:254,464` (`beginSheet:completionHandler:`,
  `addChildWindow:ordered:`).
- Re-check (mine, scratch binary): count 1 → 2 → 3, Inspector again → 3 with `focus existing
  (singleton)`, and the Inspector `attached as NSWindow child of main`. Modal: `presented as
  macOS sheet (beginSheet:)` — a real attached sheet; three topat-gated clicks on Delete →
  Delete presses 0 → 0, UNC 0 → 0, **byte-identical** before/after captures (the sheet keeps
  the parent non-key, so not even the traffic lights flip); Esc → `modal: Cancel`; control
  click → 1 press + out-of-process UNC `Delete "Apollo"? This cannot be undone.` → No →
  `delete: cancelled`. ⌘W closes the focused window (`cmd-W: closing focused window`). Veto
  (two earlier attempts were refused by the click gate — Chrome, then Safari, had been moved
  over the target area; third run relocated the window to a topat-verified clear spot):
  Edit… → OK (`modal: OK name=Apollo`, dirty) → AX close → `close: vetoed, asking
  Save/Discard/Cancel` + UNC `Save changes?` → Cancel → `close: answer=Cancel / cancelled,
  window survives`, alive → close → Discard → `close: answer=Discard` → persist → exit 0.
  Persistence: quit → relaunch → frame 300,268 identical → clean close → relaunch →
  300,268 again — **byte-exact over two save/restore cycles, no creep** (the agent's
  compensation for the window-relative geometry works; contrast freya).
- Severity: none. No FRICTION edits needed.

### floem-ledger — build PASS (only the transitive `block` note) · self-test PASS 37/37 · evidence PASS · claims PASS · re-check PASS

- Evidence: `02a-typed-raw`/`02b-tab-formatted-en-US`/`03-locale-fr-FR` pipeline,
  `04-decimal-alignment`, `05-validation-and-filter`, `tab-order.txt`, `ax-dump.txt`;
  selftest `pass=37 fail=0` (matches mine, 2 s).
- Claims vs code: covered under floem-windows (same checkout) — `TextInput` no undo /
  `text_editor` has it, `element_tab_navigation` is the framework's; the app's FOCUS trace
  binds row/col per widget at creation (`main.rs:806,928`), so the traced walk is authoritative.
- Re-check (mine): click row-1 Unit price (`FOCUS row=1 col=4`), type, Tab → screen `1,234.50`
  with Amount recomputed and focus on `row=1 col=5 (Reimb.)` (`v-02`); Locale → `LOCALE fr-FR`,
  fully converted screen (`1 234,50`, VAT `20,0`, subtotal `3 674,18`, total `4 409,02`,
  `v-03`) → toggled back (`LOCALE en-US`). **Tab-order defect reproduced with per-widget
  labels**: forward walk row 1 = Date → Description → Qty → Unit price → Reimb. (col 2
  skipped), then `row=2 col=2 (Category)` fires *before* `row=2 col=0 (Date)` — i.e. each
  row's Dropdown turns up at the row boundary, right after the previous row's checkbox,
  exactly as the FRICTION words it; ⇧Tab reproduces the same order backwards (stable, not
  reading order). a11y: axdump run twice → 97/98 nodes, all menubar/window chrome,
  `AXTextField=0`, 1 static text (the title); System Events counts 4 UI elements —
  **not-achievable** confirmed (no AccessKit integration in this floem rev).
- Severity: none. No FRICTION edits needed.

## Cross-cutting findings (wave 2b)

1. **The two AccessKit-on-winit stacks differ in kind.** Both need activation (freya: cold walk
   98 → 422 nodes only after `AXManualAccessibility`; vizia: first pass 98 → 248 on the second),
   but vizia's AX values update live (typed → formatted → locale-toggled all read back through
   AX) while freya's AXTitles are creation-time snapshots that never update and no field exposes
   an AXValue. floem has nothing to activate at all (97/98 nodes of pure chrome).
2. **Content-vs-frame origin mismatch is now a three-framework trap.** floem names it
   (`bounds_of_content_on_screen()` = winit `surface_position()`, window-relative) and its app
   compensates — verified byte-exact over two cycles; freya hits the same seam unknowingly
   (save `outer_position()`, restore `with_position` → content rect) and creeps −32 px per
   cycle; slint hit the scale-factor-before-mapping flavour in wave 2a.
3. **Meta-chord characters leak into focused text inputs** once freya's swallow-filter is
   relaxed the obvious way — the flip side of the documented `Input` trap: the editor's
   catch-all insert arm never checks modifiers (`text_editor.rs`), so an app that un-swallows
   ⌘, gets a "," typed into the field.
4. **Out-of-process rfd alerts** (waves 1/2a #2/#5) hold for all three: unbundled binaries +
   blocking `MessageDialog::show` → `UserNotificationCenter` owns every confirm/veto prompt
   (freya note added; vizia and floem had recorded it themselves).
5. **Desktop contention, again**: two of my floem veto runs were refused by the topat click
   gate (a Chrome and then a Safari window had been moved over the target); relocating the app
   window to a gate-verified clear spot fixed it. The gate catching this is the method working.

## FRICTION.md edits made (each marked "Verifier note (2026-08-30)")

1. `apps/freya-windows/FRICTION.md` — persistence round-trip creeps −32 px/cycle
   (outer_position vs with_position content rect; the agent's own log shows 240,260 → 240,228);
   the relaxed `input_keys` filter leaks the ⌘-chord's character into the focused Input
   (",Apollo", document dirtied — reproduced twice); rfd prompts are hosted out-of-process by
   `UserNotificationCenter`.
2. `apps/freya-ledger/FRICTION.md` — AX titles are creation-time snapshots (no AXValue,
   screen-verified locale toggle invisible to AX); labels yes, live values no.
3. `apps/vizia-ledger/FRICTION.md` — `TextEvent::Blur` *is* emitted (Textbox's own click-away
   listener); the click-elsewhere commit runs via `Blur`→`Submit(false)`, and setting `on_blur`
   suppresses that commit. "Nothing emits Blur / on_blur is dead code" corrected in two places.

No `src/`, Cargo, evidence or out-of-scope file was modified; nothing committed. floem-windows
and floem-ledger needed no corrections.

## Normalized capability matrix — additional columns

Same rubric ( **built-in** / **assembled** ≤ ~30 lines of glue / **hand-rolled** incl. objc/OS
calls or owning the loop / **not-achievable** ). Footnotes continue wave 2a's ([ap] there).

### SPEC-9 "Windows"

| Capability | freya 0.4.0 (freya-* 0.4.1) | vizia 0.4.0 | floem git 778bb5f |
|---|---|---|---|
| Second top-level window (open/close/singleton focus) | built-in | built-in (a window is a view) | built-in (singleton focus hand-rolled) |
| Modal dialog — kind achieved | hand-rolled (child window + overlay scrim) | assembled (focus lock + always-on-top; not OS-modal) | hand-rolled (objc2 `beginSheet:` = real OS sheet) [av] |
| Parent blocked while modal | assembled (`interactive(false)` subtree) | assembled (`.disabled()` inheritance) | hand-rolled [av] |
| Focus returns to parent after modal | assembled [aq] | built-in | assembled [aw] |
| Shared state across windows | built-in (`State::create_global`) | built-in (one Context/tree) | built-in (process-global signals) |
| Cross-window message (Ping/Pong) + wake | built-in | built-in | built-in |
| Theme/layout change to all windows | assembled [ar] | assembled | built-in (`set_global_theme`) |
| Window parenting (child/owner/transient) | hand-rolled (objc after the AccessKit crash) [as] | not-achievable (owner is `#[cfg(windows)]`) | hand-rolled (objc `addChildWindow:`) [av] |
| Close veto (Save/Discard/Cancel) | assembled (`with_on_close`, `Send`-bound) | built-in | built-in (`prevent_default`) |
| Quit semantics | assembled | built-in | assembled |
| Native confirm dialog | assembled (rfd, out-of-process) [n] | assembled (rfd, out-of-process) [n] | assembled (rfd, out-of-process) [n] |
| Position/size persistence | assembled (round-trip creeps −32 px/cycle) [at] | hand-rolled | hand-rolled (compensates window-relative geometry) |
| Multi-monitor scale change | not-verified | not-verified (broken by construction: one global `dpi_factor`) | not-verified |
| Per-window shortcuts (⌘W focused only, ⌘, , ⌘⇧I) | hand-rolled (⌘, leaks "," into a focused Input) [at] | assembled | assembled |

### SPEC-10 "Ledger"

| Capability | freya | vizia | floem |
|---|---|---|---|
| Numeric field (typed, filtered, step keys) | assembled (`on_validate` keystroke filter) | hand-rolled (`validate` is post-only) | hand-rolled |
| Locale-aware parse + format (live toggle) | hand-rolled | hand-rolled | hand-rolled |
| Decimal alignment / tabular figures (font-feature request) | not-achievable (monospace + padding; `text_align(Right)` broken) [ax] | not-achievable (monospace) [ax] | not-achievable (separator-split labels + monospace) [ax] |
| Inline validation + disabled Save + summary | assembled | assembled (`:invalid` pseudo-class built-in) | assembled |
| Dropdown with type-ahead | hand-rolled (`Select` has none) | built-in (`ComboBox`) | built-in dropdown, type-ahead hand-rolled |
| Date input (masked or picker) | assembled (masked `Input`) | assembled (mask + built-in `Calendar`) | hand-rolled (mask) |
| Slider ↔ numeric field | assembled (Slider is percentage-only) | assembled | built-in (shared signal) |
| Tab order across mixed controls | built-in | built-in | built-in (defect: Dropdown visited at the row boundary) |
| Enter-moves-down | assembled | hand-rolled | hand-rolled |
| Undo/redo (field / form) | built-in (⌘Z/⌘Y) / hand-rolled [ay] | not-achievable / hand-rolled [ay] | not-achievable (`TextInput`; `text_editor` has it) / hand-rolled |
| Live computed columns and totals | built-in | built-in (`Memo`) | built-in |
| Row copy/paste as TSV | assembled [az] | assembled [az] | assembled [az] |
| Accessibility labels (verified dump) | built-in, off by default + frozen (422 nodes, 49 titled fields, 0 values, titles never update) [au] | built-in (248 nodes, names + live values; 2nd AX pass) | not-achievable (98 nodes, chrome only) |
| IME composition (optional) | not-verified | not-verified | not-verified |

Footnotes (cells where the agent's word was changed or qualified; letters continue [ap]):

- [aq] freya "focus returns": agent built-in → assembled as [b]: an explicit
  `Platform::get().focus_window(Some(main_id))` in `close_dialog` and the dialog's `use_drop`
  (one-line composition; a newly launched freya window does not become key by itself).
- [ar] freya theme: agent built-in → assembled as slint [u]: `use_init_theme` is the framework
  piece, but each window repeats a `use_side_effect` pushing `light_theme()`/`dark_theme()`
  into its own theme state; the change itself was verified in the agent's captures.
- [as] freya parenting: agent "assembled (with a Freya-blocking bug)" → hand-rolled: the
  shipped mechanism is 20 lines of raw `sel_registerName`/`objc_msgSend` FFI, required because
  winit's `with_parent_window` crashes freya's AccessKit adapter (panic verified in
  accesskit_winit 0.33.2 source and mechanism in freya-winit 0.4.1). OS calls by the app are
  hand-rolled per [a]/[s].
- [at] freya defects found by the verifier, ratings kept: persistence (assembled) creeps −32 px
  logical per save/restore cycle (save `outer_position()`, restore
  `WindowAttributes::with_position` = AppKit content rect; 268 → 236 → 204 over three cycles,
  and the agent's own evidence shows 260 → 228); shortcuts (hand-rolled) leak the plain
  character of a ⌘-chord into a focused `Input` (⌘, prepended "," to the shared project name
  and dirtied the document — reproduced twice; the editor's catch-all insert arm never checks
  modifiers).
- [au] freya a11y: agent "built-in but off by default" → second qualifier added: after
  `AXManualAccessibility` activation the tree is complete and cell-named (79 `AXCell`s), but
  field AXTitles are creation-time snapshots that never update, no `AXValue` is exposed
  (0/49), and checkboxes are unnamable — a screen reader sees the launch-time ledger.
- [av] floem modal kind (agent: "OS window-modal assembled from objc2"), parent-blocked
  (agent: "built-in to the sheet") and parenting (agent: "assembled via addChildWindow:"):
  all three exist only through the app's own objc2 `msg_send!` calls → hand-rolled per
  [a]/[s]; the sheet and its blocking are real and verified (0 presses under the sheet,
  byte-identical captures — the sheet even keeps the parent non-key).
- [aw] floem "focus returns": agent hand-rolled → assembled as [b]/[t]: `endSheet:` +
  `close_window` + one `focus_window(parent)` line in `close_modal` (`main.rs:275-285`).
- [ax] decimal alignment, all three: rated on the row's font-feature request per [l]/[ad] —
  no OpenType-feature API exists in freya (and `Input::text_align(Right)` is broken on top),
  vizia, or this floem rev; the achieved alignments (freya monospace + space padding, vizia
  monospace, floem split-at-separator two-label trick) were all verified on screen.
- [ay] form-level undo: freya (agent: assembled, ⌘⇧Z) and vizia (agent: unrated "form-level
  only") are app-side snapshot stacks — `Vec<(row, field, prev)>` / `Vec<Vec<Row>>` — identical
  in kind to every other framework's form undo → hand-rolled per [af]. freya's field-level
  built-in (editor ⌘Z/⌘Y, verified in freya-edit source) kept.
- [az] TSV: agents said built-in (framework clipboard in all three); the row⟷TSV formatting
  and the re-bound ⌘⇧ chords (freya) are app glue on the clipboard piece → assembled per [q].

## Verified numbers (my runs)

| app | build | self-test (mine) | agent's N | interactive re-check |
|---|---|---|---|---|
| freya-windows | ok, 0 warnings | 16/16 | 16 | count 1→2→3 + singleton ✓, modal-block ✓ (control ✓ via UNC), veto Cancel alive / Discard exit 0 ✓, ⌘W focused-only ✓; persistence CREEPS −32 px/cycle; ⌘, comma leak |
| freya-ledger | ok, 0 warnings | 22/22 | 22 | Tab walk in reading order ✓, `1234.5`→`1,234.50`→`1 234,50` ✓ (screen), AX 98→422 nodes / 49 titled / 0 valued / titles frozen |
| vizia-windows | ok, 0 warnings | 17/17 | 17 | count 1→2→3 + singleton ✓, modal-block ✓ (control ✓), veto Cancel alive / Discard exit 0 ✓, persistence exact ✓ |
| vizia-ledger | ok, 0 warnings | 30/30 | 30 | `1234.5`→`1,234.50`→`1 234,50` ✓ **live via AX values**, AX 98→248 nodes, names+values ✓ |
| floem-windows | ok (transitive `block` note) | 21/21 | 21 | count 1→2→3 + singleton ✓, sheet modal-block ✓ (byte-identical pair, control ✓), veto Cancel alive / Discard exit 0 ✓ (3rd attempt; click gate refused 2 contested runs), persistence byte-exact ×2 ✓ |
| floem-ledger | ok (transitive `block` note) | 37/37 | 37 | `1234.5`→`1,234.50`→`1 234,50` ✓, Tab-order Dropdown defect reproduced ✓, AX 97/98 chrome-only |
