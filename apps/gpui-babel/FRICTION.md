# FRICTION — Babel (gpui =0.2.2), SPEC-5

Reference machine per spec (M4 Pro, macOS 26.5.2, rustc 1.96.1). crates.io
gpui with `runtime_shaders` (same pin as apps/gpui-app; see its GAPS.md).
Canonical serial release build **55 s**; binary 5.4 MiB unstripped; **398
unique crate names**. (The older ~560 figure counted repeated tree rows.)
The lockfile has 703 package entries including target-specific packages and
version duplicates; that is not the dependency metric used here. Total LoC
**1090** (main.rs 254 +
editor.rs 836 — the editor is the same hand-rolled multiline widget as
gpui-tray, extended from gpui's bundled `examples/input.rs`).

`screenshot.png` = window-scoped `screencapture -l` of the release build at
980×640 (SPEC allows resizing): all 11 corpus lines visible and unwrapped in
the `uniform_list` rendering pane, plus the editor pane seeded with [MIXED].
Read back visually — no tofu anywhere.

Verified against the *running release binary* with scripted interaction
(System Events keystrokes, CGEvent clicks/drags/scrolls via a compiled Swift
helper, window captures read back). The BiDi verdict is additionally backed
by instrumented CoreText reference renders (below), because eyeballing
Arabic screenshots turned out to be genuinely unreliable.

## The headline

On macOS, gpui is a **CoreText passthrough**: `MacTextSystem::layout_line`
builds one `CTLine` for the whole line and paints glyphs at the CTRun
*visual* positions. So paragraph-level text quality (BiDi reordering, CJK
fallback, color emoji, combining marks) is native-webview-grade for FREE —
despite RTL being officially unsupported (zed#31102). What gpui does NOT
have is (a) any text widget at all, and (b) BiDi-aware caret geometry:
`LineLayout::x_for_index` / `closest_index_for_x` assume monotonic
logical→visual order, so *editing* over BiDi text is geometrically
incoherent even though *rendering* is correct.

## Ratings

| capability | rating | note |
|---|---|---|
| bidi_render | **built-in (macOS-only, via CoreText)** | [AR]/[HE] render with full UBA (base LTR from first-strong): tag far left, RTL runs visually reversed, embedded English + Western digits + Arabic-Indic ١٢٣ all correctly ordered; in [MIXED], adjacent Arabic+Hebrew merge into one RTL run and swap correctly. Proven, not eyeballed: app crops match an `NSAttributedString`/CTLine reference render pixel-for-pixel in arrangement; `CTLineGetOffsetForStringIndex` probes + a colored-range pixel-scan render confirm the arrangement ("[AR]" at pixel cols 20–79 of 1178, Arabic caret x decreasing 214→181 as logical index advances). Caveat: this is the platform CoreText path; Linux/Windows were not exercised, so no cross-platform result is inferred. See selection below. |
| cjk_render | **built-in** | [ZH]/[JA]/[KO] all render from system fonts (PingFang/Hiragino/Apple SD Gothic via the CoreText cascade); full-width punctuation, 「brackets」 and ellipses fine. Zero config, no tofu. |
| emoji_zwj | **built-in** | 👨‍👩‍👧‍👦 is ONE glyph (Apple Color Emoji's current silhouette family design — not 4 heads); skin tones applied (👍🏽, 👩🏾‍🚀); 🏳️‍🌈 🇺🇳 🇷🇸 all single correct flags; inline e😀mo👩🏾‍🚀ji doesn't break shaping. Same rendering in the read-only list and the editor (`shape_line`). |
| mixed_fallback_line | **built-in** | [MIXED] renders every script in one paragraph without tofu in both panes. [COMBINING]/Zalgo-lite stacks marks on the right bases (cramped, as everywhere). Verified again at line ~11,000 of the big doc. |
| grapheme_caret | **hand-rolled** | gpui ships no text widget, so this is our code + `unicode-segmentation` (the same approach as gpui's bundled input.rs). Scripted end-to-end on the release binary: five ←-presses walked " fin." one grapheme each; one Shift+← then ⌘C copied **exactly** the 25-byte 👨‍👩‍👧‍👦 sequence; a single backspace from a collapsed caret deleted the whole 7-codepoint cluster (buffer re-verified byte-exact by ⌘A⌘C round-trip — no ZWJ debris). |
| selection | **hand-rolled (BiDi geometry incoherent)** | Mouse drag, Shift+arrows, ⌘A all work on a byte-range model; ⌘C/⌘X/⌘V round-trip the full multi-script line through the system pasteboard byte-exact. CGEvent drag "Hello"→"ไทย" across the whole RTL segment: logically contiguous selection ("ello 世界 مرحبا שלום नमस्ते ไทย"), single sane highlight block, clean copy. But end the drag *inside* the RTL run and rendering vs. logic split: copy returned "ello 世界 م" while the highlight stopped before the visually-reordered Arabic — selected glyphs not highlighted. That's `x_for_index` scanning visual-order glyphs for logical indices; unfixable without real BiDi caret mapping. |
| ime | **built-in plumbing, hand-rolled buffer (partially verified)** | `EntityInputHandler` + `window.handle_input` is the real NSTextInputClient bridge (same protocol Zed uses), incl. `bounds_for_range` for candidate-window placement. Verified the Cocoa marked-text path end-to-end with the ⌥e dead key: preedit → commit produced "é" in the buffer (byte-verified). A real CJK IME was untestable — only ABC/Russian input sources installed on this machine; installing one silently was out of scope. |
| large_doc_scroll | **built-in (`uniform_list`)** | The virtualized list Zed uses: 11,000 lines load *instantly* (only visible rows are shaped) and cost **+4 MiB** RSS (88→92 MiB). A 400-event CGEvent wheel burst (30 lines/event) reached line ~11,000 in ~4 s at ~35% of one core, RSS stable at 93 MiB, no jank in captures, every script/BiDi/emoji still correct at depth. Constraint: uniform row height + `whitespace_nowrap` — you buy virtualization by giving up wrapping. |
| fonts_bundled | **none** | 0 bytes bundled. Latin/Arabic/Hebrew/CJK/Devanagari/Thai/emoji all resolved by the CoreText fallback cascade from system fonts. |

## Helper crates

- `unicode-segmentation 1` — grapheme boundaries for the editor (same dep
  gpui's own input.rs example uses). Nothing else; no text/i18n crates.

## Editor limits (deliberate, hand-rolled cost)

No soft-wrap (logical lines clip), no word/double-click selection, no
scroll-caret-into-view, no caret blink, no undo. Each is more hand-rolling —
gpui gives you `shape_line` + input protocol and nothing above it.

## Gotchas / where the time went

1. **The 836-line editor is the price of entry** for one editable text box —
   already paid in gpui-tray this round; here it needed only re-seeding.
2. **Verifying BiDi honestly was the real work**: reading Arabic screenshots
   by eye produced contradictory conclusions (mixed-direction line *edges*
   are shockingly easy to misread). Settled it with CTLine caret-offset
   probes, an instrumented reference render with colored logical ranges and
   a programmatic pixel-column scan, then an app-vs-reference A/B stack:
   identical arrangement → gpui == CTLine == platform-correct.
3. The selection-highlight/logical-content divergence inside RTL runs
   (rating above) is structural: gpui's `LineLayout` maps index→x by
   scanning visual-order glyphs for a logical index; correct rendering makes
   the incoherent editing geometry *more* visible, not less.
4. `uniform_list`'s item closure gets `&mut App`, not the entity — you
   re-`entity.read(cx)` per frame and clone `SharedString`s per row
   (cheap, they're refcounted; 11k rows built in code in an eyeblink).

## Totals

- LoC: 1090 (main.rs 254 + editor.rs 836, heavily commented)
- Binary: 5.4 MiB release (unstripped; 5,615,696 bytes)
- Idle RSS ~88 MiB; 92–93 MiB with the 11k-line doc loaded/scrolled
- Fonts bundled: none
