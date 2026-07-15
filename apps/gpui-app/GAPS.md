# GAPS.md — gpui-app (gpui 0.2.2)

Framework: `gpui = "=0.2.2"` from crates.io (latest release as of 2026-07-07,
published 2025-10-22). No git dependency needed — the crate is published and
usable, including 28 bundled examples.

## Feature flags enabled beyond defaults

- **`runtime_shaders`** — required on this machine. gpui's `build.rs` normally
  precompiles its Metal shaders with `xcrun metal` at build time. Xcode 26 no
  longer bundles the Metal Toolchain, so the default build fails with:

  ```
  cargo::error=metal shader compilation failed:
  error: error: cannot execute tool 'metal' due to missing Metal Toolchain;
  use: xcodebuild -downloadComponent MetalToolchain
  ```

  (Reproduced deliberately in a scratch crate with default features.)
  `runtime_shaders` embeds the `.metal` source and compiles it at app startup
  instead. The alternative is a multi-GB `xcodebuild -downloadComponent
  MetalToolchain` download. This is the single biggest setup trap for gpui on
  a fresh macOS machine.

Default features (`font-kit`, `wayland`, `x11`, `windows-manifest`) left as-is;
the Linux/Windows ones are target-gated and pull nothing into the macOS build.

## Spec gaps / approximations

1. **Text input is hand-rolled and minimal.** gpui ships **no first-party
   high-level widget or text-input library**. It does include lower-level
   elements such as `uniform_list`; buttons here are styled `div()`s with
   `on_click`, which is idiomatic gpui.
   For the input field, the officially sanctioned approach is to implement
   `EntityInputHandler` — the bundled `examples/input.rs` does this in **746
   lines** (selection, IME marked text, clipboard, mouse selection). To keep
   the app proportionate, this app instead implements a minimal input on raw
   `on_key_down` (`Keystroke::key_char` append, `backspace` pop, `enter`
   submits). Consequences:
   - no IME composition (CJK input won't compose),
   - no cursor movement/selection/clipboard within the field,
   - no OS-level text-field accessibility semantics.
   Placeholder, focus ring, caret, Enter-to-add all work per spec.
2. **Focus-visuals only caret** — the caret is a static bar (no blink); spec
   does not require blinking.
3. **Quit-on-last-window-close is not default** — a gpui app keeps running
   when its window closes unless you wire `cx.on_window_closed(... cx.quit())`
   yourself (done in `main.rs`, copied from the bundled
   `on_window_close_quit.rs` example).

Everything else in the spec (window title/size/resizable, add via button and
Enter, delete per row, live `N task(s)` counter, scrolling via
`.overflow_y_scroll()` on an `.id()`'d div) is expressed directly.

## Launch verification (macOS, M4 Pro)

- Canonical serial `cargo build --release`: **55.16 s → 56 s** wall; binary
  `target/release/gpui-app` = **5.0 MiB** (unstripped); **391 unique crate
  names**. The earlier 525 value was a count of repeated/name-version tree
  rows, not unique names. Canonical artifacts are in
  `measurements/gpui-app-build.log` and
  `measurements/gpui-app-deps-flat.txt`.
- Launched the release binary in the background; still alive after 12 s;
  the window was manually observed with title "Tasks (gpui)", input with
  placeholder + focus ring + caret, Add button, and "0 task(s)" footer. The
  original screenshot was not retained, so this is an observation rather than
  a reproducible screenshot artifact.
  Killed cleanly afterwards.
- **No runtime warnings** on stdout/stderr (log file empty). Note:
  `runtime_shaders` moves shader compilation to startup; no visible latency.
- Scripted keystroke injection (`osascript`) was blocked by macOS automation
  permissions in this environment ("osascript is not allowed to send
  keystrokes"), so add/delete flows were verified visually and by code review
  rather than by scripted UI interaction.

## Build warnings

- `warning: the following packages contain code that will be rejected by a
  future version of Rust: block v0.1.6` — a transitive Objective-C bridge
  dependency of gpui's macOS backend (gpui itself has a TODO to migrate to
  `objc2`).
