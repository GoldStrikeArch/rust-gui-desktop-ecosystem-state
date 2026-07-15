# 07 — Dioxus (desktop) & Blitz

*RCN "Cross-Platform GUI Desktop Apps" deep dive. Researched 2026-07-07 on an
Apple M4 Pro, macOS, rustc 1.96.1. Version tested: **dioxus 0.7.9** (latest
stable on crates.io, published 2026-05-08; `0.8.0-alpha.0` exists as of
2026-05-19 — see [crates.io/crates/dioxus/versions](https://crates.io/crates/dioxus/versions)).*

---

## 1. Architecture & paradigm

Dioxus is a **React-like, renderer-agnostic** UI framework: components are
Rust functions returning `Element`, UI is declared with the **RSX macro**
(JSX-like syntax that compiles to Rust), state is **hooks + Copy signals**
(`use_signal`, rewritten in 0.5 to be `Copy` and lifetime-free —
[0.5 release post](https://dioxuslabs.com/blog/release-050)). A **VirtualDOM**
(`dioxus-core`) diffs component output into mutation batches that any renderer
can consume ([docs.rs/dioxus-core](https://docs.rs/dioxus-core)).

That renderer-agnostic core is the whole pitch: one codebase targets web
(WASM/DOM via `dioxus-web`), desktop and mobile (webview via
`dioxus-desktop`), server (SSR/LiveView/fullstack via `dioxus-server` +
Axum), and an experimental native GPU renderer (`dioxus-native`/Blitz). The
crate's feature list makes the split explicit: `web`, `desktop`, `mobile`,
`native`, `fullstack`, `server`, `liveview`, `ssr` are all sibling features of
the same `dioxus` crate ([crates.io feature data for
0.7.9](https://crates.io/crates/dioxus/0.7.9)). **Freya 0.3.4** is a historical
example of a third-party Skia renderer built on Dioxus, but the active 0.4
rewrite dropped Dioxus for its own reactive/component model
([rewrite PR #1351](https://github.com/marc2332/freya/pull/1351)). It should
not be presented as evidence that current Freya still consumes the
`dioxus-core` VirtualDOM.

Dioxus 0.7 (announcement post dated Sep 8 2025 —
[dioxuslabs.com/blog/release-070](https://dioxuslabs.com/blog/release-070/);
0.7.0 stable hit crates.io 2025-10-31) headlined **subsecond Rust
hot-patching**, the **native (Blitz) renderer**, WASM bundle splitting, a
fullstack overhaul around Axum (WebSockets, SSE, typed forms), and a
Radix-style component library.

## 2. Desktop rendering TODAY: a webview, same as Tauri

`dioxus-desktop` 0.7.9 is described by its own crates.io metadata as "WebView
renderer for Dioxus" ([crates.io/crates/dioxus-desktop](https://crates.io/crates/dioxus-desktop)).
It renders into the **system webview via `wry`** with windowing from **`tao`**
— the exact same two crates Tauri is built on, and both are maintained by the
**tauri-apps** org ([github.com/tauri-apps/wry](https://github.com/tauri-apps/wry),
[github.com/tauri-apps/tao](https://github.com/tauri-apps/tao); both actively
pushed as of July 2026). The proposed Tao→winit migration remains open
([dioxus#2706](https://github.com/DioxusLabs/dioxus/issues/2706)); Dioxus 0.7.9
still ships Tao/Wry. Version check from the tested lockfiles: Dioxus resolves
**tao 0.34.8 / wry 0.53.5**; current Tauri (`tauri-runtime-wry` 2.11.4)
requires **tao ^0.35 / wry ^0.55**
([crates.io dependencies](https://crates.io/crates/tauri-runtime-wry)) — same
crate families, but not the same versions. The tested Dioxus graph has
**muda 0.17.2 / tray-icon 0.21.3**, while the tested Tauri tray graph has
**muda 0.19.3 / tray-icon 0.24.1**. On macOS that means WKWebView; on Windows
WebView2; on Linux WebKitGTK — with all the per-platform rendering
inconsistency that implies, identical to Tauri's.

**Dioxus vs Tauri, directly:**

- *What Dioxus adds over Tauri:* the UI itself is written in Rust. There is
  no JS framework and no user-authored Tauri-style serialized `invoke`
  boundary for normal handlers: event handlers are Rust closures over Rust
  state (signals). This does **not** remove the process/runtime bridge: DOM
  events and VirtualDOM mutation batches still cross between Rust and the
  webview through `dioxus-interpreter-js`. One authored language and one Cargo
  build can retarget web/mobile/fullstack.
- *What Tauri has that Dioxus lacks:* a security/permissions model
  (capabilities, CSP, scoped APIs), a large official **plugin ecosystem**
  (updater, deep-link, autostart, notifications, SQL, etc. —
  [tauri.app/plugin](https://tauri.app/plugin/)), an integrated first-party
  runtime updater client, sidecar/runtime integration, and a much larger
  production track record. Dioxus's CLI does have deep-link configuration,
  platform signing, macOS notarization/stapling and updater-archive generation;
  these should not be described as wholly absent. Tauri is frontend-framework-
  agnostic; Dioxus is the framework. Some tauri-apps crates (muda, tray-icon,
  global-hotkey, rfd) are consumed by Dioxus directly (visible in this app's
  `deps-flat.txt`), but Tauri *plugins* proper target Tauri's runtime and are
  not drop-in for Dioxus.
- *Shared-foundation note for the fragmentation map:* Dioxus desktop is
  downstream of Tauri's windowing/webview/menu crates. A regression or
  direction change in tauri-apps land lands on both frameworks.

## 3. Blitz — the strategic piece

[Blitz](https://github.com/DioxusLabs/blitz) is DioxusLabs' webview-free
HTML/CSS engine: "a radically modular HTML/CSS rendering engine" for native
apps without browser bloat (no JS engine, no WebRTC/WebSockets/localStorage —
those are left to ordinary Rust crates). Verified current architecture (repo
README, July 2026):

| Crate | Role | Built on |
|---|---|---|
| `blitz-dom` | DOM, styling, layout, events | **Stylo** (Servo/Firefox CSS engine), **Taffy** (layout), **Parley** (text/Linebender) |
| `blitz-html` | HTML parsing | html5ever/xml5ever (Servo) |
| `blitz-shell` | windowing, events, a11y | **Winit**, **AccessKit**; current `blitz-shell` has no Muda dependency |
| `blitz-paint` | draw commands | **AnyRender** abstraction with Vello-family backends rather than one permanently fixed renderer |
| `blitz-net` | resource fetching | reqwest |
| `dioxus-native` | Dioxus VirtualDOM → Blitz, with interactivity | blitz-dom + dioxus-core |

So the earlier "stylo + taffy + vello + parley + accesskit" description is
still broadly accurate, with two version-sensitive refinements: stable
`dioxus-native` 0.7.9 uses the classic AnyRender/Vello path, while current git
defaults Dioxus Native to **Vello Hybrid**; and current `blitz-shell` does not
pull in Muda. Painting goes through the **AnyRender** abstraction rather than
hard-wiring one Vello implementation. This is one of the most
composition-heavy shared-foundation projects in this sample: it combines Servo's
CSS engine, Linebender's text and painting stack, AccessKit, Winit, and
DioxusLabs' own Taffy instead of re-implementing any of them.

**Production-readiness (verified July 2026): pre-alpha.** The README states:
*"Blitz is currently in a pre-alpha state … we would not yet recommend
building apps with it."* It already supports modern layout (flexbox, grid,
table, block, inline, absolute/fixed), advanced CSS (complex selectors, media
queries, CSS variables), form controls, and AccessKit integration. There are
no GitHub releases ([github.com/DioxusLabs/blitz/releases](https://github.com/DioxusLabs/blitz/releases)
says "There aren't any releases here"), but crates are published:
`blitz-dom` 0.2.4 stable with **0.3.0-alpha.6 published 2026-06-23**
([crates.io/crates/blitz-dom](https://crates.io/crates/blitz-dom)) — active
development. Public release/repository activity establishes activity;
post-acquisition funding was not independently sourced.

**Is dioxus-native shipping?** Yes, in the "installable but experimental"
sense: `dioxus-native` is published on crates.io at **0.7.9** in lockstep
with Dioxus ([crates.io/crates/dioxus-native](https://crates.io/crates/dioxus-native)),
and stable `dioxus 0.7` exposes it as the `native` feature flag. The Dioxus
README labels the WGPU renderer **"Experimental"**; that source does not by
itself establish a formal "Stable" designation for every other target
([DioxusLabs/dioxus README](https://github.com/DioxusLabs/dioxus)). Per this
project's rules the mini-app was built on the webview renderer, not Blitz.

## 4. Taffy — a de-fragmentation success story

**Verified: DioxusLabs maintains Taffy**
([github.com/DioxusLabs/taffy](https://github.com/DioxusLabs/taffy)), current
release **0.12.1 (2026-07-03)**, MIT license, **8.9M downloads**
([crates.io/crates/taffy](https://crates.io/crates/taffy)). It implements CSS
**block, flexbox, and grid** algorithms. The README's verified user list is
remarkable for a fragmented ecosystem: **Servo**, **Blitz**, **Bevy** (game
engine UI), **Zed** (via **GPUI**), **Lapce** (via **Floem**), **Slint**,
iocraft, Takumi. One layout engine shared across a browser engine, a game
engine, two editors, and three GUI toolkits — the clearest existing proof
that Rust GUI frameworks *can* converge on shared infrastructure. Primary
author/maintainer is Nico Burns, also the lead on Blitz (see contributor
concentration in §9).

## 5. Tooling: dx CLI, hot reload, bundling

- **`dx` CLI** (`dioxus-cli` 0.7.9 on crates.io): project scaffolding,
  `dx serve` (dev server for all targets), `dx bundle` (.app/.dmg/.msi/.deb +
  web bundles), mobile simulators/device deploy, Tailwind auto-detection.
  Since 0.7 it installs via one-liner and works on "any Rust project," not
  just Dioxus ([0.7 post](https://dioxuslabs.com/blog/release-070/)).
- **Hot reload / hot-patching:** the implementation ships, but Dioxus's
  [official guide](https://dioxuslabs.com/learn/0.7/essentials/ui/hotreload)
  labels hotpatching **experimental**. The
  [`subsecond`](https://crates.io/crates/subsecond) crate (0.7.9, "a runtime
  hotpatching engine for Rust hot-reloading") powers `dx serve --hotpatch`,
  which is intended to patch compiled Rust code at runtime while
  preserving app state, across web/desktop/mobile
  ([README](https://github.com/DioxusLabs/dioxus),
  [0.7 post](https://dioxuslabs.com/blog/release-070/)). RSX/asset
  hot-reload without recompiling also goes through `dx serve`.
- **What needs dx vs plain cargo (tested here):** a desktop app with no
  assets builds and runs with **plain `cargo build/run --release`** — the
  measured mini-app never touched dx. You need dx for: hot reload/patching,
  the `asset!()`/manganis asset pipeline, installer bundling, serving web
  builds, and mobile deploys. Tauri's normal project workflow uses its CLI and
  often a Node-based frontend, but that is not a hard toolchain distinction:
  this study's static Tauri app also built with plain Cargo and no Node
  toolchain.

## 6. Accessibility

- **Today (webview desktop):** the UI is real HTML in WKWebView/WebView2/
  WebKitGTK, giving a mature browser-derived baseline for accessibility trees,
  screen readers, keyboard navigation and IME — the same starting point as
  Tauri/Electron. This is not automatic app accessibility: semantic HTML/ARIA
  in RSX remains the developer's responsibility and must be tested with real
  assistive technology.
- **Future (Blitz):** `blitz-shell` integrates **AccessKit**
  ([Blitz README](https://github.com/DioxusLabs/blitz)) — the same
  cross-platform a11y-tree crate used by egui, Slint, and Bevy (AccessKit
  itself: 20.6M downloads, [crates.io/crates/accesskit](https://crates.io/crates/accesskit)).
  Text input/IME on Blitz rides on Parley's text editing + Winit IME events
  and is still maturing along with the rest of pre-alpha Blitz.

## 7. OS shell integration

Verified against [docs.rs/dioxus-desktop/0.7.9](https://docs.rs/dioxus-desktop/0.7.9/dioxus_desktop/)
and this app's actual dependency graph (`deps-flat.txt`):

- **Built-in (bundled by default):** menus via **muda** (`use_muda_event_handler`),
  **system tray** via tray-icon (`use_tray_icon_event_handler`,
  `use_tray_menu_event_handler`), **global shortcuts** via global-hotkey
  (`use_global_shortcut`), file dialogs via **rfd**, multi-window (supported
  since 0.3 — [0.3 post](https://dioxuslabs.com/blog/release-030); `use_window`,
  `WindowBuilder`, per-window configs), fullscreen/transparency, custom asset
  protocols (`use_asset_handler`), raw tao/wry escape hatches
  (`use_wry_event_handler`).
- **Iteration-3 result:** tray, global shortcut, native menubar, text clipboard,
  live dark mode, multi-window and close-to-tray were exercised through
  Dioxus/webview APIs and classified **built-in**. Native file-drop paths are
  also wired through Wry/Dioxus, but were source/code-path verified rather than
  exercised with a real Finder drag. Native dialogs were classified
  **assembled** because the app directly added `rfd` even though
  dioxus-desktop already carries it internally; image clipboard (`arboard`)
  and notifications (`notify-rust`) were also assembled from helper crates.
- **CLI distribution capabilities:** Dioxus 0.7.9's manifest exposes deep-link
  configuration and Apple/Windows/Android signing settings; its macOS bundler
  performs signing, notarization and stapling, and its updater module generates
  update archives
  ([manifest](https://github.com/DioxusLabs/dioxus/blob/v0.7.9/packages/cli/src/config/manifest.rs),
  [macOS bundler](https://github.com/DioxusLabs/dioxus/blob/v0.7.9/packages/cli/src/bundler/macos.rs),
  [Windows bundler](https://github.com/DioxusLabs/dioxus/blob/v0.7.9/packages/cli/src/bundler/windows.rs),
  [updater archive](https://github.com/DioxusLabs/dioxus/blob/v0.7.9/packages/cli/src/bundler/updater.rs)).
  What this audit did not find is a first-party **runtime update client**
  equivalent to Tauri's updater plugin. Dioxus also has no Tauri-style
  capability/permission sandbox around what Rust code can reach.

## 8. Platform matrix

Per the [README platform table](https://github.com/DioxusLabs/dioxus) and 0.7
release notes:

| Target | Status in the cited project material | Notes |
|---|---|---|
| macOS / Windows / Linux desktop | Supported/default desktop renderer | wry/tao webview; verified here on macOS |
| Web (WASM/DOM) | Supported, mature target | ~50 kb baseline claim, SSR/hydration, 0.7 bundle-splitting |
| iOS / Android | Supported | `dx` builds .ipa/.apk; 0.6 added simulators, 0.7 added iPad + ADB device hot-reload; younger than desktop/web |
| Fullstack / server functions | Supported | Axum-integrated; WebSockets/SSE/streaming in 0.7 |
| Native GPU (Blitz) | **Experimental** | `native` feature / `dioxus-native` 0.7.9 |

## 9. License, company status, cadence, contributor concentration, production users

- **License:** MIT OR Apache-2.0 (crates.io metadata for dioxus 0.7.9;
  Blitz dual MIT/Apache-2.0 with `stylo_taffy` additionally MPL-2.0 because
  of Stylo).
- **Company status (verified July 2026):** DioxusLabs is a **YC S23** company
  founded by Jonathan Kelley. Its official YC profile now lists the company as
  **Acquired** and gives a team size of four
  ([ycombinator.com/companies/dioxus-labs](https://www.ycombinator.com/companies/dioxus-labs)).
  The acquirer was not publicly identified in the sources reviewed. Historical
  community funding/sponsorship exists, but secondary estimates of roughly
  $500K raised and conclusions such as "independent seed-stage" or "thinly
  funded" are no longer safe current-state facts.
- **Release cadence:** roughly one minor per 9–12 months (0.3 Feb 2023, 0.4
  Aug 2023, 0.5 Mar 2024, 0.6 Dec 2024, 0.7 announced Sep 2025 / stable
  Oct 2025 — [blog index](https://dioxuslabs.com/blog/)), with frequent
  patches (0.7.1→0.7.9 between Nov 2025 and May 2026) and `0.8.0-alpha.0`
  out (May 2026). Historically large gaps between minors, and 0.x semver:
  every minor is a breaking release.
- **Contributor concentration (GitHub contributor API snapshot, July 2026):**
  the API totals used in this audit put jkelleyrtp and ealmloff far ahead of
  Dioxus's third contributor, and nicoburns far ahead of the next contributors
  in Blitz and Taffy. These totals are method-dependent snapshots (merge,
  bot, branch and identity handling can change them); they show historical
  concentration, not current staffing, funding or a literal bus-factor value.
  The repositories were active at collection time.
- **Production users:** the [homepage](https://dioxuslabs.com/) shows
  "Trusted by top companies" logos: **Airbus, ESA, Cognition, Y Combinator,
  Futurewei**. Treat with care: these are marketing logos without case
  studies (YC is the investor; Futurewei was a sponsor). Satellite.im's
  Uplink chat client was an early verified Dioxus app. Independent,
  documented production case studies remain thin compared to Tauri.

## 10. Docs & learning resources

The [0.7 learn site](https://dioxuslabs.com/learn/0.7/) was rebuilt for 0.7:
a sequential "tour" tutorial, core-concept chapters (UI, state, fullstack,
routing), and per-platform guides, plus [docs.rs API
docs](https://docs.rs/dioxus) and a large `examples/` tree in the repo.
Quality is good and improving (docs overhauls were headline items in 0.4 and
0.7), with two caveats: (a) rapid 0.5→0.6→0.7 API churn means much
third-party material (blogs, StackOverflow, LLM output) targets dead APIs;
(b) documentation is dx-CLI-centric, and the plain-cargo path is
under-documented. Discord is the primary support channel. **Rating: 4/5.**

## 11. Friction log (from building the mini-app)

Full details in `apps/dioxus-app/GAPS.md`; measurements: clean release build
**40 s**, forced incremental rebuild (after touch main.rs) **1 s**, binary **5,982,128 bytes raw /
5,108,920 bytes (4.9 MiB) stripped**, **279 unique crate names / 287
name-version entries including the app**, **90 LoC**. A contemporaneous
~98 MiB observation covered the main process only and is not comparable to the
later controlled total-process-tree result (208 MiB for the dashboard app).

1. **Remarkably low friction overall:** the spec app compiled on the first
   attempt and needed no dx CLI, no config files, no assets — one Cargo.toml
   line + one main.rs. Easily one of the smoothest paths in this study.
2. **Web knowledge required:** layout, scrolling, and styling are inline CSS
   strings in RSX. If you don't know flexbox, Dioxus doesn't help you; if
   you do, there's zero widget-catalog learning curve.
3. **Heavy default surface:** a todo app pulls 287 name-version entries,
   including menus, tray, global hotkeys and file dialogs it never uses;
   dioxus-desktop has no features to shed muda/tray-icon/global-hotkey/rfd.
4. **Webview tax:** the controlled dashboard measurement was **208 MiB total
   process-tree RSS**, including WebKit helpers. The earlier ~98 MiB todo-app
   observation was main-process-only and its raw sample was not retained;
   platform-dependent rendering applies (WKWebView here).
5. **Signal ergonomics are genuinely good:** `Copy` signals let one closure
   back both the button `onclick` and the Enter-key handler with no
   `Rc`/`clone!` ceremony — the React model translated to Rust with less
   boilerplate than most Rust GUI state systems.
