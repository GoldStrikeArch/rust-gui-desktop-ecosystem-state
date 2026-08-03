# GAPS — floem-app

**No functional spec gaps.** Every SPEC.md requirement maps onto stock floem
views:

| Requirement | floem mechanism |
|---|---|
| Window title / ~480×640 / resizable | `Application::new().window(..., WindowConfig::default().title(...).size(...))`; resizable by default |
| Text input + placeholder | `TextInput::new(signal).placeholder("What needs to be done?")` — the buffer is an `RwSignal<String>`, no change handler needed |
| Add button | `Button::new("Add").action(closure)` |
| Enter-to-add while focused | `on_event_stop(TextInputEnter::listener(), ...)` — TextInput emits a typed custom event on Enter |
| Task rows + Delete button | `dyn_stack(each_fn, key_fn, view_fn)` — keyed diffing of the row views |
| Live `N task(s)` counter | `Label::derived(move || ...)` reading the tasks signal — only this label re-renders |
| Scrolling on overflow | `.scroll()` (the `ScrollExt` trait method) |

## Version-pin deviation (research finding)

SPEC.md requires the latest stable crates.io release pinned `=x.y.z`. Floem's
latest crates.io release is **0.2.0 (2024-11)** — 20 months stale at
measurement time, with a substantially different API (winit re-export,
cosmic-text stack, no typed event listeners). The maintainers direct users to
`main`, and `main` **cannot be published**: it depends on a forked winit
(`floem-winit`) and on `understory_*` crates, both via git. Per the SPEC's
git-fallback clause we pin the git rev
`778bb5f2aa08429e579ee2e6ac97e84fbf18b618` (2026-06-21) in every floem app.
The unpublishable-main situation is itself a headline ecosystem finding.

## API-churn observation

At this rev the free-function view constructors used by ALL published floem
documentation and most examples (`v_stack`, `h_stack`, `button`, `label`,
`static_label`, `text_input`) are **deprecated** in favor of struct
constructors (`Stack::vertical`, `Button::new`, `Label::derived`,
`Label::new`, `TextInput::new`), and even `TextInput::on_enter` is deprecated
in favor of the typed-event form. Code written from the docs compiles with 9
deprecation warnings; this file uses the current API. Tracking a moving
`main` means absorbing this churn — that is the cost of the version pin
situation above.

## Runtime observations (macOS, M4 Pro)

- Release binary launched from the terminal, stayed alive >8 s with a visible
  window, and was killed cleanly. **Nothing was printed to stdout/stderr.**
- Build-time note: the same `block v0.1.6` future-incompatibility warning as
  iced (pre-`objc2` Objective-C glue), here pulled in via floem's `copypasta`
  clipboard dependency.
- `deps-flat.txt` contains **no accesskit**: floem has no accessibility
  integration at all (iced, egui, xilem all ship AccessKit). Expected but
  now evidenced finding.
- Dependency graph: 315 unique crates (deps-flat.txt) — the default feature
  set compiles the vger GPU renderer, a tiny-skia software fallback, the
  full Lapce editor core, and parley/fontique text stack.
