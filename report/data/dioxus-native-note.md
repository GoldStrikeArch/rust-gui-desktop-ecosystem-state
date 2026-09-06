# Dioxus Native (Blitz) — should it be a separate column? (research note, 2026-08-30)

Prompted by a Zulip comment: *"it might make sense for Dioxus Desktop and
Dioxus Native to have separate entries … binary size, number of
dependencies, accessibility support, etc are entirely different between the
two backends … although you end up using a very old version of Dioxus Native
if you use the version from Dioxus 0.7."* Public sources only; nothing built.

## What it is

`dioxus-native` renders a Dioxus VirtualDom through **Blitz**, DioxusLabs'
webview-free HTML/CSS engine: Stylo (Servo/Firefox CSS) + Taffy (layout) +
Parley (text) + AccessKit, painted through the `anyrender` abstraction with
Vello-family or Skia backends, windowed by winit (`blitz-shell`). No JS
engine: anything an app does through `document::eval` does not exist.
Enabled with `dioxus = { features = ["native"] }`; dioxus 0.7.9's `launch`
picks the native renderer over desktop when both features are on
(`dioxus-0.7.9/src/launch.rs:21-33, 330-333`). `dx` knows it as
`--renderer native` (dioxus-cli 0.7.10 `platform.rs:181-230`, test harness
`dx build --desktop --renderer native`). Feature set of `dioxus-native`
0.7.10 (docs.rs): default `accessibility, clipboard, file_dialog,
hot-reload, html, net, svg, system-fonts`; optional `autofocus, incremental,
tracing, prelude`.

## The version gap the commenter means — confirmed

| you depend on | dioxus-native | blitz-dom / blitz-shell | stylo | taffy | parley | winit | renderer |
|---|---|---|---|---|---|---|---|
| `dioxus = "0.7"` (0.7.9/0.7.10, our pin) | 0.7.10 (2026-07-30) | **0.2.4 / 0.2.3** (Oct 2025 / Jan 2026) | 0.8 | 0.9 | **0.6** | 0.30 | anyrender_vello 0.6 (classic Vello) |
| `dioxus = "0.8.0-alpha.1"` (2026-07-31) | 0.8.0-alpha.1 | `=0.3.0-beta.1` (2026-07-10) | — | — | — | `=0.31.0-beta.2` | vello / vello_cpu / vello_hybrid / skia selectable |
| blitz `main` (workspace 0.3.0-beta.2, 2026-08-24) | git | 0.3.0-beta.2 | 0.20 | 0.14 | 0.11.1 | 0.31.0-beta.2 | anyrender 0.13, vello 0.14, vello_cpu 0.17, hybrid 0.10, skia 0.11 |

Sources: crates.io dependency API for `dioxus-native` 0.7.10 / 0.8.0-alpha.1
and `blitz-dom` 0.2.4, `blitz-shell` 0.2.3; blitz `Cargo.toml` on main.

So a "Dioxus Native" column built from stable Dioxus 0.7 measures a Blitz
that is ~10 months behind main and carries **parley 0.6 — the same parley
whose fontique 0.6 has the macOS Han-fallback bug we hit in xilem 0.4**
(fixed in fontique 0.8.0). The 0.8 alpha is on Blitz 0.3.0-beta.x but pins
winit 0.31 beta. The Blitz README also says the git version of Dioxus Native
depends on stable 0.7.x Dioxus from crates.io, i.e. the newest Native can be
used with today's Dioxus via a git dependency.

## Maturity — the project's own words disagree with each other

- Blitz README (main, Aug 2026): *"Blitz is currently in a **beta** state. It
  can already render many popular no-JS websites (Wikipedia, (Old) Reddit,
  etc), and is usable for making apps if you are an early adopter and
  willing to live on the bleeding edge."*
- blitz.is/about (undated): *"Blitz is currently in **alpha** … not ready for
  production usage. We are aiming to reach a broadly usable beta status by
  the end of 2025, with a production-ready release sometime in 2026."*
- Dioxus 0.7 release post: Blitz *"still very young"*, *"still considered a
  'work in progress'"*, *"not every CSS feature is supported yet"*.
- Dioxus 0.8.0-alpha.0 notes (2026-05-19): *"a huge quality upgrade to
  dioxus-native, with lots more rendering capabilities, incremental
  rendering, custom elements"*; syncs Blitz 0.3.0-alpha.4.
- Our July report/07 quoted "pre-alpha" — that wording is gone from the
  README; update to "beta (README) / alpha (website)".

## Why the two backends really are different entries

| | Dioxus Desktop (measured) | Dioxus Native (from manifests) |
|---|---|---|
| windowing | tao (winit fork) + wry (WebView2 / WKWebView / WebKitGTK) | winit 0.30 (0.7) / 0.31-beta (0.8) |
| rendering | OS webview process(es) | wgpu via Vello (0.7); Vello / Vello CPU / Vello Hybrid / Skia (0.8+) |
| text | webview | parley + fontique (0.6 on stable 0.7) |
| CSS | full browser engine | Stylo subset; "not every CSS feature" |
| JS | full | none — `document::eval` unavailable |
| a11y | browser tree | AccessKit via blitz-shell (`accessibility` default feature) |
| menus / tray / hotkey | `dioxus::desktop` hooks (muda, tray-icon, global-hotkey) | none in dioxus-native 0.7.10's manifest; blitz-shell main has no muda dependency |
| clipboard / dialogs | webview + rfd | arboard + rfd behind `clipboard` / `file_dialog` features |
| mobile | iOS/Android via wry | android-activity in shell; `anyrender_vello_cpu` for the iOS simulator in 0.7.10 |
| binary / RSS shape | 5–6.5 MB exe + helper processes (Windows: 6 msedgewebview2.exe) | single process, wgpu + Vello + Stylo statically linked — expect a much larger binary and no helpers |

Binary size, dependency count, RSS, a11y path, shell integration and text
stack differ on every axis the study measures. The commenter is right.

## Feasibility with our corpus (no build yet)

- 6 of our 8 dioxus apps use no `document::eval` and no desktop-only API
  (`dioxus-app/-babel/-board/-dash/-fetch/-grid`); they are candidates for a
  `features = ["native"]` rebuild with zero source changes, which would give
  build time, binary size, dep count, RSS and the Babel text screenshots for
  a Native column.
- `dioxus-peek` (12 `eval` sites — camera/mic through browser APIs) and
  `dioxus-tray` (tray/hotkey/menubar through `dioxus::desktop`) cannot run
  on Native as written; they would be *not-achievable* or need a rewrite.
- Known risks for the stable-0.7 path: parley/fontique 0.6 on macOS 26
  (CJK tofu, same as xilem 0.4); Blitz CSS coverage for our stylesheets
  (grid, transitions, `position: sticky`); no HTML5 drag-and-drop → the
  kanban's DnD needs Rust pointer events.

## Recommendation

Add **Dioxus Native as an eleventh column, measured on two pins**: stable
`dioxus 0.7.x` + `native` (what a user gets today) and, if it builds,
`dioxus 0.8.0-alpha.x` (Blitz 0.3.0-beta). Keep "Dioxus" = Desktop/webview
everywhere else, and rename the existing rows/cards "Dioxus (desktop)".
Start with the six eval-free apps. Label it clearly as beta/alpha per the
project's own statements.
