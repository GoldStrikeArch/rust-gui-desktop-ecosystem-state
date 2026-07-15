# FRICTION.md — Babel (egui 0.35 + eframe 0.35, macOS)

App: `apps/egui-babel/` · package `egui-babel` · `cargo run --release`.
Verified on macOS: release build clean, launched, alive after 10 s, killed.
Screenshot `screenshot.png` shows all 11 corpus lines (dark theme) + the
seeded editor. Verification was three-layered: (1) the screenshot, (2) glyph
-coordinate probes against epaint's real layout engine (`cargo test --
--nocapture`, in `src/main.rs` `#[cfg(test)]` — RTL screenshots are easy to
misread, glyph x-positions are not), (3) live editor interaction scripted
via AX (AccessKit tree) + synthetic keyboard/mouse events, with the editor
contents read back byte-exact through the AX `value` attribute.

The five `#[test]` functions are **print-only diagnostics**: they exercise the
real layout engine and emit glyph/order data under `--nocapture`, but contain
no expected-value assertions. A passing `cargo test` therefore means the
probes ran without panicking, not that text correctness was automatically
verified; the recorded values were interpreted manually.

## The headline finding: fonts

**egui has NO system-font discovery and NO fallback to system fonts.** Every
non-Latin script renders as tofu until you bundle a font for it and
hand-order the fallback list. Bundled for this corpus (all in the binary via
`include_bytes!`):

| Font | Bytes |
|---|---|
| NotoSansCJKsc-Regular.otf | 16,437,364 |
| NotoEmoji-Regular.ttf (full, monochrome) | 1,982,596 |
| NotoSansDevanagari-Regular.ttf | 244,284 |
| NotoSansArabic-Regular.ttf | 234,892 |
| NotoSansThai-Regular.ttf | 37,780 |
| NotoSansHebrew-Regular.ttf | 26,860 |
| **Total (6 fonts)** | **18,963,776 B ≈ 18.1 MiB** |

Release binary: 31,674,480 B (31.67 MB / 30.21 MiB), i.e. ~60% of it is
fonts, and that covers only this corpus — the CJK font alone is 16,437,364 B
(16.44 MB / 15.68 MiB), and full Noto coverage of Unicode
would be far larger. Fallback resolution is first-face-in-list-that-has-the-
char, so ORDER is load-bearing: the full NotoEmoji must be inserted before
epaint's built-in NotoEmoji subset (or ZWJ GSUB tables are missing), and
CJK must go last (it contains Latin glyphs that would otherwise shadow the
default face). Getting this list right took real iteration.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| bidi_render | **not-achievable** | epaint 0.35 has no Unicode BiDi at all (`// TODO(emilk): heed bidi characters`, epaint `text/font.rs:830`). Nuance found by glyph-x probes: layout splits per whitespace-word and harfrust shapes each word with correct direction, so a single Arabic/Hebrew word renders internally correct (joining + RTL letter order — the [MIXED] line looks right!), but multi-word RTL sentences come out with the words marching left-to-right in logical order, i.e. the [AR]/[HE] lines read backwards. Worse, Arabic-Indic digits glued to an Arabic word get reversed with it: logical ١٢٣ renders as ٣٢١ (verified x(٣)=325 < x(٢)=334 < x(١)=342), while free-standing ASCII "456" stays correct. No helper crate can fix this — it's inside epaint. |
| cjk_render | **assembled** | [ZH]/[JA]/[KO] rendered with no observed tofu in the local screenshot, but only because the 15.68 MiB NotoSansCJKsc font is compiled into the binary. Nothing is built in; removing the bundle makes the tested CJK characters fall back to missing-glyph boxes. |
| emoji_zwj | **assembled** | Genuinely good news: epaint 0.35 shapes with `harfrust` (Rust HarfBuzz port), and GSUB works — 👨‍👩‍👧‍👦 (7 scalars) ligates to ONE advancing glyph, 👍🏽 applies the skin tone as one glyph, 🇺🇳/🏳️‍🌈 ligate too (probe: advance patterns 1-advancing + zero-width continuations; `ffi` also ligates). BUT rendering is outline-only — no COLR/CBDT/sbix — so all emoji are MONOCHROME line art (bundled Noto Emoji), and RIS flags render as Noto's fallback boxed letters ("UN", "RS"), not flag images. Color emoji: not achievable in 0.35. |
| mixed_fallback_line | **assembled** | The [MIXED] line renders every script in one paragraph, no tofu — via the hand-ordered 6-font fallback list above. Caveat discovered on [COMBINING]: 13 combining marks (U+0308 etc. — Zalgo/decomposed diacritics) exist in NO bundled font and are dropped SILENTLY — no tofu box, no stacking; "Z̷̢̈a̶͇͐l̶̠̏g̷̻̈ó̸̗" renders as plain "Zalgó". Precomposed ë is fine; decomposed e+◌̈ loses the mark. |
| grapheme_caret | **not-achievable** | egui's TextEdit caret is Unicode-scalar-based, not grapheme-based. Live-verified with probe text `ab👨‍👩‍👧‍👦cd`: ArrowLeft×3 from end puts the caret INSIDE the cluster (typing there yields `ab👨‍👩‍👧‍|👦cd`, and the on-screen ligature visibly splits into family-of-3 + boy); Backspace deletes exactly one scalar per press — first press leaves `ab👨‍👩‍👧‍` (dangling ZWJ, glyph silently reshapes to a 3-person family), 7 presses to remove the emoji. Source confirms: `cursor_left/right_one_character` is `index ± 1` (epaint `text_layout_types.rs:1317`), `delete_previous_char` removes one char (egui `text_buffer.rs:133`). No corruption/panic — the galley re-shapes whatever scalars remain — but macOS-native editing behavior it is not. |
| selection | **built-in** (scalar granularity) | Mouse drag selection works (verified with synthetic CGEvent drag: contiguous highlight across Latin→CJK→Arabic→Hebrew→Devanagari; copied span byte-exact: `lo 世界 مرحبا שלום नमस्ते ไท`). Shift+Arrow extends by one scalar (Shift+←×3 from end of `…👦cd` selects exactly `👦cd` — can select HALF a ZWJ cluster), Alt+Shift+→ word-extends, ⌘A works. "Across the BiDi boundary" is trivially sane because there is no reordering: visual order = logical order, so selections are always visually contiguous. |
| ime | **built-in** (code-level verification only) | Full plumbing exists: winit `Ime::Preedit/Commit` → egui `Event::Ime` → TextEdit renders preedit composition (`egui-winit/src/lib.rs:698-745`, egui `text_edit/builder.rs:1166-1240`, incl. macOS-specific empty-Preedit/Commit cancel handling). Activating a real CJK IME requires switching the system input source, which is not safely scriptable (shared desktop) — not exercised live. |
| large_doc_scroll | **built-in** (opt-in) | "Load big doc" = corpus ×1000 = 11,000 lines. With `ScrollArea::show_rows` (virtualized, only visible rows shaped), heavy synthetic wheel-scrolling (~40k px total) kept up and RSS stayed flat in the local observation — 117 MiB before load → 119 MiB after load+scroll (the 11k-line `Vec<String>` is ~2 MiB). Caveats: virtualization is not automatic — a naive `for line { ui.label(line) }` lays out all 11k rows every frame — and `show_rows` assumes uniform row height, which multilingual text violates slightly (Thai/Devanagari rows are taller). No raw frame-time/RSS trace was retained, so the exact values remain narrative observations. |

## fonts_bundled

6 fonts, 18,963,776 bytes ≈ 18.1 MiB (see table above). System fallback:
does not exist in egui 0.35 (`FontDefinitions` only knows bundled data;
epaint issue #1016 tracks BiDi, system-font discovery is likewise absent).

## Helper crates

- `unicode-segmentation = 1` — diagnostics only: the editor status line
  shows scalar vs grapheme counts so caret/backspace behavior is observable.
  (Would also be the building block if you hand-rolled a grapheme caret.)
- No rendering helper exists that could add BiDi/color-emoji from outside;
  those live in epaint itself.

## Other findings

- **AccessKit/AX RTL text was garbled in this experiment**: reading the editor's `AXSelectedText`
  for a selection containing Arabic returns doubled/reordered chars
  (`مرحبا` comes back as `اببحرمرحبا`) while ⌘C of the same selection is
  byte-exact; `AXSelectedTextRange`/`AXNumberOfCharacters` report
  inconsistent units and AX `SetTextSelection` writes are ignored. This shows
  that the tested AX value path is unsuitable for that selection; actual
  screen-reader speech was not tested, so its output is not claimed.
- Latin ligatures (fi/ffi) apply by default via harfrust — nice typography,
  but it means even `fin.` is one-glyph-per-cluster territory; all cursor
  geometry still works (epaint has explicit grapheme-cluster cursor
  round-trip handling for positions — movement granularity is the gap).
- Synthetic input to eframe is flaky: CGEvents/System Events key codes were
  intermittently dropped or delivered late in bursts (typed-unicode events
  always arrived; `postToPid` never did). Worked around by batching each
  test into a single activation window with per-step AX readbacks.

## LoC / time

314 LoC single file (app ~170, `#[cfg(test)]` glyph probes ~140 — the
probes are part of the deliverable: they turn "does BiDi work" from
squinting at screenshots into sorted glyph x-coordinates). Time split:
~30% font hunting/bundling and fallback-order iteration, ~40% scripting
live editor verification around flaky synthetic input (the AX value
read-back channel was the unlock), ~20% glyph-coordinate probes, ~10% app
UI (trivial).
