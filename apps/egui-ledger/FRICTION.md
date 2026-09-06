# FRICTION — Ledger (egui/eframe =0.35.0)

App: `apps/egui-ledger/` · package `egui-ledger` · `cargo run --release`.
Built and verified on macOS 26.6.2 (Apple M4 Pro, single 3024×1964 Retina
display, scale 2.00), rustc/cargo 1.96.1. Release build clean, no warnings;
`cargo build --release --locked` succeeds unchanged.

**LoC**: 1064 app (`main.rs` 769 + `model.rs` 295; 893 excluding comments and
blanks) + 295 verification-only self-test driver = 1359. Over the ~700 guide;
SPEC-10 is a wide spec and roughly a quarter of the app file is the findings
written down as comments.

**Evidence**: `evidence/` — `selftest-log.txt` (`LEDGER_SELFTEST=1`,
`SELFTEST DONE pass=26 fail=0`, exit 0), `log.txt` (each scripted step with
its command), `trace.log` (`LEDGER_TRACE=1` action log), `ax-dump.txt`
(225-node AX tree) plus `axdump.swift` (the tool that produced it), and 6
screenshots.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **hand-rolled** on `TextEdit` | self-test + synthetic-input | egui has no numeric field with a text model. `DragValue` is close but wrong for this spec: it steps only on **unmodified** ↑/↓ by its `speed` (`drag_value.rs:496` — `count_and_consume_key(Modifiers::NONE, …)`, so ⇧↑ ×10 is impossible), it has no locale, no error state, and `custom_parser` runs *after* the fact — it cannot refuse a keystroke. So a cell is `TextEdit::singleline` + a character filter run on `response.changed()` + `parse` on `response.lost_focus()` + arrow handling read off `Event::Key`. ~90 LoC. |
| Locale-aware parse + format (toggle live) | **hand-rolled** | self-test + observed | No i18n in egui and none pulled in (`icu`/`num-format` rejected as heavier than 60 LoC of grouping + a separator heuristic). Toggle re-renders every buffer (`03-locale-fr.png`: `1 250,00`, `4 991,77`). Paste accepts `$1,234.56`, `1.234,56 €`, `(12.50)` and `1 234,56` regardless of the active locale (the later of `.`/`,` wins; a lone separator with 3 digits after it is grouping). |
| Decimal alignment / tabular figures | **assembled**; font-feature request **not-achievable** | observed (`06-decimal-alignment.png`) | **egui cannot request OpenType features.** epaint 0.35 *does* shape with HarfRust, but the call is literally `shaper.shape(buffer, &[])` (`epaint-0.35.0/src/text/text_layout.rs:1434`) — the feature slice is hard-coded empty and nothing in `FontTweak`/`FontDefinitions` reaches it. `FontTweak` exposes variable-font *axes* (`coords`) but no `tnum`/`lnum`. The alignment therefore comes from `FontFamily::Monospace` (bundled Hack, equal digit advance) + always exactly 2 fractional digits + right alignment. Also: a trailing space on positive Amounts, so the accounting `( … )` negative does not shift the column by one glyph. **Trap**: do *not* group fr-FR with U+202F/U+2009 — epaint hard-codes their advance to `0.5 × space` even in the monospace face (`text_layout.rs:272-279`), which would break the alignment; this app groups with an ASCII space. |
| Inline validation UI + disabled Save + error summary | **assembled** | observed (`04-validation-errors.png`) | No validation framework. `errors() -> Vec<(row, field, msg)>` is recomputed each frame (12 rows: free), `ui.add_enabled_ui(errs.is_empty(), …)` greys Save, and the red ring is `painter().rect_stroke(response.rect, …)` because a `TextEdit` frame's stroke is not settable per widget without cloning the whole `Visuals`. Out-of-range/unparseable input keeps the previously committed `Decimal` — verified by self-test and screenshot. |
| Dropdown with type-ahead | **assembled** (combo built-in, type-ahead hand-rolled) | by-construction | `egui::ComboBox` is built-in and fine; it has **no** type-ahead (487 lines, zero keyboard search). Hand-rolled: while the popup is open, collect `Event::Text`, keep a 1 s buffer, jump to the first prefix match. ~20 LoC. Not exercised by synthetic input (the popup closes as soon as the window loses focus on this shared desktop), so: by-construction. |
| Date input (masked or picker) | **hand-rolled masked input** | observed | egui ships no date picker (`egui_extras`' `DatePickerButton` needs the `datepicker` feature + `chrono`; rejected — a mask is 12 lines and the spec only requires `YYYY-MM-DD` by keyboard). `filter_date` keeps digits and re-inserts the dashes on `changed()`; `date_valid` drives the red ring and the error summary. |
| Slider ↔ numeric field linkage | **built-in** | observed | `Slider::new(&mut self.vat, 0.0..=25.0)` and `DragValue::new(&mut self.vat)` bound to the same `f64`. Two-way with **zero** glue code — the one place in this spec where egui's `&mut` model pays off completely. |
| Tab order across mixed controls | **built-in** | self-test | egui assigns focus order by widget *creation* order, which for a `TableBuilder` body is reading order. The self-test walks it with real Tab keys: `Date[1] → Description[1] → <ComboBox> → Qty[1] → Unit price[1] → <Amount/Reimb> → Date[2] → …`. No tab-index API and none needed. Caveat: `TableBuilder::body().rows()` is virtualized, so Tab cannot reach a row that is scrolled out of view. |
| Enter-moves-down cell navigation | **hand-rolled** | self-test | `TextEdit` blurs on Enter (`lost_focus()`), which is the hook: read the Enter event's own modifiers, compute `(col, row ± 1)`, and `ui.memory_mut(\|m\| m.request_focus(cell_id(col, row)))` on the next frame. Stable ids via `TextEdit::id(Id::new(("cell", col, row)))` are what makes this possible at all. Verified: `Qty[3] → Qty[4]`. |
| Undo/redo (field-level / form-level) | **both** — field built-in, form hand-rolled | self-test | `TextEdit` has its own `Undoer` (⌘Z/⌘⇧Z inside the focused field, free). Form-level = whole-table snapshots pushed on each commit, ⌘Z/⌘⇧Z when no field has focus. The two are dispatched by `ctx.memory(\|m\| m.focused()).is_some()`; without that check the global handler eats the field's undo. |
| Live computed columns and totals | **built-in (immediate mode)** | observed | Amount, Subtotal, VAT and Total are functions recomputed every frame. No reactivity, no invalidation, no lag at 12 rows — this is the one requirement immediate mode answers for free. |
| Row copy/paste as TSV | **assembled** | synthetic-input | `ctx.copy_text(String)` and `Event::Paste(String)` are built-in and reach the real system pasteboard (verified with `pbcopy`/`pbpaste`). **Trap**: egui-winit converts ⌘C/⌘V into `Event::Copy` / `Event::Paste` *before* egui sees a key press, so `consume_shortcut(COMMAND, Key::C)` never fires — you must match the semantic event. Cost me one debugging cycle. |
| Accessibility labels (verified dump) | **built-in** (AccessKit) — but the standard probe lies | observed (`ax-dump.txt`) | eframe enables `accesskit` by default and the tree is genuinely rich: 225 nodes, `AXTextField` per cell with `Title` from the column header (via `Response::labelled_by(header_label.id)`) and `Value` = the displayed text, `AXPopUpButton`, `AXSlider` + `AXIncrementor`, `AXButton "Save"`, `AXCheckBox`, `AXStaticText` for every total and error. **But** the probe SPEC-10 suggests — `osascript … get entire contents of window 1` — returns only 5 elements (title-bar buttons + title): System Events does not descend into the AccessKit subtree on the winit NSView. A direct `AXUIElementCreateApplication` walk (`evidence/axdump.swift`) sees everything. *Verifier note (2026-08-30): only from the second walk onwards — the AccessKit tree is built lazily on the first AX request, so the first `axdump` run returned 98 nodes (menu bar + window chrome, zero `AXTextField`) and the second/third 224 nodes with 48 titled text fields. Run the walker twice, as `apps/xilem-ledger` already notes for masonry.* Any corpus row that used the osascript probe alone would score egui *not-achievable* here, wrongly. One real gap: the Reimbursable `Checkbox` has an empty label and its `labelled_by` did not surface as an `AXTitle`, so that control is exposed unnamed. |
| IME composition (optional) | **not-verified** | not-verified | Only ABC and Russian layouts are installed on this machine; adding a CJK input source changes global state on a desktop shared with eight agents. Unrelated but worth recording: egui's bundled font has **no Cyrillic and no Greek**, so text typed on the Russian layout renders as tofu — a localized business app must ship its own font. |

## Helper crates

- `egui_extras = 0.35.0` — `TableBuilder` (virtualized rows, resizable columns).
  Same repo as egui, so the version tracks it exactly.
- `rust_decimal = 1.39.0` — exact base-10 money. `f64` cannot represent `0.01`,
  and this spec's whole point is that `Qty × Unit price` must round-trip to two
  places; `Decimal::round_dp(2)` and exact `Sum` make the totals audit-clean.
- **Rejected**: `icu`/`num-format` (locale grouping is ~60 LoC and the paste
  heuristic has to be custom anyway); `egui_extras`' `datepicker` feature and
  its `chrono` dependency (a 12-line mask satisfies the spec); `egui-modal`,
  `egui_form` and similar community crates (not needed once you accept that
  validation is just a function of the model).

## Where the time went

~30% the numeric-cell state machine (filter / step / commit / error, and
getting the three not to fight each other). ~15% locale formatting and the
paste heuristic. ~15% table plumbing and the borrow-checker dance of handing
`(&mut Decimal, &mut String, &mut Option<String>)` out of one `self.rows[i]`
into a helper. ~25% scripted macOS verification on a desktop shared with eight
sibling agents. ~15% two traps that each cost a debugging cycle:
`Response::has_focus()` and the ⌘C event translation (both below).

## Surprises

- **Bad, and the biggest one**: `Response::has_focus()` is
  `input.focused && memory.has_focus(id)` (`response.rs:343`) — it is **false
  whenever the OS window is not frontmost**. Gating arrow-key stepping on it
  silently disabled the feature for every background/scripted run and made the
  self-test flaky in a way that looked like an event-injection bug. Keyboard
  focus *inside* the app is `Memory::has_focus(id)`; `Response::has_focus` is
  "focused **and** the user is looking at us".
- **Bad**: `InputState::modifiers` is a separate `RawInput` field, not derived
  from the key events. A synthesised ⇧↑ carries shift on the event but not in
  `i.modifiers`, so ⇧-stepping "worked for users and failed in tests". Reading
  the modifiers off the `Event::Key` is both correct and testable.
- **Bad**: ⌘C/⌘V never arrive as key presses (egui-winit rewrites them into
  `Event::Copy`/`Event::Paste`), so the obvious `consume_shortcut` is dead code.
- **Bad**: no OpenType feature API at all, even though the shaper underneath is
  HarfRust and would take one. Tabular figures are only reachable by choosing a
  monospaced face — i.e. you cannot have proportional text *and* tabular digits.
- **Good**: `App::raw_input_hook` makes an app self-testable with *real* key
  events; combined with `Memory::move_focus`/`focused()` the whole keyboard
  story (Tab order, Enter-down, arrow stepping, blur/commit) is testable in
  process with no OS automation. 26 of the checks in `selftest-log.txt` are
  that, and they are the only reason this app's keyboard behaviour is
  trustworthy on a contended desktop.
- **Good**: immediate mode makes computed columns, totals and validation
  literally free — they are functions, called every frame, and 12 rows cost
  nothing. No dirty flags, no observers, no stale state. This is the dimension
  where egui beats a retained toolkit outright.
- **Neutral**: the AccessKit tree is much better than the osascript probe
  suggests. Anyone measuring Rust GUI accessibility with System Events alone
  will under-report every winit-based toolkit.

## Number model

**The model holds `Decimal`; the widget holds a `String`; every cell owns
both.** egui's only text input is `TextEdit`, which is `&mut String` and has
no mask, no validator, no numeric variant and no way to veto a keystroke — the
widget hands back the already-edited string and your only hooks are
`response.changed()` (it changed) and `response.lost_focus()` (they left). So
each row carries `price: Decimal` (the committed truth), `price_buf: String`
(what is on screen) and `price_err: Option<String>` (why they disagree), and
the three transitions are: *filter* the buffer in place on `changed()` so it
can still become a number; *step* it on an `Event::Key` arrow while
`Memory::has_focus`; *parse and re-format* it on `lost_focus()`, keeping the
old `Decimal` and raising an error if the parse fails. Parsing therefore lives
entirely in the app (`model.rs`), not in the widget, and the locale is a
parameter of both directions.

`DragValue` is the counter-example worth naming: it *is* egui's numeric field,
it keeps `f64` and does its own `String` round-trip internally with
`custom_formatter`/`custom_parser` — but because the parser only runs after
the edit, it can post-validate and never constrain, and its stepping is
hard-coded to unmodified arrows. It is the right widget for "a knob with a
number on it" and the wrong one for a ledger column. `Decimal` never reaches
a widget: it is formatted into a `String` on the way out and parsed back on
the way in, once per commit — which, given that egui re-renders everything
every frame, is also the only place it *could* live without re-parsing 12 rows
sixty times a second.

## Approximated or skipped

- **Font features (`tnum`/`lnum`)** — not achievable; monospaced digits used
  instead (see the table).
- **fr-FR grouping separator** — ASCII space instead of the typographically
  correct U+202F, because epaint renders U+202F at half a space and would
  break the alignment this spec measures.
- **Type-ahead in the Category combo** — implemented, but only rated
  by-construction: the popup cannot be kept open long enough under scripted
  input on a shared desktop.
- **IME** — not verified (no CJK input source installed on the machine).
- **Home/End** — left to `TextEdit`'s own caret handling (built-in), not
  separately verified.
- **Paste into a cell** — ⌘V pastes a whole TSV *row* into the selected row.
  Pasting a single value into a single cell is `TextEdit`'s built-in paste and
  goes through the same filter/commit path.
- LoC is over the ~700 guide (1064 app); the spec has 11 functional clauses and
  the numeric cell alone is ~90 lines.
