# Structured stack rows returned by research agents (raw, for Phase C synthesis)

## dioxus

```yaml
framework: Dioxus (desktop, wry/tao webview) + Blitz native-renderer project
version_tested: dioxus =0.7.9 ("desktop" feature; latest stable 2026-05-08; 0.8.0-alpha.0 published 2026-05-19)
license: MIT OR Apache-2.0 (Blitz dual MIT/Apache-2.0, stylo_taffy also MPL-2.0)
paradigm: React-like — RSX macro, function components, hooks, Copy signals, VirtualDOM (dioxus-core) that is renderer-agnostic
windowing: tao 0.34.8 (tauri-apps) via dioxus-desktop; Blitz uses winit
renderer: system webview via wry 0.53.5 (WKWebView/WebView2/WebKitGTK) — same stack as Tauri; experimental dioxus-native 0.7.9 = Blitz (anyrender -> vello/wgpu)
text_stack: webview/browser engine text today; Blitz uses parley (Linebender)
layout: webview CSS engine today; Blitz uses stylo (Servo CSS) + taffy (maintained by DioxusLabs, 0.12.1)
widget_set: none — HTML elements styled with CSS in RSX; Radix-style component library added in 0.7
a11y_status: good today via browser a11y tree (screen readers, keyboard nav, IME inherited from webview); Blitz path uses AccessKit but is pre-alpha
os_integration_summary: bundled by default — menus (muda), tray (tray-icon), global shortcuts (global-hotkey), file dialogs (rfd), multi-window, custom protocols; missing notifications, updater, deep-links; no Tauri-style plugin ecosystem or security/capability model
platforms: macOS/Windows/Linux (stable), web (stable), iOS/Android (upstream labels stable), fullstack/liveview/SSR (Axum)
wasm: yes, first-class (~50 kb baseline claim, bundle splitting in 0.7)
mobile: iOS/Android via dx CLI (.ipa/.apk, simulators, ADB hot-reload); younger than desktop/web in practice
production_users: homepage logos Airbus/ESA/Cognition/YC/Futurewei (marketing logos, no case studies; YC=investor, Futurewei=sponsor); Satellite.im Uplink historically; documented case studies thin
backing: DioxusLabs, YC S23 (Jonathan Kelley), ~$500-520K raised (YC, Pioneer Fund, GitHub Accelerator), ~4 employees, Open Collective + sponsors; no larger round found as of 2026-07
build_ok: true (first-try compile, plain cargo, no dx CLI)
launch_ok: true (release binary alive 11 s, RSS ~98 MiB, clean SIGTERM, zero runtime warnings)
spec_gaps: none — all 7 SPEC requirements expressed directly
app_loc: 90
deps_total: 287
clean_build_secs_rough: 102
binary_size_mb: 5.7
docs_quality_1to5: 4
```

TOP FINDINGS:
- Latest stable is 0.7.9 (May 2026); 0.7 (Sep/Oct 2025) shipped subsecond Rust hot-patching, native renderer feature, Axum fullstack overhaul; 0.8 in alpha.
- Blitz is real but pre-alpha ("we would not yet recommend building apps with it"): stylo + taffy + parley + AccessKit + winit + anyrender→vello. dioxus-native 0.7.9 on crates.io behind stable `native` feature; blitz-dom 0.3.0-alpha.6 (2026-06-23); essentially one person (nicoburns).
- Dioxus desktop is downstream of Tauri's stack: tao/wry/muda/tray-icon (all tauri-apps-maintained); Dioxus adds Rust-native UI with no JS toolchain or IPC bridge; Tauri keeps the security model, plugin ecosystem, updater/signing, production track record.
- Taffy is the ecosystem's de-fragmentation success: DioxusLabs-maintained, 8.9M downloads, used by Servo, Bevy, Zed/GPUI, Lapce/Floem, Slint, Blitz.
- Plain cargo works for desktop — dx CLI only for hot reload, assets, bundling, web/mobile.
- Bus factor low: 2 people dominate dioxus, 1 person dominates Blitz+Taffy; 4-person company, ~$520K seed.
- Production-user evidence is marketing-logo-grade, no public case studies.
- OS integration bundled by default (menus/tray/hotkeys/dialogs/multi-window); notifications/updater/deep-links missing vs Tauri plugins.

FRICTION:
- Near-zero build friction: 1 dependency line + 90 LoC, first-try compile, 102 s clean release build.
- Must know CSS/flexbox — no widget set; layout/scrolling is inline CSS in RSX.
- 287-crate graph, ~98 MiB idle RSS for a todo app; muda/tray-icon/global-hotkey/rfd pulled in even when unused.
- Rapid 0.5→0.6→0.7 API churn makes older tutorials/LLM answers stale; docs dx-centric, plain-cargo path under-documented.
- Scripted UI interaction on the webview needs macOS Accessibility permissions; launch verified programmatically instead.

## slint

```yaml
framework: Slint
version_tested: "1.17.1 (released 2026-07-07; pinned =1.17.1 for slint + slint-build)"
license: "Tri-license: GPLv3 | Royalty-Free 2.0 (free for proprietary DESKTOP/mobile/web; mandatory attribution via AboutSlint widget or badge; embedded systems EXCLUDED; no re-exposing Slint APIs) | paid commercial (removes attribution, covers embedded at $1+/device royalty; Enterprise tier has perpetual-fallback)"
paradigm: "Compiled declarative .slint DSL (QML-like) with property-binding/callback reactive model; logic in Rust/C++/JS/Python via generated typed component APIs; live-preview tooling (slint-viewer, VS Code LSP+preview, SlintPad, Figma plugin, 1.17 embedded MCP server)"
windowing: "winit 0.30.13 via i-slint-backend-winit (default desktop, X11+Wayland); alternatives i-slint-backend-qt (Linux w/ Qt), -linuxkms (no compositor), -android-activity, -testing (headless); runtime-selectable via SLINT_BACKEND"
renderer: "FemtoVG 0.25 (GPU, OpenGL or wgpu) = desktop default; Skia opt-in feature; in-house software renderer (tiny-skia paths, no_std, line-by-line partial rendering for MCUs); Qt renderer with Qt backend"
text_stack: "fontdb (discovery) + rustybuzz (shaping) + swash (rasterization, since 1.16) for FemtoVG/software renderers; Skia renderer uses Skia's own text w/ subpixel positioning — shared crates, in-house orchestration"
layout: "In-house constraint row/column/grid layouts in the DSL; experimental FlexboxLayout backed by taffy 0.10 (taffy already in stable dep tree)"
widget_set: "In-house std-widgets in 6 styles (fluent/material/cupertino/cosmic/qt/native-alias); FLUENT is default on ALL platforms since 1.16 (native alias demoted); qt style renders real QStyle when Qt present"
a11y_status: "AccessKit integrated, on by default (accesskit 0.24 + accesskit_macos/winit in default build); accessible-* DSL properties + assistive actions; gaps: text-input widgets incompletely exposed (caret/braille, #2895/#8732), RTL/BiDi essentially unsupported (#1317); IME works (incl. macOS CJK); KeyBinding element since 1.16"
os_integration_summary: "Best-in-Rust-class menus: MenuBar/ContextMenuArea native on macOS via muda; SystemTrayIcon (1.17, default feature); auto dark/light (verified empirically); multi-window yes; DnD in-app only (cross-app blocked on winit); NO native file dialogs (pair with rfd, event-loop footguns documented)"
platforms: "Windows 10/11 (x64+arm64), macOS 14/15/26, Linux X11+Wayland (glibc+dbus), embedded Linux (LinuxKMS), MCUs (no_std, <300kB RAM: RP2040, STM32H7 via official STM32Cube integration, ESP32)"
wasm: "Works (winit+FemtoVG → WebGL canvas, no DOM) but officially 'not recommended for general-purpose web apps'; demo-grade, no a11y, Rust-only"
mobile: "Android supported (Rust; C++ added in 1.17; i-slint-backend-android-activity); iOS in progress toward full support (Rust-only, NLnet-funded, safe-area/vkbd in 1.15, Slint Viewer on App Store)"
production_users: "LibrePCB 2.0 (desktop, migrating off Qt), WesAudio (commercial desktop audio control), MOTOR Ai HMI (via KDAB); core commercial market is embedded HMIs; partners KDAB & Witekio"
backing: "SixtyFPS GmbH, Germany; founders Goffart/Hausmann/Jana, ex-Trolltech/Qt (Hausmann ex-QtQml lead maintainer); license-revenue funded + NLnet grants for Android/iOS ports; no public VC; ~2-3 month minor release cadence, 23.1k stars"
build_ok: true
launch_ok: true
spec_gaps: "none — all 7 spec requirements mapped 1:1 to built-ins (LineEdit.accepted=Enter, ListView scroll, tasks.length counter)"
app_loc: 94
deps_total: 310
clean_build_secs_rough: 116
binary_size_mb: 14.7
docs_quality_1to5: 4
```

TOP FINDINGS:
- Slint 1.17.1 (released 2026-07-07) was the only framework path where the Tasks spec had zero gaps; 94 LoC total; dark mode + Enter-to-submit + list virtualization came free from built-ins.
- Licensing is the adoption crux: Royalty-Free 2.0 genuinely covers proprietary desktop but requires visible attribution, forbids re-exposing Slint APIs, excludes embedded — a kiosk pivot re-triggers paid licensing ($1+/device).
- Stack is more shared than reputation suggests: winit, FemtoVG/Skia, rustybuzz+swash+fontdb, AccessKit, muda menus, taffy (experimental flexbox) — truly in-house: DSL compiler, layout engine, widgets, no_std software renderer.
- Default look changed in 1.16: Fluent everywhere (even macOS); platform-adaptive "native" style demoted to maintenance-mode alias — explicit retreat from native-look-per-platform.
- Native OS menus (real macOS menu bar via muda) and system tray ahead of iced/egui/Dioxus; native file dialogs absent (rfd + documented event-loop footguns).
- MCU support is the unique differentiator (264 kB RAM on RP2040, official STM32Cube integration) and funds the company — desktop rides on embedded revenue.
- A11y second-best in Rust ecosystem (AccessKit default-on w/ DSL-level properties), but text-input a11y incomplete and RTL/BiDi effectively unsupported.
- Qt DNA governance: ex-Trolltech founders, revenue-funded + NLnet grants, steady 2-3-month cadence.

FRICTION:
- Every value crossing the Rust/.slint boundary must be declared on the component interface (two-way plumbing property needed to clear input from Rust).
- One interaction (add task) splits across two files/languages: event wiring in .slint, trim logic in Rust.
- slint::include_modules!() codegen means MainWindow exists nowhere in source; grep fails; slint/slint-build must be version-locked in pairs.
- 310 crate versions, 14.7 MiB unstripped binary, ~116 s clean release build on M4 Pro.
- Several docs URLs 404'd mid-research (recent docs restructure); crate manifest, not the guide, authoritatively documents renderer/feature defaults.

## tauri

```yaml
framework: Tauri
version_tested: "tauri 2.11.5 (2026-07-01) + tauri-build 2.6.3; wry 0.55.1, tao 0.35.3, muda 0.19.3 in lockfile"
license: "MIT OR Apache-2.0 (note: tao is Apache-2.0 only)"
paradigm: "Rust core process + system-webview frontend (any HTML/JS); JSON-RPC-like IPC (commands/events) with raw-payload + Channel escape hatches"
windowing: "tao — in-house winit fork (GTK backend for WebKitGTK on Linux); no winit migration as of 2026, tracks differences via tao#470"
renderer: "system webview via wry: WebView2 (Win), WKWebView (macOS/iOS), WebKitGTK (Linux), Android WebView; nothing bundled; Verso/Servo alt backend archived Oct 2025"
text_stack: "the browser engine's (WebKit/Chromium text shaping via OS webview)"
layout: "CSS/DOM layout in the webview"
widget_set: "none — HTML/CSS or any JS framework"
a11y_status: "best free baseline in study: webview exposes HTML semantics to platform a11y APIs; but no a11y docs, tracking issue #207 open since 2019, Linux at-spi gaps (#4315)"
os_integration_summary: "menus+tray are core (muda/tray-icon); ~30 official plugins (updater, deep-link, autostart, global-shortcut, fs, dialog, notification, store, single-instance…) actively maintained 2026, per-plugin platform matrices; multiwebview-per-window still behind `unstable` feature"
platforms: "Windows 7+, macOS, Linux (GTK/WebKitGTK), iOS, Android"
wasm: "no browser target (it IS the web-tech shell); frontend may use wasm inside the webview"
mobile: "iOS/Android stable since 2.0 (2024-10-02); improving (2.11 added mobile multi-window, file associations); requires tauri-cli; several plugins desktop-only"
production_users: "GitButler, Hoppscotch Desktop, Rivet (Ironclad), Spacedrive, Modrinth App, Jan, Cap, pgMagic; ChatGPT-desktop claim unverified, Claude Desktop debunked (Electron)"
backing: "Tauri Programme within Commons Conservancy (elected 3–7-seat board); CrabNebula (founders' company) commercial backing; Open Collective ~$123k lifetime; NLnet/NGI grants; ~109k stars, ~50-person working group"
build_ok: true
launch_ok: true
spec_gaps: "none — all 7 functional requirements met"
app_loc: 247
deps_total: 271
clean_build_secs_rough: 67
binary_size_mb: 8.0
docs_quality_1to5: 4
```

TOP FINDINGS:
- Manual plain-cargo Tauri v2 setup (no Node/npm/tauri-cli) works but is entirely undocumented — keys: frontendDist relative to tauri.conf.json, withGlobalTauri for npm-free IPC, hand-written capability JSON.
- Verso/Servo alternative backend is dead for now: versotile-org/verso archived 2025-10-08, tauri-runtime-verso dormant, never on crates.io — production Tauri means system webviews, full stop.
- tao remains a divergent winit fork with no migration plan; Tauri v3 (draft milestone, ~26%, no date) targets GTK4/WebKitGTK 6.0 on Linux, not a winit switch; claims "Dioxus meanwhile left tao for winit" [CONFLICT: dioxus agent found tao 0.34.8 in dioxus 0.7.9 dep tree — verify in Phase C; possibly a 0.8-alpha change].
- Security model strongest in study: deny-by-default, schema-checked capability files per window, compile-time CSP nonce/hash injection, optional AES-GCM isolation iframe.
- Webview tradeoff quantified: 8.0 MB binary, 67 s clean build; third-party benchmarks ~8.6 vs 244 MB bundles but only ~172 vs 409 MB memory vs Electron — memory advantage real but modest (~115 MB RSS for a to-do).
- Linux is the weak leg: WebKitGTK NVIDIA blank-window bugs (official debug page, WEBKIT_DISABLE_DMABUF_RENDERER=1) and distro version fragmentation.
- Accessibility free and excellent (HTML semantics → native a11y tree) yet officially undocumented; a11y tracking issue open since 2019.
- Governance most formal of any Rust GUI project (Commons Conservancy programme, elected board), but core maintenance coupled to CrabNebula.

FRICTION:
- Icons required even with bundle.active:false, must be RGBA PNGs — needed custom generation script without tauri-cli.
- Capability/config JSON schemas only materialize in gen/schemas/ after first build; config errors surface via build-time validation (slower loop than rustc).
- Even a 250-line app spans five languages/files (Rust, JSON, HTML, CSS, JS) with an async serialized IPC hop per interaction.
- GitHub relative dates misled one research pass; release dates re-verified against crates.io API timestamps.
- Where others pay in widget code, Tauri pays in setup: text input, IME, scrolling, focus, a11y cost zero lines — the platform browser did it all.

## iced

```yaml
framework: iced
version_tested: "0.14.0 (crates.io, released 2025-12-07; lockfile pulls subcrate patch iced_widget 0.14.2)"
license: MIT
paradigm: "Elm architecture (State/Message/update/view), retained widget tree re-derived each update; iced::application builder + Task/Subscription for async"
windowing: "winit 0.30.13 (shared ecosystem crate) via iced_winit shell; x11+wayland default features"
renderer: "iced_wgpu on wgpu 27.0.1 (Metal/Vulkan/DX12/GL) with iced_tiny_skia software fallback (tiny-skia 0.11.4 + softbuffer 0.4.8); custom pipelines + lyon tessellation on top"
text_stack: "cosmic-text 0.15 (shared, System76) with HarfRust 0.3 shaping + swash raster; GPU glyph atlas is cryoglyph 0.1 — iced's own fork of glyphon"
layout: "in-house iced_core::layout (Limits/Node + flex), Druid-derived (ships DRUID_LICENSE); NOT taffy; flexbox-like one-pass constraints, no grid in core"
widget_set: "in-house iced_widget: button, text_input, checkbox, slider, pick_list, combo_box, scrollable, canvas, image/svg, markdown, pane_grid, + 0.14 table/grid/pin/float; extras via third-party iced_aw"
a11y_status: "NONE in stable — zero accesskit in dep tree; issue #552 open since 2020-10; PR #1849 (System76) open-unmerged since 2023; PR #3111 open draft; PR #3281 closed unmerged 2026-03-14 by hecrj ('Thanks! But I'll work on this myself.'); pop-os/iced fork ships iced_accessibility, a11y default-on in libcosmic; no Tab navigation (#489)"
os_integration_summary: "built-in: multi-window (0.12) + daemon API (0.13), dark-mode detection (0.14, PR #3051, winit+mundy), file-drop events (not on Wayland); third-party: rfd dialogs, notify-rust notifications, iced_aw in-window menus; missing: native menubar, system tray (#124 open since 2019)"
platforms: "Windows, macOS, Linux X11+Wayland all official tier-1; macOS verified first-hand"
wasm: "claimed but second-tier: 0.14 fixed WebGPU/WebGL boot, yet open breakages (#2978 examples broken, #2108 clipboard, #2843 CJK IME, #3199 canvas text)"
mobile: "not supported; issue #302 open since 2020-04, still active 2026-06"
production_users: "COSMIC desktop (System76, via pop-os/iced fork 238 ahead/278 behind upstream), Sniffnet (~39.9k stars, stable 0.14), Halloy (~4.3k stars, tracks 0.15.0-dev master), Kraken Desktop (official showcase), Icebreaker, Airshipper/Veloren (still 0.12), OctaSine, Ludusavi, UAD-NG"
backing: "solo BDFL Héctor Ramón (hecrj, ~5,503 of ~6,000 commits; FAQ: 'Every single line of code is either written or reviewed directly by me'); GitHub Sponsors 12 current ($5–50 tiers); README's Kraken/Cryptowatch sponsorship line persists but Cryptowatch sunset 2023 — current funding unverifiable; no foundation; 30.9k stars"
build_ok: true
launch_ok: true
spec_gaps: "none — all SPEC.md requirements mapped 1:1 to stock widgets (Enter-to-add via text_input::on_submit)"
app_loc: 74
deps_total: 149
clean_build_secs_rough: 78
binary_size_mb: 9.9
docs_quality_1to5: 3
```

TOP FINDINGS:
- Accessibility is the disqualifier for many use cases: no AccessKit in stable after ~6 years (#552); hecrj closed the most complete community a11y PR (#3281, 2026-03) saying "I'll work on this myself" — accessible iced exists only in System76's fork.
- iced's biggest users don't use released iced: COSMIC runs a fork 238 ahead/278 behind, Halloy pins master (0.15.0-dev), OctaSine uses iced_baseview — the 14.5-month release gap (0.13→0.14) pushes serious users off stable.
- Stack half shared, half duplicated: shares winit/wgpu/cosmic-text/tiny-skia, but re-implements layout (Druid-derived, not taffy), forked glyphon into cryoglyph, maintains its own window_clipboard.
- Bus factor is 1 by the maintainer's own written policy; funding is hobby-scale sponsors, README's Kraken claim unverifiable since Cryptowatch's 2023 shutdown.
- Desktop-shell integration weakest layer: no native menus, no tray (#124 since 2019), no notifications — scope is "pixels in a window"; rest via rfd/notify-rust/iced_aw or doesn't exist.
- 0.14 closed real gaps: IME (PR #2777), dark-mode detection (PR #3051), animations, headless testing, hot reload — but Tab navigation (#489), RTL (#250), wasm stability, mobile (#302) remain open.
- DX for spec-shaped CRUD UIs excellent: 74 LoC, zero gaps, 78 s clean build / 0.3 s incremental, 9.9 MB binary, 149 deps.
- Elm architecture is enforced, not optional — most structurally disciplined, most boilerplate-y for tiny apps.

FRICTION:
- API churn between minors: 0.14 changed iced::application to (boot, update, view), .title() to closure — 0.12/0.13 snippets don't compile; mismatches produce dense generic-trait-bound errors.
- Fresh build emits future-incompat warning for transitive block v0.1.6 (via iced's own window_clipboard → clipboard_macos).
- Official book unfinished (Layout/Styling/Concurrency chapters "More to come!"); real learning is 52 version-matched examples; README's Discourse link dead.
- Otherwise clean first run: silent Metal bring-up, instant window; iteration fast once past API-version mismatch phase.

## linebender (xilem)

```yaml
framework: xilem (Linebender stack: xilem/masonry/vello/parley/fontique/kurbo/peniko + AccessKit)
version_tested: xilem =0.4.0 (2025-10-29; pins masonry 0.4.0, vello 0.6.0, parley 0.6.0 — standalone crates are at vello 0.9.0 / parley 0.11.0)
license: "xilem & masonry: Apache-2.0 ONLY; vello/parley/kurbo/peniko/fontique/accesskit: MIT OR Apache-2.0"
paradigm: reactive view-tree (cheap view values rebuilt after every state mutation, diffed onto a retained Masonry widget tree; callbacks get &mut State — no Elm message enum)
windowing: winit 0.30 via masonry_winit (masonry_core is windowing-agnostic; git main uses ui-events, demonstrated VST-plugin embedding)
renderer: vello 0.6 (wgpu 26, GPU compute; self-declared alpha). Sparse-strips rewrite (vello_cpu/vello_hybrid 0.0.9, hybrid "roughly beta") only on xilem git main via new `imaging` abstraction
text_stack: parley + fontique (fallback) + harfrust (shaping, since parley 0.6) + skrifa + ICU4X; swash fully dropped in parley 0.11
layout: masonry's own box-constraint system (flex/grid/zstack/split/portal/virtual_scroll) — NOT taffy; new layout system landed Q1 2026 (git main)
widget_set: "~15 core views in 0.4.0: label/prose/text_input/button/checkbox/slider/progress/spinner/image + containers; missing: menus, combobox, table/tree, dialogs, keyed lists"
a11y_status: deepest in ecosystem — accesskit wired into masonry_core AND parley (text semantics) + accesskit_winit/macos adapters; but release ships accesskit 0.21 vs standalone 0.24; screen-reader polish undocumented
os_integration_summary: minimal — window, clipboard basics, initial multi-window; no native menus (open issue #1343), no tray/dialogs/notifications; dark-only default theme
platforms: Windows, macOS, Linux (primary); Android demo-grade (android_main in every example, Google-funded 2024 workstream); no iOS
wasm: split story — vello needs WebGPU (Chrome ok, Firefox/Safari experimental); xilem_web is a separate DOM-targeting sibling crate sharing xilem_core, not masonry/vello
mobile: Android examples ship; iOS absent (AccessKit has an iOS adapter but xilem has no iOS story)
production_users: "xilem: none (Runebender port + Scrolled Quran are the flagship demos). BUT lower layers: Bevy 0.19 (parley), Slint 1.14 (parley+fontique), Servo canvas (vello/vello_cpu), Blitz/Dioxus Native (parley+vello), femtovg, krilla/Typst-PDF (parley)"
backing: "Linebender community org (Zulip, weekly office hours, low-activity RFC repo, Raph Levien final decision-maker). Google Fonts funded Raph + 4 xilem devs through 2024-25; Raph left Google 2025-10-12 → Canva Jan 2026 (continuing Linebender; Canva devs contribute to Vello); 2x NLnet/NGI0 grants fund Parley in 2026"
build_ok: true
launch_ok: true (alive 10s, zero runtime warnings, UI screenshot-verified)
spec_gaps: NONE — all 7 requirements first-class (on_enter, placeholder, per-row delete, portal scroll); caveat: spec ≈ xilem's own to_do_mvc example, so this measured its best-tested path
app_loc: 81
deps_total: 153
clean_build_secs_rough: 98
binary_size_mb: 11.9
docs_quality_1to5: 3
```

TOP FINDINGS:
- Shared-foundation thesis confirmed for the MIDDLE of the stack, not the top: 2025-26 saw Bevy 0.19 switch cosmic-text→parley, Slint 1.14 unify on fontique+parley, Servo adopt vello/vello_cpu for 2D canvas, Blitz/Dioxus Native build on parley+vello. Meanwhile masonry has 3 reverse deps and xilem zero production users.
- parley vs cosmic-text: cosmic-text has larger installed base (137 reverse deps vs 65; iced, Zed's gpui, glyphon, Floem, Cushy) but momentum flipped to parley (Bevy defected citing better docs; Slint consolidated). Remaining fork = layout + font-db/fallback (fontique vs fontdb) + editing.
- Shaper convergence beneath both stacks: parley (0.6+) AND cosmic-text now shape with HarfRust (HarfBuzz org's official Rust port); rustybuzz deprecated. One shaping engine for nearly all Rust UI — de-fragmentation already happened one layer down.
- AccessKit is the de-fragmentation success story: separate org (Matt Campbell), 115 reverse deps (egui, Bevy, Slint, Servo, Blitz, parley, masonry), adapters for Win/macOS/AT-SPI/Android/iOS.
- Funding transition in progress: Google Fonts era ended with Raph's Oct 2025 exit to Canva; Parley carried by 2 NLnet grants for 2026; xilem/masonry release cadence (~6 months) and blog cadence both slowed.
- Internal version skew is xilem's biggest practical problem: latest release pins vello 0.6/parley 0.6 while standalone crates are at 0.9/0.11; Q1-2026 imaging migration only on git main; two skrifa versions in one build tree.
- Vello's unified-API plan abandoned — two competing abstractions (Blitz's AnyRender vs Linebender's imaging) sit on top; sparse-strips crates 0.0.9 with no-stability warnings; production-ready only for scoped uses.
- Mini-app hit zero spec gaps — but only because the spec matches xilem's own flagship example; one step off that path (menus, keyed lists, theming, OS integration) and the alpha shows.

FRICTION:
- One compile-fix iteration (FlexSpacer::Fixed wants Length not f64); compiler errors excellent, circulating examples stale.
- Release-vs-main gap: everything interesting from 2026 requires git main; crates.io xilem is 8 months behind its own renderer/text layers.
- Docs = docs.rs + examples only; no book; changelog only started at 0.4.0.
- Spartan defaults: dark-only theme, no menu bar, no macOS niceties.
- (Environment) macOS denied synthetic keystrokes to sandboxed shell; Enter path verified visually + by code identity with upstream example.

## gpui

```yaml
framework: gpui (Zed Industries)
version_tested: "0.2.2 (crates.io, published 2025-10-22 — still latest as of 2026-07-07)"
license: "gpui + gpui_* platform crates: Apache-2.0; Zed app and Zed's ui/ui_input component crates: GPL-3.0-or-later"
paradigm: "Hybrid retained/immediate: retained Entity<T> state + Render trait rebuilding element tree per frame; tailwind-style div() builder styling; action/keybinding system; own async executor on platform event loop"
windowing: "In-house per-platform, NOT winit (verified 0 refs): AppKit via cocoa/objc (macOS), custom Wayland(calloop)+X11(xcb) (Linux), raw Win32 via windows-rs; on main split into gpui_macos/gpui_linux/gpui_windows/gpui_platform crates"
renderer: "macOS: in-house Metal; Windows: in-house Direct3D 11 (+DirectComposition); Linux in 0.2.2: blade-graphics 0.7 (Vulkan) — REPLACED on main by wgpu (community PR #46758, merged 2026-02-13, new gpui_wgpu crate); WebGPU path for wasm in gpui_web (unreleased)"
text_stack: "Platform shapers: CoreText + zed-font-kit fork (macOS), DirectWrite (Windows), cosmic-text 0.14 + rustybuzz + fontconfig (Linux); gpui_wgpu path uses cosmic-text 0.19 + swash"
layout: "taffy (shared ecosystem crate) — 0.9.0 in released 0.2.2, '=0.10.1' exact-pinned on main"
widget_set: "NONE in gpui (no text input/button/list — compose styled divs; official input example = 746 lines via EntityInputHandler). Zed's ui crate is GPL + unpublished. De-facto widget layer: third-party longbridge/gpui-component (Apache-2.0, 60+ widgets, ~12k stars, crates.io 0.5.1)"
a11y_status: "Zero in any released version (issue #41138). AccessKit integration merged into gpui on main 2026-05-27 (PR #56065, accesskit 0.24 + macos/unix/windows adapters), Zed components not yet annotated, unreleased on crates.io. Keyboard nav: tab_stop/tab_index + focus + actions. IME works but only via DIY EntityInputHandler. RTL unsupported (#31102 open)"
os_integration_summary: "Good core: native menus+dock menu, file dialogs, reveal-in-Finder, open_url/URL schemes, dark-mode detection+observer, keychain, drag&drop incl. OS files, multi-window, custom titlebar hooks, screen capture (feature). Missing: system tray, native notifications, printing"
platforms: "macOS, Linux (Wayland+X11), Windows (first-class since Zed Windows GA 2025-10-15), FreeBSD best-effort"
wasm: "In development on main (gpui_web + gpui_wgpu/WebGPU crates, wasm target cfgs, fetch HTTP client PR #50463 Mar 2026); nothing released"
mobile: none
production_users: "Zed (86.6k stars), Longbridge Pro trading terminal (via gpui-component), helix-gpui, Loungy, Hummingbird, coop, omarchist + ~45 apps on official awesome-gpui; 100 reverse deps on crates.io"
backing: "Zed Industries — $32M Series B led by Sequoia (2024), ~$42M total, no round since; CLA required; gpui development inside zed monorepo, roadmap driven by the editor"
build_ok: true
launch_ok: true
spec_gaps: "Text input approximated with raw key events (no IME/selection/clipboard — gpui has no input widget); quit-on-last-window-close must be wired manually; otherwise full spec expressed"
app_loc: 230
deps_total: 525
clean_build_secs_rough: 66
binary_size_mb: 5.0
docs_quality_1to5: 3
```

TOP FINDINGS:
- gpui IS on crates.io (0.2.0–0.2.2, Oct 2025, Apache-2.0, docs.rs builds, 28 bundled examples) — but cadence stalled immediately: zero releases in 8.5 months while main diverged hugely, so "modern gpui" again requires a git dep on the zed monorepo.
- Main-branch gpui restructured in 2026 into a crate family (gpui + gpui_platform/gpui_macos/gpui_linux/gpui_windows/gpui_wgpu/gpui_web/scheduler), all Apache-2.0, none published yet.
- Blade is dead: Linux renderer reimplemented on wgpu by a community contributor (PR #46758, merged 2026-02-13) — gpui now shares the ecosystem-standard GPU stack on Linux/web; macOS Metal and Windows D3D11 remain in-house.
- AccessKit merged into gpui 2026-05-27 (PR #56065) with mac/unix/windows adapters — first-ever a11y for gpui, but unreleased and Zed's components aren't annotated; released versions expose nothing to screen readers; RTL unsupported.
- gpui ships no widgets at all — official input example is 746 lines. Permissive widget layer is third-party longbridge/gpui-component (60+ widgets, ~12k stars, production-proven in Longbridge Pro); Zed's own ui crate is GPL + unpublished — a license moat between framework and first-party components.
- Windowing fully custom per platform (zero winit); text uses platform shapers (CoreText/DirectWrite) + cosmic-text on Linux; layout is shared taffy — gpui both re-implements (windowing, mac/win renderers) and shares (taffy, cosmic-text, now wgpu, AccessKit).
- Real external ecosystem: 100 crates.io reverse deps, ~45 apps on official awesome-gpui, create-gpui-app scaffolder (stale since Apr 2025).
- Compile-time reputation outdated: clean release build = 66 s on M4 Pro (525 deps, 5.0 MB binary).

FRICTION:
- macOS setup trap: default build fails on Xcode 26 ("cannot execute tool 'metal' — missing Metal Toolchain"); must enable runtime_shaders feature or download multi-GB Metal Toolchain.
- Hand-rolling a text input is mandatory; doing it properly = EntityInputHandler (~750 lines) or gpui-component.
- Apps don't quit when last window closes unless you wire cx.on_window_closed(… cx.quit()).
- Keystroke::key vs key_char semantics undocumented — required reading framework source.
- block v0.1.6 future-incompat warning from Objective-C bridge on every build; runtime warning-free.

## egui

```yaml
framework: egui (with eframe)
version_tested: 0.35.0 (crates.io, published 2026-06-25, pinned "=0.35.0")
license: MIT OR Apache-2.0
paradigm: immediate mode; reactive repaint by default (repaint only on input/animation, ~0% CPU idle); full re-layout every frame
windowing: winit 0.30.13 via egui-winit/eframe (shared)
renderer: wgpu 29.0.4 default in eframe 0.35 (egui-wgpu); glow 0.17/OpenGL optional; epaint does in-house CPU tessellation to triangle meshes
text_stack: "REPLACED in 0.34/0.35: harfrust 0.7 (HarfBuzz port) shaping since 0.35 + skrifa 0.42 glyph loading/hinting + vello_cpu 0.0.9 rasterization since 0.34 (ab_glyph gone); in-house run layout; NO bidi/RTL (#1016/#5069 open), NO system font fallback (bundle your own fonts)"
layout: in-house single-pass immediate-mode layout (+ new Atom/AtomLayout primitives since 0.32); no constraint solving; first-frame sizing flicker documented
widget_set: in-house (egui built-ins; plots/tables via egui_plot/egui_extras)
a11y_status: "AccessKit pilot, now mandatory dep of egui since 0.34 and default-on in eframe; accesskit 0.24.1 adapters Win(UIA)/macOS/Linux(AT-SPI)/Android at 'rough feature parity'; web = experimental built-in screen reader only; verified working here via egui_kittest driving the app through the AccessKit tree; gaps: live regions (#2647), labelled_by panics (#3647)"
os_integration_summary: "native menus NO (#3411 open); tray NO built-in (tray-icon + new external-eventloop example); file dialogs via rfd (official example); file drag&drop built-in; multi-window built-in (viewports since 0.24, not on web); dark mode built-in (ThemePreference::System); notifications third-party (notify-rust/egui-notify)"
platforms: "Windows/macOS/Linux(X11+Wayland) first-class; all default features"
wasm: first-class (WebGL2/WebGPU canvas; egui.rs is the demo); no AccessKit on web; no multi-viewport
mobile: Android supported (eframe features + official example); iOS not an official target (DIY via winit)
production_users: "Rerun Viewer (flagship, commercial); bevy_egui = de-facto Bevy tooling UI; 1,032 crates.io reverse deps, egui 19.3M downloads (4.4M/90d); no other company-backed shipping desktop product verified"
backing: "Rerun sponsors development; Emil Ernerfeldt (creator) is Rerun co-founder/CTO, #2 maintainer lucasmerlin also Rerun-sponsored — both paid maintainers on one startup's budget; bus factor 1-2 (emilk 2,995 commits vs 179 next); cadence 2-4 breaking minors/year (0.33 Oct-25, 0.34 Mar-26, 0.35 Jun-26)"
build_ok: true
launch_ok: true (alive >10s, zero runtime warnings, clean kill; plus 2/2 headless AccessKit-driven kittest interaction tests pass)
spec_gaps: none
app_loc: 105 (168 incl. optional kittest test module)
deps_total: 163 unique external crates (216-line deps-flat.txt)
clean_build_secs_rough: 103
binary_size_mb: 12.5 (unstripped, wgpu)
docs_quality_1to5: 4
```

TOP FINDINGS:
- egui replaced its entire text stack in 0.34–0.35: ab_glyph → skrifa (hinting/variable fonts, PR #7694) + vello_cpu rasterization + harfrust OpenType shaping (PR #8031, merged 2026-04-06) — real GPOS kerning, ligatures, combining marks now; the harfrust PR author credited "Claude Code did most of the heavy lifting".
- Despite the new shaper, bidi/RTL still unimplemented (#1016/#5069) and no system-font discovery/fallback — non-Latin means bundling your own fonts.
- AccessKit went from optional to mandatory egui dependency in 0.34, default-on in eframe; official AccessKit-based test harness (egui_kittest) — mini-app verified fully operable through its a11y tree (typed, clicked, queried labels headlessly).
- Breaking API churn real: 0.34 renamed App::update(ctx) → App::ui(&mut Ui), deprecated SidePanel/TopBottomPanel; virtually all pre-2026 tutorials now wrong.
- eframe 0.35 default renderer is wgpu (glow behind feature flag); defaults include X11+Wayland+accesskit.
- OS-shell surface thin by design: no native menus (#3411), no tray; but "eframe owns the event loop" blocker now has official external-eventloop examples.
- Funding concentration: both paid maintainers financed by Rerun; commit ratio 2,995:179 → bus factor ~1-2, offset by 29.6k stars and 1,032 dependent crates.
- Build economics excellent: 103 s clean / ~1 s incremental, 12.5 MB binary, 163 deps, zero native-toolchain pain.

FRICTION:
- Enter-to-submit on TextEdit is undocumented folklore: lost_focus() && key_pressed(Enter) + manual request_focus().
- 0.34+ root Ui has no margin/background — must know to wrap in CentralPanel; old-API examples mislead.
- "Input stretches, button hugs" needs manual width arithmetic; right-aligning a row button needs nested layout gymnastics.
- kittest type_text silently no-ops unless you focus() the node first.
- List deletion requires the deferred-index idiom — boilerplate every list UI pays.

## freya

(Added 2026-08-03 with the three-framework expansion cohort. Every field below
is sourced from `apps/freya-*` — Cargo manifests, the committed `Cargo.lock` /
`deps-flat.txt`, GAPS.md and the eight FRICTION.md files. Fields that require
upstream ecosystem research was deferred; a 2026-08-04 follow-up check filled
license, maintainer concentration and download/star counts, and the fields still
marked `pending_upstream_research` — governance, production users, platform/
wasm/mobile support, docs quality — remain unresearched rather than guessed.)

```yaml
framework: Freya
version_tested: "=0.4.0 (freya 0.4.0; freya-core/components/winit/edit/engine/animation/sdk 0.4.1, freya-clipboard 0.4.2)"
license: MIT (confirmed 2026-08-04: crates.io + cargo metadata)
paradigm: "Signal-based reactive. Freya 0.4 is NOT the RSX/Dioxus-macro library it was in 0.2/0.3: components are plain values built with a chained builder API (rect().horizontal().spacing(10.)) and reactivity is State<T> (Copy). No Message enum, no central update function — handlers mutate signals directly. Async is a first-party SINGLE-THREADED executor on the UI thread (spawn -> TaskHandle, spawn_forever, use_future)."
windowing: "winit 0.30.13 via freya-winit 0.4.1"
renderer: "Skia through the freya-skia-safe / freya-skia-bindings 0.98.1 FORK (Metal on macOS). The first build of the cohort downloads a prebuilt Skia archive; later app crates reuse the cached download. render_pipeline.rs still carries `// TODO: Use incremental rendering` — every painted frame redraws the entire tree."
text_stack: "Skia textlayout (HarfBuzz shaping + ICU BiDi) over SkFontMgr/CoreText for discovery and per-script fallback; editing model is freya-edit 0.4.1 (ropey 1.6.1 + unicode-segmentation). LaunchConfig::with_font / with_fallback_font exist for the cases system discovery misses."
layout: "torin 0.4.1 — Freya's own layout engine (NOT taffy). Vocabulary traps recorded: Size::flex(1.) silently no-ops unless the parent opts into .content(Content::flex())."
widget_set: "freya-components 0.4.1: Button, Input, ScrollView, VirtualScrollView, Table (a LAYOUT helper — one element per cell, does not virtualize), Slider (percentage-only, no range/step), ProgressBar, DragZone/DropZone (typed DnD with framework-painted ghost), ImageViewer, CameraViewer (behind the `camera` feature), canvas() exposing the raw skia_safe::Canvas. Notable holes: Input is hard-wired to max_lines(1) so a text area must be assembled from the low-level use_editable hook (~90-110 LoC), and there is no double-click event."
a11y_status: "AccessKit in the default tree (accesskit 0.24.1, accesskit_consumer 0.38.0, accesskit_macos 0.26.3, accesskit_winit 0.33.2) with INLINE per-element attributes (a11y_role, a11y_alt, AccessibilityRole::ColumnHeader/Row), so the a11y tree needs no second data model. on_press unifies mouse/touch/keyboard activation. Screen-reader depth not exercised in this cohort."
os_integration_summary: "Strong first-party surface: `tray` feature (Freya itself owns the GLOBAL tray-icon/muda handlers and forwards both into one callback), on_file_drop as an ELEMENT event with a PathBuf payload, live system theme via a reactive Platform::preferred_theme, multi-window (launch_window/close_window/focus_window/with_window/post_callback), close interception (with_on_close -> CloseDecision::KeepOpen), plus optional `camera`, `plot` (plotters backend) and `query` (TanStack-Query-style cache) features. Missing: global hotkeys, file dialogs (rfd is a transitive dep but not re-exported), notifications, ANY timer/interval primitive, window screenshot, and any way to signal the reactive runtime from a platform callback."
platforms: pending_upstream_research
wasm: pending_upstream_research
mobile: pending_upstream_research
production_users: pending_upstream_research
backing: "solo project: marc2332 wrote 611 of last-12-mo commits (next contributor: 6), no employer stated; 36.5k all-time crates.io downloads, 2.9k stars (2026-08-04); no org/sponsor backing found"
build_ok: true      # all 8 apps build clean in release; --locked --release reproduces
launch_ok: true     # release binary alive >8 s with a visible window, clean SIGTERM, nothing on stdout/stderr
spec_gaps: "none for SPEC.md — every requirement maps onto stock 0.4 elements/components with default features (Input::on_submit gives Enter-to-add, ScrollView gives overflow scrolling). List reconciliation wants explicit .key(index) on repeated rows."
app_loc: 90
deps_total: 258     # unique crates in the todo app's normal-dependency tree, incl. the full image codec set, rfd, clipboard glue and AccessKit
clean_build_secs_rough: 41   # freya-app release build with the prebuilt Skia archive already cached; not a canonical measurement
binary_size_mb: pending_new_cohort
docs_quality_1to5: pending_upstream_research
```

TOP FINDINGS:
- Freya is the only framework in the study that ships a first-party CAMERA integration (`use_camera` + `CameraViewer` behind the `camera` feature, nokhwa underneath) and a stock virtualized scroll view, so two of the study's most expensive capabilities are single component calls.
- Its async model is the inverse of the Elm-shaped frameworks: a single-threaded executor ON the UI thread means `await` then assign to a signal, with no channel, no Send bound and no message enum; `TaskHandle::cancel()` is simultaneously debounce, stale-guard and protocol-level cancellation.
- The `tray` feature makes Freya own the global muda/tray-icon event handlers, which structurally prevents the channel-splitting trap that bites iced, egui and dioxus.
- The framework's own Skia fork (freya-skia-safe 0.98.1) is a real divergence point: it pins the app to that fork's API generation (PathBuilder, not Path::move_to) and to a prebuilt-binary download on first build.
- Editing is the weak layer: freya-edit moves the caret by UTF-16 code unit, so a ZWJ emoji cluster can be split and corrupted by one Backspace — in a crate that already depends on unicode-segmentation.

FRICTION:
- Release-only panic hook: freya_winit::launch installs (in release builds only) a hook that shows an rfd "Fatal Error" dialog, chains to the previous hook, then exit(1) — a panic in release is a frozen window plus a modal alert with nothing on stderr. Diagnosis requires a debug build.
- `Ref` (from State::peek()/read()) held across a write() panics, and via the hook above that becomes a hung app; `x.set(*x.peek() + 1)` is enough to trigger it.
- Canvas elements diff as UNCHANGED unless they carry an event handler (RenderCallback's PartialEq returns true unconditionally), so a data-only canvas silently never repaints.
- No timer primitive: five of the eight apps depend on async-io purely for `Timer::after`.
- Debug builds silently inject freya-performance-plugin's FPS overlay.

## vizia

(Added 2026-08-03 with the three-framework expansion cohort. Sourced from
`apps/vizia-*`; upstream ecosystem fields not researched in this pass are marked
`pending_upstream_research`.)

```yaml
framework: Vizia
version_tested: "=0.4.0 (vizia 0.4.0 = vizia_core/_reactive/_style/_storage/_input/_window/_id/_winit 0.4.0). Default features: winit, clipboard, x11, wayland, markdown, accesskit"
license: MIT (confirmed 2026-08-04: crates.io + cargo metadata)
paradigm: "Neither immediate-mode nor whole-tree-rebuild. The builder closure passed to Application::new runs ONCE; reactivity is fine-grained via Signal<T>/Memo from vizia_reactive (Label::new(cx, signal) subscribes that one label; List diffs by value and 0.4's ListItemsBinding keeps a per-item Signal so a value change costs no entity rebuild). State mutation goes through an Elm-ish Model::event + typed event enum, so the code shape is close to iced's update, but there is no view function to re-run. Model has NO Send bound and Model::event runs on the main thread, which is what makes !Send OS handles (TrayIcon, cpal streams) easy to home."
windowing: "winit 0.30.13 via vizia_winit 0.4.0, with glutin 0.32.3 / glutin-winit 0.5.0 for the GL context"
renderer: "Skia via skia-safe 0.93.1 (built with metal + gl + textlayout + svg), statically linked — the dominant term in the ~21.8-22.3 MiB release binaries. The first build downloads a PREBUILT skia-bindings binary rather than compiling Skia from source; on a cold machine that is the long pole and needs network access. vizia::vg RE-EXPORTS skia-safe, so a custom View::draw receives the same skia_safe::Canvas the framework draws with, already in view coordinates."
text_stack: "Skia SkParagraph (skia-safe textlayout feature) over CoreText's system font manager on macOS: shaping (HarfBuzz inside Skia), BiDi resolution and per-script fallback are all the platform's. There is no cosmic-text/fontdb layer to configure and no font database to warm up. vizia_core::text::EditableText uses unicode-segmentation."
layout: "morphorm 0.8.0 plus a REAL CSS engine (vizia_style 0.4.0) — the only framework in this cohort with stylesheets, CSS transitions/@keyframes and Morphorm units (1s, auto). cx.add_stylesheet is the idiomatic home for shared layout constants."
widget_set: "vizia_core views: Label, Button, Textbox, Slider (always normalised 0.0-1.0), ProgressBar, ScrollView, List, Image, MenuBar (an IN-WINDOW view drawn by Skia, not a native menu bar), Resizable, VirtualList and VirtualTable — a genuine data grid with sort_state/sort_cycle/resizable_columns/selectable/selected_row_ids modifiers and on_sort/on_row_select callbacks. Core drag-and-drop (on_drag/on_over/on_drop/has_drop_data) is part of the view API, not a widget."
a11y_status: "AccessKit is a DEFAULT feature (accesskit 0.24.1, accesskit_consumer 0.38.0, accesskit_macos 0.26.3, accesskit_winit 0.32.2). VirtualTable sets Role::Table, so the grid is exposed to assistive tech for free. Full IME plumbing exists (WindowEvent::{ImeActivate,ImePreedit,ImeCommit,SetImeCursorArea} + Textbox preedit_backup) but was unexercised — no CJK input source on the reference machine."
os_integration_summary: "Windowing-level only: multi-window (Window::new inside a Binding, with on_close/title/inner_size), file drop for free (winit DroppedFile surfaced as WindowEvent::Drop(DropData::File) through the SAME on_drop modifier used for in-app dragging), text clipboard via the default `clipboard` feature (copypasta) exposed as ADDRESSABLE EVENTS (cx.emit_to(widget, TextEvent::Paste)), and automatic light/dark theme following ThemeChanged. Missing: system tray, global hotkeys, a NATIVE menu bar, file dialogs, notifications, window-visibility getters, window screenshot, and any async executor at all."
platforms: pending_upstream_research
wasm: pending_upstream_research
mobile: pending_upstream_research
production_users: pending_upstream_research
backing: "geom3trik (Dr George Atkinson) wrote 285 of last-12-mo commits (next: 14, 13), no employer stated; 6.3k all-time crates.io downloads — smallest ecosystem in the cohort — 2.2k stars (2026-08-04)"
build_ok: true      # all 8 apps build clean in release with no warnings; --locked --release reproduces; no future-incompatibility warnings reported by cargo
launch_ok: true     # release binary shows a window titled "Tasks (vizia)", alive past the 10 s bar, clean SIGTERM, nothing on stdout/stderr
spec_gaps: "none for SPEC.md — every requirement maps onto stock 0.4 views (Textbox::on_submit for Enter-to-add, List wraps its items in a ScrollView internally). Trap: events emitted from a Model propagate UP the tree and never reach a child view, so clearing the input after Add is done by writing the model signal rather than emitting TextEvent::Clear."
app_loc: 162
deps_total: 169     # unique crates in the todo app's normal-dependency tree
clean_build_secs_rough: pending_new_cohort
binary_size_mb: 21.8
docs_quality_1to5: pending_upstream_research
```

TOP FINDINGS:
- vizia is the only framework in the study with a genuine virtualized, sortable, resizable, selectable TABLE widget in core (VirtualTable): iced's `table` materialises O(rows × cols) widgets and egui/xilem/gpui/floem all hand-roll windowing.
- It is also the only one with a real CSS engine, and `transition: height` animates a LAYOUT property — the kanban's insertion gap genuinely opens and closes, where most of the cohort animates paint only.
- Core drag-and-drop (on_drag/on_over/on_drop) is ~14 lines of modifiers AND the same API receives Finder file drops as DropData::File(PathBuf) — one drop path for in-app and OS drags.
- The renderer being Skia and being EXPOSED (vizia::vg) removes the texture/image layer entirely: a camera preview is a custom View::draw building a raster image from raw RGBA, with no framework image handle and no image feature flag.
- The cost side is memory: ≈82 KiB RSS per one-line Label (11,000 Labels = 114.9 → 1005.2 MiB), and grid RSS grows 140 → 290 MiB over a full 100k-row sweep as Skia's paragraph/glyph caches and per-entity style stores fill (it plateaus rather than leaking linearly).
- vizia has NO executor: cx.spawn is a raw std::thread plus a ContextProxy (Send, NOT Sync) whose emit posts an event through winit's user-event proxy. Every networked app re-invents the same ~30 lines — but there is also no executor MISMATCH to get wrong at runtime.

FRICTION:
- The framework is unusually silent about mistakes. An action only fires when the acted-on view is the HOVERED entity (cx.current == meta.target), so children must be .hoverable(false) — and inside the built-in VirtualTable the only reachable fix is a CSS rule (`.table-row, .table-cell { pointer-events: none; }`) against undocumented class names.
- Event coordinates (on_mouse_move, cx.bounds()) are PHYSICAL pixels while Pixels(..) is logical — a 2× display doubles every hand-computed overlay position until divided by cx.scale_factor().
- on_drop runs while WindowEvent::MouseUp is still propagating and only QUEUES the app event, so a root MouseUp handler that clears drag state inline makes every drop a silent no-op.
- Context::load_image takes &'static [u8] and is unreachable from an EventContext; ContextProxy::load_image only accepts ENCODED bytes, so both obvious entry points point away from the path that works for video.
- notify-rust from inside vizia's event dispatch ABORTS the process (NotificationHandle::drop spins the Cocoa run loop and re-enters winit's handler inside a non-unwinding block) — it must be sent from a background thread.

## floem

(Added 2026-08-03 with the three-framework expansion cohort. Sourced from
`apps/floem-*`; upstream ecosystem fields not researched in this pass are marked
`pending_upstream_research`.)

```yaml
framework: floem (lapce)
version_tested: "git-778bb5f2 — rev 778bb5f2aa08429e579ee2e6ac97e84fbf18b618 of lapce/floem `main` (2026-06-21), pinned identically in all 8 apps. The crates report themselves as 0.2.0 from git."
version_deviation: "SPEC.md requires the latest crates.io release pinned =x.y.z. Floem's latest crates.io release is 0.2.0 (2024-11) — 20 MONTHS STALE at measurement time, with a substantially different API (winit re-export, cosmic-text text stack, no typed event listeners). The maintainers direct users to `main`, and `main` CANNOT BE PUBLISHED because it depends on a forked winit (floem-winit, github.com/lapce/winit rev 133268de) and on understory_* crates, both via git. The git pin is therefore the maintainer-recommended path and the unpublishable-main situation is itself a headline ecosystem finding."
license: MIT (confirmed 2026-08-04: crates.io + cargo metadata)
paradigm: "Fine-grained reactive (floem_reactive: RwSignal/Memo/Effect, Leptos-lineage) over a retained view tree, with TYPED event listeners (listener::Click, DoubleClick, PointerMove, FileDragDrop, WindowCloseRequested, ThemeChanged) and typed custom events (TextInputEnter, SliderChanged). Paint closures inside `canvas` are signal-tracked, so custom drawing repaints reactively with zero cache/damage bookkeeping. There is NO executor; ExtSendTrigger/create_ext_action/update_signal_from_channel are first-class foreign-thread wakeup primitives."
windowing: "floem-winit 25.10.0 — a FORK of winit (github.com/lapce/winit rev 133268de), split into floem-winit-core/-common/-appkit, plus ui-events-floem-winit. Not upstream winit."
renderer: "floem_vger_renderer over floem-vger 0.3.2 (a fork of vger-rs, github.com/lapce/vger-rs rev 54ab8135) on wgpu 27.0.1, with floem_tiny_skia_renderer (tiny-skia 0.11.4 + softbuffer 0.4.8) as the software fallback; resvg 0.46 for SVG. A vello path is discussed upstream but is NOT in this rev's default dependency tree. HEADLINE DEFECT: vger's image path is a single colour ATLAS keyed by content hash, pack failures are dropped silently without cleanup and do not count toward the 70% self-heal threshold, so any image above roughly a third of the atlas dimension wedges image drawing permanently — a camera preview goes black after ~3 frames above ~320x180."
text_stack: "parley 0.7 + fontique 0.7 (system font discovery/fallback) + swash 0.2.10 + harfrust 0.3.2, on kurbo 0.13.1 / peniko 0.6.1. This is a WHOLE-STACK SWAP on main — the stale crates.io 0.2.0 used cosmic-text. Editor pane is the Lapce editor core (floem-editor-core + lapce-xi-rope 0.4.0), which gives grapheme-atomic caret motion and a real ClipboardCut/Copy/Paste/SelectAll command surface. macOS gap at this rev: fontique fails to resolve PingFang/Hiragino, so Han and kana render as PURE TOFU (Hangul, Devanagari, Thai and BiDi are fine), as do regional-indicator flag pairs and the ⌘/⇧ symbols."
layout: "taffy 0.9.2 (shared ecosystem crate) — with a load-bearing trap: taffy's default min_height:auto lets a scroll grow to min-content, which silently DISABLES VirtualStack virtualization (100k rows → 16 GiB RSS and no window; 11k lines → 1.9 GiB). One min_height(0) on the scroll's flex ancestors is the fix and nothing warns."
widget_set: "floem views plus understory_* crates (understory_virtual_list for VirtualStack, understory_focus, understory_box_tree, understory_index, understory_event_state). Present: Button, Label (+Label::derived), TextInput, Slider::new_ranged (a real range, unlike freya/vizia), scroll, dyn_stack/dyn_container, VirtualStack, canvas, img (ENCODED bytes only — no raw-RGBA view), draggable_with_config with an automatic framework-painted ghost and spring release, debounce_action. Missing: table/data grid, progress bar, window-capture API, interval timer (only one-shot exec_after). API CHURN: at this rev the free-function constructors used by ALL published documentation (v_stack, h_stack, button, label, static_label, text_input) are DEPRECATED in favour of struct constructors, and some in-source doc examples do not compile against the same rev."
a11y_status: "NONE. `deps-flat.txt` contains no accesskit at all — floem has no accessibility integration, where iced (stable), egui, xilem, slint, vizia and freya all ship or wire AccessKit. IME plumbing exists (ImePreedit/ImeCommit events, set_ime_allowed/set_ime_cursor_area, an editor preedit field) but was unexercised."
os_integration_summary: "The most complete shell integration of this three-framework cohort, and mostly undocumented outside the source: NATIVE MENUBAR built in (floem::Menu builds muda 0.17.2 menus with per-item action CLOSURES; set_window_menu installs them on NSApp — no MenuId bookkeeping, no event channel), native file DIALOGS built in (rfd 0.17.2 is a floem dependency; floem::open_file/save_as with the callback delivered on the UI thread via create_ext_action), typed FILE DROP (listener::FileDragDrop with paths: Rc<[PathBuf]>), live DARK MODE (dark_mode() style selector re-resolved on OS ThemeChanged), MULTI-WINDOW (new_window/close_window), close interception (WindowCloseRequested + cx.prevent_default(), and macOS AppConfig defaults exit_on_close:false), and DOCK-ICON REOPEN (AppEvent::Reopen / applicationShouldHandleReopen) which iced structurally cannot do. Missing: system tray, global hotkeys, notifications, image clipboard (Clipboard is text + file-list only), window screenshot."
platforms: pending_upstream_research
wasm: pending_upstream_research
mobile: pending_upstream_research
production_users: "Lapce is the parent project (floem lives under the lapce org and the editor core in the dependency tree is Lapce's); no independent production-user survey was done in this pass — pending_upstream_research"
backing: "Lapce-project crate (founded by dzhou121); 5 contributors >5 commits/12mo, led by jrmoulton (75); 28.8k all-time crates.io downloads, 4.2k stars (2026-08-04); crates.io release 21 months stale while the repo stays active"
build_ok: true      # all 8 apps build clean in release; locked rebuilds reproduce
launch_ok: true     # release binary alive >8 s with a visible window, killed cleanly, nothing on stdout/stderr
spec_gaps: "none for SPEC.md — every requirement maps onto stock views (TextInputEnter typed event for Enter-to-add, dyn_stack keyed diffing for rows, .scroll() for overflow)."
app_loc: 91
deps_total: 315     # unique crates; the default feature set compiles the vger GPU renderer, the tiny-skia software fallback, the full Lapce editor core and the parley/fontique text stack
clean_build_secs_rough: pending_new_cohort
binary_size_mb: pending_new_cohort   # floem-peek's release binary measured 19.9 MiB, but that app carries the camera/audio stack
docs_quality_1to5: pending_upstream_research
```

TOP FINDINGS:
- The version story is the finding: crates.io is 20 months stale, `main` is structurally unpublishable (forked winit + git-only understory crates), and the maintainer-recommended path is a git rev. Every consumer inherits the fork and the churn — including a deprecated constructor surface that all published docs still use.
- Floem forks aggressively at the bottom of the stack (floem-winit, floem-vger) while sharing at the middle (taffy, parley/fontique/swash/harfrust, wgpu, kurbo/peniko) — the inverse of gpui, which forks the renderer/windowing but shares layout.
- It is the only framework in the study with a built-in `debounce_action`, and its ExtSendTrigger/create_ext_action/update_signal_from_channel trio is the cleanest foreign-thread→UI bridge measured here, despite floem shipping no executor at all.
- Built-in drag-and-drop with an automatic ghost and spring release made the study's hardest interactive capability the easiest cell in two separate apps (~25-30 LoC).
- Two silent, catastrophic traps live in the shared layers: taffy's min-content default disables VirtualStack virtualization (16 GiB, no window), and vger's content-hash atlas wedges image drawing permanently above ~320x180.
- No AccessKit anywhere in the tree, and macOS Han/kana fallback is broken at this rev — two of the sharpest "missing layer" findings in the study.

FRICTION:
- Discovering what is built-in requires reading floem's source: menubar, dialogs, dock reopen and prevent_default-on-close are all undocumented at this rev.
- The muda version minefield: floem pins muda =0.17 and claims that instance's single global MenuEvent handler slot; tray-icon 0.24's muda 0.19 is a separate compiled instance whose slot happens to be free. Had the versions unified, floem's handler would silently swallow every tray-menu click. Two copies of muda ship in the binary.
- No raw-RGBA image view: floem's `img` takes encoded bytes only, so clipboard images must be PNG-encoded in memory, and raw drawing needs a direct dependency on floem's internal `floem_renderer` crate (the Img struct is not re-exported).
- No progress bar, no table widget, no window-capture API and no interval timer — the framework's own timer example hand-rolls a re-arming exec_after chain.
- Same `block v0.1.6` future-incompatibility warning as iced, here via floem's copypasta clipboard dependency.

## shared-infra (ecosystem map agent)

```yaml
# Duplication matrix (capability -> framework -> value), July 2026
# dioxus = webview desktop default; blitz noted where different
windowing:
  iced: winit
  egui: winit
  gpui: in-house (gpui_macos/gpui_windows/gpui_linux)
  tauri: tao (winit fork)
  xilem: winit
  slint: winit (+Qt, LinuxKMS backends)
  dioxus: tao (migrating to winit, dioxus#2706)
gpu_abstraction:
  iced: wgpu
  egui: wgpu (default since 0.32; glow opt-in)
  gpui: in-house Metal (macOS) / D3D (Win); wgpu (Linux, since Feb 2026)
  tauri: webview
  xilem: wgpu (via vello)
  slint: femtovg GL/wgpu; Skia; in-house software
  dioxus: webview (blitz: wgpu)
2d_renderer:
  iced: in-house (iced_wgpu + tiny-skia sw)
  egui: in-house (epaint)
  gpui: in-house (primitive shaders)
  tauri: webview
  xilem: vello
  slint: femtovg / Skia / in-house software
  dioxus: webview (blitz: vello)
text_shaping:
  iced: harfrust (via cosmic-text)
  egui: harfrust (in epaint, since 0.34)
  gpui: platform CoreText/DirectWrite; harfrust-via-cosmic-text (Linux)
  tauri: webview
  xilem: harfrust (via parley)
  slint: harfrust (via parley, since 1.14)
  dioxus: webview (blitz: harfrust via parley)
text_layout:
  iced: cosmic-text
  egui: in-house (galley; no BiDi, no color emoji)
  gpui: in-house (line_layout/line_wrapper)
  tauri: webview
  xilem: parley
  slint: parley
  dioxus: webview (blitz: parley)
font_fallback:
  iced: fontdb + cosmic-text custom (fontdb dormant since 2024-10)
  egui: in-house (bundled fonts)
  gpui: platform + font-kit fork; fontdb (Linux)
  tauri: webview
  xilem: fontique
  slint: fontique (replaced fontdb in 1.14)
  dioxus: webview (blitz: fontique)
widget_layout:
  iced: in-house (flex)
  egui: in-house (immediate)
  gpui: taffy (=0.10.1)
  tauri: webview CSS
  xilem: in-house (masonry box-constraints)
  slint: in-house DSL (+ experimental taffy FlexboxLayout)
  dioxus: webview CSS (blitz: taffy + Stylo)
widgets:
  iced: in-house
  egui: in-house
  gpui: in-house
  tauri: webview (HTML/JS)
  xilem: in-house (masonry)
  slint: in-house (DSL)
  dioxus: webview (HTML/JS)
a11y:
  iced: missing (draft PR #3111; PR #3281 closed unmerged 2026-03)
  egui: AccessKit (mandatory dep since 0.35)
  gpui: AccessKit (merged 2026-05-27, zed#56065, early stage)
  tauri: webview native tree
  xilem: AccessKit (non-optional; parley accesskit feature)
  slint: AccessKit (default feature)
  dioxus: webview native tree (blitz: AccessKit)
ime:
  iced: winit Ime (since 0.14, Dec 2025)
  egui: winit Ime (preedit visuals in 0.35)
  gpui: in-house platform (NSTextInputClient / zwp_text_input_v3; Windows rough)
  tauri: webview
  xilem: winit Ime
  slint: winit Ime + own Android bridge
  dioxus: webview
styling_theming:
  iced: in-house
  egui: in-house
  gpui: in-house (Tailwind-like)
  tauri: CSS
  xilem: in-house
  slint: in-house DSL
  dioxus: CSS (blitz: Stylo)
```

CONVERGENCE:
- wgpu won the GPU layer (1,272 rev-deps > winit's 1,225): Zed dropped blade for wgpu on Linux Feb 2026 (zed#46758, "de-facto standard"); eframe default flipped glow→wgpu (egui#5889); slint has femtovg-wgpu.
- harfrust (HarfBuzz org + Google Fonts) unified text shaping in <12 months: adopted by cosmic-text 0.15, parley 0.6, epaint 0.34; rustybuzz effectively legacy.
- parley/fontique absorbed Slint 1.14, floem (Mar 2026), Bevy 0.19 — all leaving cosmic-text/fontdb.
- AccessKit is the de-facto a11y layer: egui (mandatory), slint (default), xilem, vizia, blitz, bevy, GTK 4.18, gpui (May 2026).
- taffy crossed framework lines: gpui, blitz, floem, bevy_ui, Servo (CSS Grid), slint (experimental).

DIVERGENCE:
- Text LAYOUT still has 4 independent stacks (cosmic-text / parley / epaint galley / gpui line-layout) — System76 and Linebender both actively invest; egui's parley PR stalled on API-model mismatch.
- tao fork widening: frozen on pre-0.30 winit closure API while winit redesigns for 0.31; un-fork blocked on wry's WebKitGTK requirement, not goodwill.
- 6 2D renderers above wgpu (vello, epaint, iced_wgpu, femtovg, skia-safe, gpui shaders); vello still pre-1.0 "beta quality".
- Linux shell integration split on the GTK fault line: muda menubars impossible on winit windows, tray-icon needs parallel GTK thread, global-hotkey X11-only; 4 of 9 shell crates are tauri-apps with bus-factor ≈ 1 (amrbashir).
- winit itself in redesign limbo (0.31 beta 8+ months, notgull burnout) — risk at the most-depended-on layer.

OPPORTUNITIES:
1. Fund iced's AccessKit integration (draft PR #3111 has VoiceOver working) — finishes the ecosystem a11y map.
2. De-GTK Linux shell integration (StatusNotifier tray, DBusMenu, XDG GlobalShortcuts portal for winit) — also removes most remaining tao-fork rationale.
3. Retire dormant fontdb in favor of fontique (cosmic-text is the main holdout).
4. Sustainability grants for AccessKit (bus factor 2, STF funding ended mid-2024) and apple-codesign/rcodesign (bus factor 1, no release in 19 months).
5. Bless a framework-neutral packager: non-Tauri apps have no maintained bundle+sign+update chain; Velopack (1.x, official Rust crate) + rcodesign is nearest viable.

A11Y_VERDICT: Ecosystem-wide a11y via AccessKit is realistic and mostly already achieved at the abstraction layer — no competitor; 2026 added gpui and GTK 4.18 to egui/slint/xilem/bevy/vizia/blitz, leaving iced (draft PR) and floem the only major holdouts. But AccessKit only serializes the tree: per-toolkit semantic work remains substantial, no shipped web/canvas adapter, Linux AT-SPI substrate weak (Newton still prototype), project is bus-factor-2 with lapsed grant funding — risk is sustainability and integration depth, not architecture.
