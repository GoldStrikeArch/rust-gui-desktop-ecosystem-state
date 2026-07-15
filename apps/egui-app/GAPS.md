# GAPS — egui-app (eframe 0.35.0)

## Spec gaps

**None.** Every functional requirement of `apps/SPEC.md` is expressible in
stock egui 0.35 / eframe 0.35 with default features:

1. Window title/size/resizable — `NativeOptions.viewport` (`ViewportBuilder`).
2. Placeholder text — `TextEdit::hint_text`.
3. Add button — `ui.button("Add")`.
4. Enter-to-add — egui has no "on submit" callback (immediate mode has no
   callbacks at all); the idiomatic pattern is
   `response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))`,
   plus `response.request_focus()` to keep the field focused for the next
   task. Semantically identical to clicking "Add", so not counted as a gap —
   but note that *discovering* this pattern is non-obvious (see friction log
   in `report/02-egui.md`).
5. Per-row Delete — deletion is deferred to after the loop
   (`delete_index: Option<usize>`) because you cannot mutate the `Vec` while
   iterating it in the same frame; standard immediate-mode idiom.
6. Live counter — trivially always-correct: the label is recomputed from
   `tasks.len()` every frame (immediate mode's core strength).
7. Scrolling — `egui::ScrollArea::vertical().auto_shrink(false)`.

## Runtime notes (macOS, M4 Pro)

- Canonical `cargo build --release` clean build: **26.39 s** (**27 s**
  rounded), with a **1 s** incremental rebuild. The unstripped default-wgpu
  binary is **12,531,728 bytes** (12.53 MB / 11.95 MiB).
- Launched from terminal, stayed alive >10 s, and exposed an on-screen window;
  exited cleanly on SIGTERM. **No runtime warnings or errors printed to
  stdout/stderr.**
- Interaction verified two ways:
  - Manually (window opens, renders, responds).
  - Headlessly via `egui_kittest` 0.35.0 (dev-dependency only): the tests in
    `src/main.rs` drive the app through its **AccessKit tree**
    (`get_by_role(Role::TextInput)`, `get_by_label("Add")`, `focus()`,
    `type_text`, `key_press(Enter)`), which also confirms the input, buttons
    and counter label exist in the semantic tree. Both tests pass. This is
    not a substitute for testing behavior with a real screen reader.
- One kittest API subtlety: `Node::type_text` emits `egui::Event::Text`,
  which egui delivers to the *focused* widget — you must call
  `node.focus()` first or the text silently goes nowhere.

## Feature flags

Only `eframe` default features are used: `accesskit`, `default_fonts`,
`wayland`, `x11`, `web_screen_reader`, `wgpu` (the default renderer in
0.35), `winit/default`. Nothing extra was needed for the spec.

## Dependency footprint

- `deps-flat.txt`: **164 name-version rows including `egui-app`**, or 163
  external rows, resolving to **156 unique crate names** for the macOS target
  with the default wgpu backend. Repeated `(*)` occurrences in the original
  `cargo tree` display are not additional dependencies.
- `cargo tree --edges normal` excludes the `egui_kittest` dev-dependency,
  so `deps.txt` / `deps-flat.txt` reflect the shipped binary only.
