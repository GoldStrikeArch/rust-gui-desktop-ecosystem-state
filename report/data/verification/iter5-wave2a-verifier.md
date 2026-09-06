# Iteration 5 verification — wave 2a (slint, dioxus-native × windows/ledger)

Verifier run 2026-08-30 19:38–20:40 on the shared M4 Pro desktop while three build agents
(freya/vizia/floem) drove synthetic input. Method, rubric and footnote style are those of
`report/data/verification/iter5-wave1-verifier.md`; the footnotes below continue that report's
lettering ([s]…) so the matrices merge. Artefacts: `scratchpad/verify-iter5/` —
`build-logs-2a/`, `selftest-2a/` (my runs of the hooks, from scratch copies of the binaries so
the slint `take_snapshot` files land outside the repo), one directory per app with `run.out`,
`v*-*.png` window/region captures and `axdump-pass{1,2,3}.txt`, plus the scripts
(`run-*.sh`, `lib2.sh` = wave-1 `lib.sh` + `modkey` CGEvent helper + scratch launcher).

Method notes that matter for reading the results:

- Delivery was proven before every click (`topat` = my pid) and after every keystroke burst
  (AX value read-back for slint; `FOCUS …` trace lines for dioxus-native). Contaminated runs
  were discarded and repeated; they are kept in the per-app `run.out`/`v-*` files.
- ⌘W / ⌘A were posted with the slint agent's `modkey` (CGEvent `flagsChanged` + key with real
  flags); `osascript keystroke … using command down` is known not to reach winit.
- Two new harness traps, both my errors on first attempt: (a) AppKit shifts the slint parent
  window up when `beginSheet:` attaches (frame y 630 → 555), so Delete coordinates cached before
  the sheet were stale — same as wave-1's iced/tauri note; (b) wave-1's `SPOTS` list places a
  592-px-tall ledger window with its bottom rows below the 982-px display, so clicks on row 12
  went off-screen. Also: the repo binaries carry the agents' persisted window state
  (`insp_open 1`, `windows-state.json`), which restores the Inspector at launch and steals the
  first ⌘W; window-count runs therefore used scratch copies with no state file.
- Ghost alerts: my own positive-control clicks on dioxus-native-windows left two
  `UserNotificationCenter` "Delete project" alerts on screen (one hid the other from my
  dismissal click); both were dismissed with "No" before the next runs. None are left.

## Per-app results

Legend as in wave 1: **build** = `cargo build --release --locked` (incremental; cargo replays
cached rustc diagnostics, so "0 warnings" is meaningful); **self-test** = my run of the hook with
a 120 s alarm; **evidence** = log + 3–4 screenshots read; **claims** = FRICTION statements checked
against `src/`, slint 1.17.1 in the registry and the blitz checkout at
`~/.cargo/git/checkouts/blitz-66635cc3152d32bd/64eb278`; **re-check** = my synthetic-input run.

### slint-windows — build PASS (0 warnings) · self-test PASS 19/19 · evidence PASS (sheet shot taken after a click) · claims PASS · re-check PASS

- Rules: only `apps/slint-windows/` touched; `slint = "=1.17.1"` + `slint-build =1.17.1` as in
  `slint-app`, plus the declared `raw-window-handle-06` feature (justified: `window_handle()`);
  helpers `rfd =0.15.4`, `raw-window-handle =0.6.2`, macOS-only `objc2 =0.5.2` pinned and listed;
  `Cargo.lock` present (slint 1.17.1, winit 0.30.13); edition 2024 like the other iteration-5
  apps (baseline 2021 — corpus-wide note from wave 1).
- Build: `Finished … 0.47s`, no diagnostics. Binary 16,572,496 B as reported.
- Self-test: `SELFTEST DONE pass=19 fail=0`, exit 0, output identical to the retained log.
- Evidence: `modal-delete-blocked.png` (5 rows after the control delete, sheet up; a sibling
  dioxus-native-ledger window overlaps — interpretable, contaminated, LOW), `shared-state.png`
  (Inspector "Apollower" mirrored in row 0; also shows the Preferences window clipped to
  ~150 px, see below), `main-with-modal.png`/`main-dark.png` are `take_snapshot` renders (6 rows
  / dark palette, mean luminance 233 → 44 in the log).
- Claims vs code: `export global` is per component instance — the generator stores every
  global in the instance's `globals` (`i-slint-compiler-1.17.1/generator/rust.rs:339-366`,
  `inner.globals.get().unwrap()`), so two windows get two `Palette`s; **`modifiers.control` is
  Command on macOS** — `i-slint-core-1.17.1/input.rs:836` "Slint remaps modifiers on macOS:
  control → Command, meta → Control"; `scale_factor` is a `Property::new_named(1., …)`
  (`window.rs:580`) set only from the winit adapter's `ScaleFactorChanged` dispatch
  (`winitwindowadapter.rs:505-507`) once the winit window exists, so 1.0 before mapping is
  exact; winit 0.30.13 documents `with_parent_window` for Windows/X11 only (`window.rs:466-472`)
  and Slint has no attributes hook, so `addChildWindow:` through objc2 (`src/mac.rs:52-61`) is
  the only route; `beginSheet:`/`endSheet:` at `mac.rs:30-48` behind `Window::window_handle()`;
  `on_close_requested` → `CloseRequestResponse::KeepWindowShown` at `main.rs:398-412`;
  `Timer::start(SingleShot, 300 ms)` for the Pong flash (`main.rs:304`). All correct.
- Re-check (mine, `WINDOWS_NO_OVERLAY=1 WINDOWS_NO_CONFIRM=1`, AppKit only): control double
  click on Delete → `DELETED row 0; rows now 5/4`; Edit… → `modal open (beginSheet: true)`, sheet
  260×164 centred, parent shifted 630→555; three `topat`-confirmed clicks on the re-located
  Delete → DELETED 2 → 2, byte-identical before/after captures, window count unchanged; click
  into the sheet + Esc → sheet gone; post-control double click → DELETED 2 → 4. Window count on
  a clean scratch binary: 1 → 2 (`inspector opened (addChildWindow: true)`) → 3 → Inspector
  again → 3. Veto (`WINDOWS_DIRTY=1`): ⌘W → `close vetoed: unsaved changes` + "Save changes?"
  window, alive → Cancel → `close cancelled`, alive → ⌘W → vetoed → Discard → `state saved`, exit
  0; the saved geometry is logical (`main 400 630 720 480`). Bonus: with the persisted Inspector
  key, ⌘W closed only the Inspector (focused-window semantics confirmed). rfd Delete confirm:
  a new 260×202 window owned by `UserNotificationCenter` (out-of-process), dismissed.
- **New findings (not in the FRICTION):**
  1. **Windows first shown from a callback do not paint until a render is forced.** The sheet
     was a blank dark rounded rectangle in every capture until it was clicked (then it painted
     fully and took typing, `ApolloZ`); the prompt's body stayed *transparent* (main-window rows
     visible through it) even after a click, and painted only after an AX resize
     (`v-prompt-2-after-click.png` vs `v-prompt-3-after-resize.png`). The agent's
     `modal-open.png` shows the sheet after a click into it; the "Esc needs a prior click" item
     is the same defect. MEDIUM — a user closing a dirty window sees an empty 211×100 window.
  2. Secondary windows open at layout-minimum size, not `preferred-*`: Inspector 244×236,
     Preferences 123×202, prompt 211×100 logical (the agent's own `bounds` lines show 244/123);
     Preferences clips its controls. LOW-MEDIUM (spec asks ~360×300 / ~320×200).
  3. The rfd confirm is out-of-process (the row was "by-construction"; now exercised). LOW.
- Severity: MEDIUM (1), LOW-MEDIUM (2), LOW (3, contaminated shot). Notes added.

### slint-ledger — build PASS (0 warnings) · self-test PASS 19/19 · evidence PASS except one stale PNG · claims PASS · re-check PASS (with one app defect)

- Rules: pin identical to `slint-app` (default features); `arboard =3.6.1` listed; lock present.
- Self-test: `pass=19 fail=0`, exit 0, identical output; my `validation-error.ppm` is
  byte-identical to the agent's.
- Evidence: `en-us-aligned.png` (`1,234.50`, Menlo column aligned, `-34.75` red),
  `fr-fr-aligned.png` (`1 234,50`, total `4 953,54`), `live-locale-fr-FR.png` (mouse-driven
  toggle, `4 058,66`). **`validation-error.png` does not show a validation state**: no red
  border, `Save` enabled, no summary, totals from the previous step — `snapshot()` is called in
  the same tick as `a.commit(11, 1, "")` (`selftest.rs:337-348`) and `take_snapshot` returns
  the previous frame. The check itself passes at model level. I produced the missing evidence
  (`v3-05-validation-error.png`: red border on row 1 price, `Save (1 error…)` greyed, red
  summary "row 1: unit price out of range / not a number"). MEDIUM-LOW, note added.
- Claims vs code: `TextInput::key_event` calls the `key_pressed` callback first and returns on
  `Accept` before any insertion (`i-slint-core-1.17.1/items/text.rs:957-966`; the FRICTION's
  ":958" is exact); `input-type: decimal` → `accept_text_input` builds the candidate and returns
  `string_to_float(&candidate).is_some()` after a 2-char prefix exception (`text.rs:2202-2230`)
  — grouping separators are indeed rejected, and the separator comes from
  `SlintContext::locale_decimal_separator` (`context.rs:297-307`), which is not re-exported by
  the `slint` crate (`slint-1.17.1/lib.rs` re-exports `api::*`, models, timers… no
  `SlintContext`); no `font-feature*`/`font-variant` anywhere in `builtins.slint` or
  i-slint-core/femtovg (only `font-family/-size/-weight/-italic`, `letter-spacing`);
  `ComboBoxBase` handles only Up/Down/Return/Escape (`widgets/common/combobox-base.slint:42-48,
  105-115`); `Slider.changed` ↔ `LineEdit.edited` glue at `main.rs:344-363`; form undo is a
  `Vec<(usize, Item)>` (`main.rs:42, 131-135, 376-384`). All correct.
- Re-check: AX read-back works (fields report title and value). Mouse focus on the Unit-price
  cell + `1234.5` → `1234.55` (see defect); keyboard focus (click Qty, Tab) + `1234.5` → AX
  `1234.5` exactly → Tab → `1,234.50` (focus lands on the row's checkbox) → Locale click →
  `1 234,50`, button `Toggle locale` (`v4-0{1,2,3}.png`); Esc reverts (`99482.5` → `482.5`).
  `axdump`: 209 nodes, 49 `AXTextField` all titled and valued, 12 pop-ups, 12 checkboxes,
  1 slider on the first pass (after two `entire contents` warm-ups) — a11y **built-in**
  confirmed with names *and* values.
- **App defect found:** select-on-focus is defeated by mouse focus — the click's caret placement
  lands after `select-all()`, leaving `[0, click)` selected, so typing replaces only the head of
  the old value (`482.5`→`1234.55`, `Airport transfer`+`abc`→`abcer`, click left of the digits
  → `99482.5`). Keyboard focus selects fully. MEDIUM-LOW for the spec's "type into a cell"
  flow; note added.
- Severity: MEDIUM-LOW ×2 (stale PNG, select-on-focus).

### dioxus-native-windows — build PASS (0 warnings) · self-test PASS 13/13 · evidence PASS · claims PASS · re-check PASS

- Rules: only `apps/dioxus-native-windows/` touched. `dioxus-native` and `blitz-shell` both at
  git rev `64eb27853aa2672486b7edf825fb044be78c9db3` (lock: `git+…?rev=64eb278…#64eb278…`),
  `dioxus = "=0.7.9"` with `default-features = false, features = ["macro","html","hooks",
  "signals"]`; helpers `rfd =0.17.2`, `tokio =1.52.3` (`rt-multi-thread` added, justified) pinned
  and listed; lock present; edition 2024. **Sharing verified:** `src/` holds only `main.rs`
  (12 lines, `#[path = "../../dioxus-windows/src/app.rs"] mod app;`) and `platform.rs`; there
  is no copy of `app.rs`. `apps/dioxus-windows` and `apps/dioxus-ledger` are *untracked* in git,
  so `git diff --stat` is vacuous; mtimes prove no edit instead: `dioxus-windows/src/app.rs`
  17:30:06 and `dioxus-ledger/src/app.rs` 18:05:12 predate every dioxus-native file (18:18–19:09),
  and both still have 807/935 lines as the desktop agent reported. `platform.rs` exposes the
  same 18 public functions/types as the desktop `platform.rs`.
- Build: `Finished … 0.50s`, no diagnostics. Binary 24,533,888 B as reported.
- Self-test: `pass=13 fail=0`, exit 0; identical to the retained log except the state path.
- Evidence: `01-three-windows` (main + Inspector + Preferences; the Inspector's Status
  `<select>` is an empty gap; an "Edit project / Aurora-X" window from a sibling app is in
  frame — contaminated, LOW), `06-modal-blocks-delete` (dimmed main, 6 rows, byte-identical to
  05), `08-close-veto-prompt` (in-window Save/Discard/Cancel card), `11-theme-all-four-windows`
  (all four dark at once; a floem window behind).
- Claims vs code (blitz checkout): `DioxusNativeApplication { pending_window: Option<…> }` and
  `launch_cfg…` creates exactly one (`dioxus_application.rs:36,48`, `lib.rs:99-243`);
  `add_window` is `pub` (`:54`) but only pushes onto `BlitzApplication::pending_windows`, whose
  `can_create_surfaces` does `View::init` + `resume` with **no** context injection and no
  `initial_build()` (`blitz-shell/src/application.rs:33-35, 113-128` vs the five
  `provide_context` calls + `initial_build()` at `dioxus_application.rs:140-175`) — "renders
  blank" is exact; `BlitzApplication::window_event` drops the `View` on `CloseRequested`
  unconditionally (`application.rs:151-159`); `WindowEventHandlers` is `pub(crate)`
  (`event_handlers.rs:26`); winit 0.31.0-beta.2 `WindowAttributesMacOS` has titlebar/panel/
  tabbing fields only (`winit-appkit/src/lib.rs:329-344`) and no `with_parent_window` anywhere
  in winit-core; `<input type=checkbox|radio>` dispatch only `DomEventData::Input`
  (`blitz-dom/src/events/pointer.rs:645-695`, zero `"change"` in blitz-dom); `flush_is_focussable`
  → `tabindex < 0` not focusable (`node/element.rs:627-635`); `AppleStandardKeybinding(_) =>
  None` and `Ime(_) => None` (`dioxus-native-dom/src/dioxus_document.rs:331-335`); the
  re-implemented launcher (`platform.rs:133-159, 315-355`) does the same five injections minus
  the two crate-private ones, plus `initial_build()` and `resume()`, and the veto lives in its
  `window_event` (`:405-424`). All correct.
- Re-check (`WINDOWS_PLACE=1`): control double click on Delete → two new windows owned by
  `UserNotificationCenter` (the rfd confirm is out-of-process, like dioxus-windows); Edit… →
  4th window "Edit project" (380×252, Name field autofocused); three `topat`-confirmed clicks on
  Delete → no new window, no `deleted row` line, byte-identical captures; Cancel → `[edit]
  cancelled` / `closed edit`, alive. Veto: Edit → type `x` → OK → `[edit] committed` (`● unsaved`)
  → AX close button → `close requested while dirty -> prompt` + `CloseRequested vetoed`, card
  shown → Cancel → `close: Cancel -> vetoed`, alive → close → Discard → `close: Discard ->
  exit`, exit 0. Window count (scratch binary): 1 → 2 → 3 → Inspector again 3; `OS parenting
  unavailable on winit 0.31` logged; 1 process, RSS 117 MB with three windows.
- Severity: none. LOW: rfd hosting nuance + rubric wording ("built-in (rfd)"), note added.

### dioxus-native-ledger — build PASS (0 warnings) · self-test PASS 26/26 · evidence PASS · claims PASS · re-check PASS (Tab finding sharpened)

- Rules: same git rev; `dioxus =0.7.9` facade as above; `rust_decimal =1.39.0`, `arboard =3.6.1`,
  `tokio =1.52.3` listed; lock present; `src/` = 12-line `main.rs` (`#[path = "../../
  dioxus-ledger/src/app.rs"]`) + 66-line `platform.rs` (same 5 functions as the desktop file);
  sharing proven by mtime as above.
- Build: `Finished … 0.37s`, no diagnostics. Binary 27,858,960 B as reported.
- Self-test: `pass=26 fail=0`, exit 0, identical output (it also clobbers the shared clipboard
  with a TSV row, as documented).
- Evidence: `01-initial` (Date and Category cells are empty bars, no slider track next to
  "VAT"), `04-blur-formats-1234.50`, `05-locale-fr` (`1 234,50`, `5 546,77`),
  `06-text-cjk-emoji` (世界 and مرحبا rendered, blank gap where the ZWJ emoji should be). All
  as claimed.
- Claims vs code: `set_focus_to` only snapshots + `node.blur()/focus()` with no event dispatch
  (`blitz-dom/src/document.rs:1646-1671`); `generate_focus_events` is called from the pointer
  path (`events/pointer.rs:544,658`) and `events/mod.rs:133,163`, never from
  `focus_next_node/focus_prev_node` (`document.rs:1617-1630`) which Tab calls
  (`events/keyboard.rs:22-28`); blitz-dom intercepts ⌘C for selection copy when no text input
  is focused (`keyboard.rs:33-52`); `BlitzInputEvent { value: String }` only
  (`blitz-traits/src/events.rs:727-729`) so `checked()` cannot be derived; `SpecialElementData`
  has `TextInput`/`CheckboxInput` and no select/range/date variants (`node/element.rs:342-362`),
  and `blitz-paint/src/render/form_controls.rs` draws only the checkbox (radio via the same
  path) — nothing paints `<select>`, `type=date` or `type=range`; the AccessKit builder sets
  roles from tag/`role=` and values only for text nodes, never reading `aria-label`
  (`blitz-dom/src/accessibility.rs:35-64`; zero `aria` matches) — "names and values don't reach
  the OS" is exact. All correct.
- Re-check (`LEDGER_PLACE=1 LEDGER_TABLOG=1`): click on row 1 Unit price → `FOCUS row 1 Unit
  price`; ⌘A + `1234.5` visible in the editor; Tab → no new FOCUS line, before/after captures
  byte-identical (Tab inert as claimed). Pointer blur: click on another Unit-price cell →
  `1,234.50` with either osascript keystrokes or CGEvent keys (`v3-A/B`), Locale → `1 234,50`
  (`v3-C`). **Sharpened finding:** in the run where a Tab preceded the blur click, the click
  fired `FOCUS row 2 Description` but the price input never got `blur`; the edit was lost and
  the locale toggle re-rendered `482,50` — Tab silently moves blitz-dom's internal focus and
  misdirects the next pointer blur (worse than "moves nothing"). `axdump`: pass 1 = 98 nodes,
  zero fields (lazy); passes 2–3 = 337 nodes, 37 `AXTextField` with **0 titles and 0 values**,
  12 pop-ups, 12 checkboxes, 1 slider — the agent's "roles yes, names/values no" is exact.
  1 process, RSS 108 MB idle (125 MB after the interactive session).
- Severity: LOW (Tab wording), note added.

## Side-by-side dioxus-native vs dioxus (re-measured)

Same machine, my runs (`run-rss.sh`, plain launch, sampled at 7 s and 10 s). Binary sizes are
`ls -l`; crate counts are `cargo tree -e normal --prefix none | sort -u | wc -l` in each app
dir; `[[package]]` counts from `Cargo.lock` added as a second yardstick. Build times were not
re-measured (all four incremental `--locked` builds are 0.3–0.5 s no-ops); the agent's clean
times stand as reported.

| | dioxus-native-windows | dioxus-windows | dioxus-native-ledger | dioxus-ledger |
|---|---|---|---|---|
| binary (B) | **24,533,888** (agent: same) | 6,477,664 (same) | **27,858,960** (same) | 6,424,992 (same) |
| unique crates (`cargo tree`) | **520** (same) | 377 (same) | **524** (same) | 380 (same) |
| `[[package]]` in Cargo.lock | 622 | 543 | 646 | 568 |
| idle RSS, own process | **105 MB** (agent 107) | 100 MB (agent 94) | **108 MB** (agent 112) | 98 MB (agent 88–98) |
| WebKit XPC helpers | 0 | 4 with 2 windows restored (GPU 33 + Networking 18 + 2× WebContent 34/33 = 119 MB); 3 with one window | 0 | 3 (GPU 35 + Networking 18 + WebContent 41 = 96 MB; agent 79) |
| processes | **1** | 1 + 3 (or +4 per extra window) | **1** | 1 + 3 |
| RSS with 3 windows | 117 MB (agent 113) | not measured | — | — |
| clean build (agent) | 76.5 s | 35.0 s | 75.2 s | 54.4 s |

Verdict: the agents' tables are confirmed within run-to-run noise (±6 MB RSS); the crate counts
and binary sizes are exact. The "291/293 crates" figures in the dioxus section of
`report/data/iter5-rows.md` are from a different count and should not be compared with 520/524.

## Cross-cutting findings (wave 2a)

1. **Blank-until-event windows in Slint (winit backend, femtovg).** Both windows the slint
   agent shows from inside a callback (sheet, close prompt) paint nothing until a click/resize
   forces a render; the prompt is literally transparent. Windows shown at startup (main,
   restored Inspector) paint normally. Not recorded by the agent; the sheet screenshot in the
   evidence was taken after a click. Worth a follow-up against Slint (`show()` from a callback
   + `orderOut:`/`beginSheet:`, and a plain `show()` for the prompt).
2. **Slint preferred sizes ignored on secondary windows** (244×236 / 123×202 / 211×100 vs
   360×300 / 320×200 / 340×120): possibly the same scale-factor-1.0-before-mapping trap the
   agent found for persistence, applied to the initial size.
3. **Select-on-focus + mouse** is an app-level pattern that fails in Slint (caret placement
   after `select-all`); the self-test cannot catch it because it focuses by property.
4. **Blitz Tab** moves internal focus without events and thereby loses the next pointer-driven
   commit — a data-loss-shaped defect, not just a missing feature.
5. **Out-of-process rfd alerts** (wave-1 #2) also hold for slint-windows and
   dioxus-native-windows (both unbundled, `AsyncMessageDialog`); the slint row had only been
   rated by construction.
6. **Lazy AccessKit** (wave-1 #3) confirmed again: dioxus-native-ledger 98 → 337 nodes between
   AXUIElement passes; slint needed two `entire contents` warm-ups before AX button clicks
   worked (my AX `click button "Cancel" of window "Save changes?"` returned `missing value`
   names on the first pass).
7. **Harness:** the `SPOTS` list in `lib.sh` is unsafe for 592-px-tall windows (bottom rows
   off-screen on the 982-px display); and the repo binaries' persisted window state changes
   what a "fresh launch" shows — use a scratch copy with no state file for count/veto runs.

## FRICTION.md edits made (each marked "Verifier note (2026-08-30)")

1. `apps/slint-windows/FRICTION.md` — rfd confirm is out-of-process (`UserNotificationCenter`);
   sheet/prompt paint nothing until a render is forced (+ parent shift on `beginSheet:`);
   secondary windows open at layout-minimum sizes.
2. `apps/slint-ledger/FRICTION.md` — `validation-error.png` is a stale frame (UI verified by the
   verifier instead); select-on-focus defeated by mouse focus, keyboard focus works.
3. `apps/dioxus-native-windows/FRICTION.md` — rfd confirm is out-of-process; rubric wording.
4. `apps/dioxus-native-ledger/FRICTION.md` — Tab moves internal focus silently and loses the
   next pointer blur/commit.

No `src/`, Cargo, evidence or out-of-scope file was modified; nothing committed. The scratch
copies of the binaries and their state files live only under `scratchpad/verify-iter5/`.

## Normalized capability matrix — additional columns

Same rubric as wave 1 (**built-in** = framework API does it; **assembled** = framework pieces +
≤ ~30 lines of glue; **hand-rolled** = the app implements the mechanism, incl. objc2/OS calls
or owning the event loop; **not-achievable** = no path in the pinned framework). Footnote
letters continue wave 1's ([a]–[r] there); wave-1 footnotes are referenced where they apply.

### SPEC-9 "Windows"

| Capability | slint 1.17.1 | dioxus-native (Blitz 64eb278) |
|---|---|---|
| Second top-level window (open/close/singleton focus) | built-in [w] | hand-rolled (own `ApplicationHandler` over blitz-shell) [y] |
| Modal dialog — kind achieved | hand-rolled (objc2 `beginSheet:` = real OS sheet) | hand-rolled (window + scrim) |
| Parent blocked while modal | hand-rolled [s] | hand-rolled |
| Focus returns to parent after modal | assembled [t] | assembled |
| Shared state across windows | assembled (one `ModelRc` + `changed` down-sync) | built-in |
| Cross-window message (Ping/Pong) + wake | assembled (`set_row_data` + `Timer`) | built-in (signal write → `BlitzShellEvent::Poll`) |
| Theme/layout change to all windows | assembled (per-window `Palette.color-scheme`) [u] | built-in mechanism, dead controls (no `change` event) [aa] |
| Window parenting (child/owner/transient) | hand-rolled (objc2 `addChildWindow:`) | not-achievable |
| Close veto (Save/Discard/Cancel) | built-in (`on_close_requested`) [x] | hand-rolled (own `window_event`, real veto) |
| Quit semantics | built-in | assembled |
| Native confirm dialog | assembled (rfd, out-of-process) [n] | assembled (rfd, out-of-process) [ab] |
| Position/size persistence | assembled (logical units, 120 ms timer) | assembled |
| Multi-monitor scale change | not-verified | not-verified |
| Per-window shortcuts (⌘W focused only, ⌘, , ⌘⇧I) | assembled [v] | not-achievable [ac] |

### SPEC-10 "Ledger"

| Capability | slint | dioxus-native |
|---|---|---|
| Numeric field (typed, filtered, step keys) | assembled (`key-pressed` pre-insertion filter) [ah] | hand-rolled |
| Locale-aware parse + format (live toggle) | hand-rolled | hand-rolled |
| Decimal alignment / tabular figures (font-feature request) | not-achievable (alignment via Menlo) [ad] | built-in (CSS accepted; glyph application not isolated) [ai] |
| Inline validation + disabled Save + summary | assembled | assembled [aj] |
| Dropdown with type-ahead | assembled (`ComboBox` + `FocusScope`) | not-achievable (`<select>` paints nothing) |
| Date input (masked or picker) | assembled (char filter + calendar check; picker not per-cell) [ag] | not-achievable (`<input type=date>` paints nothing) |
| Slider ↔ numeric field | assembled [ae] | not-achievable (`<input type=range>` paints nothing) [ak] |
| Tab order across mixed controls | built-in | not-achievable [an] |
| Enter-moves-down | assembled | not-achievable [an] |
| Undo/redo (field / form) | built-in / hand-rolled [af] | not-verified / hand-rolled [am] |
| Live computed columns and totals | built-in | built-in |
| Row copy/paste as TSV | assembled (⌘⇧C/V + buttons) | assembled (buttons only) [ap] |
| Accessibility labels (verified dump) | built-in (209 nodes, 49 fields named + valued) | not-achievable for names/values (337 nodes, 37 fields, 0 named) [al] |
| IME composition (optional) | not-verified | not-achievable [ao] |

Footnotes (cells where the agent's word was changed or qualified):

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
  `builtins.slint`; alignment via Menlo + fixed two decimals verified in my captures.
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
  `aria-label`); 37 fields, 0 titled, 0 valued on my dump; first AXUIElement pass 98 nodes.
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

## Verified numbers (my runs)

| app | build | self-test (mine) | agent's N | interactive re-check |
|---|---|---|---|---|
| slint-windows | ok, 0 warnings | 19/19 | 19 | modal-block ✓ (control ✓, parent shifted by sheet), count 1→2→3 ✓, veto Cancel alive / Discard exit 0 ✓, ⌘W closes focused window ✓ |
| slint-ledger | ok, 0 warnings | 19/19 | 19 | `1234.5` → Tab → `1,234.50` → `1 234,50` ✓ (keyboard focus; mouse-focus defect), Esc reverts ✓, validation UI ✓, AX 209 nodes / 49 named+valued fields |
| dioxus-native-windows | ok, 0 warnings | 13/13 | 13 | modal-block ✓ (control ✓ via UNC alerts), count 1→2→3 ✓, veto Cancel alive / Discard exit 0 ✓, 1 process |
| dioxus-native-ledger | ok, 0 warnings | 26/26 | 26 | Tab inert ✓, pointer blur → `1,234.50` → `1 234,50` ✓, AX 98 → 337 nodes, 37 fields / 0 names / 0 values, 1 process |
