# FRICTION — slint-babel (Babel, Slint =1.17.1)

Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
Default slint features (winit backend, femtovg GL renderer). Since 1.14 all
renderers share one text stack: **fontique (system font discovery/fallback) +
Parley (layout/BiDi) + HarfRust (shaping)**; swash rasterizes glyphs on the
FemtoVG/software path. This is confirmed in the 1.17.1 crate metadata
(`shared-parley`/`shared-fontique` features are on by default).

Headline: **zero fonts bundled, zero text-specific code written**, with all
tested scripts, CJK, emoji and UBA BiDi ordering rendered successfully. One
rare combining mark displayed as notdef and tall combining stacks clipped at
the row height, so the complete corpus is not literally defect-free. The
iteration-1 research predicted weak BiDi, but the 1.17.1 on-screen result
contradicted that expectation.

UI shape: stacked panes (rendering pane = virtualized `ListView` with one
`Text` per paragraph; editing pane = `TextEdit` std-widget). One deliberate
consequence: the rendering pane is per-line Text elements, so this measures
paragraph-level rendering, not one giant text block.

Verification method notes: the desktop was shared with 6 parallel framework
agents that stole focus/clipboard continuously, so editor behavior was tested
in-process by dispatching `slint::platform::WindowEvent` key/pointer events
(the same pipeline the winit backend feeds) and reading the text property
back — deterministic and OS-focus-proof. BiDi run order was verified by
Vision-OCR token bounding boxes on the screenshot rather than by eyeballing.
The screenshot and in-app probe hooks remain, but the raw probe session output
was not retained; exact interaction sequences below are narrative evidence,
not an automated regression suite.

## Capability ratings

| Capability | Rating | Notes |
|---|---|---|
| bidi_render | **built-in** | Measured, not assumed: OCR token x-positions on the [AR]/[HE] rows match the Unicode BiDi algorithm output exactly — RTL runs reversed per-run, embedded "English (words)" runs kept LTR in their logical slot, European digits (456/789) and Arabic-Indic digits (١٢٣) upright and correctly ordered inside the RTL runs, trailing period at the line end. [MIXED]'s adjacent Arabic+Hebrew words render as one reversed R run ("שלום مرحبا" for logical "مرحبا שלום") — correct. Caveats: no `text-direction`/base-direction property exists, so an RTL-dominant paragraph still left-aligns (base direction is effectively LTR), and [#2294](https://github.com/slint-ui/slint/issues/2294) tracks RTL alignment and UI mirroring — but plain BiDi *rendering* is right. |
| cjk_render | **built-in** | [ZH]/[JA]/[KO] all perfect via fontique system fallback (PingFang/Hiragino/Apple SD Gothic picked up automatically): full-width punctuation, 「引号」, ……, kana/kanji mix, Hangul — no tofu anywhere. |
| emoji_zwj | **built-in** | 👨‍👩‍👧‍👦 renders as ONE color glyph (the macOS-26 boxed-silhouette family design from Apple Color Emoji — a single cluster, not 4 heads). 👍🏽 skin tone applied; 🏳️‍🌈 ZWJ flag correct; 🇺🇳/🇷🇸 regional-indicator flags correct; inline e😀mo👩🏾‍🚀ji (skin tone + ZWJ astronaut) correct mid-word. |
| mixed_fallback_line | **built-in** | The [MIXED] line renders Latin+CJK+Arabic+Hebrew+Devanagari+Thai+Hangul+emoji in one paragraph with per-run font fallback and correct BiDi — no tofu, no spacing glitches. Bonus: [HI] conjuncts (क्ष त्र ज्ञ श्री द्ध) are true ligated conjunct forms; [TH] stacked vowels fine; [COMBINING] Zalgo-lite stacks render (one rare combining mark in the Zalgo cluster falls back to a small notdef box, and tall stacks clip at the 26px row height). |
| grapheme_caret | **built-in** (with a deliberate caveat) | Split verdict, verified in-process: **arrow movement is grapheme-correct** — 2×Left from the end of "a👨‍👩‍👧‍👦b" put the caret before the whole family, and inserting X yielded "aX👨‍👩‍👧‍👦b". **Backspace deletes by codepoint on purpose**: 7 presses to remove the family (source comment in i-slint-core items/text.rs: "backspace breaks the grapheme and selects the previous character"). Intermediate states re-render as progressively smaller valid emoji (👨‍👩‍👧 …) — no string corruption. Shift+arrow selection is grapheme-stepped. |
| selection | **built-in** | Sane across the BiDi boundary, both keyboard and mouse (verified in-process): Shift+Left×5 from the end of "abc שלום def" selected exactly the last 5 graphemes logically ("ם def" → typing Y gave "abc שלוY"); a mouse drag across "[MIXED] Mixed: Hello 世界 مرحبا שלום …" deleted one contiguous logical range spanning Latin→CJK→Arabic→Hebrew. Selection is logical-order (split visual highlights over BiDi text — the standard correct behavior). Copy/paste round-trip through the real system clipboard: "קפה coffee 커피" survived select-all→copy→paste byte-identical. |
| ime | **built-in (source-verified, not exercised)** | Untestable here: the machine has no CJK input source enabled, and silently switching the user's input sources mid-session was out of bounds. Plumbing exists end-to-end in source: the winit backend forwards `Ime::Preedit`/`Ime::Commit` (`event_loop.rs`) and TextInput has preedit state; Slint documents IME support on macOS. No helper crate or app-level assembly was required. |
| large_doc_scroll | **built-in** | The contemporaneous in-app probe reported a 120 µs model swap, no missed 160 ms scroll steps, and roughly 33→106 MiB main-process RSS for 11,000 virtualized rows. Its raw output was not retained, so those exact figures are narrative evidence; the reproducible architectural finding is that `ListView` instantiates only visible rows and the tested run completed without a stall/crash. A single 11k-line `TextEdit`/`Text` block would be a different path. |
| fonts_bundled | **none** | System fonts + fontique automatic fallback covered every script and emoji. 0 bytes bundled. |

## Helper crates

None. (`slint` + `slint-build` only.)

## LoC

- Rust: 223 (`src/main.rs`, of which ~95 are the two env-var test hooks) + 3 (`build.rs`)
- Slint DSL: 86 (`ui/main.slint`)
- Total: 312

## Screenshot

`screenshot.png` — window-region capture (light mode) showing all 11 corpus
lines plus the editor seeded with [MIXED]; PNG read back and verified
visually, with per-row magnified crops inspected for AR/HE/HI/EMOJI/MIXED/
COMBINING.

## Measurements

- Canonical clean release build **48 s**; no-op incremental build **4 s**.
- Dependency graph: **306 unique crate names / 315 name-version entries
  including the app**.
- Binary: **15,445,616 bytes raw / 13,910,440 bytes (13.3 MiB) stripped**.

## Where the time went

1. ~40% verifying BiDi honestly: repeated misreadings of reshuffled Arabic at
   screenshot scale forced the Vision-OCR bounding-box method; the happy
   ending (it's correct) took the longest to prove.
2. ~25% editor tests: discovering that AX-tree clicks don't grant Slint
   keyboard focus and OS keystrokes were being stolen by sibling agents →
   rebuilt as in-process `slint::platform::WindowEvent` dispatch. Also
   discovered `Key::End`/`Home` are cfg'd out on Apple targets (had to use
   select-all + RightArrow as "go to end").
3. ~15% screenshot logistics on a desktop with 6 other test apps popping
   windows and crash dialogs over mine.
4. Rest: the app itself (~30 min) — the text stack needed nothing.

## Surprises

- Good: BiDi run reordering is simply correct in 1.17.1 — contradicts the
  iteration-1 "RTL is weak" expectation for plain rendering (open
  [#2294](https://github.com/slint-ui/slint/issues/2294) is about base
  direction/alignment and UI mirroring, not UBA reordering).
- Good: 11k mixed-script lines are a non-event for ListView — 120 µs model
  swap, on-budget scrolling, ~70 MiB extra RSS.
- Bad: backspace intentionally deletes by codepoint (grapheme-splitting) —
  defensible for text entry correction, but surprising after arrows move by
  grapheme.
- Bad: `Key::End`/`Home` silently do nothing on Apple targets and there's no
  documented "move to end" key substitute for synthetic input.
