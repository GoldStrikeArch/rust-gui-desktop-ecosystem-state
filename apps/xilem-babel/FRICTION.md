# FRICTION — xilem-babel ("Babel", xilem 0.4.0 from crates.io)

Observed on macOS 26.5 (M4 Pro): release build, 10 s launch checks, retained
`screenshot.png` of all 11 corpus lines, and a contemporaneous scripted editor
run using synthetic CGEvent input and the system clipboard. The editor harness
and raw output were not retained, so interaction details below are narrative;
the screenshot remains directly inspectable. Text stack under test: Masonry
0.4 TextArea → Parley 0.6 + fontique 0.6 → Vello 0.6 / wgpu 26.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| bidi_render | **built-in** | Flawless. [AR]/[HE] verified glyph-by-glyph from zoomed slices: LTR paragraph base (first-strong `[AR]`/`[HE]` tag), RTL runs correctly reversed (first Arabic/Hebrew word at the run's visual right), embedded `English words` between the runs with correct neighbors, digits `456`/`789` in LTR order and `١٢٣` Arabic-Indic digits rendered, trailing period resolved to paragraph level (far visual right). [MIXED] also orders its Arabic+Hebrew run correctly (שלום left of مرحبا). |
| cjk_render | **not-achievable** (as shipped) | The headline. [ZH] is near-total tofu; [JA] kanji are tofu while kana render; [KO] Hangul is perfect. Root cause found in vendored fontique 0.6 `backend/coretext.rs`: `SystemFonts::new()` scans only `*/Library/Fonts/` directories, then `fallback()` asks CoreText (`CTFontCreateForString`) for a covering font and looks the *family name* up in that scan. On macOS 26 PingFang lives in `/System/Library/AssetsV2/com_apple_MobileAsset_Font7/` (on-demand font assets), not `/System/Library/Fonts/` — so CoreText's "PingFang SC" answer resolves to `None` and Han silently falls through to tofu. Every other script's fallback font still lives in `/System/Library/Fonts` (GeezaPro, SFHebrew, AppleSDGothicNeo, Thonburi, Kohinoor…) which is why *only* Han dies. Fix would be bundling a Noto CJK subset and registering it in the fontique `Collection` (not wired in this app — documented as the gap instead). |
| emoji_zwj | **built-in** (except flags) | 👨‍👩‍👧‍👦 renders as ONE color glyph (Apple's current silhouette-style family design) at native res, in the [EMOJI] line, the [MIXED] line and the editor. Skin tone applied (👍🏽), 🏳️‍🌈 rainbow-flag ZWJ correct, inline 😀 and 👩🏾‍🚀 correct single color glyphs. BUT country flags 🇺🇳 🇷🇸 are two pairs of tofu boxes — regional-indicator pairs never ligate into flag glyphs (Apple Color Emoji implements them via AAT tables the swash pipeline doesn't apply here). |
| mixed_fallback_line | **not-achievable** (as shipped) | Same single root cause as cjk_render: in "[MIXED] Hello 世界 مرحبا שלום नमस्ते ไทย 한글 👨‍👩‍👧‍👦 fin." everything renders — correct BiDi order, Devanagari conjuncts, Thai, Hangul, ZWJ emoji — except 世界, which is two tofu boxes. One paragraph, ten scripts, exactly one hole: Han. |
| grapheme_caret | **not-achievable** | Caret/selection/deletion are per Unicode *scalar*, not per grapheme cluster, even though rendering ligates. Measured in the editor (pbpaste hexdumps): with content `ab👨‍👩‍👧‍👦cd`, Shift+Left×3 from the end selects `👦cd` (steps: d, c, 👦) and ×6 selects `ZWJ+👧+ZWJ+👦cd` — the caret walks *through* the single family glyph. Backspace×3 from the end leaves `ab👨‍👩‍👧‍` + dangling ZWJ (`61 62 f09f91a8 e2808d f09f91a9 e2808d f09f91a7 e2808d`) — i.e. backspace dismembers the family. Nothing in masonry 0.4 TextArea exposes grapheme-cluster movement. |
| selection | **built-in** | Sane and *logical* across the BiDi boundary. Shift+Left×29 from the end of the [MIXED] line copied `حبا שלום नमस्ते ไทย 한글 👨‍👩‍👧‍👦 fin.` — a contiguous logical suffix reaching mid-مرحبا. A mouse drag across `世界 مرحبا שלום नमस्ते` was contemporaneously observed as one continuous highlight and copied the exact logical substring; that interaction screenshot was not retained. ⌘A/⌘C/⌘V round-trips were reported byte-perfect for the full multi-script line including ZWJ emoji. |
| ime | **built-in** (source-verified only) | Full plumbing exists and is wired: masonry_winit 0.4 forwards `WindowEvent::Ime` → `TextEvent::Ime` (`event_loop_runner.rs:710`), TextArea handles `Ime::Preedit`/`Ime::Commit`/`Ime::Enabled` (`text_area.rs:770-784`), and `RenderRootSignal::{StartIme,EndIme,ImeMoved}` call back into winit to enable and position the candidate window. Could NOT be exercised live: this machine has no CJK input source enabled (ABC + Russian only), and installing one would modify the user's system. |
| large_doc_scroll | **not-achievable through the naive `prose` path tested** | In two contemporaneous runs, clicking "Load big doc" (corpus ×1000 ≈ 11k lines into one `prose`/portal) grew the scene until Vello/wgpu aborted: the reported 192 MiB binding exceeded the device's 128 MiB limit. The raw run output was not retained, so the exact timing/RSS figures are narrative rather than independently reproducible evidence. `prose`/portal has no virtualization, but Xilem 0.4 *does* ship `virtual_scroll` (`xilem::view::virtual_scroll`, backed by Masonry `VirtualScroll`), which could chunk the document into per-line labels. That path was not wired or tested; this result must not be generalized to it. |
| fonts_bundled | **none** | Nothing bundled; system fallback via fontique. That is exactly why Han is tofu (see cjk_render) — a deliberate record of what stock font discovery does and does not find on macOS 26. |

## Corpus line-by-line (what the window actually shows)

- [EN] ✓  [KO] ✓  [HI] ✓ (conjuncts क्ष त्र ज्ञ श्री द्ध correct)  [TH] ✓ (stacked
  vowels/tones correct)  [COMBINING] ✓ (ë a̐ é ö̲ n̈ correct; Zalgo-lite renders
  with all stacked marks, slightly crowded/overlapping into neighbors, no tofu).
- [AR]/[HE] ✓ correct BiDi (see table).
- [ZH] ✗ tofu except ASCII-ish punctuation; [JA] kana ✓ / kanji ✗ — the
  kana/kanji split inside one line is the fontique scan bug made visible.
- [EMOJI] ✓ except country flags (tofu pairs).
- [MIXED] ✓ except 世界.

## Totals

- LoC: **110** (single main.rs; stock views only — `prose` in a `portal`,
  `text_input`, two `text_button`s, corpus via `include_str!`).
- Helper crates: **none** (xilem =0.4.0 only; 403 locked deps).
- Release binary ~11.5 MiB. Launch checks: 10 s, both this and xilem-tray,
  zero stdout/stderr.
- Screenshot: `screenshot.png` (window-cropped, all 11 lines legible).

## Measurements

- Canonical clean release build **27 s**; no-op incremental build **2 s**.
- Dependency graph: **143 unique crate names / 154 name-version entries
  including the app**.
- Binary: **12,011,424 bytes raw / 10,288,040 bytes (9.8 MiB) stripped**.

## Where the time went

1. **Parallel-agent screen warfare (~40 %)**: egui-babel re-activating itself
   in a loop over the same coordinates, gpui-babel covering the screen center,
   and a ChatGPT.app pasteboard-privacy alert parked exactly over the toolbar.
   Ended up pixel-verifying button ownership (BMP compare of a reference crop)
   before every toolbar click, relocating the window twice, and discovering
   winit clamps initial positions to the visible frame (can't spawn off-screen).
2. **Shared clipboard races (~20 %)**: sibling agents' pbcopy calls interleaved
   with mine; every clipboard assertion had to become one atomic
   activate→click→keys→pbpaste sequence with sentinel values.
3. **Proving BiDi from pixels (~20 %)**: vision transcriptions of RTL text
   re-order runs; needed 5× zoomed slices of line start/middle/end to pin the
   actual visual order of each token.
4. Root-causing Han tofu + the wgpu crash in vendored sources (~20 %) — both
   turned out to be short, satisfying reads (`coretext.rs` is 111 lines).

## Surprises

- Good: BiDi passed this corpus out of the box — including Arabic-Indic digits,
  digit-order preservation inside RTL runs, and paragraph-level resolution of
  the trailing period. Selection across the boundary is logical and sane.
- Bad: **fontique 0.6 as pinned by Xilem 0.4 on macOS 26** failed Han fallback
  in file discovery: the tested build did not scan the on-demand AssetsV2
  location containing PingFang. Kana-but-not-kanji rendering in the [JA] line
  is the observed symptom. This does not establish the same defect in current
  standalone fontique releases or on other operating-system versions.
- Bad: an 11k-line document is not slow — it is a deterministic wgpu
  validation panic (192 MiB > 128 MiB buffer binding). The framework's own
  escape hatch (`virtual_scroll`) exists but nothing stops you from feeding
  `prose` a large string and shipping a crash.
- Odd: rendering treats 👨‍👩‍👧‍👦 as one glyph, editing treats it as seven scalars —
  arrow keys walk through the middle of a single visible glyph and backspace
  leaves a dangling ZWJ.
