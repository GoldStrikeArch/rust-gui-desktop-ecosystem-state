# GAPS / notes — tauri-app

## Spec gaps

None. All seven functional requirements are implemented (window title/size,
placeholder input, Add button, Enter shortcut via form submit, task list with
per-row Delete, live `N task(s)` counter, scrolling list).

## Design choice: state lives in Rust, on purpose

Unlike the other frameworks in this study (which keep app state in-process
with the UI), this app keeps the task list in the **Rust core process**
(`Mutex<Vec<String>>` behind `tauri::State`) and the webview frontend is a
thin view. Every add/delete/initial-load is a `window.__TAURI__.core.invoke()`
round-trip to a `#[tauri::command]`, and the UI re-renders from the canonical
list Rust returns. This was chosen deliberately to exercise Tauri's IPC
bridge — the architectural core of the framework — and matches Tauri's own
guidance to keep business logic in Rust. The cost: every interaction is an
async JSON-serialized cross-process hop that a purely in-webview JS app would
not pay. Keep this in mind when comparing "app LoC" and architecture with the
other mini-apps.

## Manual plain-cargo setup (no Node.js, no npm, no tauri-cli)

A fully manual setup worked; `tauri-cli` was **not** needed:

- `tauri` + `tauri-build` from crates.io, hand-written `tauri.conf.json`
  with `build.frontendDist: "ui"` pointing at the static vanilla
  HTML/CSS/JS directory (paths are relative to `tauri.conf.json`).
- `app.withGlobalTauri: true` exposes the IPC API as `window.__TAURI__`
  so no `@tauri-apps/api` npm package is required.
- Capabilities: one hand-written `capabilities/default.json` granting
  `core:default` to the `main` window. Note that **app-defined commands do
  not need permissions** — the capability system gates core/plugin commands.
  `tauri-build` generates the capability JSON schemas into `gen/schemas/`
  at build time.
- Icons: this app explicitly configured `bundle.icon`, so those paths had to
  exist even while its iteration-1 `bundle.active` setting was false
  (tauri-build reads configured icons; on Windows they can also become the
  window icon). This is not a universal requirement when no icon is
  configured. The current config enables bundling for the later packaging
  round.
  Generated a minimal RGBA PNG with a stdlib-only Python script + `sips`
  for resizing and the `.icns`. Tauri requires the PNGs to be RGBA —
  RGB-only PNGs make tauri-build/tauri error.

## Runtime notes (macOS 26.5.2, M4 Pro)

- Launched from the raw release binary (`target/release/tauri-app`) without
  a `.app` bundle: window and WKWebView come up fine.
- Runtime warnings: none printed to stdout/stderr during a normal
  launch/interact/quit cycle.
- The process tree is just one process for the app; WKWebView's actual web
  content runs in Apple's shared `com.apple.WebKit.WebContent` XPC helper
  processes, so `ps` on the binary alone understates true memory use.

## Measurements (this machine)

- Canonical serial clean `cargo build --release`: **36.34 s → 36 s** wall
  (M4 Pro). The later 96 s packaging invocation included a config-triggered
  re-link and is not the canonical build measurement.
- Release binary: **8.0 MiB** (single file; frontend assets are embedded by
  `tauri::generate_context!`, so the binary is self-contained apart from
  the system WKWebView).
- Canonical normal dependency set: **204 unique crate names**. The old 271
  value counted flattened name-version/tree rows, not unique names.
- The early main-process-only observation was ~115 MiB and omitted shared
  WebKit helpers. The controlled dashboard sample later measured 211 MiB for
  the Tauri process tree including three attributed helpers; the two figures
  are not directly comparable.
- Production source: **208 LoC** = 54 Rust (`src/main.rs`) + 6 (`build.rs`) +
  26 HTML + 50 JS + 72 CSS. Current config adds **40 LoC**
  (`tauri.conf.json` + capability), for 248 physical lines; the pre-packaging
  config-inclusive total was 247.
