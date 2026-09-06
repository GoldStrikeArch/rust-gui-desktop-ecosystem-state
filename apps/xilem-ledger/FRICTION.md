# FRICTION — Ledger (xilem =0.4.0)

App: `apps/xilem-ledger/` · package `xilem-ledger` · edition 2024 ·
`cargo run --release`. Built and verified on macOS 26.5 (Apple M4 Pro,
rustc/cargo 1.96.1). Evidence in `evidence/`: 15 window screenshots captured
by CGWindowID, `log.txt` (the exact command for every step), `ax-dump.txt`
(AccessKit tree via System Events), `tab-order-walk.txt` (focus log from real
Tab/Shift+Tab/Enter presses) and `selftest-log.txt`
(`LEDGER_SELFTEST=1` → `SELFTEST DONE pass=13 fail=0`).
`verify/evidence.sh` + `verify/windows.swift` reproduce the run.

LoC **1821** production (`main.rs` 630 · `model.rs` 477 · `shell.rs` 489 ·
`views.rs` 225) + 120 verification (swift/bash, outside the binary). That is
well over the ~700 guide and the split is the finding: **489 lines are a winit
event-loop embedding that exists only because xilem 0.4 has no focus, no blur
and no way to intercept a key its `TextArea` already ate**, and **477 lines
are a plain-Rust number model** (parse/format/validate/step/TSV) that no layer
of the stack offers anything for. The actual form is ~630 lines.

## Architecture (the headline)

xilem 0.4 gives a form author exactly four relevant views: `text_input`,
`checkbox`, `slider`, `label`. There is **no numeric input, no locale, no
validation state, no focus API, no blur event, no combobox, no date control,
no undo, no clipboard access and no way to ask for an OpenType feature**.
Three of those gaps cannot be closed at the view layer at all, so — as in
`apps/xilem-tray` — the app owns the winit `ApplicationHandler` and embeds
xilem through `Xilem::into_driver_and_windows` + `MasonryState::new`
(`src/shell.rs`). That buys three things:

1. **Focus/blur.** After every winit event the shell reads
   `RenderRoot::focused_widget()` and diffs it against the previous value.
   That single poll is *both* the "normalise and format on blur" trigger and
   the tab-order log in `evidence/tab-order-walk.txt`. A `CellView`
   pass-through view (`src/views.rs`) publishes each `text_input`'s inner
   `TextArea` `WidgetId` into a shared registry so the poll can name the cell.
2. **Key interception.** masonry's `TextArea::on_text_event` handles
   ArrowUp/ArrowDown (caret movement) and calls `ctx.set_handled()`, so an
   ancestor widget can never see them — arrow *stepping* is impossible above
   the winit layer. Up/Down (+Shift ×10), Esc-revert, ⌘Z/⌘⇧Z and ⌘⇧C/⌘⇧V are
   handled in `window_event` and simply not forwarded to masonry.
3. **Programmatic focus.** `RenderRoot::focus_on(Some(id))` from a wrapper
   `AppDriver` is the only way to implement Enter-moves-down.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **hand-rolled** | synthetic-input + self-test | No numeric view exists. Filtering rides `text_input`'s `on_changed`: the callback sanitises the string (`model::typing_filter`) and writes it back to state; xilem's rebuild calls `TextArea::reset_text` when state ≠ widget text, which is what actually removes the rejected characters (`10a-typing-filter.png`: typing `12ab3` leaves `123`). Stepping is winit-layer interception → `STEP row 0 / Unit price x1 -> 482.51`, `x10 -> 482.61` (`05a`/`05b`). Home/End work because `TextArea` implements them itself. |
| Locale-aware parse + format (toggle live) | **hand-rolled** | synthetic-input + self-test | Nothing locale-aware anywhere in the stack; ~120 lines of `model.rs` do grouping, decimal separator, accounting negatives and paste-tolerant parsing. The toolbar toggle is a plain state flip and the whole table re-formats on the next rebuild (`04-fr-fr-locale-toggle.png`). Self-test: `1,234.50` ⇄ `1 234,50`; paste of `$1,234.56`, `1.234,56 EUR` and `(12.50)` all parse. fr-FR grouping uses ASCII space, not U+202F, because fontique 0.6 renders the narrow no-break space as tofu. |
| Decimal alignment / tabular figures (font-feature request) | **hand-rolled** | observed | The feature *can* be requested, but not from any stock view: `label` exposes only `FontSize`/`FontWeight`/`FontStack`, so `views::num_label` is a 70-line re-implementation of the label view that adds `StyleProperty::FontFeatures(FontSettings::Source("\"tnum\" 1, \"lnum\" 1"))` (parley 0.6 has the variant; masonry re-exports the type). It is accepted and rendered without error, but at 13 px against the macOS system face the difference is not visually detectable, so alignment is *actually* guaranteed by splitting the formatted number at the separator and laying the halves out in two fixed-width boxes. `LEDGER_MONO=1` swaps in `GenericFamily::Monospace` for comparison (`09-mono-figures.png`). Trap found: masonry's default theme gives every `Label` 2 px horizontal padding, which showed up as a gap around the decimal point until the view inserted `Padding::all(0.0)` into `NewWidget::properties`. Negative amounts render red and in parentheses. |
| Inline validation UI + disabled Save + error summary | **assembled** | self-test + observed | No form/validation concept, but the pieces compose: `text_input` supports `BorderColor`/`BorderWidth`/`Background` props directly (`HasProperty` impls on `masonry::widgets::TextInput`), `text_button(..).disabled(bool)` exists, and the summary is a `flex_col` of labels. Committed values stay typed; a failed commit keeps the previous `Decimal` and re-shows the bad text (`06-validation-error.png`: red cell, `Save (1 errors)` disabled, `row 3 / Description: description: 1-60 characters`). |
| Dropdown with type-ahead | **hand-rolled (approximated)** | self-test | There is no combobox, no menu, and no way to position a popup relative to a widget — a floating list would need the compose-pass geometry registry + `zstack` machinery from `apps/xilem-board`. Shipped approximation: the option list is spliced into the table as an extra row directly under the cell, filtered by what has been typed (`07-category-typeahead.png`, `S` → `Software`). Prefix completion also happens on commit, so typing `Tr` + Tab commits `Travel`. |
| Date input (masked or picker) | **hand-rolled (mask)** | synthetic-input | No date view and no OS date picker binding. `model::mask_date` keeps digits and re-inserts the dashes; typing `20260722` into a cleared cell yields `2026-07-22` (`12-date-mask.png`). **Honest limitation:** typing into an already-filled masked cell corrupts it (`2026-01-08` + `20260722` → `2026-20-10`) because `text_input` exposes no caret or selection API and `TextArea::reset_text` resets the caret to 0 — a rewrite-on-every-keystroke mask fights the caret. A real masked field needs a custom masonry widget. |
| Slider ↔ numeric field linkage | **assembled** | observed | Stock `slider(0.0, 25.0, v, cb)` and a `text_input` both write the same `Decimal`; each direction re-renders the other on the next rebuild. This is the one row where xilem's reactive rebuild does all the work. |
| Tab order across mixed controls | **built-in** | synthetic-input | masonry has a real focus chain (`find_next_focusable`, forward/backward, `accepts_focus` on `TextArea`/`Button`/`Checkbox`/`Slider`). Real Tab presses walk Date → Description → Category → Qty → Unit price → the row's Reimbursable checkbox → next row, and Shift+Tab walks back (`tab-order-walk.txt`). The focus ring is visible in every screenshot. Nothing in xilem exposes this — the *order* is whatever the widget tree order is, and there is no tab-index. |
| Enter-moves-down cell navigation | **hand-rolled** | synthetic-input | `text_input(..).on_enter` gives the hook (with `InsertNewline::Never`, `TextArea` emits `TextAction::Entered` and marks the event handled, so it never bubbles). Moving the focus is the hard half: the callback records the target cell in shared state and the wrapper `AppDriver` applies `RenderRoot::focus_on`. Verified: row 0/Date → row 1/Date → row 2/Date. |
| Undo/redo (field-level / form-level) | field-level **not-achievable**, form-level **hand-rolled** | synthetic-input + self-test | masonry's `TextArea` has no undo stack at all (confirmed again here; the same finding as `apps/xilem-tray`), and ⌘Z reaches no handler, so there is nothing to reach for at field level. Form level is a `Vec<Edit>` of committed (row, field, before, after) with ⌘Z/⌘⇧Z intercepted at the winit layer: `11-undo.png` shows 482.61 → 482.51, status `undo row 1 / Unit price`. |
| Live computed columns and totals | **built-in** | observed + self-test | `app_logic` re-runs after every state mutation and the view tree is diffed; Amount, Subtotal, VAT and Total are recomputed each rebuild from `Decimal`. 12 rows × 7 columns (61 live `TextInput`s) with no perceptible lag. |
| Row copy/paste as TSV | **assembled (chord changed)** | synthetic-input | No clipboard API at the view layer; `arboard` for both directions. **⌘C/⌘V could not be used**: `TextArea` implements ⌘C on the text selection, and `masonry_winit` itself converts ⌘V into `TextEvent::ClipboardPaste` before any widget sees it. Row operations therefore use ⌘⇧C/⌘⇧V. Copy verified via `pbpaste` (`2026-01-09\tHotel, 3 nights\tLodging\t3\t219.00\t657.00\tyes`); paste verified in `13-tsv-paste.png`. |
| Accessibility labels (verified dump) | **partial (values built-in, names not-achievable)** | synthetic-input | AccessKit is wired through `accesskit_winit` and the tree is real: `ax-dump.txt` has 61 `text field`s each exposing its **value** (`2026-01-08`, `Flight LHR-JFK`, `482.51`, …), 12 checkboxes with true/false, `slider 1` with value 20, and named buttons (`Locale: en-US`, `Save`). What is missing is the **name**: `description of every text field` returns the generic `text field` for all 61, because xilem's `text_input` has no accessible-label parameter and column headers are unrelated `static text` nodes. Also: masonry builds the tree lazily on `InitialTreeRequested`, so the *first* AX query returns only the title-bar elements — a real trap for anyone testing this. Cost of the decimal-alignment trick: each Amount reads as two nodes (`static text 482`, `static text .51`). |
| IME composition (optional) | **by-construction** | not-verified | masonry_winit forwards `WindowEvent::Ime` → `TextEvent::Ime` and `TextArea` handles composition (`accepts_text_input`, `set_ime_area`), and `RenderRoot` emits `StartIme` when a text widget takes focus — the code path is complete. Not exercised: switching the macOS input source is not scriptable from this harness, and the shared machine made an interactive check unreliable. |

## Helper crates

- `masonry_winit =0.4.0` — xilem does not re-export `MasonryState` /
  `AppDriver` / `DriverCtx`, which the event-loop embedding needs. Same
  version as the one xilem 0.4.0 already pulls in, so no duplicate in the tree.
- `rust_decimal =1.39.0` (`default-features = false`, `std`) — the whole point
  of SPEC-10 is that the value is not an `f64`. Exact 2-dp arithmetic for
  Amount/Subtotal/VAT/Total and exact round-tripping through the formatter.
- `arboard =3.6.1` — reading and writing the clipboard for row TSV. masonry
  can *set* the clipboard from inside a widget (`EventCtx::set_clipboard`) but
  there is no read path outside its own ⌘V handling and nothing at all at the
  view layer.

**Tried and rejected:** `icu` / `num-format` for locale formatting — both drag
in large data or a locale database for what turned out to be ~120 lines of
grouping/separator logic, and neither would have helped with the *parsing*
side (currency stripping, accounting negatives, `1.234,56` vs `1,234.56`
disambiguation), which is where the real work is. `unicode-segmentation` was
not needed: all the string surgery here is on ASCII digits and separators.

Dependency graph: **462** name-version entries in `Cargo.lock` including the
app (vs 403 for `apps/xilem-grid`, which is xilem-only).

## Measurements

- Clean release build **29.93 s** wall (`target/release` removed first);
  `cargo build --release --locked` afterwards: **0.24 s, no lock change**.
- Binary **11,611,792 bytes** raw / **9,360,640 bytes (8.9 MiB)** stripped.
- Self-test: `SELFTEST DONE pass=13 fail=0` (12 s wall including startup).

## Where the time went

1. **The shell layer** (~30 %): realising that focus, blur, arrow keys and Esc
   are all unreachable from the view layer, then rebuilding the
   external-event-loop embedding around them. The `CellKey ↔ WidgetId`
   registry and the focus-diff poll are the load-bearing parts.
2. **The number model** (~20 %): locale parse/format, the
   grouping-vs-decimal-separator disambiguation, draft-vs-committed state.
3. **Scripted verification on a shared machine** (~30 %): seven sibling
   research apps were running throughout, several with always-on-top windows
   over this one and all of them stealing key focus. Every synthetic click and
   keystroke needed a re-activation + a read-back of the app's own stdout
   before the next step, window images had to be captured by CGWindowID
   (`screencapture -l`) rather than by screen rectangle, and
   `scripts/window-count.swift` was unusable because it filters to
   CGWindowLevel 0. The `LEDGER_DEMO=N` hook (replay the same interactions
   through injected `TextEvent`s and stop at step N) exists purely so a few
   screenshots could be made deterministic.
4. **Layout/props debugging** (~15 %): the `Label` default padding breaking
   decimal alignment; getting `Prop<..>` ordering right so `cell()` still sees
   a `TextInput` widget; `text_alignment` having to precede `.color()`.
5. The actual form (~5 %).

## Surprises

- **Good:** masonry's focus chain is genuinely good — Tab and Shift+Tab walk
  every editable cell, the checkbox, the slider and the buttons in tree order
  with a visible focus ring, and `RenderRoot::focus_on` / `focused_widget()`
  are public. None of it is exposed by xilem, but it is all *there*.
- **Good:** `NewWidget::properties` is public, so a hand-written view can set
  masonry properties the stock views don't expose (this is how the label
  padding was zeroed and how `FontFeatures` got in).
- **Good:** AccessKit is real and free. Every value in the form showed up in
  the macOS AX tree without a line of app code.
- **Bad:** the "filter while typing" mechanism is a round trip — widget →
  `on_changed` → app state → rebuild → `reset_text` — and `reset_text` moves
  the caret. Anything that rewrites the string on every keystroke (a mask,
  auto-grouping) is therefore unusable without a caret API that does not exist.
- **Bad:** `TextArea` swallowing Up/Down means the single most characteristic
  behaviour of a numeric field cannot be implemented anywhere in xilem.
- **Bad:** the two clipboard chords a spreadsheet needs are already taken, one
  by the widget and one by the *event-loop runner*, and neither is
  configurable.
- **Bad:** the AccessKit tree is empty on the first AX query. Anyone doing a
  one-shot accessibility audit of a masonry app would report "no a11y tree".

## The number model

The value travels as **three** representations, and the app owns all the
conversions. The model holds `Decimal` (`rust_decimal`) — `qty`, `price`, and
`amount()` computed as `(qty * price).round_dp(2)`. The widget holds a
`String`, always, because `text_input` is the only input xilem has. Between
them sits a per-field `draft: Option<String>`: `Some` while the cell is being
edited, `None` when the cell shows the formatted committed value. Parsing
therefore lives in exactly two places: `typing_filter` on every keystroke
(reject-only; it never parses, it just refuses characters that could not
appear in a valid number) and `Row::commit` on blur / Enter / Tab, which
parses, range-checks and either replaces the `Decimal` or records an error and
keeps both the old value and the bad text.

Can the framework's text input be constrained? **No — only post-validated,
and only through a round trip.** There is no input mask, no validator, no
`on_key` filter and no rejected-input signal. The one lever is that
`text_input` is a controlled component: the view feeds it a `String` from app
state and its `rebuild` calls `TextArea::reset_text` whenever the state string
differs from the widget's. Sanitising inside `on_changed` and writing the
sanitised value back is therefore *effective* — the rejected characters do
disappear — but it is a full state-update-and-diff cycle per keystroke, and it
resets the caret to 0, which is why the date mask breaks on a pre-filled cell.
A framework-level "this field is a number" would replace roughly 200 lines
here.

## Approximated or skipped

- **Category dropdown** is an inline expanding row, not a floating popup —
  xilem 0.4 has no popup/menu primitive and no widget-relative positioning.
- **Date** is a mask, not a picker, and the mask only behaves correctly when
  the cell is cleared first (caret limitation above).
- **Field-level undo** is not-achievable (no undo stack in `TextArea`); only
  form-level undo of committed cell edits is implemented.
- **Row copy/paste** uses ⌘⇧C/⌘⇧V instead of ⌘C/⌘V (both chords are already
  claimed below the app).
- **Accessible names** for the cells are not-achievable through `text_input`;
  only values are exposed.
- **IME** is by-construction only; not exercised.
- **Decimal alignment** is achieved by geometry (two fixed-width boxes), not
  by the font feature. The `tnum`/`lnum` request is made and accepted, but its
  visual effect could not be demonstrated with the system UI face, so it is
  not what the alignment relies on.
- The `Qty`/`Unit price` *editable* columns are right-aligned rather than
  split-aligned, which is equivalent only because every committed value is
  normalised to a fixed number of decimals; a `text_input` cannot be split
  into two boxes.
