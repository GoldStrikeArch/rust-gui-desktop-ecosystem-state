# GAPS — freya-app

**No spec gaps.** Every functional requirement of SPEC.md maps onto stock
Freya 0.4 elements/components with default features:

| Requirement | Freya mechanism |
|---|---|
| Window title / ~480×640 / resizable | `WindowConfig::new(app).with_title("Tasks (freya)").with_size(480.0, 640.0)`; windows are resizable by default |
| Text input + placeholder | `Input::new(input).placeholder("What needs to be done?")` bound to a `use_state(String::new)` signal |
| Add button | `Button::new().on_press(...).child("Add")` |
| Enter-to-add while focused | `Input::on_submit(...)` — built in, no manual key handling |
| Task rows + Delete button | `ScrollView::children(...)` of `rect().horizontal()` rows, each with a `Button` |
| Live `N task(s)` counter | `label().text(format!("{count} task(s)"))`; reading `tasks` subscribes the component, so any mutation re-runs `app()` |
| Scrolling on overflow | `ScrollView::new()` |

## Notes on the idiom

- Freya 0.4 is **not** the RSX/Dioxus-macro library it was in 0.2/0.3: components
  are plain values built with a chained builder API (`rect().horizontal().spacing(10.)`),
  and reactivity is signal-based (`State<T>`, which is `Copy`). There is no
  `Message` enum and no central `update` function — event handlers mutate signals
  directly, which is roughly half the ceremony of the Elm-style frameworks in
  this cohort for an app this size (86 LoC vs iced's 74, and most of the delta is
  the explicit `Size`/`Alignment`/`Gaps` layout vocabulary).
- `Input` has a fixed `width` of 150 px by default; `Size::flex(1.)` was needed to
  make it share the top row with the Add button. `Size::fill()` on a child of a
  horizontal container makes it consume the whole row instead, pushing the button
  out of view — a small layout-vocabulary trap.
- List reconciliation wants explicit `.key(index)` on repeated rows (documented in
  the framework's own "Lists and Keys" guide); without it, deleting a middle row
  can mis-associate per-row state.

## Runtime observations (macOS 26.5.2, M4 Pro)

- Release binary launched from the terminal, stayed alive >8 s with a visible
  window, and exited cleanly on SIGTERM. **Nothing was printed to stdout or
  stderr** — no runtime warnings.
- Renderer is Skia on **Metal** (`freya-skia-safe` fork, `freya-skia-bindings`
  0.98.1). The first build of the cohort downloads a prebuilt Skia archive;
  subsequent app crates reuse the cached download, so a clean release build of
  this crate takes ~41 s.
- Debug builds automatically inject `freya-performance-plugin`'s FPS overlay
  (`#[cfg(debug_assertions)]` inside `freya::prelude::launch`). Release builds
  do not — worth knowing when eyeballing a debug run.
- Default features pull in a large tree (258 unique crates, incl. the full
  `image` codec set, `rfd`, `arboard`-style clipboard glue and AccessKit). No
  extra feature flag was needed for this spec.
