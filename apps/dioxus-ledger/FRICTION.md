# FRICTION — Ledger (dioxus =0.7.9 desktop/webview)

Built as `apps/dioxus-ledger` (package `dioxus-ledger`, edition 2024), same pin
as iteration 1. One window, 12 seed expense rows, live Amount/Subtotal/VAT/Total.

**Structure (deliberate, for the Dioxus-Native follow-up):**
`src/app.rs` (935 lines) is pure `dioxus::prelude` + `dioxus::html` + `rust_decimal`
— no `dioxus::desktop`, no `document::eval`, **zero JavaScript**. All parsing,
locale rules and formatting are plain Rust functions (`fmt_dec`, `parse_dec`,
`typeable`, `normalize`, `stepped`) that the self-test drives directly.
`src/platform.rs` (57 lines) has exactly three OS-facing functions: `launch`,
`clipboard_write`, `clipboard_read`. `src/main.rs` is 8 lines.

Verified on macOS 26.5.2 / M4 Pro, release binary; evidence in `evidence/`:
`log.txt` (commands + results), 5 screenshots, `ax-dump.txt`,
`selftest-log.txt` (`SELFTEST DONE pass=26 fail=0`).

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **hand-rolled** | self-test + synthetic-input | There is no numeric widget and no way to *constrain* an `<input>` — `oninput` only ever hands you the whole new string. Every numeric cell keeps an edit buffer (`qty_text`/`price_text`) beside the committed `i64`/`Decimal`; `oninput` runs `typeable()` (locale-aware prefix test), `onblur` runs `normalize()`. **The rejection path is the interesting part:** writing the old string back does nothing, because Dioxus diffs against the previous *VDOM* value, not the DOM — so the rejected character stays on screen. The fix is to bump a counter in the row's `key:`, which remounts the input with the correct value (and costs the caret position). Stepping: `onkeydown` ArrowUp/Down ±1 / ±0.01, Shift ×10. Real key events: Qty 1 → 4 after 3× Up; Qty 3 → 23 after 2× Shift-Up; Unit price 34,75 → 34,83 after 2× Down + Shift-Up. Home/End are the webview's native behaviour, untouched. |
| Locale-aware parse + format (toggle live) | **hand-rolled** | self-test + synthetic-input | ~60 lines of Rust: `fmt_dec` groups the integer part in threes and substitutes the locale's separators; `parse_dec` is deliberately lenient. Toolbar toggle re-formats every buffer and every computed cell in place. Typed `1234.5` + Tab → **`1,234.50`** (en-US, `02-formatted-enus.png`); one click on the Locale button → **`1 234,50`** and the whole table, the totals and the negative row switch to fr-FR (`03-formatted-frfr.png`). No `icu`/`num-format`: the corpus wants the same code on Native, and the two locales the spec names need arithmetic, not CLDR. fr-FR grouping uses U+0020 rather than U+202F to keep the evidence greppable — recorded as a deliberate simplification. |
| Decimal alignment / tabular figures (font-feature request) | **built-in** | observed | `text-align: right` + `font-variant-numeric: tabular-nums lining-nums` and `font-feature-settings: "tnum" 1, "lnum" 1`. **This is the one place the webview simply wins:** requesting OpenType features is one CSS line and the system UI face honours it. Because blur always normalises to exactly 2 decimals, right-alignment + equal digit advance *is* decimal-point alignment; `12,50`, `189,99` and `1 295,00` line up (`04-validation.png`). Negatives render red (`-34,75`). |
| Inline validation UI + disabled Save + error summary | **assembled** | synthetic-input | `validate(&rows) -> Vec<(row, field, msg)>` recomputed each render; a `bad` class puts a red border + tint on the offending input, the footer lists `row N · Field: message`, and `disabled: !errs.is_empty()` on the button. Cleared Description row 3 with real keys → red border, `1 problem(s): row 3 · Description: Description is required`, AX reports `Save (1 errors) / enabled=false`. |
| Dropdown with type-ahead | **built-in** | observed | `<select>` + `<option>`; WebKit gives the native popup, keyboard opening and letter type-ahead for free. Zero Rust beyond the `onchange`. |
| Date input (masked or picker) | **built-in (picker)** | observed | `<input type="date">` → WebKit's segmented `MM/DD/YYYY` field with a calendar popover; the model stores `YYYY-MM-DD` and the browser accepts that verbatim. Two costs, both recorded: the displayed order is the OS locale's, not the stored ISO order; and the three segments each take a Tab, so a Tab walk spends 3 stops inside one cell (visible in the Tab log). |
| Slider ↔ numeric field linkage | **assembled** | self-test | `<input type=range min=0 max=25>` and a filtered numeric field over one `Signal<Decimal>`; both directions are just the shared signal. The numeric side blurs through the same clamp+format path as the table. |
| Tab order across mixed controls | **built-in** | synthetic-input | DOM order *is* reading order, so this is free. Logged from the app's own focus events (`LEDGER_TABLOG=1`), 16 × Tab from Description row 1: `Description → Category → Qty → Unit price → Reimbursable → row 2 Date → …` across text inputs, a `<select>`, a checkbox and a date picker, without a single line of tab-order code. |
| Enter-moves-down cell navigation | **assembled** | synthetic-input | Not a webview behaviour; hand-wired. `onkeydown` Enter/⇧Enter computes the neighbour and focuses it through an `Rc<MountedData>` handle stashed per cell by `onmounted` (`set_focus(true).await`). Logged: `Qty row 1 → 2 → 3 → 4` on Enter, `→ 3 → 2` on ⇧Enter. The handle map is the only slightly awkward part — Dioxus has no "focus this element by id". |
| Undo/redo (field-level / form-level) | **built-in (field) + hand-rolled (form)** | self-test + synthetic-input | Field-level ⌘Z/⌘⇧Z inside a focused input is the webview's native editing stack plus dioxus' default muda Edit menu — nothing written. Form-level undo/redo of the last *committed cell edit* is a `Vec<Undo>{row, field, before, after}` on ⌥⌘Z / ⌘⇧Z and two toolbar buttons. Verified live: cleared Description row 3 → Save disabled; Undo → `Hotel Manhattan` back, `Save / enabled=true`. **Trap found the hard way:** text cells write straight into the model on `oninput` (that is what makes the live totals live), so at blur time the "before" value is already gone — the undo entry has to come from a snapshot taken in `onfocus`. Without it, text edits are silently un-undoable. |
| Live computed columns and totals | **built-in** | self-test | Amount, Subtotal, VAT and Total are derived expressions in the render body over `Decimal`; any signal write re-runs them. 12 rows is far below anything measurable — no lag, no memoisation needed. |
| Row copy/paste as TSV | **assembled (arboard)** | self-test + synthetic-input | `to_tsv`/`from_tsv` + `arboard`. `[ledger] copied TSV: 2026-01-10\tConference pass\tTraining\t1\t1295.00\tyes`, then paste filled row 12. **Approximated:** the shortcut is ⌘⇧C/⌘⇧V, not ⌘C/⌘V — plain ⌘C/⌘V are claimed by the webview's native editing stack and by muda's predefined Edit roles, and a DOM keydown handler cannot tell whether the event target is a text input, so overriding them would break copying *inside* a cell. |
| Accessibility labels (verified dump) | **built-in** | observed (`ax-dump.txt`) | `"aria-label"` written as a quoted rsx attribute passes straight through to the DOM, and WebKit's AX bridge exposes it as the AX name: `text field "Unit price row 1"`, `slider "VAT percent slider"`, `UI element "Date row 1"` (with `incrementor month/day/year` children), plus every header as `static text`. Values are both readable *and settable* through AX — the entire verification above was driven that way, which also means it is the only Dioxus app in this corpus that can be scripted without coordinates. |
| IME composition | **not-verified** | — | No CJK input source installed on this machine. The text stack is WKWebView's, i.e. the same one apps/dioxus-babel exercised for shaping/BiDi. |

## Helper crates

- **rust_decimal `=1.39.0`** (`default-features = false`, `std`) — exact base-10 money. `f64` cannot hold `0.01`, and 0.01 stepping plus 2-dp rounding has to be exact. Pure Rust, so it ports to Native unchanged.
- **arboard `=3.6.1`** — system pasteboard for whole-row TSV. The webview's native clipboard only covers text *inside* an input.
- **tokio `=1.52.3`** (`time` only) — paces `LEDGER_SELFTEST`. The recurring finding again: dioxus runs on tokio, re-exports no timer.
- **Rejected:** `icu`/`num-format` (two locales' grouping and decimal rules are ~60 lines of arithmetic, and the spec wants the formatting to live in Rust so the same code runs on Blitz); masked-input crates (all target web/wasm); `serde` (nothing is serialised).

## Where the time went

- ~30% the controlled-input problem in both of its forms: discovering that a
  rejected keystroke cannot be undone by writing the old value back (VDOM-vs-DOM
  diffing), and then discovering that fast typing *drops* characters for the
  same reason.
- ~25% verification on a shared desktop (six sibling agents; two evidence runs
  were killed outright when another agent's ⌘W landed on this always-on-top
  window, which is why `LEDGER_PLACE` also switches the window to hide-on-close).
- ~20% the self-test (26 assertions over the same functions the widgets call —
  no copies, so the test cannot drift from the UI).
- ~15% keyboard plumbing: the `Rc<MountedData>` focus map, Enter/⇧Enter, arrow
  stepping, and the `onfocus` snapshot that makes form-level undo possible.
- ~10% CSS (grid table, tabular figures, error states, dark mode).

## Surprises

- (+) OpenType numeric features are one CSS line — the thing the spec expected
  to be hard is the easiest cell in the table.
- (+) Tab order, type-ahead, the date picker and field-level undo are all free.
- (+) `aria-label` gives a genuinely good AX tree, good enough to *drive* the
  app; this is the first app in the corpus that could be verified without a
  single coordinate click.
- (−) **Fast typing loses characters.** System Events' default keystroke rate
  (~30 ms/char) typing `1234.5` into a controlled input produced `12.5`
  (→ `12.50`), reproducibly; at 250 ms/char it is exact. Every keystroke
  round-trips DOM → IPC → VirtualDom → diff → DOM and the diff rewrites
  `input.value`, clobbering anything typed during the round trip. A real typist
  at 8–10 chars/s is close to that edge. Uncontrolled inputs (never writing
  `value` back) would dodge it but give up filtering and blur-formatting.
- (−) Rejecting a keystroke requires remounting the element, which loses the
  caret position. There is no "set the DOM value" escape hatch that does not go
  through `document::eval`, which this app refuses on purpose.
- (−) `oninput`-into-the-model is what makes totals live and what breaks undo;
  the two pull in opposite directions.

## The number model

**The widget carries a `String`; the model carries a `Decimal`; nothing in
Dioxus bridges them.** `FormData::value()` is a `String` and there is no numeric
input type, no formatter hook, no `on_before_input`, no validator. So the model
is deliberately two-layered: `Row { qty: i64, price: Decimal, qty_text: String,
price_text: String }`. `qty_text`/`price_text` are what the `<input>` shows;
`qty`/`price` are the last value that survived parsing. Parsing lives in exactly
two places — `typeable()` on every `oninput` (a prefix test: "could this string
still become a number in this locale?") and `parse_dec()` on `onblur` (lenient:
strips `$ € £`, understands `1.234,56`, `1,234.56` and the accounting negative
`(12.50)`), followed by clamping, `round_dp(2)` and re-formatting through
`fmt_dec()`.

**Can the input be constrained at all, or only post-validated?** Only
post-validated, and even undoing a bad keystroke is awkward. Dioxus diffs the
`value` attribute against the *previous virtual* value, so re-writing the old
string emits no DOM patch and the rejected character remains visible; the only
JS-free remedy is to change the element's `key` so the input is destroyed and
recreated. That works, and it costs the caret. The same asynchrony that causes
it also drops characters when typing quickly. In other words: on Dioxus desktop
a numeric field is not a widget you configure, it is a small state machine you
write — and its correctness is bounded by how fast the IPC round trip is.

## Approximated or skipped

- **Row clipboard is ⌘⇧C/⌘⇧V**, not ⌘C/⌘V (the plain keys belong to the
  webview's native editing stack and to muda's Edit roles).
- **Form-level undo is ⌥⌘Z / ⌘⇧Z** (plus toolbar buttons), so it does not fight
  the field-level ⌘Z the webview provides.
- **fr-FR grouping uses a plain space** (U+0020) instead of the narrow no-break
  space, to keep the AX/log evidence comparable.
- **Esc in a cell** cancels the key (`prevent_default`) but the revert-to-last-
  committed-value path is only exercised through the blur normaliser, not by a
  separate test.
- **IME**: not verified.
- **Paste heuristic:** a growth of more than one character in `oninput` is
  treated as a paste and skips the typing filter, so `$1,234.56` can land in the
  field and be parsed on blur. A one-character paste is indistinguishable from a
  keystroke and is filtered as typing.

## What a Dioxus-Native (Blitz) port must replace

`src/app.rs` and `src/main.rs` should compile unchanged (pure
`dioxus::prelude` + `dioxus::html` + `rust_decimal`; no `document::eval`, no JS
in the crate). `src/platform.rs` is 57 lines and only three of its functions
touch the OS:

| function | what it does on desktop | Native note |
|---|---|---|
| `launch(fn() -> Element)` | `LaunchBuilder::desktop()` + tao `WindowBuilder` (title, 820×560) | swap for `dioxus_native::launch` |
| `clipboard_write(String)` | `arboard::Clipboard::set_text` | arboard is backend-agnostic; should port as-is |
| `clipboard_read() -> Result<String,String>` | `arboard::Clipboard::get_text` | same |
| `selftest()` / `place()` | env-var knobs (`place` also sets always-on-top / hide-on-close for verification runs) | verification only |

Everything else the spec asks for is either Blitz's problem (does it implement
`<input type=date>`, `<select>` type-ahead, `font-variant-numeric: tabular-nums`,
focus/Tab order, `set_focus` on `MountedData`, an accessibility tree?) or is
already plain Rust in `app.rs` (`fmt_dec`, `parse_dec`, `typeable`, `normalize`,
`stepped`, `validate`, `to_tsv`/`from_tsv`).
