# FRICTION — Babel (freya =0.4.0), SPEC-5

Reference machine per spec. Verified on macOS 26.5.2 with the release binary,
scripted CGEvent mouse input, and window-scoped screenshots read back visually.
Total LoC: **530** (single `src/main.rs`; ~110 are the multi-line text editor
Freya does not ship, ~90 are verification-only self-test probes and the
screenshot hook).

`screenshot.png` = window-scoped `screencapture -l` of the release build at
800×600: all 11 corpus lines visible and wrapped, plus the editor pane.
Verified visually — **no tofu anywhere**.

## Ratings

| capability | rating | note |
|---|---|---|
| bidi_render | **built-in** | Skia `textlayout` (HarfBuzz + ICU BiDi). [AR]/[HE] read right-to-left; embedded "English words", Western digits (456/789) and Arabic-Indic ١٢٣ are ordered correctly inside the RTL runs, and the leading `[AR]`/`[HE]` ASCII tag anchors the paragraph to LTR base direction as expected. No API for forcing base direction. |
| cjk_render | **built-in** | [ZH]/[JA]/[KO] all render from system fonts via `SkFontMgr` fallback — full-width punctuation, 「brackets」, the `……` ellipsis and Hangul all correct. Zero configuration. |
| emoji_zwj | **built-in** | 👨‍👩‍👧‍👦 renders as **one** glyph (Apple Color Emoji's current silhouette family design, not four heads). 👍🏽 and 👩🏾‍🚀 apply skin tone; 🏳️‍🌈, 🇺🇳 and 🇷🇸 are single flag glyphs; the inline `e😀mo👩🏾‍🚀ji` run does not break shaping. |
| mixed_fallback_line | **built-in** | The [MIXED] line renders Latin + 世界 + مرحبا + שלום + नमस्ते + ไทย + 한글 + ZWJ family in one paragraph without a single tofu box, in both the read-only pane and the editor. |
| grapheme_caret | **not-achievable** (as shipped) | **The headline defect.** `freya-edit`'s cursor is a UTF-16 offset and Arrow-Right advances it by **one UTF-16 code unit**. Probed against `RopeEditor` — the exact core the widget drives — on `a👨‍👩‍👧‍👦b`: the family is 1 grapheme / 7 chars / 11 UTF-16 units spanning offsets 1..12, and three Right presses give offsets `[0, 1, 2, 3]`. Offset 2 lands **inside the first surrogate pair**. Backspace then deletes per code point: `a👨‍👩‍👧‍👦b` → `\u{200d}👩‍👧‍👦b`, i.e. the cluster is corrupted *and* a leading ZWJ is left dangling. `unicode-segmentation` is a dependency of `freya-edit` but is not used for caret movement. |
| selection | **assembled** | Mouse press/drag selection works and highlights render as a contiguous block (verified by dragging across the wrapped [MIXED] line and reading the capture back). Shift+Right extends by the same one-UTF-16-unit step as the caret, so a selection can also end mid-surrogate. Building the selection *UI* is app work: `get_visible_selection(EditorLine::Paragraph(i))` per line, fed into `paragraph().highlights(...)`. |
| ime | **built-in (source-only)** | Freya has a first-class `on_ime_preedit` element event carrying `ImePreeditEventData { text, cursor }`, and `RopeEditor` has real preedit state (`set_preedit`, `preedit_text_segments`, underline styling) which the stock `Input` wires up. This app's editor does **not** wire preedit (out of scope for the multi-line assembly), and no CJK input source is installed on the reference machine, so nothing was exercised interactively. |
| large_doc_scroll | **built-in** | 11,000 lines behind `VirtualScrollView::new_with_data(...).length(11_000).item_size(24.)`: the toggle is **0 ms** (the state flip is `2.7 µs`; nothing is materialised eagerly) and RSS moved only **104.6 MiB → 107.3 MiB** (`ps -o rss=`). Scrolling stays interactive because only the visible rows exist. The trade-off is the fixed `item_size`, so big-doc rows are `max_lines(1)`; the 11-line corpus view uses a plain `ScrollView` with real wrapping paragraphs. Virtualization being a stock component is a genuine advantage over the non-virtualized `column`-of-widgets shape other frameworks in this cohort used. |
| fonts_bundled | **none** | 0 bytes bundled. Skia's `SkFontMgr` plus Freya's `default_fonts()` fallback list resolved Latin, Arabic, Hebrew, CJK, Devanagari, Thai and colour emoji from macOS system fonts with no configuration. `LaunchConfig::with_font` / `with_fallback_font` exist for the cases where it doesn't. |

## Helper crates

None for text/i18n. `async-io` 2.6 is verification-only (Freya's executor has no
timer, and the scripted screenshot hook needs a delay).

## Where the time went

1. **Writing a text area.** `Input` is hard-wired to `max_lines(1)`, so a
   multi-line editor is assembled from the low-level `use_editable` hook: one
   `paragraph` element per line, a persistent `ParagraphHolder` per line for hit
   testing (they must live in a non-reactive `Rc<RefCell<Vec<_>>>` — a `State`
   would re-render every time layout grew it), per-line highlight ranges, and
   manual key/pointer plumbing including a container-level `on_focus_press` so
   clicking the blank area below the text still focuses the editor.
2. Working out that the probes could run against `RopeEditor` directly — it
   needs no window and no reactive runtime, which makes the grapheme findings
   reproducible with `BABEL_SELFTEST=1` and no GUI.
3. Rendering itself: essentially free.

## Surprises

- Good: the *rendering* half of the text stack is complete and needed zero
  work — BiDi, CJK, Indic, Thai, colour emoji and ZWJ sequences all correct
  out of the box, from a Skia backend rather than cosmic-text.
- Good: `VirtualScrollView` as a stock component makes the 11k-line case a
  non-event (0 ms, +2.7 MiB).
- Bad: the *editing* half moves the caret in UTF-16 code units. Everything
  above the ASCII plane can be split; a ZWJ emoji can be silently corrupted by
  one Backspace. This is the single worst text finding of the cohort so far and
  it is in a crate that already depends on `unicode-segmentation`.
- Bad: no text area component at all, and no way to reuse `Input`'s carefully
  built editor wiring for more than one line.
