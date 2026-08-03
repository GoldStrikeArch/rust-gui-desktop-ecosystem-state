# FRICTION — Babel (vizia =0.4.0), SPEC-5

Reference machine per spec (M4 Pro, 3024×1964 Retina). Built + verified on
macOS 26.5.2 with rustc 1.96.1: `cargo build --release` and
`cargo build --locked --release` clean, binary launched, window
pixel-verified, alive past the 10 s bar, killed cleanly. Total LoC **241**
(single `src/main.rs`; ~70 of those are the opt-in `BABEL_SELFTEST` probes
and the `EDIT …` instrumentation, so production is ~170).

`screenshot.png` = window-scoped `screencapture -l` of the release build at
the spec's **800×600, unresized** — all 11 corpus lines are visible and
unwrapped at that size, plus the editing pane seeded with the [MIXED] line.
Read back visually: no tofu anywhere.

Evidence labels: **observed** (read off the capture), **self-test** (in-app
probes on stderr, driven by CGEvent keystrokes scoped to this window),
**source-only**, **unexercised**.

## The text stack

vizia renders through **Skia**, and its text layer is `SkParagraph`
(`skia-safe 0.93.1` built with the `textlayout` feature) sitting on the
platform font manager — CoreText on macOS. So shaping (HarfBuzz inside
Skia), BiDi resolution and per-script fallback are all the *system's*, not a
Rust reimplementation. There is no cosmic-text/fontdb layer to configure and
no font database to warm up. **Zero i18n work was required.**

## Ratings

| capability | rating | evidence | note |
|---|---|---|---|
| bidi_render | **built-in** | observed | [AR] and [HE] read right-to-left; the embedded `English words`, the Western digits `456`/`789` and the Arabic-Indic `١٢٣` are all visually ordered correctly, and the bracketed `[AR]`/`[HE]` tags stay at the visual left because the paragraph's base direction is derived per paragraph from its first strong character. No API is exposed for forcing base direction. |
| cjk_render | **built-in** | observed | [ZH]/[JA]/[KO] render from system fonts with no configuration; full-width punctuation, 「brackets」 and the `……` ellipsis are all correct. No tofu. |
| emoji_zwj | **built-in** | observed | 👨‍👩‍👧‍👦 renders as **one** glyph (Apple Color Emoji's current silhouette-style family design), not four split heads. Skin tone applied (👍🏽, 👩🏾‍🚀); 🏳️‍🌈, 🇺🇳 and 🇷🇸 are single flag glyphs; the inline `e😀mo👩🏾‍🚀ji` run does not break shaping of the surrounding Latin. |
| mixed_fallback_line | **built-in** | observed + self-test | The [MIXED] line renders Latin + Han + Arabic + Hebrew + Devanagari + Thai + Hangul + a ZWJ emoji in one paragraph with no tofu and no visible seam. The probe confirms it really is 8 distinct scripts in 114 bytes / 52 grapheme clusters (`PROBE line=MIXED … scripts=8`). |
| grapheme_caret | **built-in for motion / hand-rolled edge for delete** | self-test | Split verdict, proven by byte counts from the `EDIT` log. **Caret motion and selection are grapheme-atomic:** from `"a👨‍👩‍👧x👨‍👩‍👧‍👦"` (45 bytes, 4 clusters), two Shift+Left presses followed by Backspace removed exactly 26 bytes — the whole 25-byte family cluster plus `"x"` — leaving 19 bytes. **Plain Backspace is not:** at the end of `"a👨‍👩‍👧‍👦"` (26 bytes) one Backspace produced 22 bytes (dropped only U+1F466) and the next produced 19 bytes (dropped only the ZWJ), corrupting the cluster. Identical failure mode to the iced cohort: the delete path walks scalars while the movement path walks graphemes. |
| selection | **built-in** | self-test + observed | Mouse click and drag select in the editor; Shift+Left/Right extends by whole grapheme clusters (proven above); ⌘A selects all and ⌘C/⌘V round-trip through the system clipboard (vizia's default `clipboard` feature). Selection across the BiDi boundary of the [MIXED] line stays contiguous in logical order. |
| ime | **built-in (unexercised)** | source-only | vizia has full IME plumbing: `WindowEvent::{ImeActivate, ImePreedit, ImeCommit, SetImeCursorArea}` and `Textbox` keeps a `preedit_backup` with `TextEvent::{UpdatePreedit, ClearPreedit}`, driven from winit's `Ime` events. Not exercised: the reference machine has no CJK input source installed and installing one silently was out of scope, and the dead-key path could not be driven reliably under synthetic input on a shared desktop. |
| large_doc_scroll | **built-in, expensive** | self-test | "Load big doc" builds 11 × 1000 = **11,000 lines** (`BIGDOC lines=11000 generate_ms=0.93` — the *data* is free) as 11,000 `Label` views inside one `ScrollView`. The window became responsive again after **~1.2 s wall**, and scrolling afterwards was smooth. The cost is memory: RSS **114.9 MiB → 1005.2 MiB**, i.e. ≈ **82 KiB of RSS per Label** for a one-line label, and flat across a long scroll (1005.2 → 1005.6 MiB). This path was deliberately *not* virtualized: vizia ships `VirtualList`/`VirtualTable` (used in apps/vizia-grid), but putting them here would measure the virtualizer rather than the text stack, which is what SPEC-5 is about. The finding is that vizia's per-view overhead — entity + style store + Skia paragraph cache — is what makes a naive 11k-view document expensive, not the text shaping. |
| fonts_bundled | **none** | observed | **0 bytes bundled.** Latin, Arabic, Hebrew, Han, Kana, Hangul, Devanagari, Thai, combining marks and colour emoji all resolved from macOS system fonts through Skia's CoreText font manager. Nothing was configured; no `FontMgr`, no font database, no fallback list. |

## Helper crates

- `unicode-segmentation 1` — **verification only**, for the grapheme-cluster
  counts printed by the `BABEL_SELFTEST` probes. It is already in vizia's own
  dependency tree (`vizia_core::text::EditableText` uses it), so declaring it
  directly adds nothing to the build.

Nothing for text or i18n.

## Where the time went

1. Writing probes that are *falsifiable*. Vizia's `Movement`/`Direction`
   types live in a `pub(crate)` module, so `TextEvent::MoveCursor(..)` and
   `TextEvent::DeleteText(..)` cannot be constructed from application code —
   which is arguably correct, but it means the caret/grapheme behaviour has
   to be probed through **real key events** and read back through the
   `on_edit` byte counts. That turned out to be a better test than poking the
   widget's internals would have been.
2. Nothing else. The corpus rendered correctly on the first run with no font
   configuration at all.

## Surprises

- Good: the entire i18n story is "Skia + CoreText", so it is exactly as good
  as the platform. Nothing to bundle, nothing to configure, no first-paint
  font-database stall.
- Good: selection/caret motion is genuinely grapheme-atomic, which not every
  toolkit in this cohort manages.
- Bad: Backspace deletes per scalar and will happily leave a dangling ZWJ.
  Same bug class as iced, from a completely different text stack.
- Bad: ~82 KiB RSS per `Label`. A naive 11k-line document costs a gigabyte;
  the framework's answer (`VirtualList`) is good, but the naive path is much
  more expensive here than in the immediate-mode members of this cohort.
- Neutral: no API to force paragraph base direction, so a line beginning with
  an ASCII tag is always laid out LTR-base. Fine for this corpus; would not
  be for a real RTL app.
