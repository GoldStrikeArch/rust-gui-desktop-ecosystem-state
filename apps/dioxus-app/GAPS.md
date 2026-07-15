# GAPS — dioxus-app (Dioxus 0.7.9, desktop/webview renderer)

## Spec coverage

No functional gaps. All 7 requirements of SPEC.md are expressed directly:
window title/size via `dioxus::desktop::{Config, WindowBuilder}`, input +
placeholder, Add button, Enter key via `onkeydown` (checked against
`Key::Enter`), per-row Delete, live `N task(s)` counter, and scrolling via
CSS `overflow-y: auto` (webview handles it natively).

## Toolchain notes

- Built and run with **plain `cargo build --release` / `cargo run --release`**.
  The `dx` CLI was NOT needed at any point for this app (no assets, no
  bundling, no hot reload). `dx` is only required for hot-reload/hot-patching,
  the `asset!()` pipeline, .app/.msi/.deb bundling, and web/mobile targets.
- Feature flags: `desktop` only, on top of crate defaults
  (`launch, devtools, logger, lib`). `desktop` is required to get the
  wry/tao webview renderer; nothing else was enabled.
- First compile succeeded with no code changes (0 build errors).

## Measurements (Apple M4 Pro, macOS, rustc 1.96.1)

- Canonical clean release build: **40 s**; no-op incremental build **1 s**,
  cold target directory and warm registry cache. The earlier 102.3-second run
  was not the controlled serial result.
- Binary: `target/release/dioxus-app` = **5,982,128 bytes raw**;
  **5,108,920 bytes (4.9 MiB) stripped**.
- Dependency graph: **287 name-version entries including the app** (**279
  unique crate names**) — see `deps-flat.txt`.
- `src/main.rs`: 90 lines.

## Runtime verification

- Launched the release binary in the background; still alive after 11 s and
  terminated cleanly with SIGTERM. **No warnings or errors on stdout/stderr**
  (`launch.log` empty). A contemporaneous ≈98 MiB reading covered the main
  process only and its raw sample was not retained, so it is not comparable to
  later total-process-tree runtime measurements.
- Scripted UI interaction (typing/clicking) was not automated: driving a
  WKWebView from a script needs macOS Accessibility permissions. Launch
  health was verified programmatically instead.

## Observations (not gaps)

- The default `desktop` stack unconditionally pulls in `muda` (menus),
  `tray-icon`, `global-hotkey`, and `rfd` (file dialogs) even though this app
  uses none of them — OS-shell integration is bundled, not opt-in.
- Memory includes a webview cost, but the ~98 MiB observation above is only a
  noncanonical main-process reading. The experimental Blitz/`dioxus-native`
  renderer would avoid the webview but was intentionally not used here.
