# FRICTION — Ledger (iced =0.14.0)

Reference: `apps/SPEC-10.md`. Built and verified on macOS 26.5.2 (M4 Pro,
rustc/cargo 1.96.1). `cargo clean && cargo build --release` in **22.5 s** wall
(warm `~/.cargo` registry); `cargo build --release --locked` afterwards
succeeds unchanged. Binary **10.8 MB**. Source **1435 lines** in one
`src/main.rs` (1226 non-comment) — over the ~700-line guide, of which ~200 are
the scripted self-test and ~150 the locale parser/formatter.

Evidence in `evidence/`: `selftest-log.txt` (`LEDGER_SELFTEST=1`, **pass=29
fail=0**, exit 0), `log.txt` (every synthetic-input step with the command
used), `tab-walk.txt`, `ax-dump.txt`, four screenshots. **Read the header of
`evidence/log.txt` first**: this run shared a desktop with other agents driving
GUI apps, whose clicks and keystrokes repeatedly landed in this process.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **assembled** | self-test + synthetic-input | iced has no numeric widget, but `text_input` is a *controlled* widget: `on_input` hands you the candidate string and the widget draws whatever you store. Refusing the candidate means the character never appears, so `partial_number()` is real input filtering, not post-validation (`04-validation.png` status line: "rejected keystroke in row 1 price"). ↑/↓ stepping by 1 / 0.01 and ⇧↑/⇧↓ ×10 is hand-rolled from `event::listen_with` — `text_input` ignores Up/Down (it *does* handle Home/End and Left/Right natively). |
| Locale-aware parse + format (toggle live) | **hand-rolled** | synthetic-input | Nothing in iced or its dependency tree knows about locales. ~150 lines: a tolerant parser that strips currency symbols and spacing, understands the accounting negative `(12.50)`, and decides which of `.`/`,` is the decimal separator from the string itself (so `$1,234.56`, `1.234,56 €` and `(12.50)` all parse in either locale), plus a grouping formatter. The toggle re-expresses every stored string, including the totals and the VAT field (`03-locale-fr.png`). `icu`/`num-format` were considered and skipped: the formatting is ~40 lines and the parsing tolerance the spec asks for is not something either crate does. |
| Decimal alignment / tabular figures (font-feature request) | **assembled; font features not-achievable** | observed | `iced::Font` is `{ family, weight, stretch, style }` — full stop. There is **no OpenType feature list anywhere in iced 0.14**, so `tnum`/`lnum` cannot be requested even though cosmic-text/HarfRust underneath could apply them. `text::Shaping` is the only shaping knob and its three values (Auto/Basic/Advanced) are about script complexity and fallback, not features. The workaround is `Font::MONOSPACE` (equal digit advance by construction) plus right alignment plus an invariant that every money string carries exactly two fraction digits — with those three, `5.99`, `482.50` and `1,295.00` line up on the point (`01-ledger.png`). Negative amounts render as red `(34.75)`. |
| Inline validation UI + disabled Save + error summary | **assembled** | synthetic-input | `text_input.style(|theme, status| { let mut s = text_input::default(theme, status); s.border = s.border.color(red).width(2.0); s })` — starting from the default style and overriding one field is the pleasant part of iced's styling API. `button::on_press_maybe(None)` is the built-in disabled state. `04-validation.png`: two red borders, "Save (2 errors)", and a summary naming row and field. |
| Dropdown with type-ahead | **built-in** | observed | `combo_box` + `combo_box::State` does exactly this (a `pick_list` would have been the no-type-ahead version). Cost: one `combo_box::State<Category>` per row held in the app, and `State::with_selection` has to be rebuilt when the selection changes programmatically. |
| Date input (masked or picker) | **hand-rolled (mask)** | self-test | No date widget and no date crate in the tree. `partial_date()` accepts only digits in the eight digit slots and `-` in slots 4 and 7, capped at ten characters, and `valid_date()` range-checks year/month/day. There is no calendar popup — iced has no date picker, first-party or otherwise, and none was pulled in. |
| Slider ↔ numeric field linkage | **built-in (one direction) / hand-rolled (the other)** | observed | `slider(0..=25u8, value, Message::VatSlider)` and a `text_input` both write the same `vat: String`, so the two stay in sync for free. The keyboard half is not free: a `slider` cannot be focused (see below), so ←/→ adjustment only works because the app tracks `Focus::Slider` itself. |
| Tab order across mixed controls | **hand-rolled (iced cannot do it)** | synthetic-input | **The sharpest finding of this app.** In iced 0.14 only `text_input` and `text_editor` implement `Widget::operate` with `operation.focusable(..)` (*verifier note (2026-08-30): `scrollable` was listed here too, but `iced_widget-0.14.2/src/scrollable.rs` only implements `operation.scrollable(..)`, never `focusable`*); `button`, `checkbox` and `slider` do not, and `combo_box` has no `.id()` at all. So `operation::focus_next()` can only ever reach text inputs — a checkbox, a dropdown or a slider is unreachable by keyboard through iced's own focus chain. This app keeps its own `Focus` cursor over all 75 stops, mirrors it into iced with `operation::focus(id)` for the text cells, and draws its own focus ring for the rest (visible around the Reimb checkbox in `02-typed-1234.5-tab.png`). Two more traps found on the way: (a) `operation::focus` on an id that does not exist silently no-ops and leaves the previous input focused, so leaving a text cell for a checkbox needs `focusable::unfocus()`; (b) `unfocus()` and `find_focused()` are only reachable behind iced's **`advanced` feature flag** (`iced::advanced::widget::operate`), while the unflagged `iced::widget::operation` exposes focus/focus_next/is_focused but not those two. |
| Enter-moves-down cell navigation | **hand-rolled** | self-test | `text_input` only consumes Enter if `on_submit` is set, so leaving it unset lets `event::listen_with` own Enter/⇧Enter and move focus down/up the same column. |
| Undo/redo (field-level / form-level) | **form-level only; field-level not-achievable** | self-test | `text_input` has no undo stack in iced 0.14 (same as `text_editor`, recorded in `apps/iced-tray`), so ⌘Z inside a field does nothing and the key is free for the application. The app therefore implements **form-level** undo/redo: every committed cell edit pushes `(row, field, previous value)`, ⌘Z pops it, ⌘⇧Z redoes. Character-level undo inside one field is simply not reachable without reimplementing the text buffer. |
| Live computed columns and totals | **built-in** | observed | Amount, Subtotal, VAT and Total are computed in `view` from `Decimal`; 12 rows recompute on every keystroke with no perceptible cost (`/tmp/l1.png` shows the Amount column already at 1,234.50 while the cell still holds the raw "1234.5"). Elm-style re-render makes this the easy row. |
| Row copy/paste as TSV | **assembled** | self-test | `iced::clipboard::read/write` are `Task`s and compose fine. ⌘C/⌘V could **not** be used: `text_input` claims them for its own selection before the application sees anything useful, so the row-level operations are bound to ⌘⇧C/⌘⇧V. Recorded as the "best-effort" the spec allows. |
| Accessibility labels (verified dump) | **not-achievable** | observed (dump retained) | `evidence/ax-dump.txt`: the entire AX contents of the window are the three traffic-light buttons and the title's static text. Not one of the 48 text inputs, 12 combo boxes, 12 checkboxes, the slider or the Save button is exposed. This is not a wiring mistake — AccessKit does not appear anywhere in `iced-0.14.0` or `iced_winit-0.14.0`'s manifests: no dependency, no feature, nothing. |
| IME composition (optional) | **not-verified** | not-verified | `iced_widget`'s `text_input` does plumb `core::input_method` (there is an `InputMethod` type and `text_input` reports a pre-edit region), so the mechanism exists, but no CJK input source was switched to on this shared machine. Not claimed. |

## Helper crates

- `rust_decimal =1.39.0` (`default-features = false`, `features = ["std"]`) — exact money arithmetic, `round_dp`, and a `Display` impl that honours `{:.2}`. `f64` cannot represent `0.01`, and this spec is explicitly about accounting input.
- `iced` needed its **`advanced` feature** — not a helper crate, but a feature-flag finding: the two focus operations this app depends on (`find_focused`, `unfocus`) are only reachable through `iced::advanced::widget::operate`.
- Tried and rejected: `icu` / `num-format` for locale formatting (the formatter is ~40 lines and neither crate does the tolerant *parsing* the spec asks for); a masked-input crate (none targets iced 0.14); `pick_list` instead of `combo_box` (no type-ahead).

## Where the time went

1. **Reconstructing focus and blur.** iced has no `on_focus`/`on_blur`, no
   focus query outside the `advanced` feature, and no focusability for half
   the widgets in the spec. Getting "leaving a cell formats it" to work for
   Tab, Enter, Escape *and* a mouse click on another cell took three
   mechanisms: explicit commits on every navigation path, an `is_focused`
   probe after each mouse press to detect click-away, and a `find_focused`
   probe to learn which cell the click landed in.
2. **The tolerant number parser** — deciding which of `.`/`,` is the decimal
   separator from the string rather than from the locale, so that a pasted
   `1.234,56 €` works while the user is in en-US.
3. Everything else was fast. The table, the styling, the computed columns and
   the totals are the part iced is good at.

## Surprises

- **Good:** `text_input` being a controlled widget makes "filter while typing" trivial and *correct* — you never see a rejected character, unlike frameworks where you have to undo it after the fact.
- **Good:** `text_input::default(theme, status)` returns the default `Style` so a custom style can start from it and change one field. Error borders were five lines.
- **Bad:** the focusability gap. A business form is exactly the case where keyboard traversal of mixed controls matters, and iced 0.14 cannot express it at all without an application-level focus model.
- **Bad:** no font-feature API. `tnum` is the standard answer to the spec's own question and iced cannot ask for it.
- **Bad:** zero accessibility. Verified, not assumed.
- **Neutral:** ⌘C/⌘V being claimed by `text_input` is reasonable behaviour, but it means "copy the row" needs a different shortcut.

## Number model

The value that travels between the widget and the model is a **`String`**, and
that is forced by the framework: `text_input`'s `value` parameter is `&str` and
its `on_input` callback hands back a `String`. There is no numeric widget and
no way to bind a `Decimal` to an input. So `Row::qty` and `Row::price` *are*
the edited text, and `Decimal` is produced on demand:

```rust
fn parse_number(input: &str, locale: Locale) -> Option<Decimal>   // text -> value
fn format_number(v: Decimal, places: u32, locale: Locale) -> String  // value -> text
```

Parsing therefore lives in three distinct places, and the difference between
them is the whole story of this spec:

1. **While typing** — `partial_number()`, a *predicate*, not a parser: "can
   this string still grow into a number?". Because iced's text input is
   controlled, returning `false` means the keystroke never becomes the value.
   This is genuine constraint, not post-validation.
2. **On paste** — `on_paste` hands you the resulting full contents, so the
   tolerant `parse_number()` runs there and rewrites the cell (or leaves the
   previous value).
3. **On commit** — every path that leaves a cell (Tab, Enter, Escape, a click
   elsewhere detected by an `is_focused` probe) runs `parse_number` then
   `format_number`, which is what turns `1234.5` into `1,234.50`.

Validation is separate again: `errors()` re-parses every cell each frame to
build the row/field error list, which is cheap for 12 rows and would need
memoising for 12 000. `Decimal` never appears in the widget layer at all — it
exists only between `parse_number` and `format_number`, and for the computed
columns.

## Approximated or skipped

- **IME (spec 11, optional)** — mechanism present in `iced_widget`, not exercised; marked *not-verified*.
- **Date picker** — masked text input only; iced has no calendar widget and none was added.
- **⌘C/⌘V for rows** — moved to ⌘⇧C/⌘⇧V because `text_input` consumes the unshifted pair.
- **Field-level undo** — not achievable; form-level undo/redo shipped instead.
- **Home/End** — left to `text_input`'s own handling (it implements both) rather than re-implemented.
