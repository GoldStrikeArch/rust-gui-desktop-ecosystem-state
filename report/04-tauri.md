# Tauri (v2) — Deep Dive

*RCN "Cross-Platform GUI Desktop Apps" initiative — framework report 04.*
*Researched July 2026. Version tested: tauri 2.11.5 (crates.io, 2026-07-01) on macOS 26.5.2 / Apple M4 Pro / rustc 1.96.1.*

---

## 1. Architecture & paradigm

Tauri is not a widget toolkit: it is a **Rust application shell around the
operating system's webview**. The UI is an ordinary web frontend (any JS
framework or none); the application core is a Rust binary; the two talk over
an IPC bridge.

**Process model.** Tauri uses a multi-process architecture: a **Core
process** (your Rust binary — entry point, full OS access, window/tray/menu
management, all IPC routing) and **webview processes** that render the UI
using a browser engine. The default configuration uses the OS-provided engine.
The official docs are explicit that the Core should own global state and
business logic, following the principle
of least privilege: "The less access we give them \[frontend components\],
the less harm they can do if they get compromised" — logic is deferred to
Core, not the frontend ([process model docs](https://v2.tauri.app/concept/process-model/)).
Because those default engines are dynamically provided rather than bundled,
binaries stay small (§3). Windows is the important exception to the absolute
version of this claim: Tauri's installer can download, embed an offline
installer for, or bundle a fixed WebView2 runtime when deployment requires it
([WebView2 installation options](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options)).

**The webview layer (wry).** The [wry](https://github.com/tauri-apps/wry)
crate (v0.55.1, 2026-05-04, [crates.io](https://crates.io/crates/wry))
abstracts one system webview per platform:

| OS | Engine wrapped by wry |
|---|---|
| Windows | WebView2 (Chromium, Edge-distributed) |
| macOS / iOS | WKWebView (WebKit/Safari) |
| Linux | WebKitGTK |
| Android | Android System WebView |

(Source: [wry README](https://github.com/tauri-apps/wry).)

**IPC bridge.** Three mechanisms
([IPC concept](https://v2.tauri.app/concept/inter-process-communication/),
[calling Rust](https://v2.tauri.app/develop/calling-rust/),
[calling the frontend](https://v2.tauri.app/develop/calling-frontend/)):

- **Commands**: `#[tauri::command]` Rust functions invoked from JS via
  `invoke()`, in a "JSON-RPC-like" protocol — args/returns must be
  serde-serializable to JSON; errors map to rejected promises; async
  commands run on separate tasks.
- **Events**: fire-and-forget pub/sub in both directions (`emit`/`listen`);
  the docs warn payloads are always JSON strings, "not suitable for bigger
  messages".
- **Raw payloads & channels** (the v2 escape hatches from JSON overhead):
  commands can accept `ArrayBuffer`/`Uint8Array` request bodies and return
  raw bytes via `tauri::ipc::Response`, and `tauri::ipc::Channel` provides
  ordered, high-throughput Rust→frontend streaming (used internally for
  download progress, child-process output, WebSocket messages). Rust can
  also `eval()` JavaScript directly in a webview.

**Where app logic lives.** Officially: in Rust. In practice Tauri is
agnostic — many teams port an existing SPA and keep 95% of logic in JS,
using Rust only for OS access. The framework's security model, however, is
designed around Rust-owned logic, and the mini-app for this study (§10)
follows that guidance: all task state lives in a `Mutex<Vec<String>>` in the
Core process and the frontend is a thin view over three commands.

**Paradigm summary:** retained-mode DOM (the browser's), any web UI pattern
you like, plus a message-passing boundary in the middle of your app. It is
the only framework in this study where "which reactive model?" is entirely
your choice.

## 2. Full stack table

| Layer | What Tauri uses | Shared or in-house? |
|---|---|---|
| Windowing | [tao](https://github.com/tauri-apps/tao) v0.35.3 | **In-house fork** of winit (tauri-apps org) |
| Rendering | System webview via [wry](https://github.com/tauri-apps/wry) v0.55.1 by default; Windows installers may instead include fixed/offline WebView2 | **Shared with the OS by default**; optionally bundled on Windows |
| Text & layout | The webview's browser engine (WebKit / Chromium text stacks, CSS layout) | Shared with the OS browser engine |
| Widget set | None — HTML/CSS/DOM, or any JS framework | Shared with the entire web ecosystem |
| Menus | [muda](https://github.com/tauri-apps/muda) v0.19.3 (native menus) | In-house (tauri-apps), reused by others |
| Tray | [tray-icon](https://github.com/tauri-apps/tray-icon) | In-house (tauri-apps), reused by others (e.g. by winit users) |
| Accessibility | The webview's native accessibility bridge (HTML → platform a11y tree) | Shared with the OS browser engine |
| IPC / runtime | tauri, tauri-runtime-wry, tauri-plugin ecosystem | In-house |

(Crate versions observed in this study's lockfile, July 2026.)

**The tao fork.** tao is "a fork of winit which replaces Linux's port to
Gtk" ([tao README](https://github.com/tauri-apps/tao)) — Tauri needs GTK on
Linux because WebKitGTK requires it, and winit at fork time "lacked a few
general features a GUI application should have": macOS menu bar, tray, the
GTK backend ([wry discussion #1014](https://github.com/tauri-apps/wry/discussions/1014)).
Menus/tray have since been extracted into muda and tray-icon (usable with
plain winit too). The README states the long-term intent: "In the future,
we want to make these features more modular as separate crates. So we can
switch back to winit and also benefit the whole community." As of July 2026
that migration has **not** happened: tao is a maintained, divergent fork
(0.35.3, 2026-05-23, [crates.io](https://crates.io/crates/tao)) that
monitors upstream via a standing "differences vs winit" tracking issue
([tao#470](https://github.com/tauri-apps/tao/issues/470)) rather than
mechanically tracking it; a `winit-gtk` experiment exists
([repo](https://github.com/tauri-apps/winit-gtk)) but the maintainers judged
a full switch "difficult... if we want to keep supporting Tauri"
([#1014](https://github.com/tauri-apps/wry/discussions/1014)), and the v3
milestone targets a GTK4 migration, not a winit switch
([milestone #5](https://github.com/tauri-apps/tauri/milestone/5)).
Meanwhile Dioxus still ships Tao/Wry in its 0.7.9 desktop stack. An open
proposal discusses moving it to winit
([dioxus#2706](https://github.com/DioxusLabs/dioxus/issues/2706)), but that
migration has not happened. The winit/Tao split therefore remains active.

Everything above the windowing layer is the *opposite* of in-house: Tauri
re-implements almost nothing. Rendering, text shaping, layout, scrolling,
IME, and the widget vocabulary are the platform browser engine's. That is
simultaneously its superpower (battle-tested text/layout and a strong
browser-derived accessibility baseline) and its core weakness (§3).

## 3. The webview tradeoff, quantified

**Bundle size.** With Tauri's default system-webview configuration, no browser
engine ships in the app, so binaries/installers are dramatically smaller than
Electron's (which bundles Chromium + Node). A Windows build configured with a
fixed or offline WebView2 runtime gives up part of that advantage. This study's
release binary — with the whole frontend embedded — is **8.0 MiB** as a single
file. Measured third-party
comparisons agree on the order of magnitude:
[Hopp's 2025 macOS benchmark](https://www.gethopp.app/blog/tauri-vs-electron)
got an 8.6 MB Tauri bundle vs 244 MB Electron;
[Levminer's production app on Windows](https://www.levminer.com/blog/tauri-vs-electron)
got a ~2.5 MB installer vs ~85 MB. Those are each source-reported `MB`
figures and were not normalized to MiB or a common bundle definition. They
support the qualitative engine-externalization point, not a universal size
range.

**Memory reality.** The "tiny memory" claim needs honesty: webview helper
processes are not free. The controlled 30-second dashboard sample on this
machine measured **211 MiB maximum RSS for Tauri's process tree, including
three attributed WebKit helpers**. An earlier ~115 MiB observation counted
only the app process and is not comparable. Third-party comparisons find a
smaller advantage over Electron than the bundle-size delta:
[Hopp](https://www.gethopp.app/blog/tauri-vs-electron) measured ~172 MB
(Tauri) vs ~409 MB (Electron) with six windows on macOS, with "negligible"
startup difference (and Tauri's Rust build ~5× slower than the Electron
build); [Levminer](https://www.levminer.com/blog/tauri-vs-electron)
measured ~80 vs ~120 MB idle on Windows. Numbers vary strongly by platform
(WebView2 spawns several Chromium helper processes). Note that Tauri's own
official benchmarks page was removed in the v2 docs era (the old
`tauri.app/v1/references/benchmarks` URL now 404s; raw data lingers at
[benchmark_results](https://github.com/tauri-apps/benchmark_results), and
the old memory methodology was itself disputed —
[tauri#5889](https://github.com/tauri-apps/tauri/issues/5889)) — treat all
specific reported memory numbers as platform-dependent, small-sample
measurements.

**Platform inconsistency — the real price.** One codebase, three browser
engines, three release schedules:

- **Windows**: WebView2 is evergreen — "included as part of the Windows 11
  operating system", present on "the vast majority of Windows 10 devices",
  auto-updating on the Edge Stable cadence
  ([Microsoft distribution docs](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)).
  Tauri's docs: "you are guaranteed a relatively recent chromium build on
  all Windows targets" ([webview versions](https://v2.tauri.app/reference/webview-versions/)).
  Modern features, but the engine version varies per machine and updates
  outside your control.
- **macOS/iOS**: WKWebView is "updated with the regular OS updates" — i.e.
  pinned to the installed macOS/Safari version
  ([webview versions](https://v2.tauri.app/reference/webview-versions/));
  you inherit Safari's CSS/JS feature lag and bugs, per OS version, and
  unsupported macOS versions stop receiving WebKit fixes entirely.
- **Linux**: WebKitGTK is the weak leg. Tauri's own docs concede "it is
  very hard to compile accurate information about WebKitGTK on the various
  distros" ([webview versions](https://v2.tauri.app/reference/webview-versions/)),
  and there is a dedicated official
  ["Linux graphics issues" debugging page](https://v2.tauri.app/develop/debug/linux-graphics/):
  blank/white windows on NVIDIA from the DMABUF renderer, worked around
  with `WEBKIT_DISABLE_DMABUF_RENDERER=1` /
  `WEBKIT_DISABLE_COMPOSITING_MODE=1`
  ([tauri#9304](https://github.com/tauri-apps/tauri/issues/9304),
  [tauri#9394](https://github.com/tauri-apps/tauri/issues/9394)), plus
  perf oddities like
  [#10566 "poor performance until Web Inspector is opened"](https://github.com/tauri-apps/tauri/issues/10566)
  and generally worse scrolling/animation than WebView2/WKWebView.

So "cross-platform" in Tauri means *cross-browser-compatibility work
returns*: you test your CSS/JS on three engines, like the web circa jQuery,
plus per-distro Linux debugging.

**Verso integration status — archived/inactive; Servo itself remains
active.** To address engine fragmentation, the Tauri and Servo communities collaborated
(NLnet/NGI-funded: [Tauri-Servo](https://nlnet.nl/project/Tauri-Servo/),
[Verso](https://nlnet.nl/project/Verso/),
[Verso-WebView](https://nlnet.nl/project/Verso-WebView/)) on **Verso**, a
Servo-based embeddable webview, with an experimental `tauri-runtime-verso`
drop-in replacement for `tauri-runtime-wry` announced 2025-03-17
([blog: Experimental Tauri Verso Integration](https://v2.tauri.app/blog/tauri-verso-integration/))
— at announcement already able to run the CLI, React, official plugins and
window controls, but "not as feature rich and powerful as the current
backends used by Tauri in production yet". Since then the effort has
stalled: the **versotile-org/verso GitHub repo was archived on 2025-10-08**
(maintainers couldn't keep pace with Servo's rapid changes on limited
funding; major contributions were upstreamed into Servo —
[repo](https://github.com/versotile-org/verso)), and
[tauri-runtime-verso](https://github.com/versotile-org/tauri-runtime-verso)
has had no pushes since 2025-10 and was never published to crates.io. wry
retains feature-flag scaffolding "in preparation of other ports like cef
and servo" ([docs.rs/wry](https://docs.rs/wry/latest/wry/)), so the door is
open, but as of July 2026 the tested and documented production Tauri path is
the system webview. This says nothing about the health of
[Servo itself](https://github.com/servo/servo), which remains active; it is
the Verso/Tauri integration that is archived or inactive.

## 4. Security & capability model

Tauri v2's security story is the most deliberate in this study, shaped by
published independent security audits and NLnet-funded hardening
([security](https://v2.tauri.app/security/),
[NLnet: Tauri](https://nlnet.nl/project/Tauri/)). The reviewed evidence
supports major-version/security audits; it does **not** establish a fresh
external audit for every minor release. The stance is
deny-by-default: "any code executed in the WebView has only access to
exposed system resources via the well-defined IPC layer."

- **Permissions** ([docs](https://v2.tauri.app/security/permissions/)):
  every core/plugin command is gated by named permissions
  (`fs:allow-read`, `core:default`, …) with allow/deny command lists and
  **scopes** (e.g. allow `$HOME/*` but deny `$HOME/secret`), groupable
  into permission sets.
- **Capabilities** ([docs](https://v2.tauri.app/security/capabilities/)):
  JSON/TOML files in `capabilities/` grant permission sets to specific
  **windows/webviews**, optionally filtered per platform. `tauri-build`
  generates JSON schemas at build time so they're statically validated.
  Remote origins get **no IPC at all** unless a capability's `remote.urls`
  field opts them in (with a documented caveat that Linux/Android can't
  distinguish an embedded iframe from the window itself). App-defined
  commands (your own Rust) are not permission-gated by default.
- **CSP** ([docs](https://v2.tauri.app/security/csp/)): you set the policy
  in `tauri.conf.json`; "at compile time, Tauri appends its nonces and
  hashes to the relevant CSP attributes" for bundled scripts/styles, and
  assets are served over a custom protocol rather than `file://`.
- **Isolation pattern**
  ([docs](https://v2.tauri.app/concept/inter-process-communication/isolation/)):
  optional interposition of a sandboxed iframe that all IPC is forced
  through and can inspect/reject/modify every message before it reaches
  Rust, with payloads AES-GCM-encrypted under keys regenerated each run —
  a defense-in-depth layer against compromised/injected frontend code
  (e.g. supply-chain-poisoned JS deps).

**Versus Electron:** Electron's defaults have improved (contextIsolation
default since v12, sandbox since v20, nodeIntegration off since v5), but
its official posture is still a ~20-item
[security checklist](https://www.electronjs.org/docs/latest/tutorial/security)
the developer must apply to lock down a permissive Chromium+Node runtime,
with the OS-API surface defined by hand-rolled preload/IPC code. Tauri
inverts this: the frontend starts with *nothing*, and everything it can
reach is enumerated in reviewable, schema-checked capability files. The
honest flip side: Chromium's renderer sandbox is deeper than some system
webviews' (WebKitGTK especially), Electron controls its engine patch level
while Tauri inherits whatever the OS ships, and Tauri's model cannot fix
engine-level RCE. Net: stronger application-layer defaults; engine-layer
security delegated to the OS vendor, for better (auto-patching) and worse
(you can't ship the fix yourself).

## 5. Accessibility

Tauri inherits a **strong browser-derived accessibility baseline**: system
webviews can expose semantic HTML (`<button>`, `<input>`, labels and
appropriate `aria-*`) through platform accessibility APIs. That baseline is
conditional on authors using correct HTML/ARIA and testing the result; Tauri
does not make arbitrary DOM, canvas content, focus order or keyboard behavior
accessible automatically. This study's mini-app uses semantic HTML, an
`aria-live="polite"` counter and per-row labels, but no complete
screen-reader/WCAG evaluation was retained.

What's still **on the app author**:

- All of WCAG discipline — div-soup with click handlers is exactly as
  inaccessible in Tauri as on the web; the framework does nothing to stop
  you.
- Chrome outside the DOM: native menus, tray, dialogs are the OS's (fine),
  but custom window decorations, multi-webview layouts, and JS-drawn canvas
  UIs get no automatic semantics.
- Platform variance: the quality of the a11y bridge differs per engine,
  and Linux has real gaps — e.g. at-spi accessibility-bus failures in
  wry/WebKitGTK ([tauri#4315](https://github.com/tauri-apps/tauri/issues/4315)).

Notably, Tauri has **no dedicated accessibility documentation**; the
project's own tracking issue
([tauri#207](https://github.com/tauri-apps/tauri/issues/207), open since
Dec 2019) acknowledges that where Electron gets Chromium's a11y everywhere,
Tauri must ensure three different system webviews each expose their
built-in accessibility features properly.

Contrast with the Rust-native renderers in this study: they generally need an
AccessKit adapter and explicit semantic mapping. Tauri delegates that adapter
role to each system webview, gaining the browser baseline while inheriting
engine/platform variance and retaining responsibility for semantic authoring.

## 6. OS shell integration

This is one of Tauri's main strengths. **Menus and tray are core**
features in v2: menus via [muda](https://github.com/tauri-apps/muda)
(0.19.3, June 2026 — non-optional on desktop), tray via
[tray-icon](https://github.com/tauri-apps/tray-icon) (0.24.1, June 2026)
behind the `tray-icon` cargo feature
([system tray guide](https://v2.tauri.app/learn/system-tray/)). Everything
else lives in the official
[plugins-workspace](https://github.com/tauri-apps/plugins-workspace) — ~30
plugins, one `tauri-plugin-*` crate + matching JS package each,
individually permission-gated, actively maintained (releases through
May–June 2026; 1,700+ releases total):

- **Shell/OS**: `autostart`, `single-instance`, `window-state`, `cli`,
  `process`, `shell`, `opener`, `os`, `positioner`, `localhost`
- **User-facing**: `notification`, `dialog`, `clipboard-manager`,
  `global-shortcut`
- **App lifecycle**: `updater` (signed updates — desktop-only, mobile
  updates go through the stores), `deep-link`, `store`, `log`,
  `persisted-scope`
- **IO & device**: `fs`, `http`, `websocket`, `upload`, `sql`,
  `stronghold`, `biometric`, `nfc`, `barcode-scanner`, `geolocation`,
  `haptics` (the last five mobile-oriented)

Each plugin page on [v2.tauri.app/plugin](https://v2.tauri.app/plugin/)
carries a per-platform support table — read it, because desktop/mobile
parity varies (updater, autostart, global-shortcut, single-instance,
window-state are desktop-only).

**Distribution and updater signatures are separate.** The Tauri bundler can
use a configured platform signing identity and can drive Apple's notarization
workflow ([macOS signing guide](https://v2.tauri.app/distribute/sign/macos/)).
The updater plugin uses its own update-artifact signature/key mechanism. In
this repository's credential-free packaging run, no Developer ID identity was
configured, so the app received only a local ad-hoc seal; that run does not
show that Tauri lacks distribution-signing support.

**Multi-window & multiwebview.** Multiple windows are first-class
(`WebviewWindow`, per-window capabilities). **Multiple webviews in one
window** shipped with v2 behind the **`unstable` cargo feature** "while we
review the API design" and remain feature-gated in 2.11.5
([tauri Cargo.toml](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/Cargo.toml),
original request [#2975](https://github.com/tauri-apps/tauri/issues/2975)),
with open positioning/resize bugs
([#10420](https://github.com/tauri-apps/tauri/issues/10420),
[#10131](https://github.com/tauri-apps/tauri/issues/10131)).

**Mobile.** iOS and Android are **stable since v2.0** (2024-10-02,
[announcement](https://v2.tauri.app/blog/tauri-20/)) — same codebase,
`tauri [ios|android] init/dev/build`, plugins extended in Swift/Kotlin. The
team itself said at 2.0: "We are not completely happy about the developer
experience at the moment but are actively improving" — and improvements
have kept coming (2.11.0 added mobile file associations and multi-window
on mobile via Android activity embedding / iOS scenes,
[release notes](https://github.com/tauri-apps/tauri/releases/tag/tauri-v2.11.0)).
Real apps ship on both stores, but DX is younger than desktop, several
plugins are desktop-only, and mobile is the one place the plain-cargo
workflow (§10) stops being possible — the mobile toolchain requires
tauri-cli.

## 7. License, governance, funding, cadence, users

- **License**: tauri and wry are dual **MIT OR Apache-2.0**
  ([crates.io/tauri](https://crates.io/crates/tauri)); **tao is
  Apache-2.0 only** (winit-fork heritage,
  [crates.io/tao](https://crates.io/crates/tao)) — worth knowing for
  license-inventory purposes.
- **Governance**: Tauri is a **programme within the Commons Conservancy**
  (a Dutch public-benefit foundation) — verified at
  [tauri.app/about/governance](https://tauri.app/about/governance/) and
  [commonsconservancy.org/programmes](https://commonsconservancy.org/programmes/)
  (statutes: [DRACC 0035](https://commonsconservancy.org/dracc/0035/)).
  Structure: Working Group → elected **Tauri Board** (3–7 directors,
  staggered 2-year terms, yearly elections —
  [2026 election post](https://v2.tauri.app/blog/tauri-board-elections-2026/))
  → Domains with elected leads. This is among the most formal governance
  structures in the seven-framework sample.
- **Funding**: [Open Collective](https://opencollective.com/tauri)
  (~$123k raised lifetime, ~$31k/yr budget; sponsors incl. GitHub, 1Password);
  NLnet/NGI grants (security audit work, Servo/Verso); and **CrabNebula**, a
  company founded by Tauri's co-founders (Daniel Thompson-Yvetot, Lucas
  Nogueira) selling Tauri services (Cloud distribution/updates, DevTools,
  audits) — "we built Tauri" ([crabnebula.dev](https://crabnebula.dev/)).
  Several core maintainers are CrabNebula employees: the project's health is
  partially coupled to one company, but the Commons Conservancy owns the
  assets.
- **Cadence & contributor concentration**: 2.0 stable 2024-10-02; minors
  roughly every 1–2 months through 2025 (2.1–2.9), ~quarterly in 2026
  (2.10 Feb, 2.11 Apr),
  patches within days (2.11.5 on 2026-07-01)
  ([releases](https://github.com/tauri-apps/tauri/releases),
  [crates.io versions](https://crates.io/api/v1/crates/tauri/versions)).
  ~109k GitHub stars, ~560 contributors, working group of roughly 45–50
  with a handful of highly active maintainers. **Tauri 3.0** is a draft
  milestone (~26% complete, no due date,
  [milestone #5](https://github.com/tauri-apps/tauri/milestone/5)); its
  headline item is the forced **GTK4 / WebKitGTK 6.0 migration on Linux**
  (GTK3 being unmaintained) — a breaking change to watch.
- **Verified production users** (checked July 2026): **GitButler** ("a
  Tauri-based application", Svelte UI + Rust backend,
  [repo](https://github.com/gitbutlerapp/gitbutler)); **Hoppscotch
  Desktop** ("built with Tauri V2",
  [repo](https://github.com/hoppscotch/hoppscotch/tree/main/packages/hoppscotch-desktop));
  **Rivet** by Ironclad
  ([src-tauri in repo](https://github.com/Ironclad/rivet/tree/main/packages/app));
  plus Spacedrive, pgMagic, Payload, Cap, Jan, and Modrinth App via the
  official [awesome-tauri](https://github.com/tauri-apps/awesome-tauri)
  showcase. **Debunked/unverified claims to avoid repeating**: OpenAI's
  ChatGPT desktop app is *not* verifiably Tauri (the awesome-tauri
  "ChatGPT" entries are third-party wrappers). A third-party binary
  [teardown](https://www.dbreunig.com/2026/02/21/why-is-claude-an-electron-app.html)
  reports that Anthropic's Claude Desktop is Electron; this audit did not find
  an Anthropic primary source confirming that implementation detail.
  This audit found no primary evidence tying Zoom, Xmind, or cal.com to Tauri.

## 8. When Tauri is the wrong choice

- **Heavy canvas/custom rendering**: games, DAWs, CAD, 60fps data-viz.
  You'd be doing WebGL/WebGPU inside three different browser engines with
  IPC between you and your data. A native-rendering framework (or wgpu
  directly) fits better.
- **Pixel-identical, offline-deterministic UI**: the default system-webview
  configuration does not pin the rendering engine — the OS updates it under
  you or holds it back. Windows can instead bundle a fixed WebView2 runtime,
  at a substantial size and update-management cost; the system WKWebView and
  WebKitGTK paths still vary with the installed platform. Kiosks, regulated
  UIs, and long-lived screenshot-tested suites therefore need explicit engine
  and update policy.
- **Webview-hostile environments**: minimal Linux servers/containers
  without GTK/WebKitGTK, old Windows without WebView2 and no installer
  rights, locked-down enterprise images.
- **Teams with zero web stack**: you are buying HTML/CSS/JS/browser-quirks
  expertise as a hard dependency; a pure-Rust team may prefer a pure-Rust
  toolkit.
- **Latency-critical UI over large data**: every frontend↔core interaction
  is serialized IPC; raw payloads/channels mitigate but don't eliminate the
  boundary.

## 9. Docs & learning resources

The [tauri.app](https://tauri.app/start/) v2 docs are among the best in the
Rust GUI space: structured Start/Concepts/Security/Develop/Distribute/Learn
tracks, an unusually thorough **Security** section (ten dedicated
sub-pages: permissions, capabilities, scopes, CSP, runtime authority,
lifecycle threats…), per-plugin pages with permission tables and platform
matrices, a v1→v2
[migration guide](https://tauri.app/start/migrate/from-tauri-1/), Rust API
on [docs.rs](https://docs.rs/tauri) plus a separate JS API reference,
~21.7k-member Discord and the 7.9k-star
[awesome-tauri](https://github.com/tauri-apps/awesome-tauri) index.
Weak spots: both documented setup paths assume the CLI — even "Manual
Setup" means `cargo install tauri-cli` + `cargo tauri init`
([create-project](https://v2.tauri.app/start/create-project/)); the
CLI-free, npm-free setup used in this study (§10) is undocumented. Some
plugin docs lag their crates, and architecture-level guidance (how much
logic to put in Rust vs JS) is thinner than the API-level docs.
**Rating: 4/5.**

## 10. Mini-app & friction log

`apps/tauri-app/` implements the Tasks spec fully — no functional gaps
(`GAPS.md`). Setup is deliberately **plain cargo, no Node.js/npm, no
tauri-cli**: `tauri`+`tauri-build` pinned (=2.11.5/=2.6.3), hand-written
`tauri.conf.json` (`frontendDist: "ui"`), a vanilla HTML/CSS/JS frontend
using `window.__TAURI__.core.invoke` (enabled by `app.withGlobalTauri`), one
hand-written capability file, and script-generated icons. Task state lives
in Rust behind three `#[tauri::command]`s to exercise the IPC bridge — see
GAPS.md for why that differs from the other mini-apps.

**Measurements (M4 Pro, rustc 1.96.1):**

| Metric | Value |
|---|---|
| Canonical iteration-1 clean `cargo build --release` | **36.34 s → 36 s** |
| Release binary (assets embedded) | **8.0 MiB** |
| Unique crate names (normal dependency edges) | **204** |
| Production source | **208 LoC** (60 Rust/build.rs + 148 web) |
| Config-inclusive physical total | **248 LoC currently** (208 source + 40 JSON); the pre-packaging config was one line shorter |
| Launch evidence | process survived the timed check; a later independent inspection observed the titled on-screen window; no retained end-to-end interaction harness |

Enabling packaging later changed `tauri.conf.json`, forced a re-link, and made
that packaging invocation take 96 s. It is a separate packaging run, not the
canonical clean-build benchmark above. The earlier `271` count was flattened
name-version/tree output, not unique crate names.

**Friction encountered:**

1. **The no-Node path is real but undocumented.** Everything official
   assumes `create-tauri-app`/CLI + npm. Making plain `cargo build` work
   required knowing that `frontendDist` is resolved relative to
   `tauri.conf.json`, that `withGlobalTauri` replaces the npm
   `@tauri-apps/api` package, and that capabilities can be hand-written
   (schemas appear in `gen/schemas/` only *after* the first build, so
   editor validation bootstraps awkwardly).
2. **Explicitly configured icons are validated even when bundling is off.**
   During iteration 1, this app had `bundle.icon` paths while
   `bundle.active: false`; those paths therefore had to exist, and Tauri
   required the configured PNGs to be RGBA. That is not a universal icon
   requirement for every non-bundling Tauri app. The current config enables
   bundling for the later packaging experiment.
3. **Config indirection.** Window properties, CSP, and IPC exposure live in
   JSON, not code; error messages for config mistakes come from
   tauri-build/schema validation at build time, which is a slower loop than
   rustc errors.
4. **Two-language mental model.** Even this 208-line production-source app
   plus 40 lines of config spans Rust,
   JSON config, HTML, CSS, and JS with an async serialization boundary in
   the middle — per-interaction round-trips (`invoke` → serde → render) are
   boilerplate other frameworks in this study don't have.
5. **Browser primitives reduce implementation work.** The canonical clean
   build was 36 s with 204 unique crate names. Text input, scrolling, focus
   and IME used browser primitives; accessibility still depends on semantic
   HTML/ARIA and assistive-technology testing.
