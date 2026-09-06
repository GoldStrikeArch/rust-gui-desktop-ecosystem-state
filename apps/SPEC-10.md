# SPEC-10: "Ledger" — forms & numeric-input test

Iteration 5 (proposed 2026-08-30, prompted by the same r/rust comment:
"applications for accounting, engineering, industrial automation need fields
containing numbers, not alphanumeric strings … the ergonomics of numeric
input fields, like vertical alignment over the decimal separator"). SPEC.md
tested a text field and a list; nothing in the corpus has tested typed
input, validation, tab order or tabular numbers. This is the
business-forms dimension.

Kept separate from the todo app on purpose: `apps/<fw>-app` is the baseline
for every build-time/binary-size/dependency measurement, and changing it
would invalidate iteration 1.

## Functional requirements

1. **Window** titled `Ledger (<framework>)`, ~820×560, resizable. An expense
   table with 12 seed rows and columns: Date, Description, Category,
   Qty, Unit price, Amount (= Qty × Unit price, computed), Reimbursable
   (checkbox). A footer row shows Subtotal, VAT and Total.
2. **Numeric fields** (Qty: integer ≥ 0; Unit price: decimal, 2 places,
   may be negative for refunds):
   - typing accepts only characters that can still form a valid number
     (locale-aware: `-`, digits, one decimal separator, optional grouping);
   - on blur the value is **normalised and formatted** (`1,234.56` in
     en-US, `1 234,56` in fr-FR — a Locale toggle in the toolbar switches
     both parsing and display live);
   - ↑/↓ step by 1 (Qty) / 0.01 (price), ⇧↑/⇧↓ by ×10, Home/End as usual;
   - paste of `$1,234.56`, `1.234,56 €` and `(12.50)` parses (currency
     symbol stripped, accounting negative recognised);
   - invalid input shows an inline error (red border + message) and keeps
     the previous committed value.
3. **Decimal alignment**: numeric columns are right-aligned **and** line up
   on the decimal separator across rows (so `12.5` and `1,234.56` align at
   the point). Use tabular figures (`tnum`/`lnum` OpenType features or a
   monospaced-digit face) so digits have equal advance — record whether the
   framework can request font features at all. Negative amounts render in
   red or parentheses.
4. **Other controls**: Category is a dropdown/combobox with type-ahead;
   Date is a masked input or date picker (record which; `YYYY-MM-DD`
   accepted by keyboard either way); VAT % is a slider 0–25 with a linked
   numeric field (both directions).
5. **Validation & form state**: Description required (1–60 chars), Qty
   1–999, Unit price −99 999.99 … 99 999.99; a **Save** button is disabled
   until every row is valid and shows a count of errors; an error summary
   lists row/field.
6. **Keyboard navigation**: Tab/⇧Tab visits every editable cell and control
   in reading order (including checkbox, dropdown, slider); **Enter moves
   down** within a column (spreadsheet convention) and ⇧Enter moves up; the
   focus ring is visible; Esc in a cell reverts the uncommitted edit.
7. **Undo/redo**: ⌘Z/⌘⇧Z at least inside the focused text field
   (framework-provided?), and — if achievable — form-level undo of the
   last committed cell edit. Record which level was reached.
8. **Live totals**: any committed change recomputes Amount, Subtotal, VAT and
   Total; a 12-row table must recompute without visible lag.
9. **Clipboard**: ⌘C on a row copies it as tab-separated text; ⌘V of a
   TSV row into an empty row fills it (best-effort; record).
10. **Accessibility**: every field exposes a name/label and value to the
    OS accessibility tree (AccessKit or native). Verify with macOS
    Accessibility Inspector or `osascript -e 'tell application "System
    Events" to get entire contents of window 1 of process "<name>"'` and
    retain the dump; frameworks without an a11y tree record
    *not-achievable* for this row.
11. **IME (optional, record)**: switch to a CJK input source and type into
    Description; composition must be visible inline and commit correctly.

## Implementation rules

Same as SPEC-3: independent crate at `apps/<framework>-ledger/` (package
`<framework>-ledger`), same pinned framework version as
`apps/<framework>-app/`, Rust helper crates allowed and recorded
(`rust_decimal`, `icu`/`num-format`, masked-input crates, framework widget
extras), no external JS libraries for webviews, fallback rule applies.

Verification on macOS (retain the evidence): synthetic keystrokes
(`System Events`) typing `1234.5`, Tab, and a screenshot showing the
formatted `1,234.50`; the same after the Locale toggle; a screenshot of the
decimal-aligned column; a Tab-order walk logged from focus events; the
accessibility dump.

## FRICTION.md (required, per app)

Rating (built-in / assembled / hand-rolled / not-achievable) + short note for
each capability:

| Capability |
|---|
| Numeric field (typed, filtered input, step keys) |
| Locale-aware parse + format (toggle live) |
| Decimal alignment / tabular figures (font-feature request) |
| Inline validation UI + disabled Save + error summary |
| Dropdown with type-ahead |
| Date input (masked or picker) |
| Slider ↔ numeric field linkage |
| Tab order across mixed controls |
| Enter-moves-down cell navigation |
| Undo/redo (field-level / form-level) |
| Live computed columns and totals |
| Row copy/paste as TSV |
| Accessibility labels (verified dump) |
| IME composition (optional) |

Also: helper crates used + why, total LoC, where the time went, surprises,
and a **"number model" paragraph**: what type carries the value between the
widget and the model (`String`, `f64`, `Decimal`), where parsing lives, and
whether the framework's text input can be constrained at all or only
post-validated.

## Why this spec

Business apps are forms. The todo round showed every framework has *a* text
input; this round measures whether that input can be a **numeric** input:
filtering while typing, locale, formatting on blur, arrow stepping, and
digits that line up. It also measures the things around a form that
practitioners take for granted from Qt/WinForms — tab order, Enter
navigation, validation state, undo — and puts the first accessibility
*verification* (not just "AccessKit integrated") into the corpus.

## Reference machine

Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1 (same pins as
iteration 1); Linux/Windows reruns follow the round-5/round-6 harnesses.
