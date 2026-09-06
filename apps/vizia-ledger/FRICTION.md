# FRICTION — Ledger (vizia =0.4.0)

Reference: `apps/SPEC-10.md`. Built + verified on macOS 26.6.2 (M4 Pro, rustc
1.96.1): `cargo build --release` and `cargo build --release --locked` both
clean, no warnings; window pixel-verified, self-test
`SELFTEST DONE pass=30 fail=0`, exit 0. Evidence in `evidence/`
(`log.txt` with the command for each step, 10 screenshots, `focus-walk.txt`,
`ax-dump.txt`, `selftest-log.txt`).

Clean release build **34.6 s**; binary **24,686,096 B (23.5 MiB)**.
`src/main.rs` is **1280** lines: ~840 production Rust (of which ~110 are the
locale-aware parse/format/filter helpers), 245 self-test + pose hook, 35 CSS.

Evidence labels: **observed**, **synthetic-input** (CGEvent keys/clicks, shots
retained), **self-test** (`LEDGER_SELFTEST=1`, output retained),
**source-only**, **not-verified**.

> Verification note: four sibling agents were driving synthetic input on the
> same display, so all keyboard evidence is posted with `CGEventPostToPid`
> (`apps/vizia-windows/evidence/postclick.swift`), which reaches this process
> regardless of z-order, and screenshots are per-window (`screencapture -l`).

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **hand-rolled** | synthetic-input + self-test | **vizia cannot refuse a keystroke.** `Textbox::validate(\|v\| bool)` looks like input filtering and is not: it runs *after* the edit, toggles the `:valid`/`:invalid` pseudo-classes and makes `on_submit` a no-op while invalid. Filtering is therefore an application-level `on_edit` that re-writes the bound `Signal<String>` with the previous accepted text when the new text fails `typable()`. That revert genuinely works — typing `9a9` into a price cell leaves `99` (`10-filter-rejects-letter.png`) — but it is a re-render of the whole value from the signal rather than a refusal of the keystroke, so it is post-hoc by construction and a rejection in the middle of a value can move the caret (not exercised). Step keys are also hand-rolled but *cheap*: single-line `Textbox` explicitly ignores ArrowUp/ArrowDown and does not consume `KeyDown`, so a `Model` at the root sees them, looks up `cx.focused()` in its own entity→cell map and steps ±1 / ±0.01 (±10 / ±0.10 with Shift). Home/End are the Textbox's own line-start/line-end. vizia *does* ship a `Spinbox` with a built-in `Keymap` for Up/Down/Home/End, but it is `f64`-only and renders its own +/− buttons, so it cannot carry a `Decimal` cell. |
| Locale-aware parse + format (toggle live) | **hand-rolled** | synthetic-input + self-test | ~110 lines: `parse_money` (currency symbols stripped, `(12.50)` accounting negative, grouping/decimal disambiguated by "the last separator wins, otherwise ≤2 trailing digits means decimal"), `fmt_money` (always 2 dp, locale group + decimal separator) and `typable`. vizia has a real localization layer — `Environment::locale` seeded from `sys_locale`, Fluent bundles, and `number_with_fraction(f64, digits)` / `percentage(..)` helpers that produce locale-formatted numbers — but it is `f64`-based, wired to `Localized` message ids rather than to a `Textbox`, and has no parser at all, so it cannot round-trip an editable money cell. The toggle is live in both directions (`Cmd+L`): `1,234.50` ⟷ `1 234,50` (U+202F), footer, VAT field and all 12 rows re-render together (`03-blur-formatted-en-us.png`, `04-locale-fr-fr.png`). |
| Decimal alignment / tabular figures (font-feature request) | **hand-rolled** (font features **not-achievable**) | observed | **vizia 0.4 has no OpenType feature API.** `TextModifiers` exposes `font_family`, `font_weight`, `font_slant`, `font_width` and `font_variation_settings` (variable-font axes); there is no `font-feature-settings` property and the string `tnum` does not appear anywhere in `vizia_style`, even though text goes through Skia's `SkParagraph`, which supports features. Equal-advance digits therefore come from a monospaced face (`font-family: "SF Mono", "Menlo", "Courier New", monospace`), combined with `text-align: right` and an always-2-decimal formatter — which puts the separator at a fixed offset from the right edge, so `48.20`, `1,250.00`, `27.45` and `-312.50` line up (`01-main.png`). Negative amounts are red via `toggle_class("neg", ..)`. |
| Inline validation UI + disabled Save + error summary | **built-in** (UI) / **assembled** (the rest) | synthetic-input | `Textbox::validate` gives the `:invalid` pseudo-class for free, so the red border is two lines of CSS (`.row textbox:invalid { border-color: #d33a2f; border-width: 2px }`) — verified by typing `0` into a Qty cell (`06-inline-invalid.png`), and the typing filter itself is shown in
`10-filter-rejects-letter.png`. `Save` is `.disabled(errors.map(\|e\| !e.is_empty()))` with a `Memo`-driven label ("Save (2 errors)"), and the summary is a `Binding` over the same `Memo` (`05-validation-errors.png`). One caveat: `:invalid` is only recomputed on `InsertText`/`DeleteText`/`EndEdit`, so a value changed *programmatically* leaves the border stale — the app-level error list is the reliable channel. |
| Dropdown with type-ahead | **built-in** | observed + self-test | `ComboBox::new(cx, list, selected)` is a `Textbox` + filtered popup: typing narrows the list, ↑/↓ move the highlight, Enter selects, and it sets `Role::ComboBox` + `active_descendant` for accessibility on its own. `on_select(\|cx, index\|)` is the only wiring. |
| Date input (masked or picker) | **assembled** — both | synthetic-input + self-test | A hand-rolled mask (digits only, `-` auto-inserted after `YYYY` and `MM`, `validate` = `NaiveDate::parse_from_str(.., "%Y-%m-%d")`) as the `Dropdown` trigger, with vizia's **built-in `Calendar` view** as the popup — a genuine month grid with keyboard navigation and `on_select(\|cx, NaiveDate\|)`. `Calendar` is generic over `chrono::Datelike`, and vizia_core already depends on chrono, so naming the type costs nothing. Self-test `date_picker_commit "2026-06-15"`. |
| Slider ↔ numeric field linkage | **assembled** | self-test | `Slider::new(cx, vat).range(0.0..25.0).step(0.5f32).on_change(..)` is built-in (a `.step` literal needs an explicit `f32` — `SliderModifiers::step` is `Into<f32>`, which `{float}` does not satisfy). Both directions are ~6 lines through the model: `slider_to_field vat_text="7.50"` and `field_to_slider vat=12.5` (the latter from typing `12,5`, i.e. through the locale parser). |
| Tab order across mixed controls | **built-in** | synthetic-input | Nothing to write. Every view sets `navigable(true)` and vizia's `Code::Tab` handler walks the entity tree (`focus_forward`/`focus_backward`, `lock_focus_within` for popups). 14 Tab presses from a cold start walked Date → Description → Category → Qty → Unit price → Reimbursable → next row, in reading order — logged from vizia's own `WindowEvent::FocusIn` in `evidence/focus-walk.txt`. The focus ring is the theme's `*:focus-visible { outline-width: 3px; outline-color: var(--ring) }`. |
| Enter-moves-down cell navigation | **hand-rolled** | self-test | `Textbox` maps Enter to `TextEvent::Submit(true)` and does not consume the `KeyDown`, so the root model gets it, commits the row and calls `cx.with_current(next_entity, \|cx\| cx.focus())` on the same column one row down (⇧Enter goes up, both wrap). Needs an application-maintained `(row, field) ⟷ Entity` map, registered from each cell's `on_build`, because vizia has no "which cell is this" notion. Self-test `enter_moves_down` / `shift_enter_moves_up`. Esc reverts the uncommitted edit through `Textbox::on_cancel`, which *is* built in. |
| Undo/redo (field-level / form-level) | **form-level only**; field-level **not-achievable** | self-test | `Textbox` has no undo stack at all — its `KeyDown` arms cover ⌘A/⌘C/⌘V/⌘X and there is no `Code::KeyZ` anywhere in `textbox.rs`, so ⌘Z inside a field does nothing in any vizia app. Form-level undo is a `Vec<Vec<Row>>` snapshot stack pushed on every commit: ⌘Z / ⌘⇧Z, verified both by the self-test and live (`09-undo.png` restores `48.20` after a pasted `1,234.56`). |
| Live computed columns and totals | **built-in** | observed + self-test | `Memo::new(move \|_\| ..)` over `rows` / `vat` / `loc` for Amount, Subtotal, VAT and Total. Fine-grained: a committed cell edit re-renders only the labels that read the changed signals. No lag at 12 rows (and `apps/vizia-grid` measured the same machinery at 100k rows). All money is `rust_decimal::Decimal`, so `qty × unit` and the VAT split are exact. |
| Row copy/paste as TSV | **built-in clipboard**, **hand-rolled** row semantics | self-test | vizia re-exports a text clipboard on `EventContext` — `cx.set_clipboard(String)` / `cx.get_clipboard()` (copypasta under the `clipboard` default feature) — so no `arboard` was needed. The collision is the interesting part: the focused `Textbox` handles ⌘C/⌘V *first* and queues its own `TextEvent::Copy`/`Paste`, so a row-level handler that runs immediately gets overwritten. Fix: `cx.schedule_emit(Msg::CopyRow, now + 30 ms)` so the row action lands after the widget's. Self-test round-trips `2026-01-08\tConference ticket\tTraining\t2\t1,249.91\tyes` and pastes `$1,234.56` back in through the locale parser. |
| Accessibility labels (verified dump) | **built-in** | observed (dump retained) | AccessKit is a **default feature** and the modifiers are first-class: `.name(..)`, `.role(..)`, `.text_value(..)`, `.numeric_value(..)`, `.labeled_by(..)`, `.live(..)`. Setting `.name()` + `.role()` on each cell produced a 160-element macOS AX tree with `text field row 1 date`, `incrementor row 1 unit price`, `checkbox row 1 reimbursable`, `slider VAT percent slider`, `pop up button` for the ComboBox, `button Save` — `evidence/ax-dump.txt`. **Trap worth publishing: the first AX query returns an essentially empty app** (window + traffic lights). vizia_winit only sets `adapter_initialized` when accesskit_winit reports `InitialTreeRequested`, and the tree is pushed on the following frame, so a single-shot probe makes vizia look like it has no a11y tree. Query twice. |
| IME composition (optional) | **not-verified** (present by construction) | source-only | No CJK input source was switched on. The plumbing is all there: every window calls `set_ime_allowed(true)`, vizia_winit forwards `Ime::{Enabled,Preedit,Commit,Disabled}` as `WindowEvent::{ImeActivate,ImePreedit,ImeCommit}`, `Textbox` keeps a `preedit_backup`, implements `update_preedit`/`clear_preedit`, refuses `InsertText` while composing, and reports the caret area back with `SetImeCursorArea`. |

## Traps found

1. **Tabbing out of a `Textbox` does not commit.** `Textbox`'s
   `WindowEvent::FocusOut` arm emits only `TextEvent::EndEdit` — never
   `TextEvent::Submit` — and `on_blur` is driven by `TextEvent::Blur`, which
   *nothing in the framework emits*. So `on_submit(cx, text, enter=false)`
   fires when you click elsewhere (mouse-down path) but not when you Tab, and
   `on_blur` effectively never fires at all. SPEC-10's "on blur the value is
   normalised and formatted" cost a `WindowEvent::FocusOut` handler on the root
   model that maps `meta.target` back to a cell and commits it. This is the
   single highest-value finding in the app: without it, typing `1234.5` and
   pressing Tab silently leaves `1234.5` in the cell and 48.20 in the model.
   > **Verifier note (2026-08-30):** the behaviour is confirmed (FocusOut → `EndEdit` only,
   > `textbox.rs:1095-1097`), but the mechanism is stated too strongly: `TextEvent::Blur` *is*
   > emitted by the framework — the Textbox's own build-time listener emits it when a mouse-down
   > lands outside the editing textbox (`textbox.rs:188-196`), and the `Blur` arm runs `on_blur`
   > **or else** `Submit(false)` + `EndEdit` (:1538-1544). So the click-elsewhere commit happens
   > *via* `TextEvent::Blur`, Tab commits nothing, and `on_blur` is only "dead" until set —
   > setting it replaces (suppresses) the click-away auto-commit rather than adding to it.

2. **`Handle::class("a b")` does not split on whitespace.** It is
   `class_list.insert(name.to_string())`, so a space-separated list becomes one
   class literally named `"a b"` that matches nothing. The column widths and
   the monospaced-digit style both silently vanished until the calls were split
   (`.class("num")` + an inline width).
3. **`validate` is post-validation, not a filter** (see the table) — and it is
   only re-run on keystrokes and `EndEdit`, so `:invalid` goes stale when the
   bound signal is changed from code.
4. `Signal::set` does **not** compare before notifying (there is a separate
   `set_if_changed`), which is what makes the reject-by-rewrite trick work at
   all: writing the identical string back still re-runs the `Textbox`'s value
   binding and wipes the just-inserted character.
5. **`Slider::step(0.5)` does not compile** — `Into<f32>` is not implemented
   for `{float}`; it must be `0.5f32`.
6. `on_submit`'s second argument is `true` for Enter and `false` for a
   commit-by-focus-loss. vizia's own todo example names it `blur`, which reads
   as the opposite of what it means.
7. Inherited from `apps/vizia-board`: action modifiers only fire on the
   *hovered* entity, so every non-interactive `Label` here is `.hoverable(false)`.

## Helper crates

* **`rust_decimal = "=1.42.1"`** (`default-features = false`, `std`) — exact
  money. `f64` cannot represent `0.01`; with 12 rows × (qty × unit) plus a VAT
  split, a float subtotal would not tie out. Only add/mul/round/parse are used.
* **`chrono = "=0.4.45"`** — `NaiveDate` for the Date column. vizia_core
  **already** depends on chrono (its `Calendar` view is generic over
  `chrono::Datelike`), so this is the same version already in the tree and
  changes nothing about the build; it only makes the type nameable.

Tried and rejected: **`arboard`** — unnecessary, `EventContext::{get,set}_clipboard`
is in vizia's default `clipboard` feature. **`icu` / `num-format`** — the whole
locale surface needed here is two separator characters and a parser, and a full
ICU data bundle would have dwarfed the app. **`Spinbox`** — built in, but
`f64`-only with its own +/− chrome. **`VirtualTable`** (used in
`apps/vizia-grid`) — its cells are read-only templates; a 12-row form of live
`Textbox`es is a plain `VStack`.

## Where the time went

1. Trap 1 (Tab does not commit). Everything downstream — blur formatting, the
   locale re-render, live totals — appeared to be broken while the real cause
   was one missing `Submit` in a `FocusOut` arm inside the framework.
2. Trap 2 (`class("a b")`), which produced a table whose header, rows and
   footer were on three different grids and looked like a layout-engine bug.
3. Writing the number model: `parse_money` / `fmt_money` / `typable` and their
   test matrix are about a third of the production code, and *all* of it is
   application code — none of it is anything vizia offers.
4. The rest was fast. Tab order, the `:invalid` border, the ComboBox
   type-ahead, the Calendar picker, the Slider, `Memo`-driven totals and the
   whole accessibility row were essentially free.

## Surprises

* **Good — the accessibility result is the best in this corpus so far.**
  AccessKit is on by default, `.name()`/`.role()` are ordinary modifiers, and a
  12×7 grid of live inputs shows up in the macOS AX tree with correct roles and
  names, with no extra work. Verified, not claimed (`evidence/ax-dump.txt`).
* **Good — Tab traversal, Esc-to-cancel, the focus ring, the `:invalid`
  pseudo-class, a filtering ComboBox and a real `Calendar` are all in core.**
  The "things practitioners take for granted from Qt/WinForms" list is mostly
  satisfied by the widget set, which is not what the todo round suggested.
* **Good — `EventContext::{get,set}_clipboard` in core.** One less crate.
* **Bad — there is no numeric input.** Not a spin box you can bind to a
  `Decimal`, not an input mask, not a `filter` hook. `validate` is a
  *decoration*, and the type parameter on `Textbox<T>` is a `FromStr` round
  trip, not a constraint.
* **Bad — no font-feature API**, so "tabular figures" means "pick a monospaced
  font" and you lose the UI face in the numeric columns.
* **Bad — `on_blur` is dead code** in 0.4: nothing emits `TextEvent::Blur`.
  *(Verifier note (2026-08-30): overstated — the Textbox's click-away listener does emit
  `Blur`; see the correction under trap 1.)*
* Neutral: `Textbox` not consuming `KeyDown` is what makes ↑/↓ stepping and
  Enter-moves-down implementable at all — a happy accident rather than a design.

## The "number model" paragraph

**Between the widget and the model the value is a `String`, and there is no way
to make it anything else.** `Textbox::new(cx, sig)` is generic over
`T: FromStr + ToString`, so it *looks* like you can bind a `Signal<Decimal>`
directly — but the edit path stores text, re-parses it with `text.parse::<T>()`
on every `InsertText`/`DeleteText`, and simply flips `:invalid` when the parse
fails; a value that cannot round-trip through `to_string()`/`parse()` (like a
grouped, locale-formatted `1 234,50`) is unusable that way. So this app binds
`Signal<String>` *drafts* and keeps `rust_decimal::Decimal` in the committed
`Signal<Vec<Row>>`. **Parsing lives entirely in the application's `Model`**:
`on_edit` applies the typing filter and can only enforce it by writing the
previous text back to the signal (post-hoc, and it moves the caret);
`on_submit`/`FocusOut` parse and re-format; `Memo`s derive Amount, Subtotal,
VAT and Total from the `Decimal`s. The framework's contribution to the numeric
story is exactly three things — the `:valid`/`:invalid` pseudo-classes, the
suppression of `on_submit` while invalid, and the fact that it does not eat the
arrow keys. **The input cannot be constrained, only post-validated**, and the
only place a number exists as a number is on the far side of a hand-written
parser.

## Approximated or skipped

* **Filtering while typing** is a revert-after-the-fact, not a true keystroke
  filter, and it resets the caret to the end of the field. vizia offers no
  pre-edit hook to do better.
* **Field-level ⌘Z** — *not-achievable*; `Textbox` has no undo stack. Only
  form-level undo/redo is implemented.
* **⌘C/⌘V** are implemented as specified but have to be deferred 30 ms to win
  against the focused `Textbox`'s own clipboard handling; a production app
  would more likely use a distinct chord.
* **IME** (SPEC-10 item 11, optional) — not exercised; recorded as present by
  construction with the code path named.
* **`Cmd+L`** was added as a shortcut for the Locale toggle purely so the
  locale switch could be driven deterministically on a contested desktop; the
  toolbar buttons do the same thing.
* The `(row, field) → Entity` map that Enter-moves-down and the arrow stepping
  need is application state built from `on_build` callbacks; vizia has no
  concept of a "cell" and `cx.focused()` returns a bare `Entity`.
