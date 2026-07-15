# 06 — Slint

*RCN "Cross-Platform GUI Desktop Apps" deep-dive. Researched 2026-07-07 on Apple M4 Pro,
macOS, rustc 1.96.1. Version verified: **Slint 1.17.1** (released 2026-07-07; 1.17.0 on
2026-06-24) — [releases](https://github.com/slint-ui/slint/releases),
[crates.io](https://crates.io/crates/slint). Repo: 23.1k stars, active daily pushes
([slint-ui/slint](https://github.com/slint-ui/slint)).*

---

## 1. Architecture & paradigm

Slint is the outlier in the Rust GUI landscape: the UI is not written in Rust at all, but
in **`.slint`**, a purpose-built declarative markup DSL that is **compiled** (to Rust via a
proc-macro or `build.rs`, to C++ via a code generator, or interpreted at runtime via
[`slint-interpreter`](https://crates.io/crates/slint-interpreter) for the JS/Python
bindings and the live-preview tooling). The mental model is closest to QML — deliberately
so (see §7).

- **Reactive model:** components expose typed **properties**; expressions that reference
  properties form a dependency graph and are lazily re-evaluated when inputs change
  (declarative bindings, including two-way `<=>` bindings). Events flow back through
  **callbacks** declared on the component interface. Since 1.17, two-way bindings also
  work on model rows ([1.17 blog](https://slint.dev/blog/slint-1.17-released)).
  Business logic lives in the host language; the boundary is the generated component API
  (`set_x`/`on_callback`, `Model`/`VecModel` for lists).
- **Language bindings:** Rust (crates.io), C++ (CMake/esp-idf, code-generated headers),
  JavaScript/Node (npm), Python ([PyPI `slint`](https://pypi.org/project/slint/)) — the
  same `.slint` **source** can serve all four ([slint.dev](https://slint.dev/)).
  Rust/C++ use native code generation while JavaScript/Python use interpreter/runtime
  bindings; this is not one precompiled UI artifact shared unchanged by every host language.
- **Tooling is a first-class product:** `slint-viewer` binary
  ([crates.io 1.17.1](https://crates.io/crates/slint-viewer)) renders `.slint` files with
  hot-reload; the [VS Code extension](https://marketplace.visualstudio.com/items?itemName=Slint.slint)
  ships an LSP with completion, go-to-definition and an embedded **live preview** with a
  design mode ([docs](https://docs.slint.dev/latest/docs/slint/guide/tooling/vscode/));
  [SlintPad](https://slintpad.com) is a browser playground; a
  [Figma-to-Slint plugin](https://slint.dev/blog/slint-1.10-released) generates `.slint`
  code and imports Figma variables ([1.12 blog](https://slint.dev/blog/slint-1.12-released)).
  New in 1.17: an **embeddable MCP server** so AI assistants can inspect a running app
  through its accessibility tree and inject input, and a `--remote` viewer mode plus
  Slint Viewer apps on Google Play / Apple App Store
  ([1.17 blog](https://slint.dev/blog/slint-1.17-released)).

## 2. Full stack table — shared vs in-house

Verified against the `slint` 1.17.1 crate manifest
([api/rs/slint/Cargo.toml @ v1.17.1](https://github.com/slint-ui/slint/blob/v1.17.1/api/rs/slint/Cargo.toml)):
`default = ["std", "backend-default", "renderer-femtovg", "renderer-software",
"accessibility", "compat-1-2", "system-tray"]` — and against the actual dependency tree of
the mini-app built for this report (deliverable B).

| Layer | Crate(s) | Shared or in-house? |
|---|---|---|
| Windowing | [`i-slint-backend-winit`](https://crates.io/crates/i-slint-backend-winit) → **winit 0.30.13** (+ glutin 0.32.3, softbuffer, copypasta, webbrowser); alternatives: [`i-slint-backend-qt`](https://crates.io/crates/i-slint-backend-qt) (Qt windowing+rendering+native style, used on Linux if Qt is installed), [`i-slint-backend-linuxkms`](https://crates.io/crates/i-slint-backend-linuxkms) (KMS/DRI, no compositor), [`i-slint-backend-android-activity`](https://crates.io/crates/i-slint-backend-android-activity), [`i-slint-backend-testing`](https://crates.io/crates/i-slint-backend-testing) (headless); chosen at startup by [`i-slint-backend-selector`](https://crates.io/crates/i-slint-backend-selector) or `SLINT_BACKEND` env var ([backends & renderers docs](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers.html)) | **Shared** (winit) wrapped in in-house abstraction; the `Platform` trait allows fully custom backends (that's the MCU story) |
| Renderer | **FemtoVG** ([`i-slint-renderer-femtovg`](https://crates.io/crates/i-slint-renderer-femtovg) → `femtovg` 0.25, GPU canvas over OpenGL, or wgpu via `FemtoVGWGPURenderer` since 1.16) — **default on desktop**; [`i-slint-renderer-skia`](https://crates.io/crates/i-slint-renderer-skia) (opt-in feature, Metal/Vulkan/D3D, richer effects e.g. `drop-shadow-spread`); in-house **software renderer** (`i-slint-renderer-software`, CPU + tiny-skia for paths, `no_std`-capable, line-by-line partial rendering for MCUs); Qt renderer with the Qt backend ([docs](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers.html)) | **Mixed**: FemtoVG and Skia are shared community/Google tech; the software renderer is in-house and is Slint's embedded differentiator |
| Text | Since 1.14 font loading and layout across the four renderers are unified behind **fontique** (system font discovery/fallback) + **Parley** (layout/BiDi) + **HarfRust** (shaping); **swash** rasterizes glyphs for the FemtoVG/software path, while rasterization remains renderer-specific elsewhere. `rustybuzz`/`fontdb` remain visible through SVG/resvg-related transitive paths, not as Slint's main text stack. On embedded/no-std software-renderer configurations, the documented text path is limited to Western scripts, so desktop complex-script behavior should not be projected onto that target ([1.14 blog](https://slint.dev/blog/slint-1.14-released), [1.16 blog](https://slint.dev/blog/slint-1.16-released)) | **Shared** Linebender/HarfBuzz text infrastructure on supported configurations, integrated by `i-slint-core`; renderer- and target-specific limits remain |
| Layout | In-house constraint-based row/column/grid layouts in the DSL (`VerticalLayout`, `HorizontalLayout`, `GridLayout`, min/max/preferred + stretch); **experimental `FlexboxLayout` powered by `taffy` 0.10** since 1.16 ([1.16 blog](https://slint.dev/blog/slint-1.16-released)) — taffy is already in the stable dependency tree | **In-house** core; adopting shared `taffy` for flexbox |
| Widgets | In-house `std-widgets.slint` (Button, LineEdit, ListView, ComboBox, RadioGroup since 1.17, Slider, TabWidget, …) in five maintained concrete styles: **fluent** (default on *all* platforms since 1.16), material (M3), cupertino, cosmic and qt (real QStyle rendering when Qt is present). `native` is a compatibility alias selecting one of those styles, not a sixth independent style ([styles docs](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/style/), [default-style-change blog](https://slint.dev/blog/default-native-style-change)) | **In-house** (styled, not native controls — except the qt style) |
| Accessibility | **AccessKit** (`accesskit` 0.24, `accesskit_winit`, `accesskit_macos` observed in tree), default-enabled for the Winit desktop path used here; Qt and custom backends are not covered by that evidence | **Shared** (AccessKit) on supported backends |
| Menus/tray | `muda` 0.19 for native menu bars/context menus on Windows & macOS; `SystemTrayIcon` element (1.17, `system-tray` default feature) | **Shared** (muda — the Tauri-ecosystem crate) |

## 3. Licensing — read this section before adopting

Slint's runtime is **tri-licensed**; you pick one ([slint.dev/pricing](https://slint.dev/pricing)):

1. **GPLv3** — free, for open-source applications (copyleft applies to your app).
2. **Royalty-Free License 2.0** — free, for **proprietary desktop, mobile, and web
   applications**. Verified from the
   [license text](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md):
   - Grants a "world-wide, royalty-free, non-exclusive license to use, reproduce, …
     distribute the Software as part of a Desktop, Mobile, or Web Application".
   - **Explicitly excludes Embedded Systems** ("systems designed for specific tasks
     within larger mechanical/electrical systems") — the license simply does not apply
     there.
   - **Attribution is mandatory**: either show the `AboutSlint` widget in an
     about/splash screen, or put the "Made with Slint" badge on the download page.
   - You may modify Slint but not remove license notices; you may **not** distribute
     Slint standalone, and you may **not** ship an app that re-exposes Slint's APIs
     (i.e. you can't build an SDK/runtime on the free license).
3. **Commercial (paid)** — removes attribution and covers embedded. Tiers
   ([pricing](https://slint.dev/pricing)): *Startup & Individual* (≤10 employees, <2M EUR
   turnover, <5 years old), *Small Enterprise* (≤50 employees, ≤10M EUR), *Enterprise*
   (12-month subscription with **perpetual fallback license** for the last subscribed
   x.y version incl. patch releases). Embedded distribution carries a **one-time
   royalty from $1.00/device** (volume discounts; non-commercial/open-source embedded is
   royalty-free).

**Implication for a commercial desktop team:** shipping closed-source desktop apps is
free under Slint's Royalty-Free License — the obligations include the attribution
badge/widget and remaining outside the license's embedded-system exclusion. This is not
a plain MIT/Apache story like iced/egui. Comparisons with Qt must account for Qt's
LGPL route as well as its commercial terms rather than describing Slint as categorically
more permissive. If your "desktop" app later moves into a kiosk/instrument/vehicle
deployment it MAY be classified as embedded ("specific task within a larger mechanical
or electrical system") — confirm the specific deployment with Slint. (Historical note: the free tier was
called the "Ambassador license" until the Royalty-Free license replaced it in
[Slint 1.1, mid-2023](https://slint.dev/blog/slint-1.1-released).) The Rust API itself is
additionally under a stabilized semver guarantee since 1.0.

## 4. Accessibility, keyboard, IME, RTL

- **AccessKit is integrated and on by default for the Winit desktop configuration tested
  here** (the `accessibility` cargo feature; `accesskit_macos`/`accesskit_winit` compile
  into that default build — verified in the mini-app's tree). This does not establish
  equivalent adapter coverage for the Qt backend or arbitrary custom `Platform`
  implementations. The DSL has first-class `accessible-*` properties (role, label,
  value, actions), and assistive tech can trigger actions (increment spinbox, set text)
  ([Slint 1.6 blog](https://slint.dev/blog/slint-1.6-released),
  [cargo features docs](https://docs.rs/slint/latest/slint/docs/cargo_features/index.html)).
  The 1.17 MCP server is built on this same accessibility tree
  ([1.17 blog](https://slint.dev/blog/slint-1.17-released)).
- **Known gaps:** full a11y exposure of *text input* widgets is still incomplete —
  screen readers don't track the caret correctly, braille cursor missing, value re-read
  on each keystroke ([#2895](https://github.com/slint-ui/slint/issues/2895),
  [#8732](https://github.com/slint-ui/slint/issues/8732)); large list views historically
  had a11y-related perf costs ([#3867](https://github.com/slint-ui/slint/issues/3867)).
  Net: better than most Rust frameworks (roles/labels/actions work), not yet
  screen-reader-complete for text editing.
- **Keyboard:** tab-focus across widgets, `FocusScope`, and a declarative `KeyBinding`
  element with `@keys(...)` plus `shortcut` on menu items since 1.16
  ([1.16 blog](https://slint.dev/blog/slint-1.16-released)).
- **IME:** built in and source-verified in this audit, but not exercised with a
  live composition because the test machine had no CJK input source enabled.
  The macOS Chinese IME issue
  [#1644](https://github.com/slint-ui/slint/issues/1644) was closed via PR
  #1728, and preedit is handled through winit. Virtual-keyboard/safe-area
  support on mobile landed in 1.15
  ([1.15 blog](https://slint.dev/blog/slint-1.15-released)).
- **RTL/BiDi: mixed, not absent.** The Babel app showed correct Unicode BiDi
  run reordering and logical keyboard/mouse selection across Arabic and Hebrew.
  Remaining gaps include no explicit base-direction/default-alignment control,
  no automatic RTL UI/layout mirroring, and codepoint-based Backspace behavior
  that can split a grapheme. The open umbrella issue is
  [#2294](https://github.com/slint-ui/slint/issues/2294).

## 5. OS shell integration

- **Menus: genuinely native.** `MenuBar` inside `Window` renders as the real macOS
  menu bar (top of screen) and uses **muda** on Windows/macOS; `ContextMenuArea` is
  native on macOS too ([MenuBar docs](https://docs.slint.dev/latest/docs/slint/reference/window/window/#menubar),
  [ContextMenuArea docs](https://docs.slint.dev/latest/docs/slint/reference/window/contextmenuarea/),
  [1.10 blog](https://slint.dev/blog/slint-1.10-released)). This is ahead of iced/egui.
- **Tray:** `SystemTrayIcon` element, Win/mac/Linux, default-enabled feature since 1.17
  ([1.17 blog](https://slint.dev/blog/slint-1.17-released)).
- **Dialogs: no native file dialogs.** Slint's `Dialog` is Slint-rendered; the project
  points users to `rfd`/`native-dialog`, with known event-loop footguns (blocking the
  Slint loop; use `rfd::AsyncFileDialog` + `slint::spawn_local`)
  ([discussion #1959](https://github.com/slint-ui/slint/discussions/1959),
  open request [#9781](https://github.com/slint-ui/slint/issues/9781)).
- **Drag & drop:** `DragArea`/`DropArea` shipped in 1.17 but are **within-app
  only**. Winit already emits `DroppedFile`; the gap is that Slint's stable
  backend/DataTransfer API does not expose file paths. The tray experiment used
  the `unstable-winit-030` escape hatch to install a raw event filter; that code
  path compiled, but a real Finder drop was not exercised
  ([1.17 blog](https://slint.dev/blog/slint-1.17-released)).
- **Multi-window:** supported (multiple `Window` components; live-preview redesign and
  multi-window support landed in [1.7](https://slint.dev/blog/slint-1.7-released)).
- **Dark mode:** automatic — all styles except qt have light/dark variants and follow the
  OS color scheme (verified empirically: the mini-app came up fluent-dark on a dark-mode
  Mac with zero code); `Palette.color-scheme` is exposed in the DSL
  ([styles docs](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/style/)).
- Clipboard (copypasta) and `open-url` (webbrowser) are built in.

## 6. Platform matrix

| Platform | Status | Notes/source |
|---|---|---|
| Windows 10/11 (x64, arm64) | Supported | [desktop docs](https://docs.slint.dev/latest/docs/slint/guide/platforms/desktop/) |
| macOS 14/15/26 (arm64) | Supported | same |
| Linux X11 + Wayland | Supported (winit backend; optional Qt backend; glibc+dbus assumed) | same |
| Embedded Linux (no compositor) | Supported via LinuxKMS backend (KMS/DRI, GL or wgpu) | [backends docs](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers.html) |
| **Microcontrollers** | **The differentiator in this seven-framework sample.** `no_std` core + in-house software renderer with line-by-line partial rendering; runs in <300 kB RAM; demonstrated on RP2040 (Pi Pico, 264 kB), STM32H7 family via an official STM32Cube integration (C++), ESP32(-C3/S3) | [MCU port blog](https://slint.dev/blog/porting-slint-to-microcontrollers), [embedded docs](https://docs.slint.dev/latest/docs/slint/guide/platforms/embedded/), [STM32 docs](https://docs.slint.dev/latest/docs/cpp/mcu/stm32), [supported boards](https://slint.dev/supported-boards) |
| WebAssembly | Supported canvas/WebGL target (winit + FemtoVG; no DOM), but officially "not recommended for general-purpose web applications"; no DOM-derived accessibility, own text rendering, Rust only | [web docs](https://docs.slint.dev/latest/docs/slint/guide/platforms/web/) |
| Android | Supported for Rust (backend `i-slint-backend-android-activity`); **C++ on Android added in 1.17**; NLnet-funded port | [Android docs](https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/android/), [1.17 blog](https://slint.dev/blog/slint-1.17-released), [NLnet](https://nlnet.nl/project/SlintAndroid/) |
| iOS | Supported for Rust through the Winit + Skia path, with documented simulator/device builds and App Store packaging; other language bindings are not supported on iOS. Safe-area and virtual-keyboard work landed in 1.15, and Slint Viewer is on the App Store. | [iOS docs](https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/ios/), [NLnet](https://nlnet.nl/project/SlintiOS/), [1.15 blog](https://slint.dev/blog/slint-1.15-released) |

## 7. Governance & backing

- Company: **SixtyFPS GmbH** (product renamed to Slint in 2022), Germany
  (Berlin/Brandenburg), remote-first ([about-us](https://slint.dev/about-us)).
- Founders: **Olivier Goffart, Simon Hausmann, Aurindam Jana** — deep **Qt heritage**:
  Goffart and Hausmann met at Trolltech; Hausmann was later lead developer/maintainer of
  the QtQml engine, Goffart co-founded Woboq. Started SixtyFPS in 2020
  ([devclass interview](https://devclass.com/2023/04/06/interview-the-story-behind-slint-1-0-a-new-cross-platform-gui-toolkit-coded-in-rust)).
  Slint is, culturally, "QML done again in Rust, without the C++ baggage."
- Funding: no public VC rounds disclosed; revenue model is commercial licenses
  (embedded royalties + enterprise subscriptions); the Android and iOS ports received
  **NLnet/EU grants** ([SlintAndroid](https://nlnet.nl/project/SlintAndroid/),
  [SlintiOS](https://nlnet.nl/project/SlintiOS/)). Service partners include **KDAB** and
  **Witekio** ([kdab.com/slint](https://www.kdab.com/software-technologies/slint/),
  [witekio.com](https://witekio.com/embedded-gui/c-application-development/slint/)).
- Release cadence: minor release roughly every 2–3 months with fast patch follow-ups
  (1.15 → early 2026, 1.16 → 2026-04-16, 1.17 → 2026-06-24, 1.17.1 → 2026-07-07)
  ([releases](https://github.com/slint-ui/slint/releases)).
- Production users (named, verifiable): **LibrePCB** is migrating its Qt GUI to Slint
  for 2.0 (desktop, open source); **WesAudio** ships commercial desktop control apps for
  its audio hardware; **MOTOR Ai** (Berlin autonomous-driving) HMI built with Slint by
  KDAB; primary commercial traction is **embedded HMIs**
  ([slint.dev](https://slint.dev/), [KDAB case study](https://www.kdab.com/software-technologies/slint/)).

## 8. The DSL tradeoff

**Strengths**
- Designer/dev separation with real tooling: live preview, SlintPad, Figma plugin — the
  only framework in this seven-framework sample with a designer workflow comparable to
  QML/Flutter.
- Compile-time optimization: bindings are analyzed and pruned ahead of time for native
  code-generation paths. The same `.slint` source can drive Rust, C++, JS, and Python,
  but the generated-versus-interpreted artifacts differ by binding — valuable for
  mixed-language orgs and for the C++-dominated embedded industry.
- The DSL enforces UI/logic separation; property/callback interfaces are typed and
  code-generated.
- Scales *down*: the same markup runs on a 264 kB-RAM MCU — a distinctive
  explicitly supported MCU-to-desktop path in this seven-framework sample.

**Costs**
- A new language to learn (own syntax, own type system, `@tr`, states, animations);
  rustfmt/clippy/most Rust tooling don't apply inside `.slint`.
- Escape hatches are explicit and chatty: every value crossing the boundary must be
  declared as a property/callback; complex custom widgets mean either fighting the DSL
  or dropping to canvas-level primitives. Generated types (`slint::include_modules!()`)
  are invisible to grep.
- Ecosystem lock-in: `.slint` files are useless outside Slint, and third-party widget
  libraries are scarce compared to the widget ecosystems around web-based (Tauri) UIs.
- Styled, not native, widgets — and since 1.16 the default is **Fluent everywhere**, so
  a default-config app looks like a Windows app on macOS
  ([default-style-change blog](https://slint.dev/blog/default-native-style-change)).

## 9. Docs & learning resources

High quality overall: [docs.slint.dev](https://docs.slint.dev) has per-language guides
(Rust/C++/JS/Python), a language reference for the DSL, tutorials (memory game),
platform guides (desktop/embedded/MCU/Android/web), and generated API docs; SlintPad
gives zero-install experimentation; the VS Code extension documents-on-hover. Weak
spots: some docs pages moved recently (several 404s on old URLs during this research),
renderer/backend defaults are documented in the crate manifest rather than prominently
in the guide, and style-selection docs lagged the 1.16 fluent-default change in places.
Rating: **4/5**.

## 10. Friction log (from building the mini-app, deliverable B)

App: `apps/slint-app/` — Slint **1.17.1** pinned, build-script-compiled `ui/main.slint`
(52 LoC) + `src/main.rs` (39 LoC) + `build.rs` (3 LoC) = **94 LoC**. Full detail in
`apps/slint-app/GAPS.md`.

- **Zero source-level spec gaps** — all 7 requirements mapped 1:1 to DSL
  features (`LineEdit.accepted` for Enter, `ListView` + repeater,
  `tasks.length` binding for the live counter). Xilem and Dioxus also had zero
  source-level SPEC-1 gaps, so this result is not unique to Slint.
- Canonical clean release build: **42 s**; forced incremental rebuild (after touch main.rs) **4 s**;
  binary **15,434,560 bytes raw / 13,873,752 bytes (13.2 MiB) stripped**;
  **302 unique crate names / 310 external name-version entries** in the
  normal-edge dependency tree (winit, glutin, FemtoVG, AccessKit, muda, taffy,
  fontique, Parley, HarfRust, swash, tiny-skia…).
- Launched and verified alive at 10 s; fluent-dark rendering was manually
  observed on a dark-mode Mac, with no runtime warnings and a clean kill. The
  original SPEC-1 screenshot artifact was not retained.
- Friction: clearing the input from Rust needs an exported two-way property
  (`in-out property <string> input-text <=> input.text;`) because inner elements aren't
  reachable from the host language; trim/ignore-empty logic must live in Rust while the
  event wiring lives in `.slint`, splitting one interaction across two files;
  `slint`/`slint-build` versions must be pinned in lock-step.
- Nice surprise: `i-slint-backend-testing` (headless testing backend, experimental) and
  the 1.17 MCP server mean Slint has a plausible automated-UI-testing story — most Rust
  frameworks have none.
