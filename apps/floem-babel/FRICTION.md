# FRICTION — Babel (floem git @ 778bb5f2), SPEC-5

Verified on macOS (M4 Pro, rustc 1.96.1): release build clean (locked too),
launched >6 s, killed cleanly. `screenshot.png` captured window-scoped
(`screencapture -l <windowNumber>`; see hook notes below) and visually
inspected — the ratings below are read off the actual pixels.

Text stack at this rev: **parley + fontique + swash** (system-font discovery
and fallback via fontique). NOTE: the stale crates.io 0.2.0 used cosmic-text;
the whole text stack was swapped on `main`. Editor pane = Lapce editor core
(xi-rope). No fonts bundled.

## Ratings (from the running app)

| capability | rating | note |
|---|---|---|
| bidi_render | **built-in** | [AR]/[HE] read right-to-left with embedded English and digits (`456`, `789`, `١٢٣`) correctly ordered — visually equivalent to the iced (cosmic-text) rendering. |
| cjk_render | **NOT-ACHIEVABLE out of the box** (headline) | [ZH] and [JA] Han/kana render as PURE TOFU — fontique's fallback fails to resolve PingFang/Hiragino on macOS at this rev — while [KO] Hangul, [HI] Devanagari (with conjuncts) and [TH] Thai all render fine. 世界 inside the [MIXED] line is tofu too, in both the label pane and the editor. The same stack renders CJK on other platforms, so this is a macOS fallback-resolution bug rather than a missing feature; no app-side fix short of bundling a CJK font (not done — the gap is the datum). Corroborating symptom: ⌘/⇧ glyphs also tofu (floem-tray). |
| emoji_zwj | **built-in (mostly)** | 👨‍👩‍👧‍👦 renders as ONE color family glyph (label pane and editor); skin tone 👍🏽 applied; 🏳️‍🌈 (ZWJ flag) renders; 👩🏾‍🚀 renders as one glyph. Regional-indicator flags 🇺🇳 🇷🇸 are tofu (two boxes each) — RIS-pair → Apple flag glyph resolution missing. |
| mixed_fallback_line | **partial** | One paragraph walks Latin → Hebrew → Arabic → Devanagari → Thai → Hangul → emoji without tofu — EXCEPT the Han run (世界), see cjk_render. |
| grapheme_caret | **built-in** | Self-test drives the live editor through `Document::run_command` (same path as key bindings): caret byte-offsets over `a👨‍👩‍👧‍👦b` = [0, 1, 26, 27] — the 25-byte ZWJ family is ONE caret stop. Evidence: self-test. |
| selection | **built-in** | Shift+Right×16 → (0,16); ×22 crosses the BiDi boundary into Arabic → (0,24) byte-monotonic selection, no corruption. Mouse selection exercised interactively in the editor pane. Evidence: self-test + observed. |
| ime | **unexercised** | floem has IME plumbing (`ImePreedit/ImeCommit` events, `set_ime_allowed/set_ime_cursor_area` actions, editor preedit field) but activating a CJK IME requires manual keyboard-source switching that this headless-ish run couldn't script. Evidence: source-only. |
| large_doc_scroll | **built-in*** | 11k lines through `VirtualStack`: **11.7 ms** first frame after the big-doc swap, **112 fps** scripted scroll (one 120 px step per frame for 5 s), RSS **105 → 106 MiB** — flawless once correct. The asterisk is a TRAP that initially cost this app dearly: without `min_height(0)` on the scroll's flex ancestors, taffy sizes the scroll to min-content, the clip never applies, and the "virtualized" list silently materializes EVERY line — first measured run: 961 ms first frame and **1.9 GiB RSS** for the same 11k lines. Nothing warns. Diagnosed properly in floem-grid (same pathology at 100k rows = 16 GiB / no window); one obscure style line fixes it. |
| fonts_bundled | **none** | System fonts only. Consequence: the CJK tofu above is what "no bundling" actually looks like on floem/macOS today. |

## Verification hooks

- `BABEL_SELFTEST=1` — grapheme/selection/clipboard probes against the LIVE
  editor via `run_command` (arrow-key code path). Clipboard round-trip goes
  through the REAL system clipboard (`floem::Clipboard` + `ClipboardPaste`):
  `"start שלום 世界 👨‍👩‍👧‍👦" -> OK`.
- `BABEL_SHOT=path` — floem has NO window-capture API (iced does). The hook
  resolves the NSWindow `windowNumber` via the brand-new
  `WindowIdExt::with_window_handle` (added in the very commit we pin) +
  `objc2`, then shells out to `screencapture -l` — captures only this window
  even on a cluttered shared desktop.
- `BABEL_SCROLLTEST=1` — auto-loads the big doc, scrolls 120 px per rendered
  frame via a reactive `scroll_to` target + `exec_after_animation_frame`
  chain, prints achieved fps.

## Helper crates

`raw-window-handle` + `objc2` — verification-only (screenshot hook).

## Where the time went

1. Confirming the CJK tofu was real (retook the screenshot window-scoped to
   rule out capture artifacts) and separating WHICH scripts fail (Han/kana
   yes; Hangul/Devanagari/Thai no; flags yes; ZWJ emoji no).
2. Editor-core probe plumbing (`run_command`, cursor offsets, selection) —
   powerful, undocumented.
3. The screenshot hook (no capture API → windowNumber + screencapture).

## Totals

- LoC: 326 (single `src/main.rs`; ~95 verification hooks)
- Fonts bundled: none (0 bytes) — with the documented CJK consequence
