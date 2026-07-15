# iced — Deep Dive (RCN Cross-Platform GUI Desktop Apps, July 2026)

Core ecosystem research was collected on **2026-07-07**; benchmark, shell,
text, and factual reconciliation continued through **2026-07-09**. Local
execution claims refer only to macOS 26.5.2 on the Apple M4 Pro reference
machine. Other-platform claims below are source/API-path verified, not locally
executed.

**Version examined: iced 0.14.0** (latest stable on crates.io, released 2025-12-07;
subcrates have since received patches — our lockfile resolved `iced_widget 0.14.2`).
Source: [crates.io/crates/iced](https://crates.io/crates/iced) ·
[github.com/iced-rs/iced](https://github.com/iced-rs/iced)

Mini-app: [`apps/iced-app/`](../apps/iced-app/) — compiled on the reference
machine (M4 Pro, macOS, rustc 1.96.1); its process survived the launch interval
and an on-screen window was observed.

---

## 1. Architecture & paradigm

iced is explicitly "A cross-platform GUI library for Rust, inspired by Elm"
([README](https://github.com/iced-rs/iced/blob/master/README.md)). It uses an
**Elm-inspired declarative architecture**: application state is retained, the
view function rebuilds a widget-tree description, and iced diffs that
description while preserving the widgets' retained runtime state:

- **State (Model)** — a plain struct owned by the runtime.
- **Message** — a user-defined enum; every interaction produces one.
- **update(&mut State, Message)** — the only place state mutates; may return an
  `iced::Task` for async work.
- **view(&State) -> Element<Message>** — a pure function that rebuilds the whole
  widget tree; iced diffs it against the previous tree internally.

Since 0.13 the entry point is a builder: in 0.14 it is
[`iced::application(boot, update, view)`](https://docs.rs/iced/0.14.0/iced/fn.application.html)
returning an [`Application`](https://docs.rs/iced/0.14.0/iced/application/struct.Application.html)
with `.title()`, `.window_size()`, `.subscription()`, `.theme()`, `.run()`.
There is also `iced::daemon` for headless/multi-window programs. Async side
effects use `Task<Message>` (commands) and `Subscription<Message>` (event
streams); the default executor is an in-house thread pool (`thread-pool`
default feature), with optional `tokio`/`smol`
([workspace Cargo.toml @0.14.0](https://github.com/iced-rs/iced/blob/0.14.0/Cargo.toml)).

**Runtime layering** (repo directories at tag 0.14.0 — `core/`, `runtime/`,
`winit/`, `wgpu/`, `tiny_skia/`, `renderer/`, `widget/`, `program/`):
`iced_core` (types, layout, widget trait) → `iced_runtime` (state machine,
tasks) → `iced_widget` (built-in widgets) → `iced_renderer` (backend picker) →
`iced_wgpu` / `iced_tiny_skia` (paint) → `iced_winit` (the "shell" that owns
the winit event loop). Everything is swappable in theory — iced is "a renderer-
agnostic GUI runtime" — but in practice only the wgpu and tiny-skia backends
exist upstream.

**Widget model:** widgets implement the `Widget` trait in `iced_core`
(layout / draw / on_event / operate); end users compose them with helper
functions and the `column![] / row![]` macros. Custom widgets are possible via
the `advanced` feature. **Styling** is closure-based per-widget (`.style(...)`)
against a `Theme` type; 0.13+ ships ~20 built-in Catppuccin/Dracula/Nord/etc.
palettes ([docs.rs Theme](https://docs.rs/iced/0.14.0/iced/enum.Theme.html)).
There is no CSS-like external styling language.

New in 0.14: developer tooling behind feature flags — `debug` (F12 metrics
overlay via `iced_devtools`), `time-travel` debugging, `hot` reloading, and a
`tester` for recording/replaying UI tests. The
[feature list](https://github.com/iced-rs/iced/blob/0.14.0/Cargo.toml)
specifically labels `time-travel` and `hot` as very experimental; that label
does not apply indiscriminately to the debug overlay or tester.

## 2. Full stack table — shared vs re-implemented

Verified two ways: the `[workspace.dependencies]` of
[iced 0.14.0's Cargo.toml](https://github.com/iced-rs/iced/blob/0.14.0/Cargo.toml)
and the actual resolved lockfile of our mini-app (`apps/iced-app/deps-flat.txt`).

| Layer | Crate (resolved version) | Shared or in-house? |
|---|---|---|
| Windowing | **winit 0.30.13** via `iced_winit` | **Shared** ecosystem crate (same as egui/Bevy). X11/Wayland are now cargo features (`x11`, `wayland`, both default). |
| GPU renderer | **wgpu 27.0.1** via `iced_wgpu` | **Shared** (wgpu) + in-house renderer on top (custom quad/triangle/text pipelines). Lyon is optional geometry/canvas machinery and was absent from the default todo app's normal dependency tree. |
| SW fallback | **tiny-skia 0.11.4** via `iced_tiny_skia`, presented with **softbuffer 0.4.8** | **Shared** crates, in-house backend. `iced_renderer` picks wgpu first, falls back to tiny-skia (both in default features). |
| Text shaping | **cosmic-text 0.15.0** (→ **harfrust 0.3.2** shaper, swash 0.2.9 rasterizer, fontdb 0.23, unicode-bidi) | **Shared** — cosmic-text is System76's stack, reused by iced. 0.15 shapes with HarfRust (the Rust HarfBuzz port). |
| Text→GPU atlas | **cryoglyph 0.1.0** | **In-house fork**: "A fork of glyphon for iced", published by hecrj 2025-12-05 ([crates.io/crates/cryoglyph](https://crates.io/crates/cryoglyph), [iced-rs/cryoglyph](https://github.com/iced-rs/cryoglyph)). Ecosystem duplication: glyphon now has an iced-specific fork. |
| Layout | `iced_core::layout` (`Limits`/`Node` + `flex` module) | **In-house**, Druid-derived — the source carries a `DRUID_LICENSE` and the header "This code is heavily inspired by the druid codebase … Copyright 2018 The xi-editor Authors, Héctor Ramón" ([core/src/layout/flex.rs](https://github.com/iced-rs/iced/blob/0.14.0/core/src/layout/flex.rs)). **Not Taffy** — no Taffy dependency or CSS Grid algorithm appears in the tree. `iced_widget 0.14.2` nevertheless provides a responsive [`Grid`](https://docs.rs/iced/0.14.0/iced/widget/struct.Grid.html) container, including fixed-column and fluid modes. |
| Widget library | `iced_widget 0.14.x` | **In-house**: button, text_input, checkbox, radio, slider, pick_list, combo_box, scrollable, toggler, canvas, image/svg, markdown, qr_code, pane_grid, etc. ([docs.rs widget module](https://docs.rs/iced/0.14.0/iced/widget/index.html)). |
| Clipboard | **window_clipboard 0.5.1** | In-house-ish: maintained under the maintainer's umbrella specifically for iced (wraps `clipboard_macos`, which drags in the pre-objc2 `block 0.1.6` crate — see friction log). |

Total footprint for our 74-line app: **149 external name-version rows** and
**140 unique crate names** in the normal-edge dependency tree; the release
binary is **10,393,264 B** (10.39 MB / 9.91 MiB).

## 3. Accessibility

**AccessKit is NOT integrated in stable iced.** There is no `accesskit`
dependency in the 0.14.0 workspace
([Cargo.toml @0.14.0](https://github.com/iced-rs/iced/blob/0.14.0/Cargo.toml))
and none in our app's resolved dependency tree (`apps/iced-app/deps-flat.txt`).
The paper trail, all statuses re-verified 2026-07-07 via the GitHub API:

- [Issue #552 "Implement accessibility support"](https://github.com/iced-rs/iced/issues/552)
  — **open since 2020-10-05** (26 comments, last activity 2026-02-04).
- [PR #1849 "WIP: Iced accessibility"](https://github.com/iced-rs/iced/pull/1849)
  by @wash2 (Ashley Wulber, System76), opened 2023-05-11, implementing
  iced-rs/rfcs#21 — **still open, unmerged** after 3 years.
- [PR #3111 "draft: Accesskit integration"](https://github.com/iced-rs/iced/pull/3111)
  by @roboteng, opened 2025-11-11 — open draft; only Button/Text wired up so
  far (counter example works with VoiceOver on macOS).
- [PR #3281 "Accessibility support"](https://github.com/iced-rs/iced/pull/3281)
  by @dhedlund — **closed unmerged 2026-03-14 by hecrj** with the verbatim
  comment: *"Thanks! But I'll work on this myself."* (comment text verified
  via the issues API). The author says he now maintains accessibility +
  keyboard navigation in his own vendored fork.

**The System76 fork has what upstream lacks:** [pop-os/iced](https://github.com/pop-os/iced)
carries an `iced_accessibility` crate, and
[pop-os/libcosmic](https://github.com/pop-os/libcosmic) enables
`a11y = ["iced/a11y", "iced_accessibility"]` **in its default feature set**
(libcosmic `Cargo.toml`). So COSMIC apps are (partially) screen-reader-capable
while vanilla iced apps are not. This is the clearest upstream/fork divergence
in the iced world.

- **Keyboard navigation:** no automatic Tab traversal —
  [issue #489](https://github.com/iced-rs/iced/issues/489) ("cannot focus most
  widgets, control them via keyboard, or tab between widgets") open since
  2020-08-23, still active 2026-05-24. Manual plumbing exists:
  [`widget::operation::focus_next`](https://docs.rs/iced/0.14.0/iced/widget/operation/fn.focus_next.html)
  plus 0.14's new `unfocus` (#2804) / `is_focused` (#2812) operations
  ([CHANGELOG @0.14.0](https://github.com/iced-rs/iced/blob/0.14.0/CHANGELOG.md))
  — but the app author must wire Tab handling themselves.
- **Screen-reader reality today:** a stock iced 0.14 app exposes no semantic
  **widget** accessibility tree to VoiceOver/NVDA/Orca — consistent with the
  absence of an accessibility integration in the dependency graph and the open
  status of #552. The OS can still observe the application and its native
  window; the missing layer is the framework's widget semantics.
- **IME:** landed **in 0.14** via
  [PR #2777 "Input Method Support"](https://github.com/iced-rs/iced/pull/2777)
  (merged 2025-02-03; follow-ups #2785, #2790, #2793, #2819, #2897, #2918).
  Known open bugs: Windows Japanese IME popup
  ([#3189](https://github.com/iced-rs/iced/issues/3189)), CJK IME broken on
  wasm ([#2843](https://github.com/iced-rs/iced/issues/2843)).
- **RTL/bidi:** cosmic-text 0.15 supports RTL and bidirectional rendering
  ([cosmic-text README](https://github.com/pop-os/cosmic-text)). Our Iced Babel
  app rendered the Arabic, Hebrew, mixed-direction, and CJK corpus correctly
  with automatic system-font fallback. Older framework issues
  ([#250](https://github.com/iced-rs/iced/issues/250),
  [#1877](https://github.com/iced-rs/iced/issues/1877),
  [#2102](https://github.com/iced-rs/iced/issues/2102)) and the
  [official FAQ](https://book.iced.rs/faq.html) still describe RTL/CJK as
  incomplete or planned, so those pages are stale as descriptions of the
  tested 0.14 rendering path. They may still track widget- or editing-specific
  edge cases rather than a total lack of rendering.

**Bottom line:** accessibility is iced's worst gap — open for nearly six
years, with community AccessKit PRs declined or stalled in favor of an
unshipped upstream effort. System76's Iced fork is the verified shipping
integration found in this audit; that evidence does not prove no other
accessible downstream codebase exists.

## 4. OS shell integration

| Capability | Status | Evidence |
|---|---|---|
| Native menu bar | **Not built in.** `window::show_system_menu` exposes the OS window menu, not an application menu. In-window menus are available through third-party [`iced_aw`](https://github.com/iced-rs/iced_aw); on macOS our shell experiment successfully attached a real native menu with `muda`, with manual event and clipboard-role integration. | [CHANGELOG](https://github.com/iced-rs/iced/blob/0.14.0/CHANGELOG.md), local `apps/iced-tray` experiment |
| System tray | **Not built in.** [Issue #124](https://github.com/iced-rs/iced/issues/124) remains the upstream request, but `tray-icon` 0.24 worked in the macOS experiment when initialized on the main thread after the run loop started and polled through its event channel. [Issue #3114](https://github.com/iced-rs/iced/issues/3114) is closed; Linux still inherits the GTK/X11/Wayland caveats of the helper crates. | issue #124, issue #3114, local `apps/iced-tray` experiment |
| Notifications | Not built-in; the ecosystem answer is [`notify-rust`](https://crates.io/crates/notify-rust) — e.g. [Halloy depends on it](https://github.com/squidowl/halloy/blob/main/Cargo.toml). | Halloy Cargo.toml |
| File dialogs | Not built-in; **`rfd` is the de facto standard** — iced itself uses `rfd 0.16` for its `editor` example and `iced_tester` tool. | [Cargo.toml @0.14.0](https://github.com/iced-rs/iced/blob/0.14.0/Cargo.toml) |
| Drag & drop | **Receive-only, built-in**: `window::Event::FileHovered / FileDropped / FilesHoveredLeft` — docs state these are **"not implemented on Wayland"**. No drag-out support. | [window::Event docs](https://docs.rs/iced/0.14.0/iced/window/enum.Event.html) |
| Multi-window | **Built-in**: multi-window landed in 0.12 ([PR #1964](https://github.com/iced-rs/iced/pull/1964)); the `iced::daemon` API landed in 0.13 ([PR #2469](https://github.com/iced-rs/iced/pull/2469)) and is present in [0.14](https://docs.rs/iced/0.14.0/iced/fn.daemon.html). | PRs #1964, #2469 |
| Dark-mode detection | **Built-in since 0.14**: [PR #3051 "System Theme Reactions"](https://github.com/iced-rs/iced/pull/3051) (merged 2025-09-08) replaced the old `dark-light`/`auto-detect-theme` approach with winit-native detection plus [`mundy`](https://crates.io/crates/mundy) on Linux (default `linux-theme-detection` feature) and an `ICED_THEME` env override. Verified in `iced_winit 0.14.0` source. | PR #3051 |

Net: Iced does not provide an integrated desktop-app shell. Menus, tray,
notifications, and dialogs require third-party crates and framework-specific
glue; the macOS experiment demonstrates that this path is viable, but it is
less integrated and less cross-platform than Tauri/Electron-class tooling.

## 5. Platform matrix

| Platform | Status (July 2026) |
|---|---|
| Windows | Listed by the official README; wgpu Vulkan/DX12/OpenGL. |
| macOS | Listed by the official README; wgpu Metal. **Verified first-hand** on M4 Pro for this report. |
| Linux X11 | Listed by the official README; `x11` is a default cargo feature in 0.14. |
| Linux Wayland | Listed by the official README; `wayland` is a default feature. Gaps: file drag-and-drop events are unimplemented on Wayland ([docs](https://docs.rs/iced/0.14.0/iced/window/enum.Event.html)); System76 keeps additional Wayland extensions (layer-shell, cctk) in its fork. |
| WebAssembly | Supported by upstream examples, with known open bugs. 0.14 fixed WebGPU boot (#2686), a WebGL alignment crash (#2883), and wasm timers (#2780); open reports include [#2978](https://github.com/iced-rs/iced/issues/2978), canvas text (#3199), button text clipping (#3289), copy/paste (#2108), and CJK IME (#2843). |
| Mobile (iOS/Android) | **Not supported.** [Issue #302 "Mobile support"](https://github.com/iced-rs/iced/issues/302) open since 2020-04-18, still active 2026-06-17; no official target. |

## 6. License, governance, backing, users

- **License: MIT** ([LICENSE](https://github.com/iced-rs/iced/blob/master/LICENSE), confirmed on crates.io).
- **Repo metrics (fetched 2026-07-09 via GitHub API):** 30,934 stars, 1,598
  forks, 456 open issues/PRs, last push 2026-07-09 (actively developed).
- **Commit-authorship concentration heuristic:** GitHub's repository-lifetime
  contributor totals were hecrj **5,512**, tarkah 107, bungoboingo 93,
  derezzedex 83, and nicksenger 48
  ([contributors](https://github.com/iced-rs/iced/graphs/contributors)).
  Those public counts show unusually high concentration around Héctor Ramón;
  they are a maintenance-risk signal, not a precise private-employment or
  future bus-factor measurement.
- **Governance is explicitly personal.** The [official FAQ](https://book.iced.rs/faq.html)
  states iced is "just my personal project" and — verbatim — *"Every single
  line of code is either written or reviewed directly by me"*; PRs may wait
  months, and he notes he "may choose to prioritize some people, like my
  friends." Public iced-rs org members: hecrj, tarkah, derezzedex, casperstorm
  ([org API](https://api.github.com/orgs/iced-rs/public_members)).
- **Backing/funding:** [GitHub Sponsors](https://github.com/sponsors/hecrj)
  showed 12 current / 42 past sponsors and $5–$50 public tiers on the audit
  date. Those public counts do not reveal total income or employment status.
  The [README @0.14.0](https://github.com/iced-rs/iced/blob/0.14.0/README.md)
  still says development "is sponsored by the Cryptowatch team at Kraken.com";
  hecrj joined Cryptowatch as its first full-time Rust dev (~2020,
  [Kraken tweet](https://x.com/cryptowat_ch/status/1245761359410638848)), but
  Cryptowatch itself was [sunset 2023-09-30](https://www.kraken.com/cryptowatch)
  and **no public source confirming continued Kraken funding in 2026 was found**.
  A missing employer field is not evidence of being unpaid. No public Iced
  foundation or corporate governance entity was found. He appeared on
  [SE Radio #713, March 2026](https://se-radio.net/2026/03/se-radio-713-hector-ramon-jimenez-on-building-a-gui-library-in-rust/).
- **Release cadence (dated intervals)** (dates from
  [GitHub releases](https://github.com/iced-rs/iced/releases) + CHANGELOG):
  0.10.0 → 2023-07-28; *no 0.11 was ever released*; 0.12.0 → 2024-02-15;
  0.13.0 → 2024-09-18 (~7 months); **0.14.0 → 2025-12-07 (~14.5 months
  after 0.13)**. No 0.14.x patch of the top-level `iced` crate exists as of
  2026-07-07 (though subcrates like `iced_widget` are at 0.14.2); master is
  `0.15.0-dev`. Pre-1.0 with breaking API changes every minor release.

### Verified production users

- **System76 COSMIC desktop** — via [libcosmic](https://github.com/pop-os/libcosmic),
  which vendors iced as a **git submodule of the pop-os/iced fork**
  ([.gitmodules](https://github.com/pop-os/libcosmic/blob/master/.gitmodules)).
  The fork is substantially diverged: **238 commits ahead / 278 behind**
  upstream master as of 2026-07-07
  ([compare API](https://api.github.com/repos/iced-rs/iced/compare/master...pop-os:iced:master)).
  The FAQ calls it a "soft fork" — in practice COSMIC ships accessibility and
  Wayland features upstream doesn't have.
- **Sniffnet** (network monitor, ~39.9k stars) — on **stable** iced:
  `iced = { version = "0.14.0", features = ["tokio", "svg", "advanced", "lazy", "image"] }`
  ([Cargo.toml](https://github.com/GyulyVGC/sniffnet/blob/main/Cargo.toml)).
- **Halloy** (IRC client, ~4.3k stars) — tracks iced **master** (`0.15.0-dev`)
  through a pinned `squidowl/iced` fork via `[patch.crates-io]`
  ([Cargo.toml](https://github.com/squidowl/halloy/blob/main/Cargo.toml)).
- **Kraken Desktop** — listed on the [official showcase](https://iced.rs/);
  FAQ: "Kraken has been shipping a desktop application to thousands of users
  for years."
- Others: [Icebreaker](https://github.com/hecrj/icebreaker) (local AI chat, by
  hecrj), [Airshipper](https://gitlab.com/veloren/airshipper) (Veloren
  launcher — still on iced 0.12.1), OctaSine (VST synth, via the
  `iced_baseview` fork), plus Ludusavi, Universal Android Debloater NG,
  XMODITS and ~30 more on the [iced.rs showcase](https://iced.rs/).

These examples document several dependency strategies: COSMIC uses the
System76 fork, Halloy tracks an unreleased branch through a fork, OctaSine uses
`iced_baseview`, and Sniffnet uses stable Iced 0.14. The public dependency
choices alone do not establish each project's motivation or that stable Iced
is generally insufficient.

## 7. Docs & learning resources

- **Official book ([book.iced.rs](https://book.iced.rs)) is thin:** published
  chapters cover Introduction, Philosophy, Architecture, First Steps, The
  Runtime, two widget chapters (Text, Container), FAQ — while Layout, Styling,
  Concurrency, Scaling Applications, and Extending the Runtime are commented
  out as "More to come!" in the book's
  [SUMMARY.md](https://github.com/iced-rs/book/blob/master/src/SUMMARY.md).
- **API docs:** [docs.rs/iced/0.14.0](https://docs.rs/iced/0.14.0) builds with
  all features and is decent reference material; unreleased-master docs live at
  [docs.iced.rs](https://docs.iced.rs/). Module docs are good; conceptual docs
  are sparse.
- **Examples are the real documentation: 52 example directories** at the
  [0.14.0 tag](https://github.com/iced-rs/iced/tree/0.14.0/examples) (todos,
  editor, pane_grid, game_of_life, …). Learning iced in practice means reading
  examples matched to your exact version.
- **Community:** [Discord](https://discord.gg/3xZJ65GAhd) and
  [Zulip](https://iced.zulipchat.com/) linked from [iced.rs](https://iced.rs/).
  The Discourse forum still linked from the 0.14 README
  (discourse.iced.rs) was **unreachable when tested 2026-07-07** — dead link
  in the shipped README.
- **Rating: 3/5.** Excellent examples and API reference; incomplete book, no
  migration guides between breaking minors, and stale community links.

## 8. Friction log (from building `apps/iced-app`)

Everything here was observed first-hand on the reference machine.

- **Spec fit was perfect — zero gaps.** Enter-to-submit is a first-class
  `text_input::on_submit`; no manual keyboard plumbing. The whole app is one
  74-line `main.rs` (see `apps/iced-app/GAPS.md` for the requirement→API map).
- **Canonical clean release build: 21.68 s wall → 22 s rounded**, with a
  **2 s** incremental rebuild and **140 unique crate names** on the M4 Pro.
  The retained build log and `results-iter1.csv` are the source of record.
  First launch opened instantly; process stayed alive 10 s+ and exited cleanly.
- **API churn is real:** 0.14 changed the `application()` signature again
  (now `(boot, update, view)`; title is configured on the builder). Code
  snippets for 0.12/0.13 on the web don't compile on 0.14; you must match
  docs to your exact minor version.
- **`.title()` accepts either a static string or a state-dependent title
  closure.** The closure in this mini-app is optional, not a migration
  requirement; use it only when the title actually depends on application
  state.
- **Build warning out of the box:** cargo flags transitive `block v0.1.6`
  (via `window_clipboard` → `clipboard_macos`) as future-incompatible —
  "code that will be rejected by a future version of Rust". Harmless today,
  but it's legacy ObjC glue in iced's own clipboard path.
- **No runtime warnings** on macOS; wgpu/Metal came up silently. Window
  resize, scrolling, and text editing all behaved with default features.
  System dark/light detection is compiled in by default (verified in
  `iced_winit 0.14.0` source: winit `window::Theme` plus the `mundy` crate
  behind the default `linux-theme-detection` feature) — no code needed.
