# SPEC-5: "Babel" — text & i18n stress test

Iteration 3. This app makes the text-stack fragmentation VISIBLE: the same
multilingual corpus rendered by every framework, screenshotted for
side-by-side comparison. Correct text is the most expensive part of a GUI
toolkit; this round measures who actually has it.

## Functional requirements

1. **Window** titled `Babel (<framework>)`, ~800×600, two panes side by side
   (or stacked if more idiomatic).
2. **Rendering pane** (read-only, scrollable): displays the shared corpus
   from `apps/babel-assets/corpus.txt` (embed via
   `include_str!("../../babel-assets/corpus.txt")` or equivalent — do NOT
   retype it), one paragraph per line, readable size (~14-16px).
3. **Editing pane**: a multiline editable text area seeded with the [MIXED]
   line from the corpus. Must support: mouse selection, Shift+arrow
   selection, caret movement, copy/paste round-trip.
4. **"Load big doc" button**: replaces the rendering pane content with the
   corpus repeated ~1,000× (≈11k lines), generated in code. Scroll through
   it; note smoothness/jank/memory qualitatively.
5. **Fonts**: prefer system fonts + automatic fallback. If the framework
   cannot discover system fonts for a script (e.g. egui), bundle the minimal
   set of open fonts needed (e.g. Noto subsets) and DOCUMENT exactly what
   had to be bundled and how many bytes — that is a headline finding.

## What to verify and record (FRICTION.md ratings + notes)

Rate each (built-in / assembled / hand-rolled / not-achievable) based on what
the running app actually shows — look at the window, don't assume:
- **bidi_render**: do the [AR]/[HE] lines read right-to-left with the
  embedded English/numbers correctly ordered?
- **cjk_render**: do [ZH]/[JA]/[KO] render (no tofu boxes)?
- **emoji_zwj**: does 👨‍👩‍👧‍👦 render as ONE family glyph (color), or split into
  4+ heads? skin tone applied? flags?
- **mixed_fallback_line**: does the [MIXED] line render every script in one
  paragraph without tofu?
- **grapheme_caret**: in the editor, does arrow-key movement treat 👨‍👩‍👧‍👦 as one
  unit? does backspace delete it whole or corrupt it?
- **selection**: mouse + Shift+arrows across the BiDi boundary — sane?
- **ime**: can you activate a CJK IME in the editor (manual/scripted as
  possible — document what was testable)?
- **large_doc_scroll**: smooth / janky / crashes at ~11k lines? rough memory.
- **fonts_bundled**: none (system fallback worked) / list what was bundled
  and total bytes.

## Screenshot (REQUIRED)

Save a screenshot of the running app showing the FULL rendering pane (all 11
corpus lines visible; resize the window if needed) to
`apps/<framework>-babel/screenshot.png`. Use window-scoped capture:
`screencapture -l $(osascript to get window id)` or interactive-window mode,
or full-screen + crop via `sips`. This is the comparison artifact — verify
the PNG is non-empty and actually shows the corpus (read it back visually).

## Implementation rules

Same as always: independent crate at `apps/<framework>-babel/` (package
`<framework>-babel`), pinned iteration-1 framework version (crib from
`apps/<framework>-app/`), helper crates recorded, fallback rule with
documented gaps, build + ~10 s launch check. Keep the UI minimal — the text
IS the app.

## Reference machine

Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
