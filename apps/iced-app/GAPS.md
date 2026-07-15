# GAPS — iced-app

**No spec gaps.** Every functional requirement of SPEC.md maps 1:1 onto stock
iced 0.14 widgets with default features:

| Requirement | iced mechanism |
|---|---|
| Window title / ~480×640 / resizable | `iced::application(...).title(...).window_size((480.0, 640.0))`; windows are resizable by default |
| Text input + placeholder | `text_input("What needs to be done?", &state.input).on_input(...)` |
| Add button | `button("Add").on_press(Message::Add)` |
| Enter-to-add while focused | `text_input::on_submit(Message::Add)` — built in, no manual key handling |
| Task rows + Delete button | `column(iter.map(...))` of `row![text, button("Delete")]` |
| Live `N task(s)` counter | plain `text(format!(...))` — view is re-derived from state every update |
| Scrolling on overflow | `scrollable(list).height(Fill)` |

## Runtime observations (macOS, M4 Pro)

- Release binary launched from the terminal, stayed alive >10 s, and exposed
  an on-screen window. It exited cleanly on SIGTERM. **No runtime warnings or
  errors were printed.**
- Build-time (not runtime) note: cargo reports a future-incompatibility
  warning for the transitive dependency `block v0.1.6` (pre-`objc2`-era
  Objective-C bindings pulled in via `window_clipboard` → `clipboard_macos`):
  "the following packages contain code that will be rejected by a future
  version of Rust". Cosmetic today, but signals old macOS glue in the
  clipboard path.
- Default features pulled in the full stack (wgpu + tiny-skia fallback +
  x11/wayland features, which are no-ops on macOS). No extra feature flags
  were needed for this spec.
