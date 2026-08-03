# Rust GUI Ecosystem Map: Shared Infrastructure and Fragmentation Analysis

> RCN "Cross-Platform GUI Desktop Apps" initiative ([rcn#46](https://github.com/Rust-Commercial-Network/rcn/issues/46)) — centerpiece document.
> External ecosystem data snapshotted 2026-07-07; empirical rounds reconciled
> through 2026-07-10. Versions/dates came from the crates.io API,
> reverse-dependency counts from `reverse_dependencies` (`meta.total`), repo
> health from the GitHub API, and adoption claims from live manifests. Dynamic
> counters are dated snapshots, not live values. “Recent author concentration”
> below is an audit heuristic based on public commit attribution, not a measured
> probability that a project collapses if contributors leave. Empirical
> provenance is in `measurements/EVIDENCE.md`.

**Headline findings.** (1) The shared-infrastructure layer is consolidating
faster than commonly believed: the three principal reusable pure-Rust text
stacks adopted HarfRust, Zed moved its Linux renderer to wgpu,
Slint/floem/Bevy migrated to Parley/fontique, and GPUI merged AccessKit
(unreleased and currently disabled in Zed). (2) The sample still contains four
text-layout stacks, at least six framework-side renderer families targeting a
mix of APIs, and three windowing layers. (3) Several shared crates have highly
concentrated recent authorship and mixed public support. Sustainability is an
important lever, but funding and continuity risk remain audit inferences.

## 1. Layered map of the ecosystem

### 1.1 Windowing and input

| Crate | Version (rel. date) | Maintainer/org | crates.io reverse deps | Used by |
|---|---|---|---|---|
| [winit](https://github.com/rust-windowing/winit) | 0.30.13 (2026-03-02); 0.31.0-beta.2 (2025-11-16) | rust-windowing (kchibisov, madsmtm, maroider active) | 1,225 | iced, eframe/egui, xilem/masonry, slint (winit backend), freya, wgpu examples |
| [tao](https://github.com/tauri-apps/tao) | 0.35.3 (2026-05-23) | tauri-apps | 83 | tauri/wry, dioxus-desktop (migration proposed) |
| [raw-window-handle](https://github.com/rust-windowing/raw-window-handle) | 0.6.2 (2024-05-17, stable 2+ yrs) | rust-windowing | 598 | everything (the interop keystone) |
| gpui platform layer | in-tree (zed-industries/zed) | Zed Industries | n/a | gpui only |

**winit** remains the ecosystem default with 1,225 reverse dependencies, but its governance story is mixed. The API is mid-transition: 0.30's `ApplicationHandler` model is the stable workhorse in the sampled winit-based frameworks, while the trait-object redesign ("winit-next", tracked in [#3367](https://github.com/rust-windowing/winit/issues/3367) and prototyped in [rust-windowing/winit-next](https://github.com/rust-windowing/winit-next)) shipped as [0.31.0-beta.1](https://github.com/rust-windowing/winit/releases/tag/v0.31.0-beta.1) on 2025-11-16 and had remained in beta for roughly seven months and three weeks at the July 7 snapshot — nearly, not more than, eight months. The project runs weekly public maintainer meetings ([README](https://github.com/rust-windowing/winit)). Public work on the redesign is concentrated among a small set of contributors: prolific contributor notgull (X11, softbuffer) announced a burnout hiatus in March 2025 ([notgull.net/burnout](https://notgull.net/burnout/)), while kchibisov (Wayland) and madsmtm (macOS/iOS) remained prominent. This is a continuity-risk signal, not a literal bus-factor measurement. Wayland/X11 backends are actively developed (0.31 betas add Wayland pen/gesture input, X11 smooth resize).

**tao** is Tauri's fork of winit. The stated fork rationale ([tao README](https://github.com/tauri-apps/tao), [tao #509](https://github.com/tauri-apps/tao/issues/509)): winit lacked GUI-app features — menus (essential on macOS), system tray, and a GTK-based Linux backend needed for wry's webview embedding. Menus and tray have since been extracted into `muda`/`tray-icon` (winit-compatible), removing part of the original rationale. Divergence is *widening*, not narrowing: tao still uses the pre-0.30 closure-based event-loop API (verified in [source](https://github.com/tauri-apps/tao/blob/dev/src/event_loop.rs)) and ships raw-window-handle 0.4/0.5/0.6 simultaneously. The README promises an eventual return to winit, and [wry discussion #1014](https://github.com/tauri-apps/wry/discussions/1014) explores a GTK backend for winit. Dioxus 0.7.9 still uses Tao/Wry; open [dioxus #2706](https://github.com/DioxusLabs/dioxus/issues/2706) proposes a winit migration.

**Custom platform layers.** gpui (Zed) still bypasses winit entirely with a per-platform layer, restructured in 2026 into `gpui_macos` (objc2/AppKit), `gpui_windows` (windows crate), `gpui_linux` (wayland-client/x11rb directly), plus `gpui_wgpu` (verified via the [zed crates tree](https://github.com/zed-industries/zed/tree/main/crates)). Slint ships three windowing backends — Qt, winit, LinuxKMS — with runtime selection order Qt → winit → LinuxKMS ([slint backend docs](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers/)); the Rust crate's default build uses the winit backend.

**raw-window-handle** is the quiet success story: 0.6.2 has been unchanged since May 2024, giving the surface/window interop layer two years of stability after the painful 0.5→0.6 ecosystem split (winit 0.27 once blocked on wgpu's rwh upgrade — [winit #2415](https://github.com/rust-windowing/winit/issues/2415)). Current direct APIs such as wgpu, winit, and gpui use 0.6, while compatibility layers such as Tao still carry 0.4/0.5/0.6.

### 1.2 GPU abstraction and rendering

| Crate | Version (rel. date) | Maintainer/org | Reverse deps | Used by |
|---|---|---|---|---|
| [wgpu](https://github.com/gfx-rs/wgpu) | 30.0.0 (2026-07-01), ~3-month major cadence | gfx-rs; Mozilla major contributor (Firefox WebGPU) | ~1,273 | iced (default), eframe (default since 0.34), xilem (via vello), slint (femtovg-wgpu), **zed/gpui Linux**, blitz |
| [vello](https://github.com/linebender/vello) | 0.9.0 (2026-05-15); vello_cpu/hybrid 0.0.9 | Linebender | 52 | masonry/xilem, blitz (anyrender_vello) |
| [blade-graphics](https://github.com/kvark/blade) | 0.8.4 (2026-04-18) | kvark | 15 | (lost Zed in Feb 2026) |
| [skia-safe](https://github.com/rust-skia/rust-skia) | 0.99.0 (2026-06-19) | pragmatrix / rust-skia | 54 | slint (Skia renderer), freya (fork), blitz (anyrender_skia option) |
| [femtovg](https://github.com/femtovg/femtovg) | 0.25.1 (2026-05-29) | Slint-adjacent (tronical top contributor) | 16 | slint (default-feature renderer) |
| [tiny-skia](https://github.com/linebender/tiny-skia) | 0.12.0 (2026-02-02) | **Linebender** (transferred from RazrFalcon) | 364 | iced_tiny_skia (default software renderer; hence COSMIC), resvg |
| [glow](https://github.com/grovesNL/glow) | 0.17.0 (2026-03-07) | grovesNL | 123 | egui_glow (now opt-in), femtovg GL backend |
| [softbuffer](https://github.com/rust-windowing/softbuffer) | 0.4.8 (2025-12-13) | rust-windowing | 105 | slint winit backend (software path) |

**wgpu is the convergence point.** Governance is anchored by Mozilla — Firefox 141 shipped WebGPU-on-wgpu on Windows in July 2025 ([Mozilla Gfx blog](https://mozillagfx.wordpress.com/2025/07/15/shipping-webgpu-on-windows-in-firefox-141/)) — plus Servo and Deno. In the July 7 snapshot it had more reverse dependencies than winit (~1,273 vs 1,225). The decisive 2026 datapoint: **Zed removed blade and rebuilt gpui's Linux renderer on wgpu** ([zed PR #46758](https://github.com/zed-industries/zed/pull/46758), merged Feb 2026); GPUI still uses Metal directly on macOS and D3D on Windows. Together with eframe's wgpu default and Slint's wgpu path, every major **native-GPU** stack in this sample has some supported wgpu path. Tauri/default Dioxus are webviews rather than wgpu renderers.

**The framework-side renderer layer is where fragmentation persists.** At least six maintained families coexist in this sample, targeting a mix of wgpu, platform GPU APIs, OpenGL/Skia, and CPU software: Vello, Skia, FemtoVG, tiny-skia/software paths, Iced's renderer, egui/epaint, and GPUI's platform pipeline depending on the inclusion rule. Classic Vello is alpha; only Vello Hybrid was described as roughly beta. This is not literally a set of renderers all “above wgpu.” Blade survives (0.8.4, Apr 2026) after losing Zed as its anchor user.

### 1.3 Text: the flagship duplication story

Text is where the ecosystem's duplication was historically worst — and where the most dramatic consolidation of 2025–26 happened. Two headline events:

1. **Shaping converged across the principal reusable pure-Rust stacks.** [harfrust](https://github.com/harfbuzz/harfrust) — the HarfBuzz org's Rust port — shipped 0.1.0 in June 2025 and reached cosmic-text 0.15, Parley 0.6, and egui's epaint **0.35** ([egui #8031](https://github.com/emilk/egui/pull/8031)). GPUI macOS/Windows and browser/webview stacks still use platform/browser shapers. Rustybuzz remains released (with a deprecation discussion rather than an archive); swash remains important as a rasterizer.
2. **The layout layer is consolidating on parley — but not universally.** [parley](https://github.com/linebender/parley) 0.11 + [fontique](https://crates.io/crates/fontique) 0.11 (Linebender; historically funded via Google Fonts — Raph Levien left Google Oct 2025 for Canva, [Linebender post](https://linebender.org/blog/tmil-19/)) picked up **Slint 1.14** (Oct 2025 — "we've unified everything behind the Fontique and Parley crates" across all four renderers, [blog](https://slint.dev/blog/slint-1.14-released), [PR #9564](https://github.com/slint-ui/slint/pull/9564)), **floem** (Mar 2026, [PR #1034](https://github.com/lapce/floem/pull/1034), leaving cosmic-text), **Bevy 0.19** (Jun 2026, [#21765](https://github.com/bevyengine/bevy/issues/21765), leaving cosmic-text), plus masonry/xilem and blitz (dioxus-native). Meanwhile **cosmic-text** ([pop-os/cosmic-text](https://github.com/pop-os/cosmic-text), 0.19.0, 137 reverse deps) keeps iced, libcosmic/COSMIC, and — notably — **gpui's Linux text system** (zed's `gpui_wgpu` contains a `CosmicTextSystem`, [source](https://github.com/zed-industries/zed/blob/main/crates/gpui_wgpu/src/cosmic_text_system.rs)).

**Per-framework text stack (July 2026):**

| Framework | Shaping | Rasterization | Font loading/fallback | Paragraph/rich-text layout |
|---|---|---|---|---|
| iced 0.14 | harfrust (via cosmic-text) | swash (via cosmic-text) | fontdb + cosmic-text custom fallback | cosmic-text `Buffer` |
| egui 0.35 | harfrust (in epaint) | skrifa + vello_cpu (replaced ab_glyph in 0.34, [#7694](https://github.com/emilk/egui/pull/7694)) | bundled fonts; no system enumeration by default | in-house epaint galley (no paragraph-level BiDi reordering [#1016](https://github.com/emilk/egui/issues/1016), no color emoji [#2551](https://github.com/emilk/egui/issues/2551)) |
| gpui (Zed) | CoreText (macOS) / DirectWrite (Windows) / harfrust via cosmic-text (Linux) | CoreText / DirectWrite / swash | zed-font-kit fork + platform APIs; fontdb on Linux | gpui's own line_layout/line_wrapper |
| tauri | OS webview engine | webview | webview | webview (full HTML/CSS) |
| xilem/masonry 0.4 | harfrust (via parley) | Vello 0.6 | fontique | parley rich text |
| slint 1.14+ | harfrust (via parley) | swash (FemtoVG/Software), Skia, Qt | fontique (replaced fontdb, [PR #9564](https://github.com/slint-ui/slint/pull/9564)) | parley, unified across renderers |
| dioxus (webview) | OS webview engine | webview | webview | webview |
| dioxus-native (blitz) | harfrust (via parley) | vello (wgpu) | fontique | parley + Stylo CSS |

**Count: the sample maintains 4 Rust-side text-layout approaches** — cosmic-text, Parley/fontique, epaint's galley, and GPUI's platform-shaping/own line layout — plus browser/webview engines. HarfRust is shared across the first three reusable pure-Rust stacks, not everywhere. Font fallback in the sample is at least four-way: fontique, fontdb/cosmic-text, GPUI platform APIs, and egui bundled fonts; webviews externalize it to the browser.

**Why the holdouts hold out** (maintainer statements):
- **egui**: emilk in [#3378](https://github.com/emilk/egui/issues/3378) — (2024) "Parley is very promising, but not yet ready, while Cosmic Text is ready for production today." The parley integration attempt ([draft PR #5784](https://github.com/emilk/egui/pull/5784)) stalled on an API-model mismatch — its author: parley "really wants to be given a full rectangle that it can arrange text inside… and egui really doesn't work with that model." egui instead adopted the *low-level* shared bricks (harfrust, skrifa, vello_cpu) while keeping galley layout in-house — a "share the bricks, not the wall" pattern.
- **iced**: chose cosmic-text in [PR #1697](https://github.com/iced-rs/iced/pull/1697), calling it "a long-time-missing piece in the Rust GUI ecosystem"; no sign of moving.
- **BiDi remains fragmented**: cosmic-text uses unicode-bidi; parley uses ICU4X components; epaint uses neither for paragraph-level reordering (egui still shapes individual RTL runs).

### 1.4 Layout

| Crate | Version | Maintainer | Reverse deps | Adopters |
|---|---|---|---|---|
| [taffy](https://github.com/DioxusLabs/taffy) | 0.12.1 (2026-07-03) | DioxusLabs org; primary maintainer nicoburns, with Bevy contributors — explicitly cross-team | 124 (8.9M downloads) | bevy_ui, gpui/Zed, floem, Servo (grid), blitz/dioxus-native, slint (experimental) |
| [morphorm](https://github.com/vizia/morphorm) | 0.8.0 (2026-04-23) | vizia | 6 | vizia only (via git rev) |

**taffy is another major convergence layer in this sample.** It implements CSS block, flexbox, and grid, plus `calc()`. Verified adopters (all from live Cargo.tomls): [bevy_ui](https://github.com/bevyengine/bevy/blob/main/crates/bevy_ui/Cargo.toml) (taffy 0.10), [Zed's gpui](https://github.com/zed-industries/zed/blob/main/crates/gpui/Cargo.toml) (pinned =0.10.1), [floem](https://github.com/lapce/floem/blob/main/Cargo.toml) (0.9.2 with grid), [blitz](https://github.com/DioxusLabs/blitz/blob/main/Cargo.toml) (0.12.1, full), and — a notable external validation — **Servo uses taffy 0.12.1 for CSS Grid** (grid feature only; Servo keeps its own flexbox — [servo Cargo.toml](https://github.com/servo/servo/blob/main/Cargo.toml), [components/layout/taffy](https://github.com/servo/servo/tree/main/components/layout/taffy)). Surprise 2026 datapoint: **Slint now embeds taffy** for an experimental `FlexboxLayout` element ([internal/core/Cargo.toml](https://github.com/slint-ui/slint/blob/master/internal/core/Cargo.toml), [experimental docs](https://github.com/slint-ui/slint/blob/master/docs/astro/src/content/docs/guide/experimental/flexboxlayout.mdx)) alongside its DSL constraint layout.

**Holdouts:** iced (own flex layout; no taffy dep in workspace) and egui (immediate-mode placement is a different paradigm; a third-party [egui_taffy](https://crates.io/crates/egui_taffy) bridge exists at ~64k downloads). morphorm remains a single-framework engine for vizia.

### 1.5 Accessibility

**[AccessKit](https://github.com/AccessKit/accesskit)** (accesskit 0.24.1, accesskit_winit 0.33.1, both 2026-06-12; 115 reverse deps, 20.6M downloads) is the ecosystem's single cross-toolkit a11y abstraction: a **push-based, Chromium-inspired accessibility tree** — the toolkit pushes full-then-incremental tree updates; platform adapters retain the tree and speak Windows UI Automation, macOS NSAccessibility, Unix AT-SPI (via zbus/atspi), iOS UIAccessibility, and Android. The push model works even for immediate-mode GUIs. `accesskit_winit` is the standard integration path; C and Python bindings exist. A **web/canvas adapter is still only "planned"** ([README](https://github.com/AccessKit/accesskit)).

**Governance/funding — the risk section.** Public commit history shows recent work concentrated around Matt Campbell and Arnold Loubriat; that is an authorship-concentration signal, not proof that only two people understand the code or are unpaid. A 2023–24 Sovereign Tech Fund project administered through GNOME funded related accessibility work and was mostly wrapped up by 2025 ([GNOME STF report](https://blogs.gnome.org/tbernard/2025/04/11/gnome-stf-2024/)). This audit found no later direct institutional AccessKit grant; personal Sponsors are visible. The project remains active. GTK 4.18 also merged an opt-in AccessKit backend ([GTK dev blog](https://blogs.gnome.org/gtk/2025/05/12/an-accessibility-update/)).

**Adoption map (July 2026, each verified):**

| Framework | AccessKit status | Evidence |
|---|---|---|
| egui/eframe | Integrated since 0.20 (2022); AccessKit schema types are required by core egui as of 0.35, while eframe's native adapter remains optional and is enabled by default; `kittest` is built on the tree | [PR #2294](https://github.com/emilk/egui/pull/2294), [eframe Cargo.toml](https://github.com/emilk/egui/blob/main/crates/eframe/Cargo.toml) |
| slint | Integrated and default-enabled on the Winit desktop path; this does not establish equivalent AccessKit integration for Qt or custom backends | [winit backend Cargo.toml](https://github.com/slint-ui/slint/blob/master/internal/backends/winit/Cargo.toml) |
| masonry/xilem | Integrated, non-optional; parley built with its `accesskit` feature (text a11y) | [masonry Cargo.toml](https://github.com/linebender/xilem/blob/main/masonry/Cargo.toml) |
| iced | **Not merged.** Tracking [#552](https://github.com/iced-rs/iced/issues/552) open since 2020; [PR #3281](https://github.com/iced-rs/iced/pull/3281) closed unmerged 2026-03; draft [PR #3111](https://github.com/iced-rs/iced/pull/3111) still open | iced master has no accesskit dep |
| gpui/Zed | **Merged 2026-05-27** ([PR #56065](https://github.com/zed-industries/zed/pull/56065)); early stage — annotations are landing incrementally, and Zed currently opts out with `Application::inaccessible()` after integration panics; end-user screen-reader readiness is unverified | [zed Cargo.toml](https://github.com/zed-industries/zed/blob/main/Cargo.toml) pins accesskit 0.24 |
| tauri / dioxus (webview) | n/a — inherit the browser engine's native a11y tree | architectural |
| dioxus-native (blitz) | Integrated (accesskit 0.24 in workspace) | [blitz Cargo.toml](https://github.com/DioxusLabs/blitz/blob/main/Cargo.toml) |
| bevy | Integrated (bevy_a11y) since 0.10 — "first general-purpose game engine with built-in accessibility" | [bevy_a11y Cargo.toml](https://github.com/bevyengine/bevy/blob/main/crates/bevy_a11y/Cargo.toml), [accesskit.dev](https://accesskit.dev/accesskit-integration-makes-bevy-the-first-general-purpose-game-engine-with-built-in-accessibility-support/) |
| floem | **No accesskit dependency** — notable gap | [floem Cargo.toml](https://github.com/lapce/floem/blob/main/Cargo.toml) |
| vizia | Integrated behind the optional, default-enabled `accesskit` feature | [vizia Cargo.toml](https://github.com/vizia/vizia/blob/main/Cargo.toml) |

**Is a11y solvable ecosystem-wide through AccessKit?** Largely yes at the abstraction layer — it is the dominant shared path in this sample (egui, slint, masonry/xilem, bevy, vizia, blitz, GTK 4.18, and gpui all integrate it; iced and floem remain unintegrated, while Zed currently disables its merged GPUI path). What it does *not* solve: (a) no shipped web/canvas adapter, so wasm-canvas GUIs (egui web) still rely on side-channel approaches; (b) the Linux AT-SPI stack underneath remains a risk — the Wayland-native successor "Newton" is still a prototype ([GNOME STF report](https://blogs.gnome.org/tbernard/2025/04/11/gnome-stf-2024/)); (c) AccessKit is a serialization/adapter layer — semantics, focus order, and text-editing granularity must still be implemented and tested per toolkit; (d) no CI-grade cross-screen-reader test harness was found (egui's kittest tests the AccessKit tree, not NVDA/VoiceOver behavior); and (e) recent public authorship is concentrated. The funding interpretation is discussed separately from those observable facts.

### 1.6 OS shell integration crates

| Crate | Version (rel. date) | Maintainer | Rev deps | winit-compatible? | Health notes |
|---|---|---|---|---|---|
| [rfd](https://github.com/PolyMeilex/rfd) (file dialogs) | 0.17.2 (2026-01-12) | PolyMeilex (individual) | ~479 | Yes (raw-window-handle) | Healthy; Linux default is XDG portal + Wayland, GTK3 only opt-in — cleanest Linux story here |
| [tray-icon](https://github.com/tauri-apps/tray-icon) | 0.24.1 (2026-06-10) | tauri-apps (amrbashir 75 commits, next human 9) | 81 | Yes on Win/macOS; **Linux requires a second GTK event-loop thread** ([winit example](https://github.com/tauri-apps/tray-icon/blob/dev/examples/winit.rs)) | Active |
| [muda](https://github.com/tauri-apps/muda) (menus) | 0.19.3 (2026-06-17) | tauri-apps (amrbashir 135 commits) | 32 | Win/macOS yes; **its Linux menubar API needs a `gtk::Window` and cannot attach directly to a plain winit window** | Active |
| [notify-rust](https://github.com/hoodie/notify-rust) | 4.18.0 (2026-06-16) | hoodie (individual) | 389 | Yes (no event-loop coupling) | Active; macOS backend a self-admitted "small subset"; Windows backend is itself a tauri-apps crate (tauri-winrt-notification) |
| [global-hotkey](https://github.com/tauri-apps/global-hotkey) | 0.8.0 (2026-05-01) | tauri-apps (amrbashir 52 commits) | 21 | Yes | Its Linux backend is X11-only; `ashpd` exposes the Wayland portal separately |
| [arboard](https://github.com/1Password/arboard) (clipboard) | 3.6.1 (2025-08-23) | 1Password | **1,052** (highest here) | Yes | Alive but slow (no release in ~10 months); corporate side-project |
| [window-vibrancy](https://github.com/tauri-apps/window-vibrancy) | 0.7.1 (2025-11-12) | tauri-apps | 6 | Yes (rwh) | Win/macOS only; "Linux: Unsupported" |
| [auto-launch](https://github.com/zzzgydi/auto-launch) | 0.6.0 (2026-01-10) | zzzgydi (individual) | 9 | Yes | Slow-moving; matters mainly because tauri's autostart plugin depends on it |
| [keyring](https://github.com/open-source-cooperative/keyring-rs) | 4.1.4 (2026-07-06) | open-source-cooperative (Dan Brotsky) | 547 | Yes | Very healthy; v4 split into keyring-core + per-store crates |

**The tauri-apps concentration is the structural fact of this layer.** Four of nine listed crates live in the tauri-apps org, and public history shows high recent authorship concentration after amrbashir's activity declined. That does not establish private employment, sponsorship, or project ownership. The verified technical constraints remain: muda menubars require a GTK window on Linux, tray-icon's winit example runs a parallel GTK loop, and global-hotkey itself is X11-only. Protocol-specific alternatives already exist (`ashpd` GlobalShortcuts, `ksni` StatusNotifierItem, Rust DBusMenu implementations), but no single maintained framework-neutral facade matches the tauri-apps crates' combined API and platform coverage.

### 1.7 IME, i18n, RTL

**IME.** [winit's `Ime` events](https://docs.rs/winit/latest/winit/event/enum.Ime.html) (Enabled/Preedit/Commit/Disabled) are the shared substrate for winit-based frameworks, but coverage is desktop-only: `set_ime_allowed` on iOS/Android merely toggles the soft keyboard, web is unsupported, and `set_ime_purpose` hints are Wayland-only (verified in [0.30.13 source](https://github.com/rust-windowing/winit/blob/v0.30.13/src/window.rs)). Per framework: **egui** has had `Event::Ime` since 0.28 with steady fixes through 0.35's proper preedit visuals ([changelog](https://github.com/emilk/egui/blob/main/CHANGELOG.md)); **iced** landed IME in 0.14.0 (Dec 2025, [PR #2777](https://github.com/iced-rs/iced/pull/2777)); **slint** has built-in desktop and Android paths (the local test verified source/API wiring but did not exercise a CJK IME); **gpui/Zed** is mature on macOS (NSTextInputClient) and implements `zwp_text_input_v3` on Wayland, but Windows IME is rough post-launch ([#40300](https://github.com/zed-industries/zed/issues/40300), [#41881](https://github.com/zed-industries/zed/issues/41881)); **tauri/dioxus-webview** inherit browser-engine IME paths, which were not exercised with a CJK input method in this audit.

**RTL/BiDi.** Sharply split: **iced** gets BiDi via cosmic-text;
**xilem/slint/blitz/floem** via Parley/ICU4X; **egui** shapes individual RTL
runs but lacks paragraph-level reordering ([#1016](https://github.com/emilk/egui/issues/1016)); and webviews use browser engines. The Babel run found Slint's run reordering and tested selection correct; remaining gaps include explicit base direction/default alignment, UI mirroring ([#2294](https://github.com/slint-ui/slint/issues/2294)), and codepoint-oriented backspace behavior.

**i18n.** [ICU4X](https://github.com/unicode-org/icu4x) (icu 2.2.0, Apr 2026) has quietly become foundational — inside parley (hence xilem, slint, blitz, floem) and Servo; icu_properties alone has 414M downloads. Localization is less settled: [fluent-rs](https://github.com/projectfluent/fluent-rs) is maintained but slow (fluent 0.17, May 2025); slint ships gettext-style translations (`@tr()` → .po/.mo, [docs](https://docs.slint.dev/latest/docs/slint/guide/development/translations/)); [i18n-embed](https://crates.io/crates/i18n-embed) (0.16.0, Jul 2025) is the common glue. No framework-independent standard exists.

### 1.8 Packaging and distribution

| Tool | Version (date) | Maintainer | Verdict for GUI apps |
|---|---|---|---|
| [tauri-bundler](https://crates.io/crates/tauri-bundler) + [tauri-plugin-updater](https://crates.io/crates/tauri-plugin-updater) | 2.9.4 (2026-06-28) / 2.10.1 | tauri-apps | Most complete chain (AppImage/deb/rpm, dmg/.app, msi/NSIS + mandatory-signature minisign updater, [docs](https://v2.tauri.app/plugin/updater/)) — but framework-locked to Tauri |
| [cargo-packager](https://github.com/crabnebula-dev/cargo-packager) | 0.11.8 (2025-11-27) | CrabNebula | A framework-neutral option; formats: .app/dmg, NSIS/WiX, deb/AppImage/Pacman (no rpm/flatpak/snap). The measurable signal is the gap since the latest crates.io release; repository commits continued through 2026-06-23. Modest external adoption (e.g. [bitwarden-desktop-next](https://github.com/dani-garcia/bitwarden-desktop-next)) |
| [cargo-bundle](https://github.com/burtonageo/cargo-bundle) | 0.11.0 (2026-05-30) | burtonageo + mdsteele | **Revived in 2025-26** (was stale); deb/msi/osx/rpm/appimage but no signing, no updater — simple cases only |
| [dist (cargo-dist)](https://github.com/axodotdev/cargo-dist) | 0.32.0 (2026-05-22) | axodotdev — effectively 1 maintainer + dependabot; 334 open issues | Built for CLI-shaped artifacts (tarballs, installers, Homebrew, MSI); no .app/dmg/AppImage/notarization — wrong tool for GUI. Rumors of axo winding down are **unverified** (axo.dev live; blog DNS dead) |
| [Velopack](https://github.com/velopack/velopack) | velopack 1.2.0 (2026-06-03) | Velopack org | Framework-independent installer+updater with an official Rust crate, delta updates, channels, and Win/mac/Linux support; 1.0 in 2026; adoption still tiny |

**Code signing is the weakest link.** [apple-codesign/rcodesign](https://github.com/indygreg/apple-platform-rs) is the principal open-source Rust-native cross-platform implementation found; it has concentrated maintenance and no crates.io release since 0.29.0 (Nov 2024), although repository commits continued. Tauri and cargo-packager can drive configured signing/notarization workflows, while platform CI often uses Apple's own tools. On Windows, Azure **Artifact Signing** is another current path ([Microsoft FAQ](https://learn.microsoft.com/en-us/azure/artifact-signing/faq)).

**Linux formats:** AppImage is first-class in all three bundlers; Flatpak is community-glue only ([flatpak-cargo-generator](https://github.com/flatpak/flatpak-builder-tools) for offline Cargo vendoring; Tauri's dedicated Flatpak guide currently 404s); Snap has a snapcraft Rust plugin and a [Tauri guide](https://v2.tauri.app/distribute/snapcraft/) but no Rust-native emitter.

## 2. The duplication matrix

Cell values: shared crate name / "in-house" / "webview" / "missing". "dioxus" = the default webview desktop target; blitz (dioxus-native) noted where it differs. As of July 2026.

| Capability | iced | egui | gpui | tauri | xilem | slint | dioxus |
|---|---|---|---|---|---|---|---|
| **Windowing** | winit | winit | in-house (gpui_macos/windows/linux) | tao (winit fork) | winit | winit (+Qt, LinuxKMS backends) | tao; migration to winit proposed in open [#2706](https://github.com/DioxusLabs/dioxus/issues/2706) |
| **GPU abstraction / renderer backend** | wgpu | wgpu (default; glow opt-in) | Metal (macOS) / D3D (Win) in-house; **wgpu (Linux)** | webview | wgpu (via vello) | FemtoVG on OpenGL or wgpu; Skia; Qt; software | webview (blitz: wgpu) |
| **2D renderer** | in-house iced_wgpu + tiny-skia | in-house epaint | in-house primitive shaders | webview | vello | femtovg / Skia / in-house software | webview (blitz: vello) |
| **Text shaping** | harfrust (via cosmic-text) | harfrust (in epaint) | platform (CoreText/DirectWrite); harfrust via cosmic-text on Linux | webview | harfrust (via parley) | harfrust (via parley) | webview (blitz: harfrust via parley) |
| **Text layout** | cosmic-text | in-house (galley) | in-house (line_layout) | webview | parley | parley | webview (blitz: parley) |
| **Font loading/fallback** | fontdb + cosmic-text | in-house (bundled fonts) | platform + font-kit fork; fontdb (Linux) | webview | fontique | fontique | webview (blitz: fontique) |
| **Widget layout** | in-house (flex) | in-house (immediate) | **taffy** | webview (CSS) | in-house (masonry box-constraints) | in-house DSL (+ experimental taffy) | webview CSS (blitz: taffy + Stylo) |
| **Widgets** | in-house | in-house | low-level elements only (Zed `ui` is GPL/unpublished; high-level controls are app-built or third-party, e.g. gpui-component) | webview (HTML/JS) | in-house (masonry) | in-house (DSL) | webview (HTML/JS) |
| **Accessibility** | **missing** semantic tree (draft [PR #3111](https://github.com/iced-rs/iced/pull/3111)) | AccessKit schema required; native adapter enabled by default through eframe | AccessKit merged, but Zed currently opts out | browser-derived tree | AccessKit | AccessKit default on Winit desktop; Qt/custom not established | browser-derived tree (blitz: AccessKit) |
| **IME** | winit Ime (since 0.14) | winit Ime | in-house platform (NSTextInputClient / zwp_text_input_v3; Windows rough) | webview | winit Ime | winit Ime + own Android bridge | webview |
| **Styling/theming** | in-house | in-house | in-house (Tailwind-like) | CSS | in-house | in-house DSL | CSS (blitz: Stylo) |

### Duplication count

Counting the primary paths represented by the seven tested macOS applications
unless a row explicitly lists wider supported alternatives (webview =
externalized, not counted as an in-ecosystem implementation). Slint's Qt,
LinuxKMS, Skia, and software alternatives remain visible in the matrix but are
not silently added to the primary-path windowing/GPU figures:

| Capability | Independent impls | Trend |
|---|---|---|
| Text shaping | **1** (harfrust) + gpui's platform path | **Consolidated 2025-26** (was 4 in 2024) |
| Accessibility abstraction | **1** (AccessKit) | In the tested released paths, egui, Slint/Winit and Xilem integrate it; Iced and released GPUI 0.2.2 do not, GPUI main has an unreleased integration, and Floem outside the seven-framework sample is another gap |
| Windowing | **3 primary families** (winit, tao, gpui) | This primary-path count excludes Slint's supported Qt/LinuxKMS/custom alternatives; Dioxus has proposed, but not shipped, a Tao-to-winit migration |
| GPU abstraction/backend in the tested native paths | **3** (wgpu, GPUI Metal, Slint FemtoVG/OpenGL) | The wider supported matrix additionally includes GPUI D3D and Slint's wgpu, Skia, Qt, and software paths; Zed's Linux switch and eframe's default show wgpu convergence without universality |
| Font loading/fallback | **4** (fontique, fontdb, gpui platform, egui bundled) | Converging on fontique (slint, bevy, floem migrated) |
| Text layout | **4** (cosmic-text, parley, epaint galley, gpui) | Slowly converging on parley (slint, floem, bevy in 12 months) |
| Framework-side renderer | **at least 6 families**, counting Vello, epaint, Iced, FemtoVG, Skia, and GPUI while treating software fallbacks as paths rather than extra families | Diverging/stable; targets span wgpu, platform GPU APIs, OpenGL/Skia, and software |
| Widget layout | **5** (taffy, iced flex, egui, masonry, slint DSL) | taffy growing (gpui, blitz, Servo-grid, slint-experimental) |
| Widgets | multiple framework-owned sets | No useful hard count without deciding whether browser widgets, Zed UI, and community GPUI libraries count; this layer is framework identity |
| Styling/theming | **6** | No consolidation attempted |
| IME plumbing | **3** (winit, gpui, tao/webview) | Follows windowing |

**Where consolidation is most tractable, by adoption trend:** (1) accessibility — Iced and Floem still lack integrations, while GPUI's merged path remains unreleased and disabled in Zed; (2) font loading — exploring fontique interoperability with fontdb/cosmic-text is a bounded direction, not a pre-decided retirement; (3) widget layout via taffy — already crossed the framework boundary (gpui, blitz, Servo, slint-experimental); (4) glyph rasterization — egui's adoption of skrifa+vello_cpu shows the "share the bricks" path even where whole-stack swaps fail. Widgets/styling are intrinsically framework identity and not consolidation targets.

## 3. Long-tail frameworks

**makepad** ([makepad/makepad](https://github.com/makepad/makepad), 6.5k stars, daily commits; makepad-widgets [1.0.0, May 2025](https://crates.io/crates/makepad-widgets)) — the maximal not-invented-here framework: its own windowing layer (wasm/WebGL, Metal, DX11, OpenGL), own shader-based renderer, own font stack, live-editable DSL; no winit/wgpu/taffy/AccessKit anywhere. Repositioned in 2026 as an "AI-accelerated application development environment" (UI runtime + Studio + AI automation). The [Project Robius](https://github.com/project-robius) org builds a multi-platform app framework on top of it; flagship is the [Robrix](https://github.com/project-robius/robrix) Matrix client.

**freya** ([marc2332/freya](https://github.com/marc2332/freya), 2.8k stars; 0.4.0-rc train active) — formerly "Dioxus + Skia"; **as of the 0.4 rewrite (["A new begining", PR #1351](https://github.com/marc2332/freya/pull/1351), Dec 2025) it dropped Dioxus entirely** for its own reactive/component model. Stack: winit 0.30 + Skia (pinned skia-safe fork) + its own torin layout + AccessKit 0.24. Niche: batteries-included, web-like styling, headless testing. Single lead maintainer (marc2332).

**floem** ([lapce/floem](https://github.com/lapce/floem), 4.2k stars) — fine-grained-reactivity UI built for the [Lapce](https://github.com/lapce/lapce) editor (which pins it by git rev; last crates.io release Nov 2024). A heavy shared-crate consumer: forked winit, taffy 0.9 (grid), muda menus, four renderer backends (vger default, vello, skia, tiny-skia), and it **switched cosmic-text → parley in Mar 2026** ([PR #1034](https://github.com/lapce/floem/pull/1034)). Notable gap: **no AccessKit**.

**fltk-rs** ([fltk-rs/fltk-rs](https://github.com/fltk-rs/fltk-rs), fltk 1.5.23, May 2026, 1.4M downloads) — bindings to C++ FLTK by MoAlyousef; everything (windowing, rendering, text, layout) comes from FLTK. Niche: tiny static binaries, instant compiles, old/odd platforms, mature widgets — the "boring but works" option; a11y minimal (FLTK limitation). Best-known user: Weylus; showcase in [#418](https://github.com/fltk-rs/fltk-rs/issues/418).

**relm4 + gtk4-rs** ([relm4 0.11.0](https://crates.io/crates/relm4), Apr 2026; [gtk4 0.11.4](https://crates.io/crates/gtk4), Jun 2026) — Elm-style components over the gtk-rs bindings, with libadwaita and a Flatpak template. GTK supplies windowing/GSK rendering/Pango text/layout and built-in AT-SPI plumbing (plus, since GTK 4.18, optional AccessKit on Windows/macOS), reducing adapter work without removing the need for correct widget semantics and assistive-technology testing. The path of least resistance for GNOME-ecosystem apps; not truly cross-platform in look-and-feel.

**cxx-qt** ([KDAB/cxx-qt](https://github.com/KDAB/cxx-qt), 0.9.1, Jul 2026) — not a widget toolkit but safe Qt interop built on `cxx`: define QObjects in Rust, consume from QML/C++. Qt supplies everything including accessibility. Structurally backed by KDAB (the major Qt consultancy). Niche: teams with existing Qt products/licenses wanting Rust business logic.

**Ribir** ([RibirX/Ribir](https://github.com/RibirX/Ribir), 1.7k stars) — a "non-intrusive" declarative framework using winit 0.30 + wgpu 29. Its current text crate uses a Parley 0.8 backend rather than a wholly in-house Fontations layout stack ([text source](https://github.com/RibirX/Ribir/blob/master/text/src/lib.rs)); no AccessKit integration was found. The last audited commit/release was 2026-04-21 after a weekly alpha train, but an approximately eleven-week quiet period is not enough evidence to label the project stalled; the last stable release was 0.3.0 (Aug 2024).

**vizia** ([vizia/vizia](https://github.com/vizia/vizia), 2.2k stars; [0.4.0, Apr 2026](https://crates.io/crates/vizia)) — declarative framework with its own **morphorm** layout, Skia rendering (skia-safe + SkParagraph; the old femtovg renderer is gone), AccessKit integrated, and dual windowing: winit *or* baseview. The baseview backend is its niche: audio-plugin UIs — [nih-plug](https://github.com/robbert-vdh/nih-plug) ships `nih_plug_vizia` as an official GUI adapter.

*(License note: Slint's tri-license is unchanged in 2026 — GPLv3, royalty-free for proprietary desktop/mobile/web, paid commercial for embedded — [slint.dev/pricing](https://slint.dev/pricing).)*

## 4. Cross-cutting findings for the initiative

### 4.1 Where the ecosystem is converging

1. **wgpu is the strongest GPU-abstraction convergence point in this sample, not a universal winner.** It had ~1,273 reverse dependencies in the dated snapshot and a Mozilla/Firefox anchor. Zed replaced Blade with wgpu **on Linux** in Feb 2026 ([PR #46758](https://github.com/zed-industries/zed/pull/46758)), while GPUI retained Metal on macOS and D3D on Windows; eframe flipped its default from glow to wgpu ([egui #5889](https://github.com/emilk/egui/issues/5889)).
2. **harfrust unified the three principal reusable pure-Rust shaping stacks in under a year** — cosmic-text, parley, and epaint all shape with it now; it exists explicitly for consolidation ([README](https://github.com/harfbuzz/harfrust)). GPUI's macOS/Windows paths and browser/webview stacks still use platform engines.
3. **AccessKit is the de-facto shared a11y layer** — egui (schema required; native adapter enabled by default through eframe), Slint's Winit desktop path, masonry/xilem, Vizia (optional but default-enabled), Blitz, Bevy, GTK 4.18, and GPUI main (May 2026). Iced and Floem remain unintegrated; GPUI's merge has not yet translated into a release or an enabled Zed product path.
4. **taffy crossed framework lines** — DioxusLabs-hosted but maintained cross-team (nicoburns + Bevy contributors); used by gpui, blitz, floem, bevy_ui, Servo (grid), and now experimentally slint.
5. **parley/fontique are absorbing the text layer** — slint (1.14), floem (Mar 2026), and bevy (0.19) all migrated within 12 months; Linebender: "It seems like recognition that Parley is a viable text layout library for a broad range of applications" ([Q1 2026](https://linebender.org/blog/tmil-25/)).

### 4.2 Where it is diverging

1. **Text layout is still 4 independent stacks** (cosmic-text, parley, epaint, gpui). Parley's adopters in and around the sample include Xilem/Masonry, Slint, Blitz, current Floem and Bevy; it is not literally "everyone else." No merger with cosmic-text is documented, and both remain active.
2. **The tao fork is widening, not closing.** tao froze on winit's pre-0.30 closure API while winit redesigns for 0.31; the promised un-fork ([tao #509](https://github.com/tauri-apps/tao/issues/509), [wry #1014](https://github.com/tauri-apps/wry/discussions/1014)) never shipped. GTK-for-WebKitGTK remains the hard constraint.
3. **The framework-side renderer layer has at least six maintained families in this sample**, targeting a mix of wgpu, platform GPU APIs, OpenGL/Skia, and software. Classic Vello is alpha; Vello Hybrid alone has been described as roughly beta.
4. **Linux shell integration is split along the GTK fault line**: Muda's Linux menubar path requires a GTK window, tray-icon's official winit example adds a GTK event-loop thread, and global-hotkey is X11-only. This is not a claim that every tauri-apps crate assumes GTK or that all shell features are impossible in winit applications.
5. **winit's redesign remained in beta for nearly eight months at the snapshot.** A prolific contributor's public burnout hiatus is a continuity signal at a widely depended-on GUI layer, not proof of project-wide maintainer burnout or a claim that winit exceeds wgpu's dated reverse-dependency count.

### 4.3 Highest-leverage consolidation opportunities

1. **Fund and staff Iced accessibility work** — Iced is one open cell in the seven-framework map, while Floem remains another and GPUI still needs product-readiness work. Draft [PR #3111](https://github.com/iced-rs/iced/pull/3111) demonstrates a path, but completing it also requires widget semantics, adapter QA, maintenance, and upstream acceptance; it is not credibly a one-patch promise.
2. **Integrate existing non-GTK Linux protocols into the common facades** — [`ksni`](https://docs.rs/ksni/latest/ksni/) implements StatusNotifierItem, [`ashpd`](https://docs.rs/ashpd/latest/ashpd/desktop/index.html) exposes the GlobalShortcuts portal, and Rust DBusMenu implementations exist. The gap found here is maintained integration into `tray-icon`, `global-hotkey`, and menu APIs used by ordinary winit applications, with fallback/platform policy made explicit.
3. **Explore fontique interoperability for cosmic-text** — fontdb's public release cadence has slowed, while fontique adoption is growing. A migration or adapter could reduce duplicate discovery/fallback code, but calling fontdb "dying" or prescribing replacement without maintainer agreement would overstate this audit.
4. **Underwrite AccessKit and apple-codesign maintenance** — recent authorship is concentrated in both projects. The 2023–24 GNOME/STF work funded related accessibility work and no later direct AccessKit institutional grant was found; `rcodesign` has a 19-month release gap but recent commits. These observable signals make support worth evaluating without inferring private employment or project-collapse probability.
5. **Improve a framework-neutral packaging composition** — Tauri has the most integrated first-party chain. Non-Tauri apps can compose maintained pieces such as cargo-packager, Velopack, platform tools, and the principal Rust-native cross-platform `rcodesign` path, but the sample did not find one equally integrated bundle/sign/notarize/update workflow.

### 4.4 Honest blockers: why frameworks re-implement

- **Performance and control (gpui):** Zed's founding blog ("[Leveraging Rust and the GPU to render user interfaces at 120 FPS](https://zed.dev/blog/videogame)") — with existing stacks "there was always something in the way of delivering frames on time"; they rasterize the window like a videogame with a shader per primitive. On choosing blade over wgpu, a Zed engineer: "our renderer is simple enough that we would have preferred to use Vulkan APIs directly… Blade is a thinner abstraction" ([HN](https://news.ycombinator.com/item?id=40288507)). Note both decisions partially reversed by 2026 (wgpu adopted on Linux; taffy and AccessKit and cosmic-text adopted) — control-driven duplication erodes once shared crates mature.
- **API-model mismatch (egui):** immediate mode wants incremental galley caching and a glyph atlas; "Parley… *really* wants to be given a full rectangle that it can arrange text inside… and egui really doesn't work with that model" ([egui #5784](https://github.com/emilk/egui/pull/5784)). Also compile-time/binary-size discipline: "ab_glyph is very minimal, leading to fast compiles and small binaries (important for .wasm bundle size)" — emilk ([#3378](https://github.com/emilk/egui/issues/3378)).
- **Adoption timing (iced):** Iced adopted cosmic-text in 2023; [iced #1697](https://github.com/iced-rs/iced/pull/1697) calls cosmic-text "a long-time-missing piece in the Rust GUI ecosystem." That source proves the adoption and praise for cosmic-text, but does **not** document a comparative decision that Parley was not ready.
- **Platform constraints (tao):** "running webkitgtk outside of gtk is not fun or not even possible" (FabianLars, [tao #509](https://github.com/tauri-apps/tao/issues/509)) — the fork exists because of WebKitGTK, and un-forking is blocked on wry becoming windowing-agnostic, not on goodwill.
- **Business span (slint):** four principal renderer paths — FemtoVG, Skia, Qt, and software — serve targets from GPU-less MCUs (line-by-line software rendering) to desktop ([renderer docs](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers/)). Duplication here is market-driven, not accidental.
- **Funding and continuity are uneven:** Raph Levien left Google in Oct 2025 and joined Canva, which contributes to Vello work; the 2023–24 GNOME/STF accessibility project was mostly wrapped up; notgull publicly described burnout; and RazrFalcon handed tiny-skia/resvg to Linebender. These are distinct employment, grant, and stewardship events—not proof that all support ended—but they make contributor continuity worth monitoring.
- **Coordination exists but is informal:** at RustWeek 2025's UI summit, "various project leaders were increasingly interested in pooling resources and using common libraries, but there is still a long way to go on that front" ([Linebender TMIL-17](https://linebender.org/blog/tmil-17/)); RustWeek 2026 ran the summit again. This audit found no formal cross-framework working group in the sources it reviewed; that is an absence-of-evidence result, not proof that no coordination venue exists.

## 5. Empirical measurements

One identical app (`apps/SPEC.md`: todo list with text input, Add button, Enter
shortcut, per-row delete, live counter, scrolling) was built in all seven
frameworks and measured serially on the same machine (Apple M4 Pro, macOS
26.5.2, rustc 1.96.1). Full methodology, caveats, and analysis:
[10-empirical-results.md](10-empirical-results.md); raw data in
`measurements/` and per-app `deps-flat.txt`.

<!-- BEGIN GENERATED: iter1-ecosystem -->
| App (version) | Clean build | Incremental | Binary (stripped MiB) | Unique crate names | AccessKit in tree | Recorded source LoC¹ | SPEC-1 result |
|---|---:|---:|---:|---:|---|---:|---|
| iced 0.14.0 | 22 s | 2 s | 8.5 | 140 | no | 74 | source-complete |
| egui 0.35.0 | 27 s | 1 s | 10.5 | 156 | **yes** | 168 | source-complete |
| xilem 0.4.0 | 28 s | 1 s | 9.7 | 143 | **yes** | 81 | source-complete |
| tauri 2.11.5 | 36 s | 10 s | 6.4 | 204 | no (browser-derived a11y) | 208 | source-complete |
| dioxus 0.7.9 | 40 s | 1 s | 4.9 | 279 | no (browser-derived a11y) | 90 | source-complete |
| slint 1.17.1 | 42 s | 4 s | 13.2 | 302 | **yes** | 94 | source-complete |
| gpui 0.2.2 | 56 s | 1 s | 4.3 | 391 | no (merged upstream, unreleased) | 230 | approximated² |
| freya 0.4.0 | 28 s | 1 s | 18.3 | 192 | **yes** | 90 | source-complete |
| vizia 0.4.0 | 22 s | 1 s | 19.6 | 128 | **yes** | 162 | source-complete |
| floem git-778bb5f2 | 42 s | 1 s | 14.2 | 226 | no (not integrated) | 91 | source-complete |
<!-- END GENERATED: iter1-ecosystem -->

¹ Recorded source counts include verification hooks stored in measured source
files and exclude JSON/TOML configuration; they are not production-only. See
the empirical report for the exact counting rules. ² GPUI has low-level core
elements but no first-party high-level widget/text-input set; the input was hand-rolled from raw key events
(no IME/selection). The remaining features are present in source; all processes
survived the scripted check and a later audit observed their windows. Retained
interaction evidence varies by app as described in the empirical report.

Measured takeaways that sharpen the sections above:

- **For these seven small apps on this M4 Pro**, every framework clean-built in
  under a minute from warm registries and empty target directories, then rebuilt
  in 1–4 s (Tauri: 10 s in this build-script path). This does not establish a
  general compile-time ranking for larger applications.
- **The duplication matrix shows up in the lockfiles**: `raw-window-handle` is
  the only universal **cross-platform GUI-interoperability abstraction** (the
  interop keystone, §1.1); other universal names include platform-specific GUI
  crates such as `objc2-app-kit` and Core Graphics. HarfRust reaches 4/7 trees
  via three different text stacks (§1.3);
  AccessKit appears in exactly the 3 trees §1.5 predicts (egui, slint, xilem);
  winit 4/7 vs tao 2/7 vs gpui in-house matches §1.1.
- **Version skew is visible in this sample**: of the 29 crate names shared by
  all seven apps, 13 resolve to different major/minor versions across the
  selected trees. The exact generated set is published by
  `python3 scripts/overlap.py --round iter1`; `hashbrown` has five versions,
  while raw-window-handle still splits 0.5/0.6 because of Tao compatibility.
- **Pairwise dependency overlap** peaks inside the winit+wgpu family
  (egui↔xilem 45.9%, iced↔xilem 43.7%) and bottoms out across paradigms
  (iced↔tauri 13.9%). The graph suggests native-GPU and webview groupings, but
  GPUI is not an isolated cluster: it overlaps Dioxus and Slint by roughly
  38–39%. No formal clustering analysis was performed.

## Appendix: verification caveats

Items flagged as unverified or medium-confidence during research; treat with care and re-verify before quoting externally:

- Matt Campbell's (AccessKit) current employment/affiliation — no evidence found either way; project demonstrably active regardless.
- "axo/cargo-dist shut down / handed to Astral" — **rumor, not confirmed**; axo.dev is live, blog DNS is dead, maintenance is effectively one person.
- Shipped Zed currently opts out of the merged GPUI AccessKit path after integration failures; the date at which that product-level opt-out will be removed is unknown.
- iced's explicit "why not parley" rationale — inferred from timeline; no maintainer statement found.
- libcosmic enables `a11y` (iced/a11y + iced_accessibility) in default features — source-verified; real screen-reader completeness unverified.
- Slint's pre-1.14 Skia text path (SkParagraph) — from prior knowledge/blog inference, not re-verified.
- ~~NLnet grants for 2026 parley work~~ — **since confirmed**: two active NGI0 Commons Fund grants, [nlnet.nl/project/Parley](https://nlnet.nl/project/Parley/) (started Oct 2025, deadline 2026-08-01) and [nlnet.nl/project/Parley-copypaste](https://nlnet.nl/project/Parley-copypaste/).
