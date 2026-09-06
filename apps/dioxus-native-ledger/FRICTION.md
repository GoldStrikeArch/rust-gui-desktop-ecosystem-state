# FRICTION — Ledger (dioxus-native, Blitz main 64eb278 / 0.3.0-beta.2)

SPEC-10 built at `apps/dioxus-native-ledger`, package `dioxus-native-ledger`,
edition 2024, plain cargo. Renderer: Blitz — Stylo 0.20 + Taffy 0.14 + Parley
0.11.1 + AccessKit, painted through anyrender / **Vello Hybrid**, windowed by
winit 0.31.0-beta.2. No webview, no JS engine, one OS process.

**The UI is shared, not copied.** `src/main.rs` is

```rust
#[path = "../../dioxus-ledger/src/app.rs"]
mod app;
mod platform;
fn main() { platform::launch(app::App) }
```

`apps/dioxus-ledger/src/app.rs` (935 lines) compiles and runs **unchanged, with
zero diff hunks**. Only `platform.rs` was re-implemented: 66 lines here vs 57 in
the webview build (`launch` + `clipboard_read/write` + the two env knobs). Own
LoC: 12 + 66 = **78**; plus 935 shared = 1013 total. `apps/dioxus-ledger` was not
touched.

Verified on macOS 26.5.2 / M4 Pro. Evidence in `evidence/`: `selftest-log.txt`
(`SELFTEST DONE pass=26 fail=0`, exit 0), ten screenshots, `ax-dump.txt`, and
`log.txt` with the exact command for every step.

## Capabilities

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| Numeric field (typed, filtered input, step keys) | hand-rolled (unchanged) | self-test + synthetic-input | The whole model lives in app.rs (`typeable` filter on `oninput`, `Decimal` on commit) and is renderer-independent, so it ports for free. Typing into a Blitz `<input>` fires `oninput` with the full new value; ↑/↓ stepping commits and the input's text updates from the changed `value` attribute (10). |
| Locale-aware parse + format (toggle live) | hand-rolled (unchanged) | self-test + synthetic-input | 05: one click flips every cell **and every controlled `<input>`'s text** to `1 234,50` / `5 546,77`. Proves Blitz applies `value` attribute updates to a live text input. |
| Decimal alignment / tabular figures | assembled | observed | 08. `font-variant-numeric: tabular-nums lining-nums` and `font-feature-settings: "tnum" 1,"lnum" 1` are parsed by Stylo without error; the two-decimal formatting does the aligning. Whether Parley actually applies the OpenType feature was **not** isolated — the system UI face has uniform digit advance either way, so this row is "requested, and the result is correct", not "feature proven applied". |
| Inline validation UI + disabled Save + error summary | hand-rolled (unchanged) | self-test | `SELFTEST ok  empty Description raises an error / Qty 5000 raises an error / Save is gated (2 errors)`. `button { disabled: … }` renders greyed (01). |
| Dropdown with type-ahead | **not-achievable** | observed | `<select>` **paints nothing** — the Category column is an empty box in every row (01). It exists in the DOM and in the accessibility tree (12 `pop up button`s in `ax-dump.txt`), but there is no popup, no options, no type-ahead, no rendered value. |
| Date input (masked or picker) | **not-achievable** | observed | `<input type=date>` paints an empty box with no value and no picker (01). Same shape as `<select>`: present in the tree, absent on screen. The webview build got a real WebKit date picker here. |
| Slider ↔ numeric field linkage | partially not-achievable | observed + self-test | `<input type=range>` **paints nothing at all** (01 — the toolbar shows "VAT", a gap, then the numeric field). It is in the AX tree as `slider 1`. The linked numeric field works and drives the model (`SELFTEST ok  VAT numeric field drives the model`), so the pair is half-functional. |
| Tab order across mixed controls | **not-achievable** | synthetic-input | 03 / 09: Tab and ⇧Tab move nothing and fire nothing. blitz-dom *does* implement `focus_next_node`/`focus_prev_node` on Tab (`events/keyboard.rs:22`), but (a) it did not move the ring in this document, and (b) `set_focus_to` never calls `generate_focus_events`, so even a successful Tab would deliver **no** `focus`/`blur` to the VirtualDom. Pointer-driven focus changes do fire both. **Verifier note (2026-08-30):** Tab is worse than inert — it *does* move blitz-dom's internal focus (`focus_next_node` → `set_focus_to`), silently. Verifier sequence: type `1234.5` into row 1 Unit price, Tab, click row 2 Description → `FOCUS row 2 Description` fired but the price input never received `blur`, the edit was lost and the locale toggle re-rendered `482,50`; the identical sequence without the Tab (with either osascript keystrokes or CGEvent keys) committed `1,234.50` on the pointer blur. |
| Enter-moves-down cell navigation | not-achievable in practice | synthetic-input | Built on `MountedData::set_focus` via a `focus_map`. `set_focus` itself works (the Edit dialog's autofocus in the sibling SPEC-9 app proves it), but the ring did not move on Enter and no focus event was emitted — for the same "`set_focus_to` dispatches no events" reason, so the app cannot even observe it. |
| Undo/redo (field-level / form-level) | hand-rolled, form-level | self-test | `SELFTEST ok  form-level undo restores Qty / restores Description / Save re-enabled`. Field-level ⌘Z inside a Blitz text input: not verified (the shortcut path goes through the root keydown handler, which is unreachable — see below). |
| Live computed columns and totals | built-in | observed + self-test | Amount / Subtotal / VAT / Total recompute on every committed edit with no visible lag (04, 05, 06). |
| Row copy/paste as TSV | assembled (arboard) | synthetic-input + self-test | 06: a TSV row pasted from the real pasteboard into row 12. `SELFTEST ok  ⌘⇧C put a 6-field TSV row on the pasteboard / ⌘⇧V filled the target row`. Driven by the toolbar buttons: ⌘⇧C does not reach the app (verified — see the keyboard note below). |
| Accessibility labels (verified dump) | partial | observed (`ax-dump.txt`) | AccessKit is on by default and produces a 252-node tree with correct **roles** — 37 `text field`, 12 `pop up button`, 12 `checkbox`, 1 `slider`, 10 `button`. But `AXTitle` and `AXValue` are `missing value` on every field: the `aria-label`s and the values do not reach the OS. Also: the **first** `entire contents` request returns only the window chrome; AccessKit activates lazily and the **second** request returns the tree. |
| IME composition | **not-achievable** | by-construction | `dioxus-native-dom/src/dioxus_document.rs` maps `DomEventData::Ime(_) => None` with the comment "TODO: Implement IME handling", so a composition can never reach the VirtualDom. blitz-dom does drive winit's IME enable/cursor-area, so the OS panel would appear, but the app is deaf to it. Not attempted. |

## Helper crates

Identical set and pins to `apps/dioxus-ledger`, plus the renderer:

| crate | pin | why |
|---|---|---|
| `dioxus-native` | git `64eb2785`, `features = ["prelude"]` | the renderer. crates.io `dioxus-native 0.7.x` is Blitz 0.2 (Oct 2025) and was deliberately not used. Default features kept, so the renderer is Vello Hybrid. |
| `dioxus` | `=0.7.9`, `default-features=false`, `["macro","html","hooks","signals"]` | keeps `use dioxus::prelude::*;` in the shared app.rs. Everything unifies onto one dioxus-core/-html/-signals/-hooks **0.7.10** — the same one dioxus-native uses — so plan option (a) worked with no shim. |
| `rust_decimal` | `=1.39.0` | exact base-10 money arithmetic. Unchanged. |
| `arboard` | `=3.6.1` | whole-row TSV copy/paste. blitz-shell already depends on arboard for `<input>` copy/paste but only exposes it through the private `ShellProvider` context, so the direct call stays. Zero new crates. |
| `tokio` | `=1.52.3`, `["time"]` | paces `LEDGER_SELFTEST`. `dioxus_native::launch_cfg` builds and enters a multi-thread runtime itself (its default `net` feature), so unlike the SPEC-9 crate this one needs no `rt-multi-thread`. |

Nothing was tried and rejected: the first `cargo check` compiled clean.

## Where the time went

Almost none of it went into building the app — `cargo check` passed on the
first attempt and `LEDGER_SELFTEST` printed 26/26 on the first run. All of the
time went into *finding out what Blitz actually does*, because the failures are
silent: an unsupported control renders as an empty box rather than erroring, and
`<input type=range>` renders as nothing at all, so the first screenshot looks
like a layout bug until you check the accessibility tree and find the slider
sitting there. A second sink was distinguishing "the event did not fire" from
"the window was not key": this desktop is shared with six sibling agents whose
windows steal focus constantly, and macOS does not deliver the first click to an
inactive window. Two conclusions had to be revised after re-testing with an
activating click first (blur *does* work; the row remount *does* update the
input text).

## Surprises

**Good.** Zero-diff source sharing between a webview renderer and a native one
is real: parsing, formatting, validation, undo, clipboard, the reject-and-remount
trick — all of it is renderer-independent and all of it passed. Stylo is
genuinely complete for this kind of layout: CSS grid with `minmax()`, sticky
headers, `color-mix(in srgb, …)` for the invalid-field tint, `:focus` outlines
with negative offset, `flex`, `overflow:auto` — nothing had to be simplified.
Parley shapes CJK (世界) and Arabic (مرحبا, right-to-left) correctly from system
fonts with no configuration (06). Selection, caret placement, ⌘A and
click-to-position in text inputs all behave. And the app is **one process**:
no WebContent/GPU/Networking helpers.

**Bad.** The form-control gap is the story: `<select>`, `<input type=date>` and
`<input type=range>` are all DOM-and-a11y-present but paint nothing, which is
three of this spec's fourteen rows. Focus is the second story: `blur` and `focus`
fire on pointer-driven changes but **not** on programmatic `set_focus` and not
on Tab, so every keyboard-navigation feature in the spec is unobservable to the
app even where the ring would move. `<input type=checkbox>`/`radio` dispatch only
`input`, never `change` (`blitz-dom/src/events/pointer.rs:645`), and
`FormData::checked()` is always false because `BlitzInputEvent` carries only a
`value: String` — the Reimbursable checkboxes toggle visually and do nothing to
the model. The ZWJ family emoji 👨‍👩‍👧‍👦 renders as blank space (no colour-emoji
font); CJK and Arabic in the same string are fine. Lastly `@media
(prefers-color-scheme: dark)` follows the document's `color-scheme`, not the OS
— this app gets dark because its CSS declares `:root { color-scheme: light dark }`,
and the sibling SPEC-9 app, which does not, renders light on the same dark-mode Mac.

## The number model

Unchanged from the webview build, and that is the finding: **the number model
never touched the renderer.** The widget carries a `String`, the model carries a
`rust_decimal::Decimal`, and every numeric cell keeps both (`qty_text`/`price_text`
next to `qty`/`price`). There is no constrainable numeric widget in Dioxus/HTML
on either backend — `<input>` only ever hands you the full new string in
`oninput` — so parsing lives in plain Rust functions (`typeable`, `parse_dec`,
`fmt_dec`, `normalize`, `stepped`) that the self-test drives directly. Blitz
changes exactly two things about this. First, rejecting a keystroke still needs
the remount trick (bump a `key`), and it still works, because Blitz honours
`value` attribute updates on a live text input — verified by the locale toggle
rewriting every input's text in place (05) and by ↑/↓ stepping rewriting "6" to
"26" (10). Second, and much more seriously, the *commit* half of the model
depends on `blur`, and on Blitz `blur` only exists for pointer-driven focus
changes: Tab does not blur, and neither does a programmatic `set_focus`. So on
the webview a user can type a number and Tab away and see `1,234.50`; here they
must click somewhere else. `inputmode: "numeric" | "decimal"` is inert (no soft
keyboard on desktop, and no filtering) on both backends.

## Approximated or skipped

- **Category dropdown, Date picker, VAT slider** — `<select>`,
  `<input type=date>` and `<input type=range>` paint nothing in Blitz. Fixing
  this would mean rewriting them as div-based custom widgets in `app.rs`, which
  the sharing rule forbids; reported as three not-achievable rows instead.
- **Tab / ⇧Tab / Enter-moves-down / Esc-reverts** — no focus or blur events
  from non-pointer focus changes. Marked not-achievable with the source
  reference rather than approximated.
- **⌘⇧C / ⌘⇧V / ⌥⌘Z / ⌘⇧Z shortcuts** — **not-achievable, verified.** With the
  window frontmost and `FOCUS row 3 Description` confirming a focused input in
  *our* process, `osascript … keystroke "c" using {command down, shift down}`
  left the pasteboard on its sentinel value and `copy_row` never ran. Command
  chords are consumed by AppKit as *standard key bindings*: blitz-shell routes
  them to `handle_apple_standard_keybinding`, and dioxus-native-dom drops them
  with the comment "AppleStandardKeybinding events are not exposed to script"
  (`dioxus_document.rs`), while blitz-dom additionally intercepts ⌘C itself for
  text-selection copy (`events/keyboard.rs:33-52`). Plain keydowns (typing,
  arrows) do reach the VirtualDom. The same functions were exercised through
  the toolbar buttons and the self-test instead.
- **Field-level undo inside a text input** — not verified.
- **IME** — not attempted; `DomEventData::Ime(_) => None` upstream.
- **Font-feature application** — `tnum`/`lnum` are accepted by Stylo but not
  proven to reach Parley; the column aligns regardless.
- **Save-gating by synthetic input** — covered by the self-test; emptying a
  field with ⌘A + Delete needs the window to be key, which this shared desktop
  would not reliably grant.

## Side-by-side with the webview build

Same machine, same day, same commands.

| | `dioxus-native-ledger` (Blitz) | `dioxus-ledger` (wry/tao) |
|---|---|---|
| clean `cargo build --release --locked` | **75.2 s** | 54.4 s |
| binary | **27,858,960 B** | 6,424,992 B |
| unique crates (`cargo tree -e normal --prefix none \| sort -u \| wc -l`) | **524** | 380 |
| idle RSS | **112 MB** | 88–98 MB (own process) + 79 MB across 3 WebKit helpers |
| processes | **1** | 1 + 3 `com.apple.WebKit` XPC helpers (GPU / Networking / WebContent) = 4 |
| own LoC (main + platform) | 78 | 65 |
| shared `app.rs` | 935, identical file | 935 |
| self-test | 26/26 pass | 26/26 pass |

(The 293-crate figure quoted for `dioxus-ledger` in the brief comes from a
different counting command; 380 vs 524 above are both measured the same way.)
