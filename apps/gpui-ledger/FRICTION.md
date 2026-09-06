# FRICTION — Ledger (gpui =0.2.2)

Reference: `apps/SPEC-10.md`. Built and verified on macOS 26.5.2 (M4 Pro,
rustc/cargo 1.96.1) against the same pin as `apps/gpui-app`
(`gpui = "=0.2.2"`, feature `runtime_shaders` — see `apps/gpui-app/GAPS.md`).

- `cargo build --release`: clean (only the known transitive `block v0.1.6`
  future-incompat note); `cargo build --release --locked` afterwards succeeds
  unchanged. *Verifier note (2026-08-30): not warning-free — rustc emits
  `warning: unused variable: cx` at `src/main.rs:892`; the build is otherwise
  clean.* Cold clean build of the crate + all deps: ~1 m 30 s, the same as
  every other gpui app here (gpui itself dominates).
- Binary: **5.25 MiB** unstripped (5,510,448 B).
- LoC: **1521** = `src/main.rs` 1381 + `src/cell.rs` 140, of which **212 are
  the self-test**, so ~1309 production. Over the brief's ~700 guide; reported
  as-is. Two structural reasons: gpui's fluent builder style spends roughly
  twice the lines of a markup-shaped framework for the same UI, and every
  control in this spec (text field, dropdown, date mask, slider, checkbox,
  validation, undo, locale formatting) is written from scratch here.
- Helper crates: **none** — `gpui` alone.
- Evidence: `evidence/log.txt` (every command), `evidence/selftest-log.txt`
  (`SELFTEST DONE pass=22 fail=0`, exit 0), `evidence/ax-dump.txt`,
  11 screenshots.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **hand-rolled** | self-test + synthetic-input | gpui ships no text input of any kind. `src/cell.rs` (140 lines) is a byte-caret buffer with Left/Right/Home/End, Backspace/Delete and a per-field `Filter`; a numeric field accepts a digit always, `-` only at offset 0, the locale's decimal separator only once, and the group separator only after the first character — so a value that cannot become a number cannot be typed (self-test `typing_filter`). ↑/↓ step Qty by 1 and price/VAT by 0.01, ⇧↑/⇧↓ by ×10 (`arrow_step`: 2 → +1 → +10 = 13). The caret is drawn *between* two text spans, which gives a moving caret with no text measurement at all. |
| Locale-aware parse + format (toggle live) | **hand-rolled** | self-test + synthetic-input | ~90 lines, no `icu` / `num-format` / `rust_decimal`. `fmt_money` always emits two fraction digits with grouping; `parse_money` strips `$ € £ ¥` and every kind of space, recognises the accounting negative `(12.50)`, and decides which of `.`/`,` is the decimal separator (the last one when both appear, otherwise the locale's unless a 3-digit tail makes grouping the only sane reading). Verified: `1234.5`→1,234.50, `$1,234.56`, `1.234,56 €`, `(12.50)`→−12.50, `1 234,56`@fr, `1,234`@en→1,234.00. The toolbar toggle reformats every cell, the in-progress edit buffer and all three totals in the same frame (03 vs 04 screenshots). |
| Decimal alignment / tabular figures (font-feature request) | **built-in** | self-test + observed | The headline positive. `Styled::font(Font { features: FontFeatures(Arc::new(vec![("tnum",1),("lnum",1)])), .. })` reaches CoreText (`platform/mac/open_type.rs::apply_features_and_fallbacks`), and it measurably works: shaping through gpui's own text system gives `'1'×10 = 63.44 px` vs `'0'×10 = 86.68 px` with the default system UI font, and `86.68 px` for both with `tnum`. Right-alignment + a fixed two-digit fraction + equal advances is all the alignment needs; no per-column measurement, no split integer/fraction cells. Negative amounts render red. |
| Inline validation UI + disabled Save + error summary | **hand-rolled** | synthetic-input | Ratings apply to the app-side code only: `errors()` recomputes `(row, field, message)` from the model on every frame (12 rows — free), the cell paints a red border + red text, the toolbar shows a red error count and greys the Save button, and a summary strip lists `row N · Field — message`. Screenshot 07 shows all three at once. A *parse* failure is separate from a *range* failure: an unparseable commit keeps the previous committed value and flags the cell (self-test `invalid_keeps_previous`). |
| Dropdown with type-ahead | **hand-rolled** | synthetic-input | No combobox in gpui. A styled `div` plus a popup list; Down/Space opens, letters accumulate a type-ahead query that jumps to the first matching entry, ↑/↓ move, Enter commits, Esc closes, and a mouse click selects. The one framework-shaped problem: **gpui has no z-index** — the popup painted *behind* the rows rendered after it until it was wrapped in `gpui::deferred(gpui::anchored().snap_to_window()…)`, which defers a subtree's paint until after all its ancestors. That is a real first-party mechanism, and it is the only reason a popup is possible at all. |
| Date input (masked or picker) | **hand-rolled (masked)** | observed | No date picker and nothing to build one from. A mask in the key handler: digits only, auto-inserting `-` after positions 4 and 7, capped at 10 characters, `-` also accepted explicitly, validated on commit with a leap-year-aware `valid_date` (screenshot 07 shows the failure state). `YYYY-MM-DD` typed straight through works. |
| Slider ↔ numeric field linkage | **assembled** | synthetic-input | The slider is a track `div` with gpui's typed DnD: `.on_drag(VatDrag, invisible_ghost)` plus `.on_drag_move::<VatDrag>` on the same element, which hands the listener *its own bounds* every mouse move, so the value is one subtraction and a clamp (the trick already used by `gpui-grid`'s column resize). It is also a tab stop with ←/→ keys. Both directions verified: Right ×4 moved the thumb and set the field to 24.00 (09), and pasting `$12.50` into the field moved the thumb back (10). |
| Tab order across mixed controls | **built-in** | synthetic-input | `cx.focus_handle().tab_index(n).tab_stop(true)` + `window.focus_next()/focus_prev()` — a real per-frame tab-stop map sorted by index (`src/tab_stop.rs`), covering 74 targets here: the locale button, 12×6 cells, the VAT field, the slider and Save, wrapping at both ends. The `FOCUS …` log in `evidence/log.txt` is the walk. This is the single best-supported form feature in gpui. |
| Enter-moves-down cell navigation | **assembled** | synthetic-input | The framework gives focus handles addressable by `(row, field)`; the spreadsheet convention is four lines (`self.handle(Cell(r±1, f)).focus(window)` after committing). Verified row0 → row1 → row2 → (⇧Enter) row1 with the column preserved. |
| Undo/redo (field-level / form-level) | **form-level, hand-rolled** | self-test + synthetic-input | **Field-level undo does not exist**: gpui has no text widget, so there is no editor undo stack to inherit — even the 836-line `EntityInputHandler` editor in `apps/gpui-tray` has none. Form-level undo is a snapshot stack (`Vec<Row>` + VAT, 12 rows so cloning is free) pushed before every committed change; ⌘Z / ⌘⇧Z, depth 64. Verified round-trip in the self-test and visually in screenshot 08. |
| Live computed columns and totals | **built-in (by re-render)** | observed | There is no reactivity system to fight or to lean on: `render` recomputes Amount per row and Subtotal/VAT/Total from the model every frame, and `cx.notify()` schedules the frame. At 12 rows this is invisible; `apps/gpui-grid` already showed the same immediate-mode pattern holding at 100k rows. Money is `i64` minor units throughout, so the totals identity `total == subtotal + vat` is exact (self-test). |
| Row copy/paste as TSV | **built-in (clipboard) + hand-rolled (format)** | self-test + synthetic-input | `cx.write_to_clipboard(ClipboardItem::new_string(..))` / `cx.read_from_clipboard()`. Verified against the real system pasteboard: ⌘C then `pbpaste` printed `2026-01-08\tTeam dinner\tMeals\t1\t213.90\tyes`. ⌘V dispatches on content — a tab-separated payload replaces the row, anything else goes into the focused field's buffer, which is how `$12.50` reaches the numeric parser. |
| Accessibility labels (verified dump) | **not-achievable** | observed (dump retained) | `evidence/ax-dump.txt`: `count of UI elements of window 1` = **4** — the three traffic lights and the window title. None of the 74 focusable controls exists in the OS accessibility tree. `grep -ril 'accesskit\|NSAccessibility'` over the whole of gpui 0.2.2 (sources + Cargo.toml) returns **nothing**: there is no accessibility implementation, not a partial one. A gpui app is, today, unusable with VoiceOver. Ironically the only accessible UI this corpus' gpui apps produce is the `NSAlert` sheet from `window.prompt` (see `apps/gpui-windows`), because AppKit owns it. |
| IME composition (optional) | **not-achievable as built** | not-verified | The cell editor is a raw `on_key_down` buffer, so a CJK input source would deliver committed characters with no inline composition. This is a *cost* finding rather than a framework gap: gpui does expose the real protocol (`EntityInputHandler`, the same one Zed uses), but the bundled `examples/input.rs` spends 746 lines on it for **one** single-line field. 74 fields made that unaffordable; `apps/gpui-tray/src/editor.rs` (836 lines) is the corpus' proof that it works when you can pay for it. |

## Helper crates

**None.** `gpui = "=0.2.2"` with `runtime_shaders`. Considered and rejected:

- `rust_decimal` — unnecessary: quantities are integers and prices are exact
  cents, so `i64` minor units give exact amounts and totals with no rounding
  policy to argue about (the only rounding in the app is the VAT
  round-half-up, four lines).
- `icu` / `num-format` — the spec needs two locales, two separators and one
  grouping rule; ~90 lines beats a multi-megabyte CLDR dependency and keeps
  the parse rules auditable. A real product with 30 locales should take the
  dependency.
- `unicode-segmentation` — the cell editor moves by `char_boundary`, which is
  correct for everything in this form; grapheme clusters would matter for a
  general-purpose editor (and `gpui-tray`'s does use the crate).
- `gpui-component` (third-party, on crates.io, has an input, a dropdown and a
  table) — not evaluated, per the core-only rule the other gpui apps follow.
  The ratings above therefore measure core gpui, not the ecosystem.

## Where the time went

1. **Two silent double-dispatch bugs**, each of which looked like a framework
   failure until it was isolated:
   - `on_key_down` on a cell **and** on the root div both fire in the bubble
     phase, so every Tab advanced focus twice — the tab order appeared to skip
     every other cell. The root listener alone is correct.
   - gpui **synthesises `ClickEvent::Keyboard`** when Space/Enter is pressed on
     a focused element that has an `on_click`, so buttons and the checkbox are
     keyboard-activatable for free — and handling those keys yourself fires the
     action twice (the locale toggled to fr-FR and straight back on one press).
     A genuinely good built-in with a sharp edge.
2. **Verification on a contended desktop**, not construction. Six sibling
   agents' windows covered the screen; synthetic clicks landed on their apps
   and screenshots caught their windows. Every check below §2 of the log had to
   be re-expressed as keystrokes, sent from inside an AppleScript that
   activates first, and retried until the app's own stdout proved the event
   arrived.
3. **Popup z-order.** The dropdown painted under the rows below it; gpui has no
   z-index and `absolute()` does not lift anything. `deferred()` + `anchored()`
   is the answer and is not obvious from the element list.
4. The parser's separator disambiguation (`1.234,56` vs `1,234` vs `1 234,56`)
   — the only genuinely fiddly piece of pure logic, and the reason the
   self-test has seven parse cases.

## Surprises

- **Good, and unexpected:** `tnum` works, and gpui gives you a way to *prove*
  it works (`window.text_system().layout_line(..).width`) without a screenshot.
  Being able to measure text from application code turned "does the framework
  support font features?" from an opinion into two numbers.
- **Good:** tab order is first-class (`tab_index` / `tab_stop` /
  `focus_next`), which is rare — most immediate-mode-ish toolkits make you
  build the focus ring yourself.
- **Good:** `deferred()` exists at all. Popups over a scrolling table are
  where a lot of hand-rolled UI stacks fall over.
- **Bad:** zero accessibility. Not "partial", not "AccessKit is integrated but
  unlabelled" — the framework has no a11y code, and the dump proves the window
  is an empty box to the OS.
- **Bad:** the text-input hole, for the fourth time in this corpus, is what
  actually sets the price of a business form. Everything expensive here —
  caret, filtering, masking, paste, undo, IME — descends from it.
- **Neutral but notable:** immediate-mode recomputation of every derived value
  each frame is fine at this size and removes an entire class of stale-total
  bugs. There is no data-binding layer to learn, and none to fight.

## Spec items approximated or skipped

- **§2 paste** is handled at the *field* level by ⌘V (verified with `$12.50`);
  a middle-click/drag paste path does not exist.
- **§3** negatives render red; parentheses were not used (red was clearer
  against the tabular columns and the spec allows either).
- **§4 Date** is a mask, not a picker — gpui has no calendar widget and no
  native date control is reachable.
- **§6 focus ring** is a 1 px accent border plus a tinted cell background, not
  a system focus ring (gpui exposes no platform focus-ring drawing).
- **§7 field-level undo** is absent (nothing to inherit); form-level undo is
  what was reached.
- **§10 accessibility** is not-achievable; the dump is retained as proof
  rather than as a pass.
- **§11 IME** not exercised; see the table for why.
- Selection inside a cell (shift-arrows, double-click word select) was not
  implemented — it is the next 200 lines of `cell.rs` and adds nothing the
  spec asks for.
- `LEDGER_KEYLOG=1` prints one line per keystroke; it exists only because the
  shared desktop made "did the key arrive?" the hardest question in the build.

## Number model

**`i64` minor units in the model, `String` at the widget, and parsing lives in
exactly one place in between.** `Row { qty: i64, price: i64 }` with `price` in
cents and `amount = qty * price`; VAT is percent × 100 (`2000` = 20.00 %), and
`vat_amount()` is the only rounding in the program. Nothing is ever an `f64`,
so `total == subtotal + vat` is an identity rather than an approximation, and
the self-test asserts it.

The widget side is unavoidably `String`, because gpui hands you keystrokes and
nothing else: there is no numeric widget to bind to, no "value" property, and
no framework-supplied commit event. The app therefore owns a small state
machine — one `Option<Cell>` buffer for whichever target has focus, filled from
the model when focus arrives and parsed back into the model when focus leaves.
`render` runs `sync_focus()` first, which detects the focus transition by
scanning the focus handles, commits the cell being left and opens a buffer for
the cell being entered; that is the whole "blur" mechanism, and it exists
because gpui's per-element focus events (`Window::on_focus_in/out`) are
subscription-shaped and awkward to hold for 74 handles.

Can the input be **constrained** at all, or only post-validated? Constrained —
but only because the app writes the constraint. `Cell::insert` consults a
`Filter` before every character, so `Filter::Number { decimal, group }` makes
an unparseable value untypeable rather than merely invalid; the price cell will
not accept a letter or a second decimal separator at all. That is the strongest
form of the guarantee, and gpui neither helps nor hinders: it is a `String` and
a `match` in application code. Post-validation still exists on top, for what
filtering cannot express — ranges, required fields, real dates — and for pasted
text, which bypasses the filter by design so that `$1,234.56` can be pasted and
then normalised. The honest summary is that gpui has no number model; it has a
key event, and everything above it is yours.
