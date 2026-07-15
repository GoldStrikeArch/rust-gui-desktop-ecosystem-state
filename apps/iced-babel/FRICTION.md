# FRICTION — Babel (iced 0.14.0), SPEC-5

Reference machine per spec (M4 Pro, 3024×1964 Retina, 120 Hz). Verified with
`cargo run --release` + scripted interaction (CGEvent clicks/drags, System
Events keystrokes) and by reading the captures back. Total LoC: **336**
(single `src/main.rs`, ~80 of which are verification hooks: selftest probes,
screenshot saving, fps scroll driver).

`screenshot.png` = window-scoped `screencapture -l` of the release build at
1150×780 (SPEC allows resizing): all 11 corpus lines visible, unwrapped, plus
the editor pane. Verified visually — no tofu anywhere.

## Ratings

| capability | rating | note |
|---|---|---|
| bidi_render | **built-in** | cosmic-text BiDi. [AR]/[HE] read right-to-left with correct visual reordering; embedded "English words", Western digits (456/789) and Arabic-Indic ١٢٣ ordered correctly. No API for forcing paragraph base direction (derived per paragraph — fine here since lines start with an ASCII tag). |
| cjk_render | **built-in** | [ZH]/[JA]/[KO] all render from system fonts (PingFang/Hiragino/Apple SD Gothic via fontdb fallback); full-width punctuation and 「brackets」 fine. Zero config. |
| emoji_zwj | **built-in** | 👨‍👩‍👧‍👦 renders as ONE glyph (Apple Color Emoji's current silhouette-style family design — not 4 split heads). Skin tones (👍🏽, 👩🏾‍🚀) applied; 🏳️‍🌈, 🇺🇳, 🇷🇸 all correct single flag glyphs; inline e😀mo👩🏾‍🚀ji does not break shaping. |
| mixed_fallback_line | **built-in** | [MIXED] renders every script in one paragraph without tofu, in both the `text` widget and the editor. Default `Shaping::Auto` (0.14) upgrades non-ASCII runs automatically; the [COMBINING]/Zalgo line stacks marks correctly (slightly cramped, as everywhere). |
| grapheme_caret | **built-in / hand-rolled edge** | Split verdict, verified two ways (Content probes + live widget): caret MOTION is grapheme-atomic — Right/Left and Shift+Right jump the whole 👨‍👩‍👧‍👦 cluster (columns 1→26, byte-indexed), interactively one Shift+Right selected exactly one family. But BACKSPACE deletes only the trailing scalar: "a👨‍👩‍👧‍👦" → "a👨‍👩‍👧‍" — corrupts the cluster (cosmic-text editor deletes per-char, not per-grapheme). |
| selection | **built-in** | Mouse click/double-click(word)/drag and Shift+arrows all verified in-widget. Drag across the BiDi boundary grows the selection contiguously in logical order ("Mixed: Hello 世界 مرحبا שלום नमस्ते"); highlight renders as a sane contiguous visual block; ⌘A/⌘C/⌘V built-in bindings round-trip the full multi-script line (family emoji intact) through the system clipboard. |
| ime | **built-in (partially verified)** | 0.14 text_editor has full IME plumbing (preedit state, candidate-window positioning via `InputMethod::Enabled`). Verified the Cocoa marked-text path end-to-end with the ⌥e dead-key: preedit → commit produced "é" in the editor. A real CJK IME could not be tested — no CJK input source installed on the reference machine (only ABC/Russian layouts), and installing one silently was out of scope. |
| large_doc_scroll | **built-in (with a hitch)** | 11,000 lines as one `column` of `text` widgets in a `scrollable`: a local interactive run observed a one-time 803 ms load pause, **118.5 fps over 5 s** of scripted scrolling on the 120 Hz display, and 97 MiB → 410 MiB memory. These exact probe outputs were not archived as raw logs, so they are narrative observations rather than independently reproducible benchmark rows. The structural finding remains: this path lays out all 11,000 widgets because it is not virtualized. |
| fonts_bundled | **none** | 0 bytes bundled. iced 0.14 default features no longer embed Fira Sans; everything (Latin/Arabic/Hebrew/CJK/Devanagari/Thai/emoji) resolved from macOS system fonts via fontdb + cosmic-text per-script fallback. This is the headline: iced's text stack needed literally zero i18n work. |

## Helper crates

None for text/i18n. Verification-only: `png` 0.18 (encode `window::screenshot`
output) and `smol` 2 (timer; already in tree via iced's executor feature).

## Gotchas / where the time went

1. **`window::screenshot` does not render `text_editor` content** — the
   editor pane came out empty in the offscreen capture while every `text`
   widget rendered fine (editor's cosmic buffer isn't drawn in that pass).
   Burned a diagnosis cycle; final artifact uses window-scoped
   `screencapture` instead.
2. `text_editor::Content::with_text` needs a type annotation when used
   standalone (generic over Renderer) — trivial but non-obvious.
3. Verifying interactively on a machine where 6 other agents fight over
   focus/clipboard/theme cost more time than the app itself (selection
   "failures" that were actually clicks landing on other frameworks'
   windows). The BABEL_TRACE hook (log every editor action + selection)
   settled it conclusively.
4. `iced::time::every`/timers require the `smol`/`tokio` feature — the
   default thread-pool executor has no timer (same finding as iteration 2).
