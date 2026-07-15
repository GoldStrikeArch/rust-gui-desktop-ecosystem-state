# FRICTION — tauri-babel ("Babel", SPEC-5)

Tauri =2.11.5 / tauri-build =2.6.3 (same pins as ../tauri-app), same manual
no-Node setup. Corpus embedded in Rust via
`include_str!("../../babel-assets/corpus.txt")` and served over one
command; **zero helper crates** beyond iteration 1's set (tauri,
tauri-build, serde, serde_json — **204 unique crate names**, unchanged). All text work is
WKWebView's (CoreText + ICU): the app itself contains no text stack at all.

The editing pane is a plain `<textarea>`; the rendering pane is one
`<div dir=…>` per corpus line at 15 px system-ui.

## Capability ratings (from the running app — screenshot.png)

| Capability | Rating | Note |
|---|---|---|
| bidi_render | **built-in** | [AR]/[HE] render right-to-left with embedded English words and digits (Latin 456/789 and Arabic-Indic ١٢٣) correctly ordered — see screenshot. One honest asterisk: the corpus's ASCII "[AR] " prefix makes any first-strong heuristic (`dir="auto"`, `unicode-bidi: plaintext`) resolve the paragraph base direction LTR, so the app picks per-line `dir` from the first strong char *after* the tag (6 lines of JS). The engine does all actual BiDi; only the base direction needed app help, and only because of the tag prefix. |
| cjk_render | **built-in** | ZH/JA/KO all render, zero tofu, correct fullwidth punctuation and 「quotes」 — automatic fallback to PingFang SC / Hiragino / Apple SD Gothic Neo, nothing configured. |
| emoji_zwj | **built-in** | 👨‍👩‍👧‍👦 renders as ONE glyph (note: Apple's ≥14.4 *silhouette* family design — a single duotone group glyph by design, not a fallback artifact). Skin tones applied (👍🏽, inline 👩🏾‍🚀 ZWJ astronaut in color), 🏳️‍🌈 and regional-indicator flags 🇺🇳 🇷🇸 all correct. |
| mixed_fallback_line | **built-in** | [MIXED] shows Latin+CJK+Arabic+Hebrew+Devanagari+Thai+Hangul+emoji in one paragraph, no tofu, no per-script styling. |
| grapheme_caret | **built-in** | Measured with real (CGEvent) arrow keys, caret positions streamed to stdout: five ←-presses moved 1 UTF-16 unit each across " fin.", the sixth jumped **11 units** over 👨‍👩‍👧‍👦 — one cluster, one keypress. Backward-delete (execCommand probe, same editing command as backspace) removed the whole 11-unit cluster, no corruption. Caveat: programmatic `setSelectionRange(idx+5)` is NOT snapped — mid-cluster offsets are representable via API even though the UI never produces them. |
| selection | **built-in** | Shift+→ from before the family emoji selected exactly (49,60) — the whole cluster as one unit. Mouse selection works in a textarea by nature but mouse-across-the-BiDi-boundary behavior was not exercisable headlessly (WebKit uses standard visual-order selection there); recorded as untested-in-run. |
| ime | **built-in** (not exercised) | WKWebView text fields participate in the native macOS input-method machinery (marked text/candidate window) like any Cocoa text view. A CJK IME cannot be activated by script without changing user input sources; not exercised in this automated run. |
| large_doc_scroll | **built-in** | "Load big doc" = corpus×1000 → **11,000 lines / 11,000 DOM nodes** (423,016 px scrollHeight): built in ~20 ms, then 149 frames of continuous programmatic scrolling at **mean 16.60 ms/frame, max 18.0 ms, zero frames >33 ms** — locked 60 fps, no jank. RSS ~121 MiB for the app process (WKWebView's web content additionally lives in Apple's shared WebContent XPC processes, not counted). |
| fonts_bundled | **none** | 0 bytes. Every script above came from system fonts via automatic CoreText fallback. |

## WebKit quirks recorded (honesty items)

- `document.execCommand("delete")` still works on a `<textarea>` in WKWebView
  (deprecated everywhere, but it is what made the backspace probe scriptable).
- `selectionchange` on textareas is inconsistent in WebKit → the caret
  reporter polls instead.
- Programmatic selection offsets are code-unit-based and unsnapped
  (`selectionStart` landed mid-emoji when asked to); only *user* caret
  movement is grapheme-cluster aware.
- First-strong base-direction detection is defeated by the corpus's ASCII
  tag prefixes — any framework relying on `dir=auto`/plaintext heuristics
  will show [AR]/[HE] with an LTR base unless it, too, skips the tag.

## LoC (321 source; 360 including config) & size

- Rust: **65** (59 `src/main.rs` — of which ~15 is the selftest/eval hook —
  + 6 `build.rs`)
- Frontend: **256** (30 HTML + 147 JS + 79 CSS) — of the JS, ~60 lines are
  the selftest/scroll/caret probes, ~25 are the actual rendering.
- Config: 39 (`tauri.conf.json` 32 + capability 7)
- Release binary **7.9 MiB**; **204 unique crate names** (identical to
  iteration 1 —
  the entire i18n stack cost zero new dependencies and zero bundled bytes).

Canonical serial clean build: **36 s**.

## Where the time went

1. The verification harness, not the app: stdout report channel, scroll
   frame-timing probe, caret reporter + osascript keystroke driving, and
   window-scoped screenshot tooling (CGWindowID via a Swift one-liner).
2. The per-line base-direction heuristic (the only "text code" in the app).
3. Nothing else — rendering the corpus correctly required literally a
   `<div>` per line.

## Verification

Built release (first build clean); plain launch alive at 10 s, killed
clean. BABEL_SELFTEST run: 11 corpus lines rendered (2 auto-detected RTL),
grapheme delete 11/11 units, big-doc 11,000 lines, scroll stats above, all
via stdout. Real arrow-key caret trace: 65→64→63→62→61→60→**49** then
Shift+→ **(49,60)**. screenshot.png is window-scoped (`screencapture -l`),
900×780 logical (SPEC allows resizing so the full pane fits), all 11 lines
visible, read back and inspected at full resolution (emoji line re-cropped
and zoomed to confirm single-glyph family + skin tones + flags).
