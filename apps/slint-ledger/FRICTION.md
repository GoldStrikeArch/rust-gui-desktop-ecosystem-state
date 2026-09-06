# FRICTION — Ledger (Slint =1.17.1)

SPEC-10. Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2, rustc 1.96.1.
Default slint features (winit backend, femtovg renderer, **accessibility on by
default**); cupertino style.

Built: `apps/slint-ledger/` — 12-row expense table, 7 columns, live
Amount/Subtotal/VAT/Total, locale toggle, per-cell validation. Verified by a
self-test hook (`LEDGER_SELFTEST=1`, 19 checks, `evidence/selftest-log.txt`,
all pass) that types through `Window::try_dispatch_event` — the same
`i-slint-core` input pipeline the winit backend feeds from real OS keys — plus
live CGEvent clicks and a real macOS accessibility dump
(`evidence/ax-dump.txt`, `evidence/log.txt`).

LoC: app 976 (`src/main.rs` 451, `ui/main.slint` 363, `src/num.rs` 159,
`build.rs` 3) + 472 lines of self-test harness (`src/selftest.rs`).
Binary 17,959,040 bytes. Clean release build 54.9 s.

## Headline

Slint's text input turns out to be **genuinely constrainable while typing**:
`TextInput.key-pressed` is invoked *before* the character is inserted and
returning `accept` swallows it (`i-slint-core/items/text.rs:958`), so a real
input filter is ~10 lines of DSL calling one `pure callback` into Rust. There
is also a built-in `input-type: decimal` that filters locale-aware, but it
accepts only what `str::parse::<f32>` accepts — **no grouping separators** —
so it is unusable for a field that displays `1,234.56`, which is exactly the
business-forms case. Everything above the character level (parse, locale,
format, validation state, decimal alignment, form undo) is hand-rolled: Slint
has no number widget beyond an integer `SpinBox`, no locale API in its public
Rust surface, and **no way to request OpenType features** — `Text`/`TextInput`
expose `font-family`, `font-size`, `font-weight`, `font-italic` and
`letter-spacing` and nothing else, so tabular figures mean naming a
monospaced face.

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **assembled** | self-test | `LineEdit.key-pressed` → `pure callback allow(kind, current, char) -> bool` in Rust; returning `accept` swallows the keystroke. ↑/↓ and ⇧↑/⇧↓ are handled in the same callback (`qty 1 →Up→ 2 →⇧Up→ 12`, `price -34.75 →Down→ -34.76 →⇧Down→ -34.86`). Home/End/⌘←→ are TextInput built-ins. Caveat: the filter only sees the *whole* text, not the cursor/selection offsets (`LineEdit` exposes none), so the candidate string is `text + char` rather than a true splice. |
| Locale-aware parse + format (toggle live) | **hand-rolled** | self-test + synthetic-input | 90 lines in `src/num.rs`. Slint *has* a locale notion (`SlintContext::locale_decimal_separator`, ICU-backed) but `SlintContext` is not reachable from the `slint` crate, so it is unusable. Verified: type `1234.5` + Tab → `1,234.50`; one click on the Locale button re-renders every cell and the footer as `1 234,50` / `4 058,66` (`evidence/live-locale-fr-FR.png`). Paste-shaped strings `$1,234.56`, `1.234,56 €`, `(12.50)` all parse (`(12.50)` → `-12.50`, rendered red). |
| Decimal alignment / tabular figures (font-feature request) | **hand-rolled; feature request not-achievable** | self-test | No `font-features`/`font-variant` property exists anywhere in `builtins.slint`, so `tnum`/`lnum` cannot be requested. Alignment is `horizontal-alignment: right` + `font-family: "Menlo"` (fixed advance) + fixed 2-decimal formatting. Result in `evidence/en-us-aligned.png`: `1,234.50 / 34.75 / 219.00` line up on the point. Negative amounts render red. |
| Inline validation UI + disabled Save + error summary | **assembled** | self-test | Slint has no validation state on any widget. Each cell is wrapped in a `Rectangle` whose `border-color` turns red from an `e-*` bool on the row struct; `Save` binds `enabled: error-count == 0` and its label carries the count; a conditional `Text` lists `row N: field …`. **Verifier note (2026-08-30):** `evidence/validation-error.png` does **not** show this state — `take_snapshot()` is called in the same tick as the commit and captures the previous frame (no red border, `Save` enabled, no summary; the retained `.ppm` is byte-identical on a verifier re-run). The UI itself was verified by the verifier with real input: red cell border, `Save (1 errors)` disabled, red summary line (`scratchpad/verify-iter5/slint-ledger/v3-05-validation-error.png`). |
| Dropdown with type-ahead | **assembled** | self-test | `ComboBox` is built-in (and reports as `AXPopUpButton`), but `ComboBoxBase` only implements ↑/↓/Enter/Esc — no type-ahead, no public `select()`. Added by wrapping it in a `FocusScope`: the ComboBox rejects unhandled keys so they bubble up, and Rust cycles the matching category (`'s'` → Software). |
| Date input (masked or picker) | **assembled (mask) + built-in (picker)** | self-test + observed | The cell is a masked `LineEdit` (filter = digits and `-`, validated as a real calendar date on commit, so `2026-02-30` is an error). `DatePickerPopup` from std-widgets does exist and is wired to a toolbar button, but it is a `PopupWindow`: it cannot be attached to a table cell without one instance per row, so it fills the focused row instead. |
| Slider ↔ numeric field linkage | **built-in** | self-test | `Slider { changed(v) => … }` and a `LineEdit` both write the same Rust value; each writes the other's property back. Verified in both directions (slider 25 → field `25.00`; field `7.5` → slider 7.5). |
| Tab order across mixed controls | **built-in** | self-test | Slint's focus traversal follows tree order with no extra work, and it crosses the repeater boundary correctly: focus events logged `r8:date → r8:desc → r8:qty → r8:price → r9:date → r9:desc`. (The ComboBox and CheckBox between them are focusable too but have no `has-focus` change hook to log from, so they do not appear in the trace.) |
| Enter-moves-down cell navigation | **assembled** | self-test | Not a framework concept. `key-pressed` intercepts `Key.Return`, commits, then Rust calls `focus-cell(row±1, col)`; each cell has `changed want => le.focus()`. Verified r6c4 →Enter→ r7c4 →⇧Enter→ r6c4. |
| Undo/redo (field-level / form-level) | **built-in (field) + assembled (form)** | by-construction + self-test | TextInput keeps its own undo/redo stacks and maps `StandardShortcut::Undo/Redo` (⌘Z / ⌘⇧Z) internally — field-level undo is free. Form-level undo is a `Vec<(usize, Item)>` snapshot stack behind an "Undo edit" button; verified restoring a cleared description and clearing the error with it. |
| Live computed columns and totals | **built-in** | self-test | One `VecModel::set_row_data` per changed row plus three window properties; 12 rows recompute with no perceptible cost. |
| Row copy/paste as TSV | **assembled (arboard)** | self-test | Slint's public Rust API has no clipboard; only `TextInput.copy()/paste()` inside a focused field. `arboard` for the row-level TSV, on ⌘⇧C/⌘⇧V and toolbar buttons (⌘C/⌘V are left to the text field, which is what a user in a cell expects). |
| Accessibility labels (verified dump) | **built-in** | observed (`evidence/ax-dump.txt`) | AccessKit is on by default on the winit backend and it is *good*: 49 `AXTextField`, 12 `AXPopUpButton`, 12 `AXCheckBox`, 1 `AXSlider`, 7 `AXButton`, each carrying the `accessible-label` written in the .slint (`text field Unit price row 1`, `slider VAT percent`). std-widgets set `accessible-role`/`accessible-value` themselves. **Trap:** the first `System Events → entire contents` call returns only the window chrome; AccessKit activates lazily on the first AX query, so the dump must be taken twice. |
| IME composition (optional) | **not-verified** | — | Not exercised. From the code, `TextInput` has `preedit-text` and the LineEditBase placeholder already accounts for it, so inline composition is modelled; no CJK input source was switched to on this machine. |

## Helper crates

| Crate | Pin | Why |
|---|---|---|
| `arboard` | `=3.6.1` | Row-level TSV copy/paste. Slint exposes no clipboard in its Rust API. |

Tried and rejected: `rust_decimal` (the money here is 2-decimal and
round-tripped through `f64` with explicit `(v*100).round()/100`; adding a
decimal crate would not change any observable behaviour and would inflate the
dependency graph the corpus measures); `num-format`/`icu` (grouping is 12
lines and the ICU data Slint already links is not reachable anyway);
`input-type: decimal` (built-in, but rejects grouping separators — see
Headline); `StandardTableView` (text-only cells, single-row selection, no
editing — the same reason `apps/slint-grid` rejected it).

## Where the time went

1. ~25% the widget↔model contract: a `LineEdit` owns its `text`, so a plain
   binding is destroyed by the first keystroke. Every cell needs a
   `changed value => le.text = value` sync-down *and* a `rev` counter on the
   row struct so Esc/undo/paste can force a re-sync when the committed value
   did not actually change.
2. ~20% `num.rs` (locale parse heuristics, accounting negatives, grouping).
3. ~15% layout: std-widgets' `LineEdit` carries `min-width: 160px`, which
   silently blows a `HorizontalLayout` of 7 fixed-width columns apart and
   desynchronises the header from the cells. `min-width: 0px` on the cell
   wrapper fixes it, and you cannot set both `width` and `min-width` on the
   same instance.
4. ~15% focus mechanics (focus-by-property, select-on-focus, Enter navigation).
5. ~25% the self-test harness and the accessibility/synthetic-input evidence.

## Surprises

- **Good:** the typing filter. Being able to reject a character before it is
  inserted, from the DSL, with the decision made in Rust, is more than most
  toolkits in this corpus offer.
- **Good:** field-level undo/redo, ⌘C/⌘V/⌘X, Home/End, word motion and a
  right-click Cut/Copy/Paste/Select-All context menu are all already in
  `TextInput`/`LineEditBase`.
- **Good:** AccessKit output is genuinely usable — roles are right without
  being asked, and one `accessible-label` per cell produced a complete,
  screen-reader-shaped tree.
- **Bad:** no font-feature access at all. For a spec whose whole point is
  digits that line up, "name a monospaced font" is the only lever.
- **Bad:** `SlintContext::set_locale` and `locale_decimal_separator` exist in
  `i-slint-core` and drive the built-in `input-type: decimal`, but neither is
  re-exported by the `slint` crate — so the one locale-aware behaviour the
  framework *has* cannot be pointed at the application's locale.
- **Bad:** `LineEdit` exposes `set-selection-offsets()` but no way to *read*
  the cursor or selection, so an input filter cannot know where the character
  would land.
- **Neutral:** focus-by-property (`changed want => le.focus()`) works but is
  deferred one event-loop iteration, which every scripted test has to
  account for.

## Number model

**The value that crosses the widget boundary is always a `string`.** Slint has
no numeric input type (only `SpinBox`, which is `int`-only and has no
formatting), and `LineEdit.text` is a `string` the widget owns. So the model is
a plain Rust `Vec<Item>` with `qty: i64` and `price: f64`, and the DSL never
sees a number at all: `App::row()` renders each `Item` into a `Row` struct of
pre-formatted `SharedString`s (`qty`, `price`, `amount`) plus four `e-*`
validity bools, and pushes it with `VecModel::set_row_data`. Parsing lives in
exactly one place — `num::parse(&str, Loc)` — called from the commit callback;
formatting lives in `num::fmt(f64, decimals, Loc)`, called from `App::row()`.
That means:

- the locale toggle is a single `Cell<Loc>` plus a full `refresh()`;
- an invalid entry sets `bad[field] = true` and leaves the numeric field
  untouched, so "keeps the previous committed value" is automatic;
- the only awkward part is pushing back *down*: because the committed value
  may be unchanged (Esc, or a rejected edit), the row carries a `rev: i32`
  that Rust bumps to force `changed rev => le.text = value` to fire.

And the input **can** be constrained rather than only post-validated:
`key-pressed` returning `accept` is a true pre-insertion filter. It is just
character-at-a-time and cursor-blind, so it stops garbage but cannot enforce
a mask.

## Approximated / skipped

- **IME (req. 11)** — not exercised; recorded as not-verified.
- **⌘C/⌘V on a row (req. 9)** — bound to ⌘⇧C/⌘⇧V plus toolbar buttons,
  because plain ⌘C/⌘V inside a focused cell must stay text copy/paste.
- **Date picker (req. 4)** — `DatePickerPopup` is wired to a toolbar button
  rather than to each cell (a `PopupWindow` cannot be attached to a repeater
  cell without one instance per row); it sets year/month and keeps the day.
- **Qty ≥ 0 vs 1–999 (reqs. 2/5)** — the stricter 1–999 rule from req. 5 is
  the one enforced.
- **fr-FR grouping separator** — a plain ASCII space, not U+202F, so the
  strings survive terminal logs and screenshots unmangled. The parser accepts
  U+00A0 and U+202F on input.
- **Verifier note (2026-08-30) — select-on-focus is defeated by the mouse.**
  `changed has-focus => self.select-all()` runs on mouse-down, but the click's
  own cursor placement lands afterwards and leaves only `[0, click position)`
  selected, so typing replaces the *head* of the old value: `482.5` + `1234.5`
  → `1234.55`; `Airport transfer` + `abc` → `abcer`; a click left of the
  right-aligned digits inserts at the head (`482.5` → `99482.5`). Keyboard
  focus (Tab into the cell) selects fully, and the spec sequence then works
  with real OS keystrokes (`1234.5` → Tab → `1,234.50` → Locale →
  `1 234,50`, read back through AX by the verifier). The self-test cannot
  see this because it focuses cells by property, not by click.
- **Live CGEvent keystroke run** — attempted three times and abandoned: on
  this shared verification desktop other agents' app-modal dialogs took key
  focus mid-sequence. Mouse-driven checks (focus, locale toggle) did land and
  are retained; the keystroke checks are covered by the self-test through the
  same input pipeline.

## Measurements

- Clean release build **54.9 s**; incremental no-op **0.3 s**.
- Binary **17,959,040 bytes** (unstripped, `debug = false`).
- `cargo build --release --locked` succeeds unchanged; `Cargo.lock` committed.
