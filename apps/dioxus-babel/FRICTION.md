# FRICTION — dioxus-babel ("Babel", Dioxus 0.7.9 desktop/webview)

The text stack is 100% WebKit: shaping, BiDi, font fallback, grapheme-cluster
editing, IME, scrolling. Dioxus contributes RSX + two signals. **Zero helper
crates, zero bundled fonts, zero text-related code.** The app is a layout
shell around a `<pre>` and a `<textarea>`.

## Capability ratings

(Everything below was verified against the *running* app: window screenshots,
plus scripted CGEvent/AX keyboard tests read back through two stdout probes —
`BABEL_ECHO` echoes the editor value on every input; `BABEL_SEL` streams
`selectionStart/End` (UTF-16 units) from a JS interval via `document::eval`.)
The probe hooks and screenshot remain in the app, but the raw probe session
output was not retained; exact interaction sequences below are narrative
evidence rather than an automated regression suite.

| Capability | Rating | Notes |
|---|---|---|
| bidi_render | **built-in** | [AR]/[HE] lines lay out right-to-left with embedded "English words" and digits (123/456/789, incl. Arabic-Indic ١٢٣) in correct order and position. See screenshot.png. |
| cjk_render | **built-in** | [ZH]/[JA]/[KO] all render in proper system CJK fonts (PingFang/Hiragino/Apple SD Gothic via fallback), full-width punctuation and 「quotes」 correct, no tofu anywhere. |
| emoji_zwj | **built-in** | 👨‍👩‍👧‍👦 renders as ONE glyph (macOS ≥14.4 draws family emoji as Apple's gray-silhouette design — that is the correct current platform glyph, not a fallback split). Skin tones apply (👍🏽, inline 👩🏾‍🚀), 🏳️‍🌈 and flag pairs (🇺🇳 🇷🇸) render as single color glyphs. |
| mixed_fallback_line | **built-in** | The [MIXED] line renders Latin+CJK+Arabic+Hebrew+Devanagari+Thai+Hangul+ZWJ-emoji in one paragraph with silent per-script font fallback; no tofu, no spacing glitches. |
| grapheme_caret | **built-in** | Measured, not assumed: with the caret after "… 한글 👨‍👩‍👧‍👦" a single ←-press moved `selectionStart` 60→49 (the full 11-UTF-16-unit ZWJ cluster as one step). Backspace-at-end sequence deleted `.` `n` `i` `f` ` ` one scalar each, then removed the ENTIRE family emoji (7 scalars, 11 units) in one press, leaving clean text — no orphan ZWJ/surrogate corruption (verified in the echoed value). |
| selection | **built-in** | One Shift+→ selected the whole emoji cluster (24..49→60 anchor fixed). Across the BiDi boundary: anchored before مرحبا, Shift+→ extended end monotonically in *logical* order (24→30→36) into שלום; visually the highlight is a single correct RTL run (screenshot). ⌘A/⌘C copied the exact 49-scalar content to the pasteboard; ⌘V pasted an Arabic+Hebrew+CJK+ZWJ string intact (echo confirmed). Mouse click focuses/places the caret (first click on an unfocused webview only activates — second click lands; standard macOS). |
| ime | **built-in (not exercised)** | Input-source switching could not be scripted in this environment, so no live CJK composition test was run. The editor is a stock WKWebView `<textarea>`, putting composition on the browser/platform path with no Dioxus widget code in between. That is a strong browser-derived baseline, not direct evidence from this test run. |
| large_doc_scroll | **built-in** | Corpus ×1000 = 11,000 lines (~1.3 MiB) was swapped in as one text node. A contemporaneous rAF probe reported 181 frames in 3010 ms (≈60 fps) while scrolling; its raw output was not retained, so the exact FPS and main-process RSS values are narrative evidence. The run completed smoothly, while unfocused rAF throttling was observed. WKWebView also keeps page memory partly in a separate WebContent process, so app-only RSS is not total memory. |
| fonts_bundled | **none** | System fonts + WebKit automatic fallback covered every script and emoji. 0 bytes bundled. |

## Helper crates

None. `dioxus = "=0.7.9"` (desktop) is the only dependency.

## Notes / quirks

- The one-line corpus embed is `include_str!("../../babel-assets/corpus.txt")`
  rendered inside a `pre` with `white-space: pre-wrap; font: inherit` — one
  DOM text node even at 11k lines, so the big-doc swap is a single edit over
  the webview IPC (no per-line elements, no virtualization needed at 60 fps).
- Controlled `<textarea>` (`value` + `oninput`) round-trips complex Unicode
  through Rust `String` losslessly — the echo probe printed byte-exact values
  including ZWJ sequences and combining marks.
- The [COMBINING] line (ë composed live, a̐ é ö̲ n̈, Zalgo-lite stacks) renders
  with marks correctly attached — WebKit handles mark stacking without any
  framework involvement.
- Verification env hooks left in (gated by env vars, inert otherwise):
  `BABEL_BIG` preloads the ×1000 doc, `BABEL_SCROLL` runs the fps probe,
  `BABEL_ECHO`/`BABEL_SEL` stream editor state to stdout.
- Same shared-machine chaos as SPEC-4: sibling agents stole focus and the
  clipboard mid-test; every keyboard result above was taken from runs where
  the app was verified frontmost immediately after the keystrokes.

## Where the time went

Writing the app: ~30 minutes including CSS; first `cargo check` was clean
(0 errors, 0 warnings). Essentially all remaining time went into *driving*
the editor from scripts (focus-theft retries, discovering that the first
click on an unfocused WKWebView doesn't reach content, building the SEL/ECHO
probes) — i.e. into measurement, not into making text work. Correct text was
free.

## Measurements

- `src/main.rs`: 188 lines (incl. ~40 lines CSS-in-Rust and the probe code).
- Canonical serial clean release build: **34 s**; no-op incremental build:
  **1 s**. The 208-second parallel-load run is retained only as a noncanonical
  observation. Dependency graph: **284 unique crate names / 292 name-version
  entries including the app**. Binary **6,092,016 bytes raw / 5,192,968 bytes
  (5.0 MiB) stripped**.
- Launch checks: multiple release runs, each windowed and alive for minutes; empty stderr; clean SIGTERM exits.
- screenshot.png: 880×652pt window, all 11 corpus lines visible in the rendering pane, editor seeded with [MIXED] (light theme).
