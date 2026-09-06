# FRICTION — Ledger (floem git @ 778bb5f2)

Reference: `apps/SPEC-10.md`. Built and verified on macOS 26.5.2 (M4 Pro,
rustc/cargo 1.96.1). `cargo build --release` clean from scratch in **81 s**;
`cargo build --release --locked` succeeds unchanged; binary **18.3 MB**.
LoC: **1108** total in one `src/main.rs` (941 non-blank/non-comment), of which
~150 are the `LEDGER_SELFTEST` hook and ~65 the locale parse/format code.

Version note: same pinned git rev as `apps/floem-app` (crates.io 0.2.0 is
stale; `main` is unpublishable — forked `floem-winit`). See
`apps/floem-app/GAPS.md`.

Evidence: `evidence/log.txt` (step-by-step with the command for each step),
`evidence/selftest-log.txt` (`SELFTEST DONE pass=37 fail=0`),
`evidence/tab-order.txt`, `evidence/ax-dump.txt`, `evidence/drive.sh` and
six screenshots.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **hand-rolled** | self-test + synthetic-input | `TextInput::new(buffer: RwSignal<String>)` is the *entire* input API — no mask, no filter/validator callback, no `on_char`, no numeric variant, and the buffer type is fixed to `String`. Filtering is therefore **post-hoc**: an `Effect` watches the buffer and writes back a sanitised copy, so an illegal character is visible for one frame (`FILTER "a4" -> "4"` on stdout, `05-validation-and-filter.png`). It is survivable only because `TextInput::event` clamps `cursor_glyph_idx` to the buffer length — a workaround floem's own source labels *"Workaround for cursor going out of bounds when text buffer is modified externally / TODO: find a better way"*. Step keys are a `KeyDown` handler: ↑/↓ ±1 (Qty) / ±0.01 (price), ⇧ ×10; plain arrows reach the handler because floem's built-in arrow navigation is bound to **Alt**+arrows only. Home/End are `TextInput` built-ins. |
| Locale-aware parse + format (toggle live) | **hand-rolled** (~65 LoC, no `icu`) | self-test + observed | floem has a `localization` module, but it is message/`fluent`-shaped — no number formatting. The two locales differ only in separators, so hand-rolling was cheaper than a dependency. The parser is deliberately tolerant: `$1,234.56`, `1.234,56 €`, `(12.50)` and `1 234,56` all parse, by taking the **rightmost** of `.`/`,` as the decimal point when both appear. The toggle rewrites every buffer from the untouched `Decimal`s, so switching locale cannot lose precision (`02b` vs `03` screenshots). |
| Decimal alignment / tabular figures (font-feature request) | **assembled**; the font-feature request itself is **not-achievable** | observed | floem's `Style` exposes `font_family`, `font_size`, `font_weight`, `font_style` and `line_height` — and **no font-feature or font-variation property at all** (`grep -rn 'font_features\|FontFeature\|font_variations'` over the pinned checkout returns nothing), even though the text stack underneath is parley/swash, which supports them. So `tnum`/`lnum` cannot be requested; the substitute is a monospaced family (`font_family("Menlo, Courier New, monospace")`) plus `text_align(Alignment::End)`, which `TextInput` honours. The computed Amount column additionally splits the string at the separator into a right-aligned integer part and a fixed-width fraction part, so it stays aligned even when a value has a different number of decimals. `04-decimal-alignment.png`. Negatives render red. |
| Inline validation UI + disabled Save + error summary | **assembled** | synthetic-input + self-test | Pure signal derivation: `Row::errors()` returns `(field, message)` pairs, a reactive style closure paints the red border, a `dyn_container` renders the summary, and the Save control's label is `Save (N errors)` and refuses to act while `N > 0`. Nothing framework-provided (floem has no form/validation concept); nothing missing either. `05-validation-and-filter.png`. |
| Dropdown with type-ahead | **built-in dropdown, hand-rolled type-ahead** | by-construction + observed | `Dropdown::new_rw(signal, items)` is a real floem view with `on_accept`, custom item views and a `DropdownCustomStyle`. It has **no type-ahead**, and it is not focusable by default — `.style(\|s\| s.keyboard_navigable())` is required before Tab will visit it. Type-ahead is 6 lines in a `KeyDown` handler (first category whose name starts with the typed letter; prints `TYPEAHEAD x -> Y`). |
| Date input (masked or picker) | **hand-rolled mask** | observed + self-test | floem has no date picker and no input mask. The field is a `TextInput` plus an `Effect` that keeps digits, re-inserts the dashes at positions 4 and 7 and caps at 8 digits, with an `is_iso_date` check driving the red border. Typing `20260104` yields `2026-01-04`; `YYYY-MM-DD` pasted or typed with dashes also works. |
| Slider ↔ numeric field linkage | **built-in, both directions** | self-test + observed | `Slider::new_ranged(\|\| vat.get(), 0.0..=25.0)` + `on_event_stop(SliderChanged::listener(), …)` writes the field; writing the field writes `vat`, and `new_ranged`'s internal `UpdaterEffect` pushes that back into the slider position. One papercut worth knowing: a floem slider has **no intrinsic height** — without an explicit `.height(20.0)` it lays out 0 px tall and is silently invisible (cost ~10 minutes and one screenshot). |
| Tab order across mixed controls | **built-in, but not in reading order** | synthetic-input | Tab/⇧Tab traversal is a floem default behaviour (`element_tab_navigation`, `event/dispatch.rs`), and it does visit the text inputs, the dropdown and the checkbox once `keyboard_navigable()` is on the latter two. But the order is wrong: measured with the app's own focus events (`evidence/tab-order.txt`), a row goes Date → Description → **Qty** → Unit price → Reimb., and the row's Category **dropdown** turns up one row late, after the *previous* row's checkbox. ⇧Tab reproduces the same sequence backwards, so it is a stable ordering, just not the visual one, and floem exposes no tab-index to correct it. |
| Enter-moves-down cell navigation | **hand-rolled** | self-test | `TextInputEnter` (a floem custom event) and a `KeyDown` check for ⇧Enter call `ViewId::request_focus()` on the cell below/above. The awkward part is *finding* that cell: floem gives you no cell/grid model and no way to ask "what is the view at row+1, same column", so the app keeps a `thread_local` `Vec<Vec<ViewId>>` filled while the view tree is built. |
| Undo/redo (field-level / form-level) | **field-level: not-achievable · form-level: hand-rolled** | self-test | `TextInput` has **no undo stack at all** (`grep -n 'undo' views/text_input.rs` → nothing), so ⌘Z inside a focused field does nothing — a floem gap, not an app choice. (floem's *other* text widget, the Lapce `text_editor`, does have full undo/redo, but it is a multi-line editor, not a form field.) Form-level undo/redo of the last committed cell edit is a 40-line `Vec<Undo>` + redo stack driven by ⌘Z / ⌘⇧Z; verified both directions. |
| Live computed columns and totals | **built-in** | observed + self-test | Amount, Subtotal, VAT and Total are plain `Label::derived` closures over the `Decimal` signals. A committed edit repaints all of them in the same frame with no visible lag at 12 rows; `02b-tab-formatted-en-US.png` shows one Tab moving Subtotal 2,439.68 → 3,674.18 and Total → 4,409.02. Fine-grained reactivity means only the affected labels re-render. |
| Row copy/paste as TSV | **built-in clipboard, assembled semantics** | self-test | `floem::Clipboard::{set_contents,get_contents}` is a floem built-in (text + file-list; images need `arboard`, as `floem-tray` found). ⌘C serialises the focused row to seven tab-separated fields in the active locale; ⌘V parses a TSV row into the focused row and re-commits the numerics. Round trip verified through the *real* system clipboard in the self-test. The shortcuts ride on floem's rule that modified keys fall back to the listener registry, so a single root-level `KeyDown` handler sees them wherever focus is. |
| Accessibility labels (verified dump) | **not-achievable** | synthetic-input (dump retained) | floem has **no AccessKit integration and no accessibility tree of any kind**: `grep -rn accesskit` over the pinned checkout returns zero hits, and `Decorators` has no name/label/role method. `evidence/ax-dump.txt`: `count UI elements of window 1` = **4** — the three traffic-light buttons and the window title. The whole 820×560 form (12 rows × 6 controls, the slider, the toolbar, the totals) is invisible to VoiceOver and to `System Events`. Nothing an app can configure. |
| IME composition (optional) | **unexercised** | not-verified | The plumbing exists: `ImeEnabled/ImePreedit/ImeCommit` listeners, `action::set_ime_allowed` / `set_ime_cursor_area`, and `TextInput` carries a `Preedit` that it splices into the layout at the caret. Switching the macOS input source would have hijacked the keyboard for the sibling agents sharing this machine, so this is recorded source-only. |

## Helper crates

- **rust_decimal =1.39.0** (`default-features = false, features = ["std"]`) —
  exact base-10 money. Same exact pin as `iced-ledger`, `xilem-ledger` and
  `dioxus-ledger`, so the money stack is identical across frameworks.

Nothing else, and three plausible dependencies were deliberately *not* taken:
`icu`/`num-format` (the two locales differ only in separators — 65 lines beat
a new dependency graph), a masked-input crate (none exists for floem; the mask
is 8 lines of `Effect`), and `arboard` (floem's built-in `Clipboard` covers
text, which is all TSV needs).

## Where the time went

1. **Deciding what "filtered input" can even mean in floem.** `TextInput`
   accepts an `RwSignal<String>` and nothing else — there is no hook between a
   keystroke and the buffer. Establishing that the post-hoc `Effect` rewrite is
   safe (because `TextInput::event` clamps the cursor) took reading the widget
   source, not the docs.
2. **The locale parser.** Making `$1,234.56`, `1.234,56 €`, `(12.50)`,
   `1 234,56` and `12.5` all parse under one set of rules, without accepting
   garbage, is most of the non-UI code.
3. **Verifying on a contended desktop** — ~10 sibling GUI apps overlapping this
   window and driving their own synthetic input; every click and keystroke
   needed a raise-and-retry wrapper. (One of their ⇧Tab runs also left a shift
   modifier applied to *this* app's keystrokes for a while, which is worth
   knowing if these numbers ever look odd.)
4. The table, the totals, the validation and the undo stack were quick —
   ordinary signal code.

## Surprises

- **Good**: fine-grained reactivity makes the whole "live totals + validation +
  disabled Save + error summary" half of the spec nearly free. Every derived
  cell is one closure, and nothing has to be invalidated by hand.
- **Good**: `Dropdown`, `Slider::new_ranged`, `Checkbox::new_rw` and
  `Clipboard` all exist and all work, which is more than several frameworks in
  this corpus ship.
- **Bad, and the headline**: floem's text input cannot be constrained. The
  only lever is to let the wrong character in and take it out again.
- **Bad**: no undo in `TextInput`, while floem's *other* text widget has a
  full undo stack — the form field is the weaker of the two.
- **Bad**: Tab order is stable but is not reading order, and there is no
  tab-index to fix it.
- **Bad**: no font-feature property anywhere in `Style`, on a parley/swash
  stack that supports OpenType features natively.
- **Bad**: zero accessibility. This is the first *verified* a11y datum in the
  corpus for floem and it is an empty tree.
- Papercut: a slider with no explicit height is invisible, not obviously
  broken.

## Skipped / approximated

- **IME (req 11)** — not exercised, see above.
- **Type-ahead in the dropdown (req 4)** matches on the first letter only, not
  on an accumulating prefix.
- **Date (req 4)** is a mask, not a picker; floem has neither, and a picker
  would have been a from-scratch calendar widget.
- **Undo at field level (req 7)** is not implemented because it cannot be:
  `TextInput` has no undo stack and no API to inject one.
- The **Save** button validates and logs; there is no persistence target in
  this spec.
- Grouping in the fr-FR locale uses an ordinary ASCII space rather than the
  narrow no-break space, on purpose: `floem-babel` measured that fontique
  0.7 on macOS at this rev drops to tofu for several non-Latin runs, so the
  app stays in ASCII for the separators (the parser accepts U+00A0 and
  U+202F on paste anyway).

## Number model

**The value crosses the widget boundary as a `String`, and only as a
`String`.** `TextInput::new` takes an `RwSignal<String>`; there is no generic
`value: T` input, no parse/format pair, no validator. So every numeric cell in
this app is a pair — `buf: RwSignal<String>` (what the widget owns) and
`val: RwSignal<Decimal>` (what the program owns) — plus a small state machine
that moves between them: an `Effect` sanitises `buf` while typing;
`FocusLost`, `TextInputEnter` and the arrow-step keys **commit** (parse `buf`,
round to the column's decimal places, write `val`, then write the *formatted*
value back into `buf`); Esc reverts `buf` from `val`; and a failed parse sets
an error flag and leaves `val` alone, which is what makes "keeps the previous
committed value" fall out naturally.

Parsing therefore lives entirely in the application, at exactly two moments
(commit, and paste). Nothing downstream ever sees a string: Amount, Subtotal,
VAT and Total are `Decimal` arithmetic on `val` only, so the locale toggle is
a pure re-render — it rewrites twelve `buf`s from untouched `Decimal`s and
cannot lose a cent. The one place this shape hurts is that the *displayed*
text and the *committed* value are genuinely two pieces of state, and every
path that changes one has to remember the other; a framework with a
`value: T` + `format`/`parse` input would collapse the whole state machine.

And the direct answer to the spec's question: floem's text input **cannot be
constrained at all** — it can only be post-validated, or post-*corrected* by
writing a cleaned string back into the same signal, which is what this app
does. There is no keystroke hook to say "no".
