# FRICTION — Ledger (freya =0.4.0)

Reference: `apps/SPEC-10.md`. Built and verified on macOS 26.5.2 (M4 Pro,
rustc 1.96.1). `cargo build --release` clean from an empty `target/` in
**64.8 s**; `cargo build --release --locked` reproduces with no lockfile
change. Binary **23.3 MB**. LoC **1490** in a single `src/main.rs`, of which
~160 are the `LEDGER_SELFTEST=1` hook and ~90 are the locale-aware
parser/formatter that would be a crate in a real app.

Transitive `freya-*` crates are pinned to **0.4.1** in `Cargo.lock`, matching
the rest of the Freya cohort (a fresh resolution today picks 0.4.3).

Evidence: `evidence/log.txt` (every command), `evidence/selftest-log.txt`
(`SELFTEST DONE pass=22 fail=0`, exit 0), `evidence/ax-dump.txt` (331-element
accessibility tree) and nine screenshots.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **assembled** | self-test + synthetic-input | Freya's `Input` is a text editor, but it has a real *filter*: `on_validate` hands you an `InputValidator` with the prospective text, and `set_valid(false)` makes the widget **undo the keystroke** (`input.rs:456-476` calls `editor.undo()` and clears the redo stack). That is a genuine "you cannot type a letter into this field", not a post-hoc check — the only one in the cohort so far that rejects at the keystroke. Stepping is not built in: ↑/↓ (±1 Qty, ±0.01 price) and ⇧↑/⇧↓ (×10) are handled in a replaced `on_pre_key_down`, which is also the only place they *can* live, because the root's Tab handler would otherwise consume arrows. Home/End fall through to the editor. |
| Locale-aware parse + format (toggle live) | **hand-rolled** | self-test + synthetic-input, 03-/04- | Nothing locale-shaped exists in Freya (no `i18n` number API; the optional `freya_i18n` is message translation). ~90 LoC: a loose parser that strips `$ € £ ¥`, NBSP/NNBSP and accounting parentheses, decides which of `.`/`,` is the decimal separator from their positions, and a grouping formatter. The toolbar toggle re-formats every cell and both totals in one pass. Verified: `1234.5` → `1,234.50`, toggle → `1 234,50`, toggle back → `1,234.50`. |
| Decimal alignment / tabular figures (font-feature request) | **hand-rolled** | observed, 09- | **Freya cannot request font features at all** — there is no `font_features`/`tnum` anywhere in freya-core's text style (`style/` has font_size, font_slant, font_weight, font_width and nothing else), even though the renderer is Skia and `SkTextStyle` supports them. Fallback: `font_family("Menlo")` on the cell rect, which **is inherited into `Input`'s internal paragraph** (nice), plus left-padding the committed text with spaces to a fixed character count. That is only necessary because of the `text_align` bug below; with tabular digits + a fixed field width the decimal points line up in both the editable column and the computed one. Negative amounts render red. |
| Inline validation UI + disabled Save + error summary | **assembled** | synthetic-input, 05- | Validation is a pure function over the cell text; the cell rect adds a red `Border` when it fails, `Button::enabled(false)` disables Save and its label carries the count, and the footer lists `row N · Field: message`. All first-party primitives, no validation framework. |
| Dropdown with type-ahead | **hand-rolled** | synthetic-input, 06-/07- | Freya ships `Select` + `MenuItem`, but `Select` opens only on a pointer press, is not reachable by keyboard as a unit, and has no type-ahead. The Category cell is therefore a focusable rect with `a11y_role(ComboBox)` + `a11y_builder(\|n\| n.set_value(..))`, an overlay list on `Layer::Overlay`, letter type-ahead, ↑/↓ cycling and Escape to close — ~55 LoC. |
| Date input (masked or picker) | **assembled (masked)** | self-test + observed | A masked `Input`: `on_validate` allows only digits with `-` fixed at offsets 4 and 7 and a length ≤ 10, and blur normalises 8 loose digits into `YYYY-MM-DD`. Freya *does* ship a `Calendar` component behind the non-default `calendar` feature; it was not used because the spec asks for keyboard `YYYY-MM-DD` entry, which is the mask. |
| Slider ↔ numeric field linkage | **assembled** | observed | `Slider` is **percentage-only**: `value()` clamps to 0..=100 and `on_moved` yields a percentage, so the 0..25 VAT domain is mapped by hand in both directions. Its keyboard step is a hard-coded 4 % (`slider.rs:130-146`) with no way to change it. The linked `Input` shares the same `Decimal` and both drive the totals. |
| Tab order across mixed controls | **built-in** | synthetic-input | The single best row here. `freya_components::integration` wraps every window's root in an `on_global_key_down` that maps Tab/⇧Tab to `AccessibilityFocusStrategy::Forward/Backward`, so the order is the accessibility-tree order — reading order, for free, across `Input`, the custom ComboBox, `Slider`, `Checkbox` and `Button`. Logged live from `Platform::focused_accessibility_node` (see evidence/log.txt §1). Zero app code. |
| Enter-moves-down cell navigation | **assembled** | synthetic-input | In the replaced `on_pre_key_down`: commit, then `next_row.id(field).request_focus()` (⇧Enter goes up, both wrap). Focus-by-id is first-party (`AccessibilityIdExt::request_focus`), so this is ~10 lines. Verified from the focus log: row 1 → row 2 → row 3 of the *same* column. |
| Undo/redo (field-level / form-level) | **built-in / assembled** | by-construction + self-test | Field level is free: `use_editable`'s rope history implements ⌘Z and — note — **⌘Y, not ⌘⇧Z** (`freya-edit text_editor.rs:669-687`), which is wrong on macOS and cannot be reconfigured. Form level is app code: because ⌘Z belongs to the editor, form undo is bound to ⌘⇧Z (and an "Undo edit" button) over a stack of `(row, field, text-before-focus)` pushed on blur. |
| Live computed columns and totals | **built-in** | self-test + synthetic-input, 03- | Every cell is its own `State<String>`; `amount()` `read()`s the two it depends on, which subscribes the row scope *and* the totals scope, so a committed edit repaints exactly the row plus the footer. No `memo`, no recompute pass. 12 rows re-total with no visible lag. **Trap:** using `peek()` there instead of `read()` silently produces a table that never updates — that bug cost a debug cycle here. |
| Row copy/paste as TSV | **built-in** | synthetic-input, 08- | `freya::prelude::Clipboard::{get,set}` (freya-edit → copypasta) is a static text clipboard, so `arboard` is not needed. Verified with `pbpaste \| od -c`. Bound to ⌘⇧C/⌘⇧V plus toolbar buttons, because plain ⌘C/⌘V belong to the focused editor. |
| Accessibility labels (verified dump) | **built-in, but off by default** | synthetic-input, ax-dump.txt | Every cell carries `a11y_role` + `a11y_alt`, the combobox also `a11y_builder(\|n\| n.set_value(..))`, and the tree that reaches the OS is complete and correctly nested (AXWindow → AXCell "Unit price row 1" → AXTextField "    482.50"), 331 elements. **The finding is the activation**: AccessKit's macOS adapter is lazily activated, and the spec's `System Events … entire contents` recipe returns *only the title bar* on a cold process. Setting `AXManualAccessibility` on the application element (the Chromium/Electron switch) turns it on, after which both that recipe and a raw AXUIElement walk work. Gap: `Checkbox` exposes `role=CheckBox` with **no name** and offers no `a11y_id`/`a11y_alt`, so the name can only be put on the surrounding cell. |
| IME composition (optional) | **built-in (by-construction)** | not-verified | Not exercised. `Input` handles `on_ime_preedit`, keeps preedit out of the undo history, and renders the composing run as an underlined `Span` (`input.rs:403-421, 697-716`), so inline composition exists by construction. |

> **Verifier note (2026-08-30):** the accessibility rating needs a second qualifier: the
> AX **titles are creation-time snapshots that never update**. On the verifier's dump (422
> elements after `AXManualAccessibility`; tree shape exactly as above, checkbox gap confirmed)
> no `AXTextField` exposes an `AXValue` (0/49), a typed-and-committed `1,234.50` never appeared
> in any field's AXTitle, and after a screen-verified locale toggle the same instant's AX walk
> still reported `AXButton Title=Locale: en-US` and en-US prices everywhere. The current text is
> only reachable through nested static-text children (a System Events `entire contents` sweep
> does find edited values there). So: labels yes, live values no — a screen reader sees the
> ledger as it looked when the tree was built.

## The sharpest edge: `Input::text_align(TextAlign::Right)` does not work

Setting it on an `Input` pushes the text out of the widget's viewport: a Qty
cell containing `1` renders **empty**, and `482.50` renders as `482.`. `Input`
lays the text out as

```rust
ScrollView::new().width(Size::flex(1.)).direction(Horizontal).show_scrollbar(false)
    .child(paragraph()
        .min_width(Size::func(|c| Some(c.parent - inner_margin.horizontal())))
        .margin(inner_margin)
        .text_align(self.text_align) …)
```

so the paragraph's box is at least the full parent width *plus* its own
margins, i.e. slightly wider than the scroll viewport; with right alignment
the text sits at the far right of that box and the viewport, which is parked
at scroll offset 0, shows the empty left part. With the default left
alignment nothing is visible because nothing overflows, which is why the bug
is easy to miss. Screenshots of both states were taken while diagnosing it.

Since right alignment is exactly what SPEC-10 asks for, the workaround is to
align the *glyphs* instead of the box: monospaced digits (`font_family` on the
parent rect is inherited into the `Input`) plus left-padding the committed
text with spaces to a fixed character count. It looks right (evidence 09-),
the parser trims the padding, and the TSV export trims it too — but the model
text now carries presentation, which is the sort of compromise a working
`text_align` would make unnecessary.

## Second edge: `Input` swallows every key it does not itself use

The stock `on_pre_key_down` is `Enter|Escape|Shift => true`, `Tab => false`,
and *everything else* → `stop_propagation()` + `prevent_default()`. That
cancels the root's `on_global_key_down` too, so while any field has focus the
app's own shortcuts are dead — ⌘⇧C, ⌘⇧V and ⌘⇧Z all had to be re-implemented
inside the replaced filter (the sibling `apps/freya-windows` hit the same wall
with ⌘, / ⌘⇧I / ⌘W). Replacing the filter is also the only way to get arrow
stepping and Enter-moves-down, so in practice **every non-trivial form
replaces `Input`'s key handling wholesale** and has to re-implement its
defaults. `apps/freya-board` reported the same for Escape; this is the third
app in the cohort to pay it.

## Third edge: pointer focus loses a race with `Input`

Clicking the custom Category cell with `on_press` + `request_focus()` left the
*window root* focused. `Input` handles `on_global_pointer_press` by calling
`request_unfocus()` whenever a press lands outside itself, and that request
arrives after ours. The fix is the ordering the built-in components use:
`on_focus_press` with `stop_propagation()` + `prevent_default()` *then*
`request_focus()`. `on_focus_press` is not mentioned anywhere in the event
docs; it was found by reading `input.rs`.

## Fourth edge: `Ref` held across a `write()` aborts the app

`if let Some(v) = parse(&state.peek()) { state.set(..) }` keeps the `Ref` from
`peek()` alive for the whole body and panics ("Writing to the State failed
because it is already borrowed"), which in a release build becomes a modal
"Fatal Error" panel rather than a backtrace. Third Freya app in this corpus to
trip on it (`freya-board`, `freya-tray`, here). Copy the value into a local
first.

## Helper crates

- `rust_decimal` **=1.39.0** (`default-features = false`, `std`) — exact
  base-10 money; same pin as iced-/xilem-/dioxus-/floem-ledger.
- `async-io` **=2.6.0** — Freya's executor has `spawn` but no timer; the
  self-test needs to wait a frame between a programmatic edit and reading back
  what rendered.

**Not needed, unlike other frameworks in this cohort:** `arboard` (text
clipboard is `freya::prelude::Clipboard`), any number-formatting crate
(`icu`/`num-format` — hand-rolled instead, see the honest rating), any masked
input crate, and any table/virtualisation crate.

## Where the time went

1. **Diagnosing `text_align(Right)`** and settling on space padding
   (three attempts: `text_align`, `Size::Inner` on the `Input` so the box
   shrink-wraps and can be right-aligned inside the cell — Freya sized it to
   the parent anyway — and finally padded monospaced text).
2. Re-implementing `Input`'s key filter three times (arrow stepping,
   Enter-moves-down, app shortcuts).
3. The `peek()` vs `read()` subscription bug: the table rendered but never
   recomputed, which looks exactly like "the model did not change".
4. Getting the accessibility tree to appear at all (`AXManualAccessibility`).
5. The locale parser/formatter — unavoidable, no framework helps here.

## Surprises

- Good: `Input::on_validate` is a true keystroke filter that *rejects* input
  by undoing it. Most toolkits only let you validate after the fact.
- Good: Tab order across mixed controls is free and correct, because it is the
  AccessKit tree order, and it is *observable* from app code
  (`Platform::focused_accessibility_node` is a reactive `accesskit::Node`) —
  which is what made a real Tab-order walk loggable.
- Good: `font_family` set on a plain `rect` is inherited into the built-in
  `Input`'s text.
- Bad: no font-feature API on a Skia renderer.
- Bad: `Input` has no `on_blur` (the whole commit/normalise/revert mechanism
  is built on watching `Platform::focused_accessibility_id`), is hard-wired to
  one line, and its `text_align` is broken.
- Bad: `Slider` is percentage-only with a hard-coded 4 % keyboard step;
  `Checkbox` cannot be given an accessible name; `Select` cannot be opened
  from the keyboard.
- Bad: editor redo is ⌘Y, not ⌘⇧Z, on macOS.

## Skipped / approximated

- **Font features (`tnum`/`lnum`)**: not requestable; approximated with a
  monospaced family. Rated *hand-rolled* and recorded as a framework gap.
- **Right alignment**: approximated with space padding because
  `Input::text_align(Right)` is broken. The model text therefore carries
  display padding; parsing and TSV export trim it.
- **⌘C/⌘V on a row**: bound to ⌘⇧C/⌘⇧V (plus toolbar buttons) because the
  plain combinations belong to the focused editor.
- **Form-level undo**: bound to ⌘⇧Z for the same reason; one step per
  committed cell edit, no redo.
- **IME**: not exercised, source path recorded, rated *not-verified*.
- Freya's `Calendar` (behind the `calendar` feature) was not used; the Date
  cell is a masked `Input`.
- The fr-FR group separator is a plain space rather than U+202F, so the value
  survives a TSV round-trip and a screenshot diff.

## "Number model" paragraph

**The widget carries a `String`, the model carries a `Decimal`, and there is
no type in between — because Freya's `Input` is the text editor and nothing
else.** `Input::new(value: impl Into<Writable<String>>)` binds one
`State<String>` per cell; there is no numeric widget, no `value: f64` variant,
no formatter hook, and no blur event. So every cell here is a `State<String>`
that is parsed on demand: `parse_loose(&text, locale) -> Option<Decimal>` runs
in the render path to compute Amount and the validation state, and again on
blur to normalise. `rust_decimal::Decimal` never lives in a signal except for
the VAT rate; it is derived, which is affordable because parsing 24 short
strings per frame is nothing next to a Skia repaint, and it means there is
exactly one source of truth (the text the user sees) rather than a text/value
pair that can disagree. What makes this workable rather than merely tolerable
is that **the input can be constrained, not just post-validated**:
`on_validate` receives the prospective text before it is committed and
`set_valid(false)` makes the editor undo the keystroke, so `1234a` is
unreachable rather than merely flagged — a stronger guarantee than most of the
cohort offers. What is missing on the other side is the round trip:
normalisation, locale formatting, blur commit and revert-on-Escape are all
app-side, built on top of `Platform::focused_accessibility_id` (the only
signal that tells you a field lost focus) and on a replaced key filter. A
`NumericInput<T>` with `value: State<Decimal>`, a formatter and an `on_blur`
would delete about 200 lines of this app; everything needed to build it is
already public, which is probably why it does not exist yet.
