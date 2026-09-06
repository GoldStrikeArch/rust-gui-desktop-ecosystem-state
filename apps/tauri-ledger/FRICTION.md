# FRICTION — Ledger (Tauri =2.11.5)

SPEC-10. Tauri =2.11.5 / tauri-build =2.6.3 — the same pins and the same manual
no-Node setup as `../tauri-app` (hand-written `tauri.conf.json`, hand-written
capability, `withGlobalTauri`, static vanilla HTML/CSS/JS in `ui/`, copied
icons, `edition = "2021"` as in every other Tauri app in the corpus). No npm,
no JS libraries: the only third-party thing in the frontend is the browser.

Built and verified on macOS 26.5.2 (M4 Pro). Evidence: `evidence/log.txt`,
`evidence/selftest-log.txt` (`LEDGER_SELFTEST=1` → `SELFTEST DONE pass=29
fail=0`, exit 0), `evidence/ax-dump.txt` (654 accessibility elements), and 5
PNGs.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | **hand-rolled** | self-test + synthetic-input | There is no numeric widget: an `<input type="text" inputmode="decimal">` plus a `beforeinput` handler that reconstructs the prospective value and `preventDefault()`s it if it can no longer become a number. `<input type="number">` was rejected (see below). ↑/↓/⇧↑/⇧↓ are a `keydown` handler calling a Rust `step_cell` command; Home/End/word-motion come free from WebKit. Real keystrokes produced `FILTER rejected "a" for unit` and `FILTER rejected "1234.31.75" for unit` in stdout. |
| Locale-aware parse + format (toggle live) | **built-in (format) / hand-rolled (parse)** | self-test + synthetic-input | **The finding:** the webview already contains full ICU. `Intl.NumberFormat(locale)` formats, and `.formatToParts()` hands back that locale's decimal and grouping separators, which is what the keystroke filter is built from — no `icu`, no `num-format`, no locale data in the binary. Parsing is the half the platform does *not* provide (`Intl` has no parser), so it is ~55 lines of Rust. `evidence/02/03`: the same value as `1,234.50` and `1 234,50`, switched by one toolbar click. |
| Decimal alignment / tabular figures (font-feature request) | **built-in** | self-test + observed | `font-variant-numeric: tabular-nums lining-nums` (plus the low-level `font-feature-settings: "tnum" 1, "lnum" 1`) on the numeric cells; the self-test reads it back from `getComputedStyle` and gets `lining-nums tabular-nums`. Requesting an OpenType feature is one CSS line — this is the one row where the webview is strictly ahead of every native toolkit. Combined with right alignment and an always-2-decimal canonical form, the separators line up (`evidence/04`). |
| Inline validation UI + disabled Save + error summary | **hand-rolled** | self-test + synthetic-input | Rust owns the rules (desc 1–60, qty 1–999, price ±99 999.99) and returns per-row failing field names plus human strings; the frontend paints a red border, disables Save, writes `Save (1 error)` and fills an `aria-live` summary strip. ~25 lines of JS, ~20 of Rust. `evidence/05`. |
| Dropdown with type-ahead | **built-in** | observed + by-construction | A plain `<select>`. WKWebView renders a real macOS pop-up menu (the AX dump calls it `pop up button`), so keyboard type-ahead inside the open menu is AppKit's, not ours. Zero code. |
| Date input (masked or picker) | **built-in — segmented native control** | observed | `<input type="date">`. Safari/WKWebView renders a three-segment mm/dd/yyyy field with steppers and a calendar popover; the AX dump shows `incrementor month / day / year`. It is a *masked* control, not a free-text field: `YYYY-MM-DD` typed as digits works segment-by-segment, and the value handed back is always ISO or empty — which is why the Rust side needs no date validator at all. Cost: the three segments each take a Tab stop. |
| Slider ↔ numeric field linkage | **assembled** | self-test | `<input type="range" min=0 max=25 step=0.5>` and a text field, both invoking the same `set_vat` command, both re-rendered from the broadcast. Two directions verified (`slider -> field: "7.50"`, `field -> slider: 21`). |
| Tab order across mixed controls | **built-in** | synthetic-input | DOM order is reading order and nothing sets `tabindex`. 19 real Tab keystrokes walked `desc → cat → qty → unit → reimb` across four rows, correctly skipping the computed Amount cell (`evidence/log.txt` §E). |
| Enter-moves-down cell navigation | **hand-rolled** | self-test | ~8 lines: on Enter, commit, collect `[data-f="<column>"]`, focus index ±1. ⇧Enter goes up. The web platform has no notion of a grid cursor. |
| Undo/redo (field-level / form-level) | **both — built-in + hand-rolled** | self-test + by-construction | Field-level ⌘Z is WebKit's own text undo, but only because the app installs an Edit menu with `PredefinedMenuItem::undo/redo` — without those native roles ⌘Z never reaches the webview on macOS (the same trap as `../tauri-tray`'s ⌘V). Form-level undo of the last *committed* cell edit is a `Vec<(row, field, previous)>` in Rust (~25 lines), verified both ways. The two collide on one keystroke: the app gives ⌘Z to the field while it has an uncommitted edit and to the form otherwise — a policy, not a fix. |
| Live computed columns and totals | **built-in (for this size)** | self-test | Every commit is one IPC round trip that returns a full 12-row snapshot; Amount/Subtotal/VAT/Total are recomputed in Rust with `rust_decimal` and re-rendered wholesale with no perceptible lag. This is the shape that would have to change at 10 000 rows (cf. `../tauri-grid`, which pages slices instead). |
| Row copy/paste as TSV | **assembled** | self-test | `tauri-plugin-clipboard-manager` =2.3.2 (arboard underneath), called from Rust. Writing could have been done in the webview; **reading** could not — without a user paste gesture the web clipboard API is unavailable in this context, so the plugin is the only route to ⌘V-on-a-row. Round trip verified (6 TSV columns out, row 12 filled from them). Cell-level paste is separate and better: the `paste` event hands the raw text straight to the Rust parser. |
| Accessibility labels (verified dump) | **built-in** | observed | `evidence/ax-dump.txt`: 654 elements, every control with a real macOS role (`text field`, `pop up button`, `slider` + value indicator, `checkbox`, `incrementor`, `table`/`row`) and the name from its `aria-label`. The app writes `aria-label`; WKWebView does the rest. No AccessKit, no work. |
| IME composition (optional) | **not-verified** | not-verified | No CJK input source was installed and the display was shared with five other agents' apps. The field is a stock `<input type=text>` in WKWebView, i.e. Safari's text stack, so inline composition is expected to work untouched — unproven here. |

## Two things that were rejected

- **`<input type="number">`.** It looks like the answer and is not: browsers do
  not filter typing in it (you can type `e`, `--`, `1.2.3`), `.value` is `""`
  for anything unparseable so the previous value cannot be kept, its spinner
  arrows are unstyleable, and — decisively — it is **locale-fixed to a `.`
  decimal point** in every engine, so the fr-FR half of the spec is impossible
  with it. A `type="text"` field with `inputmode="decimal"` and an explicit
  filter is the only shape that satisfies §2.
- **A JS-side parser.** Keeping any parsing in the frontend would have split
  the number model in two. The frontend only ever *filters*.

## Helper crates

| Crate | Why | Underneath |
|---|---|---|
| `rust_decimal` =1.42.1 | exact base-10 money; `Decimal::from_str`, `round_dp(2)`, exact `qty * unit` and VAT with no binary-float error | — |
| `tauri-plugin-clipboard-manager` =2.3.2 | the only way to *read* the clipboard without a user paste gesture (row-level ⌘V) | arboard 3.6 |
| `serde` / `serde_json` | `#[tauri::command]` (de)serialisation across IPC | — |

Not needed, and this is the notable part: **no `icu`, no `num-format`, no
`unicode-segmentation`, no masked-input crate, no date crate, no accessibility
crate**. `Intl.NumberFormat`, `<input type=date>`, `<select>` and the
WKWebView accessibility bridge cover four of the spec's rows at zero
dependency cost. The clipboard plugin is driven only from Rust, so the
capability file needs no permission entry beyond `core:default`.

## LoC (855 source; 891 including config) & size

- Rust: **465** (459 code lines in `src/main.rs` + 6 `build.rs`); 536 lines
  including comments and blanks. ~55 of those are the locale-aware parser.
- Frontend: **390** (48 HTML, 213 JS in `main.js`, 61 CSS, plus 123 code lines
  of `ui/selftest.js` which is verification, not app code — app frontend
  alone is **267**).
- Config: 36 (`tauri.conf.json` 29 + capability 7).
- Release binary **8.87 MiB**; **505 packages** in `Cargo.lock` vs the
  `tauri-app` baseline's 418 — `rust_decimal` and the clipboard plugin add 87.
- Clean `cargo build --release`: **57.0 s**. `cargo build --release --locked`
  afterwards: clean, no lockfile churn.

## Where the time went

1. The parser. Not the arithmetic — deciding, for a string with both `.` and
   `,` in it, which one is the decimal separator. The rule that survived: the
   *last* one wins when both appear; when only one appears with exactly three
   digits behind it the string is ambiguous and the active locale breaks the
   tie. That single branch is the whole difference between "`1.234` is 1234"
   (fr-FR) and "`1.234` is 1.234" (en-US).
2. The edit/display duality. A numeric cell has three representations —
   canonical (`-31.75`), plain-in-locale (`-31,75`, what you edit), and
   formatted (`-1 234,50`, what you read) — and every render has to know which
   one the cell is currently in and must never clobber the field the caret is
   sitting in.
3. Verification under contention (see `evidence/log.txt`): five sibling agents
   were driving GUI apps on the same display, several with always-on-top
   windows, so every mouse click had to be gated on a CoreGraphics
   "who owns this point" probe, and the *system clipboard was shared* — one
   run picked up another agent's TSV.

## Surprises

- **Good:** the ICU that a native Rust app would pay ~2 MB and a `icu`/
  `num-format` dependency for is already sitting in the process. `Intl
  .NumberFormat(loc).formatToParts()` is also how the app discovers the
  locale's separators, so the keystroke filter is locale-aware for free.
- **Good:** `font-variant-numeric: tabular-nums` — the spec asks "record
  whether the framework can request font features at all"; here it is one CSS
  declaration, and `getComputedStyle` proves it was honoured.
- **Good:** the accessibility row, usually the hardest in this corpus, is
  `aria-label` and nothing else. `<select>` becomes a real AX `pop up button`;
  `<input type=range>` a real `slider`.
- **Bad:** `<input type="number">` is unusable for a real form (above).
- **Bad:** ⌘Z inside a text field only works because you remembered to add a
  native Edit menu with the predefined undo/redo roles; nothing warns you.
- **Bad:** the native date control eats three Tab stops per row and cannot be
  made to accept a pasted `YYYY-MM-DD` string as one gesture.

## Number model

**The value is a `rust_decimal::Decimal` and it only ever exists in Rust.**
Nothing else in the pipeline is a number at all:

```
 keyboard ──beforeinput filter (locale regex from Intl.formatToParts)──▶ String
    │                                                                      │
    │  ↑/↓ steps and paste bypass the field entirely                       │ IPC
    ▼                                                                      ▼
 #[tauri::command] set_cell / step_cell / paste_row  ──▶ parse_decimal(&str, locale)
                                                          ──▶ Decimal  (the model)
                                                          ──▶ validate, clamp, undo
 snapshot: canonical Strings ("-31.75", "2469.00") ──event──▶ Intl.NumberFormat ──▶ pixels
```

Parsing lives in exactly one function, `parse_decimal(raw, locale) ->
Result<Decimal, String>`, and every entry point — typing, blur, arrow
stepping, cell paste, TSV row paste, the VAT field — goes through it. `f64`
never appears in the model (only in the frontend, where `Number(canonical)` is
handed to `Intl` for display; two fraction digits is far inside f64's exact
range). The wire format between the two halves is a **canonical decimal
string**, never a JSON number, because JSON numbers are f64 and would quietly
re-introduce the error `rust_decimal` was chosen to avoid.

Can the framework's text input be constrained at all, or only post-validated?
**Both, and the distinction is the interesting part.** The web platform gives a
genuine pre-commit hook — `beforeinput` is cancellable, so a character that
cannot possibly extend into a valid number is never inserted and the field is
never in an illegal intermediate state. That is real constraint, not
post-validation, and it is stronger than what `<input type=number>` offers.
But it is only *syntactic*: it cannot know that `1000` is out of range for Qty
or that `999999.99` exceeds the price bound, because a prefix of a legal value
must stay typeable. So the app is constrained while typing and post-validated
on commit, with two different failure modes: the filter silently drops the
keystroke (and logs `FILTER rejected …`), while a commit that the parser or
the range check rejects **keeps the previous committed value** and paints the
cell red. Getting that split right — which errors are preventable and which
can only be reported — is most of what makes a numeric field feel native.

## Approximated or skipped

- **§2 stepping:** ↑/↓ and ⇧↑/⇧↓ are implemented; Home/End are left to
  WebKit's own caret motion (the spec's "Home/End as usual"), i.e. they move
  the caret rather than jumping to a min/max value.
- **§2 paste:** cell paste routes the raw text to the Rust parser and covers
  `$1,234.56`, `1.234,56 €` and `(12.50)`. Currency stripping is a fixed set
  (`$ € £ ¥ %` and the space family), not an ICU currency table.
- **§7 undo:** form-level undo covers the last committed *cell* edit, not
  structural changes (there are no add/delete-row operations in this app). The
  ⌘Z collision policy is described above.
- **§9 clipboard:** ⌘C/⌘V act on the row containing the focused cell and are
  intercepted only when focus is *not* inside a text field, so ordinary text
  copy/paste inside a cell still behaves normally.
- **§11 IME:** not verified (no CJK input source; shared display).
- The seeded date column uses `<input type="date">`'s native rendering, which
  displays in the *system* locale (mm/dd/yyyy here) regardless of the app's
  Locale toggle — the toggle deliberately governs numbers only, since the
  browser gives no way to set a date field's display locale.
