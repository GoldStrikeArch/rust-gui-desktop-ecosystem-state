# Text & i18n: "Babel" in seven frameworks (macOS)

**Run date:** 2026-07-09.

Iteration 3, SPEC-5 (`apps/SPEC-5.md`): the same 11-line multilingual corpus
(`apps/babel-assets/corpus.txt` — Latin ligatures, Arabic/Hebrew BiDi with
embedded English and both digit systems, Chinese/Japanese/Korean, Devanagari,
Thai, ZWJ/skin-tone/flag emoji, one all-scripts line, combining-diacritics
stress) rendered, edited, and screenshotted in every framework. Screenshots:
`apps/<fw>-babel/screenshot.png` (downscaled gallery copies in
`report/data/shots/`). Verification combined screenshots, OCR bounding boxes,
CoreText caret probes, glyph x-coordinate diagnostics, in-process events,
synthetic input, and source inspection because reading RTL screenshots by eye
proved unreliable. Evidence levels differ by cell and are stated in each
FRICTION file. Not every raw probe output was retained, so exact diagnostics
without a checked-in trace remain local observations.

Evidence labels used below: **observed** (screen/OS result retained),
**self-test/synthetic**, **source-only**, and **unexercised**.

## The rendering matrix

| Capability | iced | egui | gpui | tauri | xilem | slint | dioxus |
|---|---|---|---|---|---|---|---|
| BiDi paragraphs | **correct** | **broken**¹ | **correct**² | correct | **correct** | **correct**³ | correct |
| CJK | correct | correct (bundled)⁴ | correct | correct | **broken**⁵ | correct | correct |
| ZWJ/skin/flag emoji | correct | monochrome⁶ | correct | correct | flags tofu⁷ | correct | correct |
| All-scripts line | correct | correct (bundled) | correct | correct | 世界 tofu⁵ | correct | correct |
| Combining stress | correct | **13 marks dropped** | correct | correct | correct | **1 notdef; tall stacks clip** | correct |
| Fonts bundled | 0 B | **18.1 MiB**⁴ | 0 B | 0 B | 0 B⁵ | 0 B | 0 B |

¹ egui has no paragraph BiDi reordering (epaint TODO): harfrust shapes each *word* correctly, so
single RTL words look right, but **multi-word Arabic/Hebrew sentences read
backwards** and **Arabic-Indic digit runs mirror (١٢٣ → ٣٢١)** — a
severe visual ordering error that's easy to miss in review precisely because
short strings look fine (glyph x-coordinates verified).
² Revises iteration 1: gpui renders **full correct BiDi on macOS for free**
(its mac backend paints CoreText's CTLine visual positions) despite RTL being
"officially unsupported" (zed#31102) — the actual deficit is caret/selection
geometry, which assumes logical==visual order (a selection endpoint inside an
RTL run highlights the wrong cells).
³ Revises iteration 1: Slint's BiDi *run reordering* is correct (verified via
OCR bounding boxes against the UBA). slint#1317 is closed; current paragraph
base-direction/alignment/UI-mirroring work is tracked by slint#2294.
⁴ egui discovers no system fonts: rendering the corpus required bundling six
Noto fonts, **18,963,776 bytes (15.7 MiB of it CJK)**, pushing the binary to
30.2 MiB; fallback order is hand-maintained and load-bearing.
⁵ **Candidate upstream issue in the tested stack**: fontique 0.6, as pinned by
Xilem 0.4, only scans
`*/Library/Fonts`, but macOS 26 stores PingFang (the Han fallback) in
`/System/Library/AssetsV2/…` on-demand storage — so Han fallback silently
returns None: Chinese is tofu, Japanese kanji are tofu while kana render,
Korean is fine. It affects Xilem 0.4 here. Current fontique must be retested
before generalizing the defect to later Parley/fontique consumers; Slint
1.17's newer integration was unaffected.
⁶ egui's rasterizer is outline-only: ZWJ sequences genuinely ligate (family =
one glyph, probe-verified) but render monochrome; regional-indicator flags
show as boxed letters. Color emoji (COLR/sbix) unsupported.
⁷ xilem/parley: color emoji work (family, skin tones, 🏳️‍🌈) but
regional-indicator flag pairs never ligate → tofu pairs.

Note for screenshot comparisons: on macOS ≥14.4, Apple's *correct* family
emoji is a single duotone silhouette glyph by design — don't misread it as a
fallback artifact.

## The editing matrix (the harder half)

| Capability | iced | egui | gpui | tauri | xilem | slint | dioxus |
|---|---|---|---|---|---|---|---|
| Caret moves by grapheme | yes | **no** (scalar) | yes (own editor) | yes | **no** (scalar) | yes | yes |
| Backspace over 👨‍👩‍👧‍👦 | **splits cluster**¹ | **splits cluster** | deletes whole² | deletes whole | **splits cluster** | shrinks by design³ | deletes whole |
| Selection sanity (BiDi) | correct | correct-by-accident⁴ | highlight splits⁵ | correct | correct | correct | correct |
| IME plumbing | dead-key observed | source-only | dead-key observed | WebKit path, unexercised | source-only | source-only | WebKit path, unexercised |

¹ The tested stacks exhibited **three observable outcomes** across four
implementation paths on a ZWJ cluster: WebKit (tauri/dioxus) and gpui's
hand-rolled unicode-segmentation editor delete the whole cluster; Slint deletes
codepoint-by-codepoint *by design*, passing through valid smaller emoji; iced
(cosmic-text), xilem (parley), and egui delete one scalar and leave an
incomplete/dangling ZWJ sequence. The last outcome splits the displayed
grapheme but does not corrupt the underlying UTF-8 buffer.
Grapheme-cluster editing is therefore a concrete gap in these stock editor
integrations and versions, not a claim that the layout engines can never
support it.
² GPUI 0.2.2 ships no reusable first-party high-level text-input widget;
every editing behavior here is the 836-line hand-rolled editor's, not a stock
framework control.
³ Arrow keys move by grapheme while backspace deletes by codepoint — jarring
but non-corrupting.
⁴ egui's selection is trivially contiguous across the "BiDi boundary" only
because its rendering is visually logical-ordered (i.e., wrong).
⁵ gpui's `x_for_index` assumes monotonic logical→visual order, so a selection
endpoint inside an RTL run divorces highlight from content.
Also found: **macOS AX selected-text queries returned garbled RTL selections for egui**
(`AXSelectedText` returns doubled/reordered characters while ⌘C is
byte-exact). Actual screen-reader speech/output was not exercised, so
assistive-technology impact is inferred.

## Large-document implementation outcomes (~11k mixed-script lines)

This was not a controlled apples-to-apples framework benchmark, and many exact
timing/RSS values cannot be recomputed because their raw traces were not
retained:
implementations used virtual lists, one text node, 11,000 DOM nodes, or one
monolithic prose scene, and some RSS values omit WebKit helper processes. The
table records the chosen implementation outcome, not an intrinsic framework
limit.

| Framework | Mechanism | Result |
|---|---|---|
| gpui | `uniform_list` (virtualized) | instant load, +4 MiB app RSS, smooth full scroll in ~4 s |
| slint | `ListView` (virtualized) | 120 µs model swap, steady scroll, app RSS 33→104 MiB |
| egui | `show_rows` (virtualized, opt-in) | smooth, app RSS ~+2 MiB; naive loop would re-shape every frame |
| tauri | DOM | locked 60 fps (149-frame probe), app RSS ~121 MiB (helpers excluded) |
| dioxus | DOM | 60 fps sustained; rAF throttles to ~12 fps unfocused |
| iced | none (no virtualization) | 803 ms load freeze, app RSS 97→410 MiB, then smooth 118 fps |
| xilem | `prose` (naive; `virtual_scroll` exists, untested) | local path reproduced an abort twice: app RSS→613 MiB then wgpu validation kill (192 MiB scene > 128 MiB buffer limit) |

## What the round settled

1. **Rendering is closer to solved than the fragmentation map implied**: 5 of
   7 frameworks rendered every script category without a major missing-script
   gap and with zero bundled fonts, but Slint still showed a rare notdef/clip
   and egui dropped combining marks.
   Two iteration-1 claims got *revised upward* (gpui and Slint BiDi are
   correct); the real outliers are egui (architectural: no BiDi, no system
   fonts, no color emoji) and a single fixable fontique discovery bug.
2. **Editing is where these integrations were fragile**: only the webviews
   (and one hand-rolled editor) handle grapheme clusters correctly on
   deletion. The tested Iced/cosmic-text and Xilem/Parley editor paths left
   dangling ZWJs on backspace — a concrete, version-scoped upstreamable item.
3. **Implementation choice made the difference between fine and fatal** here —
   from GPUI's +4 MiB to the tested Xilem prose path's wgpu validation error,
   with Iced's +313 MiB in between. Xilem has `virtual_scroll`, but this app did
   not test it.
4. Two upstream candidates need minimized reproductions and issue links:
   fontique 0.6's AssetsV2 blind spot and egui's macOS AX RTL selection result.

## Caveats

Same as SPEC-4: macOS only; CJK IME could not be activated on this machine
(no CJK input source) — IME verified via dead-key composition and source
inspection; shared-desktop verification races were compensated with
sentinels/pixel checks and flagged where residual. Exact FPS/freeze/RSS/OCR
values without retained raw output should be treated as local observations;
future runs should store them alongside `measurements/EVIDENCE.md`.
