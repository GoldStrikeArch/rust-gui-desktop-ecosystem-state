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
