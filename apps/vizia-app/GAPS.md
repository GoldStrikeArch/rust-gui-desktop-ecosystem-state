# GAPS — vizia-app

**No spec gaps.** Every functional requirement of SPEC.md maps onto stock
vizia 0.4 views with default features:

| Requirement | vizia mechanism |
|---|---|
| Window title / ~480×640 / resizable | `Application::new(..).title("Tasks (vizia)").inner_size((480, 640))`; windows are resizable by default |
| Text input + placeholder | `Textbox::new(cx, input_signal).placeholder("What needs to be done?")` |
| Add button | `Button::new(cx, \|cx\| Label::new(cx, "Add")).on_press(..)` |
| Enter-to-add while focused | `Textbox::on_submit(\|cx, text, blur\| ..)` — built in; `blur == true` means the edit was committed (Enter or focus loss), no manual key handling |
| Task rows + Delete button | `List::new(cx, tasks_signal, \|cx, index, task\| ..)` of `HStack![Label, Button]` |
| Live `N task(s)` counter | `Memo::new(move \|_\| format!("{} task(s)", tasks.get().len()))` bound into a `Label` — recomputed only when `tasks` changes |
| Scrolling on overflow | free: `List` wraps its items in a `ScrollView` internally |

## Architectural note (why this looks different from iced/egui)

Vizia 0.4 is neither immediate-mode nor whole-tree-rebuild. The builder
closure passed to `Application::new` runs **once**; reactivity is
fine-grained via `Signal<T>` from `vizia_reactive`. `Label::new(cx, signal)`
subscribes that one label; `List::new(cx, vec_signal, ..)` diffs the vector
by value and rebuilds only the rows whose structure changed (0.4's
`ListItemsBinding` keeps a per-item `Signal<T>` so a *value* change costs no
entity rebuild at all). State mutation still goes through an Elm-ish
`Model::event` + typed event enum, so the code shape is close to iced's
`update`, but there is no `view` function to re-run.

Two small consequences visible in `src/main.rs`:

- Clearing the input after "Add" is done by writing the model signal
  (`self.input.set(String::new())`), which re-runs the `Textbox`'s own value
  binding and re-shows the placeholder. The upstream `todo` example instead
  emits `TextEvent::Clear`, but that only works from *inside* a textbox
  callback — events emitted from a `Model` propagate **up** the tree and
  never reach a child view. This is an easy trap: the emit compiles, runs,
  and silently does nothing.
- Layout constants live in a CSS stylesheet (`cx.add_stylesheet`), which is
  the idiomatic vizia place for anything shared; per-widget one-offs stay as
  inline modifiers. Vizia is the only framework in this cohort with a real
  CSS engine (`vizia_style`), including `1s`/`auto` Morphorm units.

## Runtime observations (macOS 26.5.2, M4 Pro, rustc 1.96.1)

- `cargo build --release` clean, no warnings; `cargo build --locked --release`
  also succeeds against the committed `Cargo.lock`.
- Release binary launched from the terminal, exposed an on-screen window
  titled "Tasks (vizia)" (window id resolved via `CGWindowListCopyWindowInfo`),
  stayed alive past the 10 s bar, exited cleanly on SIGTERM.
  **Nothing was printed to stdout or stderr** — no runtime warnings.
- The window picked up the OS dark appearance with no code: vizia ships a
  light/dark theme pair and subscribes to the system appearance by default
  (`Environment` / `ThemeMode`).
- Release binary size: **21.8 MiB** — vizia statically links Skia
  (`skia-safe 0.93.1`, Metal + GL + textlayout + svg features), which
  dominates. 169 unique crates in the normal-dependency tree
  (`deps-flat.txt`).
- Build-time note: the first build downloads a **prebuilt** `skia-bindings`
  binary rather than compiling Skia from source; on a cold machine this is
  the long pole and needs network access. No future-incompatibility warnings
  were reported by cargo.

## Verification limits (honest labelling)

Interactive add/delete could **not** be exercised with synthetic HID input in
this session: an always-frontmost terminal window occludes the whole display,
so `CGEvent` clicks aimed at the Tasks window are consumed by the terminal
(`uihelper topat` reports `Ghostty` at every in-window point, and
`NSRunningApplication.activate` does not raise above it). CGEvent posting
itself works (the cursor warps), so this is a shared-desktop occlusion issue,
not a permissions or app issue. The launch, the rendered window and the
"0 task(s)" counter are **observed** (window-scoped `screencapture -l`); the
add/delete/Enter wiring is **source-only** — it follows the upstream
`examples/todo` shapes 1:1, and the same `Textbox` edit/submit path is driven
programmatically (and asserted) by the in-app self-tests in
`apps/vizia-babel` and `apps/vizia-tray`.
