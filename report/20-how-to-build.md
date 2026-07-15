# How to build a cross-platform desktop GUI app in Rust (July 2026)

A practitioner's decision guide, grounded in the deep dives
(`01-iced.md` … `07-dioxus.md`), the ecosystem map (`00-ecosystem-map.md`),
and four empirical rounds covering eight specifications per framework (56
applications), beginning with the identical baseline app in
[`10-empirical-results.md`](10-empirical-results.md).

## 0. The one-paragraph answer

You can ship production cross-platform desktop products with Rust GUI stacks
today: Zed, COSMIC, Rerun, GitButler, and Spacedrive are Rust applications,
while LibrePCB is a primarily C++ application using Slint through its C++ API.
Together they demonstrate deployment across several stacks. The practical
choice is between **two broad paradigms**:
render your UI in the OS **webview** (Tauri, Dioxus — HTML/CSS, small Rust
binaries, and browser-derived text/IME/accessibility plumbing, with webview
quirks especially on Linux) or use a **framework-drawn native surface** (iced,
egui, Slint, gpui, xilem). The latter group gives the framework more control
over rendering, but it is not uniformly GPU-only: Iced and Slint have software
paths, Slint also has Qt/Skia backends, and platform fonts and APIs can still
change the result across operating systems. A third path — bindings to mature
non-Rust toolkits (relm4/GTK, cxx-qt/Qt, fltk-rs) — trades "pure Rust" for
toolkit maturity. There is no default answer; the decision factors below
matter more than any ranking.

## 1. Decision tree

**Q1. Is a screen reader / accessibility a hard requirement today?**

- Yes, non-negotiable, shipping this year → start with **Tauri/Dioxus**
  (browser-derived accessibility tree) or **Slint / egui** (AccessKit
  integrated and enabled by default), then test the authored semantics with
  the actual target screen readers. Neither HTML nor AccessKit makes an app
  accessible automatically. Stable **iced** has no semantic widget tree;
  GPUI's AccessKit plumbing is merged but unreleased in 0.2.2 and currently
  disabled by Zed; **Xilem 0.4** does ship its AccessKit integration, but the
  framework remains alpha-quality and was not screen-reader-audited here.

**Q2. Does your team know (and like) HTML/CSS? Is some of your app already web tech?**

- Yes → **Tauri** if the frontend team is JS-first or you need the
  security/plugin/updater machinery; **Dioxus** if you want the whole app in
  Rust with a React-like model (note: same tao/wry stack underneath, no
  capability-security model, fewer plugins).
- No, and you want everything in one Rust process and one language →
  framework-drawn family (Q3).

**Q3. Within the framework-drawn family, what shape is your app?**

- **Standard forms-and-lists product UI, want structure that scales with a team**
  → **iced** (enforced Elm architecture; leanest dep tree at 140 crates;
  fastest clean build at 22 s; but: no stable a11y and no built-in menus/tray,
  although third-party integration works with platform caveats; recent
  upstream authorship is concentrated and the last top-level release gap was
  about 14 months). Production dependency
  choices vary: COSMIC uses a fork, Halloy tracks master through a fork, and
  Sniffnet ships stable Iced 0.14.
- **Tool/editor/debug UI, data-dense, fast iteration valued over pixel polish**
  → **egui** (a simple immediate-mode mental model and notable tooling incl. headless
  AccessKit-driven testing via `egui_kittest`, a11y on by default; but:
  immediate-mode styling limits, no paragraph-level BiDi, and
  bundled-fonts-only fallback by default).
- **Designer-driven product, embedded ambitions, or you want a markup DSL + live preview**
  → **Slint** (completed the same baseline app specification as its peers,
  had one of the broadest first-party native menu/tray surfaces among the
  framework-drawn implementations in this sample, enables AccessKit by default
  on the Winit desktop path, and spans MCU to desktop; but: `.slint` is a real second
  language, the royalty-free license requires attribution and excludes
  embedded use, and Fluent is the default desktop style).
- **Maximum performance/control, macOS-first, team can absorb sharp edges**
  → **gpui** (Zed's engine; smallest stripped binary in our baseline test;
  but: it has low-level elements and a hand-built input example rather than a
  reusable first-party high-level widget/text-input suite, so reuse Zed code,
  implement the control, or adopt
  third-party `gpui-component` (note: Zed's first-party UI crates are
  GPL-3.0-or-later and unpublished — a permissive proprietary path means gpui
  core + your own controls, or a permissively licensed third-party set like
  gpui-component); crates.io releases stalled since Oct 2025, so
  current features require git-pinning the Zed monorepo; a11y is not in a
  release and product-level RTL editing remains incomplete).
- **Research/greenfield, want an architecture-first reactive model, can tolerate alpha**
  → **xilem** (its AccessKit integration includes text semantics via Parley,
  on a shared-crate stack of winit+Vello+Parley; but: no shipping production
  user was documented in this audit, its widget set is still small, and its
  release lags newer versions of its own lower-level stack).

**Q4. Do you need mobile from the same codebase?**

- iOS + Android today → **Tauri**, **Dioxus**, or **Slint**. Slint now
  documents Rust support for both [iOS](https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/ios/)
  and Android. egui/eframe documents Android, but not official iOS support;
  iced/gpui do not offer a supported same-codebase mobile path.

**Q5. Special contexts**

- GNOME/Linux-first app → **relm4/gtk4-rs** (native GNOME look and a
  toolkit-supplied accessibility baseline; still author and test semantics).
- Existing Qt investment → **cxx-qt** (KDAB-backed).
- Audio plugin UIs → **vizia** (baseview backend) or iced_baseview.
- MCU/embedded HMI + desktop companion → **Slint** (the broadest explicitly
  documented MCU-to-desktop option in this seven-framework sample; budget for
  the commercial license on the embedded side).
- Tiny utility where build time and visual polish are secondary → consider
  **fltk-rs** or **egui**, but benchmark the chosen dependency set; this audit
  did not measure FLTK or establish instant compilation.

## 2. What you give up per paradigm (the honest ledger)

| Concern | Webview (Tauri/Dioxus) | Framework-drawn native (iced/egui/slint/gpui/xilem) |
|---|---|---|
| Text quality (shaping, BiDi, emoji, fallback) | Browser-engine baseline; still test fonts, editing, and platform engines | The main hidden cost; principal reusable engines in the sample are Parley and cosmic-text; egui lacks paragraph BiDi and GPUI behavior is platform-dependent |
| IME (CJK input) | Browser-engine path; not live-composition-tested in this audit | Plumbing exists in the sampled stacks, but several paths were source-inspected rather than exercised; GPUI's platform maturity varies |
| Accessibility | Browser-derived tree, conditional on semantic authoring and real assistive-technology testing | AccessKit is a baseline where integrated (egui/slint/xilem; GPUI unreleased; iced missing), not a substitute for widget semantics and AT testing |
| Memory | **208–211 MiB** maximum process-tree RSS in the controlled dashboard run | **79–109 MiB** maximum RSS in the same native-dashboard run |
| Stripped baseline binary | **4.9–6.4 MiB**; normally relies on the OS webview, although Tauri can bundle fixed/offline WebView2 on Windows | **4.3–13.2 MiB**; rendering code/assets are linked, but these are not universally self-contained executables |
| Visual consistency across OSes | Three browser engines (WebKit vs WebView2 vs WebKitGTK) can differ; Linux has additional WebKitGTK/driver constraints | More renderer control, but not guaranteed identical pixels: fonts, platform APIs, and selectable GPU/software/Qt backends differ |
| OS shell integration | Tauri offers a broad integrated plugin set, still subject to each plugin's platform matrix | Slint/GPUI expose first-party menu paths; the others rely more on third-party crates, with Linux integration caveats |
| Security model | Tauri: capability system plus configurable CSP/isolation controls; CSP protects only when configured. Dioxus has no equivalent built-in capability model | In-process; no webview privilege boundary to configure |
| Dispatch / latency path | IPC hop + browser scheduler | Direct in-process dispatch avoids webview IPC; end-to-end input latency was not measured here |

## 3. The steps (whatever you picked)

1. **Prototype the riskiest screen first** — not hello-world. Every framework
   in this sample satisfied the baseline todo specification, with 22–56 s
   clean builds. Source-size totals use language- and artifact-specific
   definitions, so consult [the empirical table](10-empirical-results.md)
   rather than treating one LoC range as directly comparable. Differences
   appeared in virtualized big lists, rich text editing, complex nested
   layouts, drag-and-drop, and multi-window work.
2. **Pin your framework version exactly** and vendor the examples for that
   version. The audited Iced 0.14, egui 0.34, and Dioxus 0.7 transitions all
   included breaking API changes, so unversioned tutorials and generated
   answers are risky. Use the framework's examples at the pinned tag alongside
   its versioned documentation.
3. **Wire up the OS-integration crates early** (they carry event-loop
   constraints that are painful to retrofit): `rfd` (dialogs), `muda` (menus),
   `tray-icon`, `notify-rust`, `arboard` (clipboard), `global-hotkey`,
   `keyring`. Caveats: on Linux, muda's native menubar path needs a GTK
   window, tray-icon's winit example needs a parallel GTK loop, and
   global-hotkey is X11-only. Tauri plugins reduce application glue, but they
   wrap the same underlying capabilities. In the tested macOS integrations,
   `notify-rust` was rejected in egui/Xilem and could not be called inside a
   Slint timer callback, so test notification delivery and event-loop behavior
   in the exact framework/bundle context and keep a native or out-of-process
   fallback where appropriate. Configure permissions and retain
   each plugin's platform support matrix rather than assuming the caveats
   disappear.
4. **Decide the a11y bar now, not later.** If AccessKit-based: verify with a
   real screen reader (VoiceOver/NVDA) early — integration ≠ usable semantics;
   focus order and text-editing granularity are per-app work. egui's
   `kittest` lets you regression-test the a11y tree in CI.
5. **Test Linux on both X11 and Wayland, and on NVIDIA** — the audit found
   recurring platform-specific risks there in both webview and native stacks.
6. **Plan packaging/signing/updating — but separate the layers**
   ([macOS experiment](14-packaging-results.md)):
   - **Bundling was inexpensive in this credential-free macOS sample**:
     four-line `cargo-bundle` metadata produced the six non-Tauri `.app`
     bundles and Tauri used its own bundler. Plain `hdiutil create` produced
     the final DMG for all seven after built-in DMG paths proved inconsistent
     on this machine. That does not establish Windows or Linux installer
     behavior or a universal rule against built-in DMG support.
   - **Signing capability was not the tested variable.** No Developer ID or
     notarization credentials were configured, so the experiment used local
     ad-hoc seals and expected rejection by Gatekeeper's distribution
     assessment, even though the locally built apps launched. Tauri and
     cargo-packager can drive configured signing/notarization workflows;
     `rcodesign` is a principal Rust-native cross-platform implementation,
     while platform CI can use Apple's tools. None supplies the identity or
     credentials.
   - **Updating is another layer:** Tauri has an integrated updater path;
     cargo-packager can produce distribution/update artifacts; Velopack is a
     framework-independent installer and delta-update option with a Rust
     runtime client. Validate signing of both installers and updater artifacts.
   - Note: notification attribution and banner delivery can differ for a bare
     process versus a bundled, identified `.app`; package early and test the
     exact delivery path. This audit did not establish a universal bundle
     requirement for dock behavior or every shell feature.
7. **Have a text-input reality check**: if your app needs serious text editing
   (not just single-line inputs), cosmic-text and Parley are the main reusable
   pure-Rust engines represented here, but they are not the only possible
   implementation. Zed demonstrates a mature product editor on GPUI's lower
   level primitives; GPUI itself supplies no reusable first-party high-level
   text-input widget, and this
   audit's focused editor implementation was 836 lines.

## 4. Cross-cutting hard parts (set expectations)

- **Rich text editing** — no shared, framework-level rich-editor solution;
  products still integrate engines or build substantial editor behavior.
- **Native menus/tray on Linux** — current winit integration with the
  tauri-apps shell crates crosses a GTK fault line (see
  `00-ecosystem-map.md` §1.6); protocol-specific Rust implementations exist,
  but there is no equally broad framework-neutral facade.
- **Wayland** — the audited source paths imposed extra portal/protocol and
  compositor-specific constraints for global shortcuts, cross-app drag-and-drop,
  and accessibility. These were not empirically compared across all platforms.
- **Printing** — not a first-class feature in most sampled native frameworks;
  webviews can reach browser printing paths, which this audit did not test.
- **wasm** — egui and Dioxus treat the browser as first-class. Iced supports
  web builds; Slint supports WASM application/demo embedding but does not
  recommend it for general-purpose websites; Vello's web path needs WebGPU.
- **Sustainability of your foundation** — the dated public indicators in the
  [load-bearing-crate table](data/load-bearing-crates.md) show several
  concentrated-maintenance and stalled-project risks. They do not establish
  private employment, funding, or project knowledge; treat them as signals to
  assess for the exact dependency set you ship.

## 5. Recommended "reference stacks" (opinionated)

- **Product app, JS-capable team:** Tauri 2 + your web framework +
  tauri-plugin-updater; use semantic HTML and real assistive-technology tests;
  test WebKitGTK on Linux early.
- **Product app, all-Rust team, a11y required:** Slint (attribution ok) or
  egui (tool-like UI ok); choose a bundler, an explicit platform-signing path,
  and, if needed, a runtime updater such as Velopack.
- **Product app, all-Rust, architecture-first, a11y deferrable:** iced —
  watch PR #3111 for a11y; Sniffnet demonstrates stable 0.14, while COSMIC
  uses its fork and Halloy tracks master through a pinned fork.
- **High-performance editor-class app:** gpui + gpui-component, git-pinned;
  accept the platform-polish gradient and the lack of a reusable first-party
  high-level text-input widget; reuse proven editor code or budget the
  implementation, and revisit
  when the GPUI crate family ships to crates.io.
- **Bet-on-the-future experiment:** xilem — the stack (winit/vello/parley/
  AccessKit/masonry) is where the ecosystem is converging; the framework
  itself is not there yet.
