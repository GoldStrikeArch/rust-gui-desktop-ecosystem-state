# Empirical results: one identical app, seven frameworks

**Run date:** 2026-07-07 (claims reconciled through 2026-07-09).

All numbers measured on the same machine (Apple M4 Pro, 24 GiB RAM, macOS 26.5.2,
rustc/cargo 1.96.1 stable) from the same functional spec (`apps/SPEC.md`: a
"Tasks" todo app — text input, Add button, Enter shortcut, per-row delete, live
counter, scrolling). Each app is an independent crate with its pinned framework
version. Builds ran **serially** with a warm cargo registry cache but a fully
cleaned `target/` (`cargo clean` → `cargo build --release`). Reproduce with
`./measure.sh --round iter1`; raw data in `measurements/results-iter1.csv` and per-app
`deps-flat.txt`.

Framework versions: iced 0.14.0 · egui/eframe 0.35.0 · gpui 0.2.2 ·
tauri 2.11.5 · xilem 0.4.0 · slint 1.17.1 · dioxus 0.7.9 (desktop).

## Headline table

<!-- BEGIN GENERATED: iter1-headline -->
| App | Clean build | Incremental | Binary (raw MiB) | Binary (stripped MiB) | Unique crate names | AccessKit in tree | LoC (Rust) | LoC (other UI) | Process survived 8 s |
|---|---:|---:|---:|---:|---:|---|---:|---:|---|
| iced | 22 s | 2 s | 9.9 | 8.5 | 140 | no | 74 | 0 | yes |
| egui | 27 s | 1 s | 12.0 | 10.5 | 156 | **yes** | 168¹ | 0 | yes |
| xilem | 28 s | 1 s | 11.4 | 9.7 | 143 | **yes** | 81 | 0 | yes |
| tauri | 36 s | 10² s | 8.0 | 6.4 | 204 | no³ | 60 | 148 | yes |
| dioxus | 40 s | 1 s | 5.7 | 4.9 | 279 | no³ | 90 | 0 | yes |
| slint | 42 s | 4 s | 14.7 | 13.2 | 302 | **yes** | 42 | 52 (.slint) | yes |
| gpui | 56 s | 1 s | 5.0 | 4.3 | 391 | no⁴ | 230 | 0 | yes |
| freya | 28 s | 1 s | 20.3 | 18.3 | 192 | **yes** | 90 | 0 | yes |
| vizia | 22 s | 1 s | 21.8 | 19.6 | 128 | **yes** | 162 | 0 | yes |
| floem | 42 s | 1 s | 16.8 | 14.2 | 226 | no | 91 | 0 | yes |
<!-- END GENERATED: iter1-headline -->

¹ 105 LoC app + optional 63-line AccessKit-driven `egui_kittest` test module.
² Tauri's build script re-validates config/assets every build.
³ Webview frameworks get accessibility from the browser engine's native a11y
tree instead of AccessKit.
⁴ gpui 0.2.2 predates the AccessKit integration merged to Zed main 2026-05-27
(zed#56065, unreleased); Zed currently opts out after integration failures.

## What the numbers say

- **For small apps on this machine, compile-time folklore is stale.** Every framework clean-builds this app in
  22–56 s on an M4 Pro and rebuilds incrementally in 1–4 s (except Tauri's 10 s
  build-script tax). This result is for one small spec, a warm registry, and
  cold target directories; it is not a general bound for GUI applications.
- **Executable size does not map cleanly to paradigm.** Native GPUI was the
  smallest raw executable (5.0 MiB), followed by webview-based Dioxus
  (5.7 MiB); Tauri was 8.0 MiB. A small webview executable externalizes the OS
  rendering engine and does not predict total runtime RSS.
- **Dependency count ≠ paradigm:** leanest tree is iced (140) and xilem (143);
  heaviest are gpui (391) and slint (302). The webview approach does not
  minimize the Rust dep tree (dioxus 279, tauri 204).
- **Rust LoC was lowest where the UI lived in another language:** slint used
  42 Rust + 52 DSL lines and tauri used 60 Rust + 148 HTML/JS lines. They were
  not the smallest implementations by total measured source: iced used 74,
  xilem 81, dioxus 90, slint 94, egui 168 including its optional tests (105
  production), tauri 208, and gpui 230. GPUI was largest because its core crate
  ships no ready-made text-input/high-level widget set, so this app hand-rolled
  text input from raw key events.

## Dependency overlap (fragmentation, measured)

From `scripts/overlap.py` over the 7 lockfile-resolved trees:

- **29 crates appear in all 7 trees.** `raw-window-handle` is the only
  universal **cross-platform GUI-interoperability abstraction**. Other
  universal names include GUI-related platform bindings such as
  `objc2-app-kit`, `core-graphics`, and `core-graphics-types`, as well as
  foundational crates such as libc, log, and syn.
  **13 of the 29** have version skew: `bitflags`, `block2`,
  `core-foundation`, `core-graphics`, `core-graphics-types`, `objc2`,
  `objc2-app-kit`, `objc2-foundation`, `raw-window-handle`, `rustc-hash`,
  `syn`, `thiserror`, and `thiserror-impl`. Other non-universal crates such as
  `hashbrown` and `kurbo` also have substantial skew.
- **Windowing:** winit in 4/7 trees (egui, iced, slint, xilem); tao (winit
  fork) in 2/7 (tauri, dioxus); gpui in-house — the measured picture of the
  fork/divergence story.
- **Text:** HarfRust shaping reaches 4/7 trees (egui in epaint; iced via
  cosmic-text; slint + xilem via parley) — shaping convergence confirmed at
  the lockfile level. But it arrives via 3 different text-layout stacks, and
  `swash`, `skrifa`, `fontdb`, `fontique`, `rustybuzz` coexist across trees —
  the layout/rasterization/fallback layers are still plural. gpui's tree
  carries cosmic-text 0.14 + rustybuzz + fontdb for Linux while using
  CoreText on macOS.
- **GPU:** wgpu in 3/7 (egui, iced, xilem/vello); slint resolved to
  femtovg+glow (GL) by default on desktop; gpui 0.2.2 still blade (wgpu on
  Zed main since Feb 2026); tauri/dioxus render in the webview.
- **Layout:** taffy in 2/7 (gpui as its core layout; slint as optional
  flexbox) — plus Blitz/bevy/Servo outside this sample.
- **AccessKit:** 3/7 (egui, slint, xilem) — exactly the frameworks the
  research identified as integrated; 4 accesskit crates each (core +
  platform adapters).
- **Pairwise similarity** peaks inside the winit+wgpu family (egui↔xilem
  45.9%, iced↔xilem 43.7%) and bottoms out across paradigms (iced↔tauri
  13.9%). The sample clusters by paradigm, but GPUI is not an island: its
  overlap with Dioxus and Slint is roughly 38–39%.

## Caveats

- Clean builds used a warm crates.io cache (network excluded) but cold
  `target/`; first-ever builds add download time.
- Release profile defaults per framework; no LTO/strip tuning beyond
  `strip -Sx` for the stripped column.
- LoC counts `.rs`, `.slint`, `.html`, `.js`, `.css` only (not Cargo.toml or
  tauri.conf.json). It includes any verification hooks stored inside those
  source files, so it is not a production-only LoC measure.
- The CSV launch field means only that the process survived eight seconds. A
  later independent audit observed an on-screen window for every current
  binary. Interaction evidence ranges from source review to app hooks and
  synthetic input; only egui retains executable assertion-based interaction
  tests for these early rounds.
- gpui measured at the last published crates.io release (0.2.2, Oct 2025);
  Zed main has since restructured the crate family, swapped Linux to wgpu,
  and merged AccessKit — none of it released.
