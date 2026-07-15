# Mini-App Spec: "Tasks" — identical across all frameworks

Every framework builds THIS app, as idiomatically as possible for that
framework. The goal is comparability: same features, same rough visual
structure, measured on the same machine.

## Functional requirements

1. **Window** titled `Tasks (<framework>)`, default size ~480×640, resizable.
2. **Text input** at the top with placeholder "What needs to be done?".
3. **"Add" button** next to the input. Clicking it appends the trimmed input
   text as a new task and clears the input. Empty/whitespace input is ignored.
4. **Keyboard shortcut:** pressing **Enter** while the input is focused does
   the same as clicking "Add".
5. **Task list** below: each row shows the task text and a **"Delete" button**
   (or ✕) that removes that row.
6. **Live counter** label: `N task(s)` — updates on add/delete.
7. **Scrolling** when the list overflows the window.

## Non-goals (keep it small)

- No persistence, no editing, no completion checkboxes, no theming work beyond
  framework defaults, no async.

## Implementation rules

- Each app is an **independent crate** at `apps/<framework>-app/` — its own
  `Cargo.toml`, `Cargo.lock`, and `target/`. NOT a cargo workspace member.
- Package name: `<framework>-app` (e.g. `iced-app`). Binary must be runnable
  with `cargo run --release`.
- Use the latest stable release of the framework on crates.io (pin exact
  version in Cargo.toml with `=x.y.z` so measurements are reproducible).
  Use a git dependency ONLY if the framework has no usable crates.io release —
  pin the exact rev and document it.
- Default framework features unless something in the spec needs more; document
  every feature flag you enable and why.
- Idiomatic style for the framework (Elm loop for iced, immediate-mode for
  egui, RSX for Dioxus, `.slint` DSL for Slint, HTML/JS frontend for Tauri…).
- Single `src/main.rs` where reasonable (Slint may add `.slint` files, Tauri
  its frontend dir). Keep code minimal but not code-golfed.
- **Fallback rule:** if the framework cannot express part of this spec, build
  the closest approximation and record the gap in `GAPS.md` inside the app
  directory — the gap itself is research data.
- After building, save `cargo tree --edges normal --prefix depth` output to
  `deps.txt` and `cargo tree --edges normal --prefix none | sort -u` to
  `deps-flat.txt` inside the app directory.
- Verify the app actually launches on macOS (run it, interact if possible,
  then quit). Note any runtime warnings in GAPS.md.
- For Tauri: vanilla HTML/CSS/JS frontend in a static dir — **no Node.js
  toolchain**, no npm. Frontend LoC counts toward app LoC.

## Reference machine (all measurements)

- Apple M4 Pro, 24 GB RAM, macOS 26.5.2 (build 25F84)
- rustc 1.96.1 / cargo 1.96.1 (stable, 2026-06-26)
