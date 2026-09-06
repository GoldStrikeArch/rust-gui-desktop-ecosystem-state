# Next apps — ranked backlog (2026-08-30)

What the corpus still cannot answer, ordered by how much a new spec would
change the picture. The first two came from an r/rust reply ("most GUI apps I
developed contained at least 10 windows, mostly modal … numeric fields …
vertical alignment over the decimal separator") and are drafted in full as
`apps/SPEC-9.md` and `apps/SPEC-10.md`. The rest are one-paragraph
candidates; each names the gap in the existing eight rounds it would close.

| # | Spec | Dimension | Why it is next |
|---|---|---|---|
| 1 | **SPEC-9 "Windows"** — multi-window + modal | window model: modality, parenting, shared state, cross-window messages, close veto, persistence, DPI | Every existing app is one window; SPEC-4 opened an About window but moved no data through it. winit has no modal API; macOS sheets / Windows owner-disabling / Wayland `xdg_dialog` are three shapes. Reactive frameworks either shine (one signal graph) or hurt (per-window state) here, and nobody has measured it. |
| 2 | **SPEC-10 "Ledger"** — forms + numeric input | typed input, locale parse/format, decimal alignment, tab order, validation, undo, first a11y *verification* | The corpus knows every framework has *a* text input; it does not know whether any can be a numeric input (filter while typing, `tnum`, step keys). Also carries the first screen-reader-tree dump per framework — the NVDA/VoiceOver pass was never done. Kept out of the todo app so iteration-1 build/size baselines stay valid. |
| 3 | "Editor" — multi-line text editing | grapheme-correct deletion, IME composition (CJK), undo/redo, find/replace, word wrap, 10 MB file | The sharpest finding so far was editing (5/10 paths split a ZWJ cluster on Backspace). SPEC-5 only probed a single-line caret; freya's `Input` is one line, xilem needed custom focus plumbing, gpui ships no text input. A dedicated editor spec turns "text-input gap" from an aside into a measured row. |
| 4 | "Lifecycle" — the boring stuff | single-instance, URL-scheme deep link, file-association open (Finder double-click a `.txt` → path delivered to the running app), settings persistence, auto-update check, crash reporting hook | This is the layer the original post's author gave up on years ago and the one every shipped app needs on day two. None of it is measured; Tauri has plugins for all of it, everyone else has crates or nothing. |
| 5 | "Chrome" — window chrome & presentation | custom titlebar with native traffic lights/snap layouts, transparency/blur, always-on-top, fullscreen, per-monitor DPI change while running, window snapping/resizing edges | Designers ask for it first; it is where winit feature flags (`with_titlebar_transparent`, `with_fullsize_content_view`) and framework wrappers diverge. Cheap to specify, high friction expected on Linux. |
| 6 | "Print" — print & PDF export | native print dialog, page setup, print a rendered view, export the same to PDF | Zero coverage in the corpus and the ecosystem: no cross-platform Rust print crate exists. Likely a not-achievable column for most, which is itself the finding for the funding list. |
| 7 | "Embed" — hosting a foreign view | a native platform view inside the framework's window (webview in a native app, `AVPlayer`/video, a map control); and the reverse: the framework's surface inside a foreign host (plugin/`baseview`) | Interop is how teams migrate incrementally. vizia already runs inside audio-plugin hosts; wry-in-winit is a known-hard problem on Linux. |
| 8 | "Mirror" — RTL UI mirroring | flip the whole layout for an Arabic/Hebrew locale, not just text: mirrored toolbars, scrollbars, drawers, keyboard arrows | SPEC-5 measured text BiDi only. Layout mirroring is table stakes in Qt/Flutter and untested in every Rust framework. |
| 9 | A11y traversal pass (not an app) | VoiceOver / NVDA / Orca walk of the todo app: reach every control, read every label, activate by keyboard only | README says "the NVDA pass was not performed". SPEC-10 adds a tree dump; a scripted screen-reader walk is the missing verification of the "AccessKit integrated" checkbox. |
| 10 | Speed study (separate) | frame time, input latency, scroll smoothness at 120 Hz, large-scene rendering — controlled, same GPU, same scene | Explicitly out of scope so far ("not a general performance ranking"). Needs its own methodology (tracing, frame pacing) and should not be bolted onto the friction apps. |

Notes on scope discipline:

- Each spec keeps the iteration-1 rule: independent crate, same pinned
  framework version, FRICTION.md with built-in / assembled / hand-rolled /
  not-achievable per capability, retained synthetic-input evidence.
- Ten frameworks × one new spec is ten apps; specs 1–3 together are thirty.
  Suggested order: SPEC-9, then SPEC-10, then "Editor" (specs 3 and 10 share
  a text-input core, so build 10 first and reuse).
- Linux and Windows reruns of new specs use the existing round-5/round-6
  harnesses unchanged.
