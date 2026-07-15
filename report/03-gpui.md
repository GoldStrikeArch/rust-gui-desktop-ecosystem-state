# gpui — Deep Dive (RCN Cross-Platform GUI Desktop Apps, July 2026)

**Framework:** `gpui` — Zed Industries' GPU-accelerated UI framework, extracted
from and developed inside the [zed monorepo](https://github.com/zed-industries/zed/tree/main/crates/gpui).
**Version tested:** `gpui 0.2.2` from crates.io (published 2025-10-22; still the
latest release as of 2026-07-07).
**Verified on:** Apple M4 Pro, macOS, rustc 1.96.1 — mini-app in
[`apps/gpui-app/`](../apps/gpui-app/).

---

## 1. Architecture & paradigm

gpui describes itself as "a hybrid immediate and retained mode, GPU
accelerated, UI framework for Rust" ([README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md),
[gpui.rs](https://www.gpui.rs)). The README lays out three "registers" an app
is written in:

- **Entities** — all long-lived state lives in `Entity<T>` handles owned by
  gpui itself (an `Rc`-like smart pointer resolved through a context). This is
  the retained half: entities persist across frames, can observe/subscribe to
  each other, and emit events. Everything is single-threaded ownership with
  `Context<T>` / `App` passed explicitly into every callback — no
  `Rc<RefCell>` soup, but also a very unusual API shape for newcomers
  (documented in [docs/contexts.md](https://github.com/zed-industries/zed/blob/main/crates/gpui/docs/contexts.md)).
- **Views** — any entity implementing the [`Render` trait](https://docs.rs/gpui/latest/gpui/trait.Render.html)
  is a view. Each frame, gpui calls `render(&mut self, &mut Window, &mut
  Context<Self>) -> impl IntoElement` on the window's root view, which builds
  a brand-new element tree — the immediate half. There is no diffing/VDOM;
  the whole element tree is rebuilt on `cx.notify()` and re-laid-out.
- **Elements** — the low-level imperative layer (`Element` trait) with full
  control over layout/paint, used for things like Zed's editor and
  `uniform_list` virtualization.

Styling is a **tailwind-style builder API** on `div()`:
`div().flex().flex_col().gap_2().bg(rgb(0x...)).rounded_md().hover(|s| ...)` —
many names and spacing conventions resemble Tailwind, which makes some web
muscle-memory transfer, but this is not a claim of one-to-one Tailwind API or
behavioral parity
([hello_world.rs](https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/hello_world.rs)).

Event flow: `Application::new().run(|cx: &mut App| ...)` →
`cx.open_window(WindowOptions, |window, cx| cx.new(|cx| RootView...))`.
Interactivity is attached per-element (`.on_click`, `.on_key_down`,
`.on_drag/.on_drop`), plus a first-class **action system** (`actions!` macro +
`KeyBinding`) for keymap-driven commands — the same system that powers Zed's
keymap ([docs/key_dispatch.md](https://github.com/zed-industries/zed/blob/main/crates/gpui/docs/key_dispatch.md)).

gpui ships **its own async executor** integrated with the platform event loop
(`ForegroundExecutor`/`BackgroundExecutor`, `cx.spawn(...)`); on current main
the core scheduling has been extracted into a `scheduler` crate
([crates/scheduler](https://github.com/zed-industries/zed/tree/main/crates/scheduler)),
and a `gpui_tokio` crate exists for tokio interop
([crates/gpui_tokio](https://github.com/zed-industries/zed/tree/main/crates/gpui_tokio)).
A `#[gpui::test]` macro provides deterministic async tests with simulated
platform input.

**Big structural news (2026):** gpui on main has been split into a crate
family — `gpui` (core) + `gpui_platform`, `gpui_macos`, `gpui_linux`,
`gpui_windows`, `gpui_wgpu`, `gpui_web`, `scheduler`, `gpui_util`,
`gpui_shared_string` — all Apache-2.0
([crates/ listing](https://github.com/zed-industries/zed/tree/main/crates)).
None of these platform crates are on crates.io yet; the published 0.2.2 is
still the monolithic layout.

## 2. Full stack table

| Layer | What provides it (v0.2.2 = what external users get) | In-house or shared? |
|---|---|---|
| **Windowing** | Custom per-platform, **not winit** (zero winit references in the crate). macOS: AppKit via `cocoa`/`objc` ([src/platform/mac](https://github.com/zed-industries/zed/tree/gpui-v0.2.2/crates/gpui/src/platform/mac)); Linux: in-house Wayland (`wayland-client` + `calloop`) and X11 (xcb) backends; Windows: raw Win32 via `windows-rs`. FreeBSD piggybacks on the Linux backend. | **In-house** (uses low-level binding crates only) |
| **GPU renderer** | macOS: in-house **Metal** renderer (`metal_renderer.rs`; shaders precompiled by build.rs or at runtime with `runtime_shaders`). Linux/FreeBSD in 0.2.2: **blade-graphics 0.7** (Vulkan). Windows: in-house **Direct3D 11** (+DXGI, DirectComposition) — verified via `windows-rs` feature list in [gpui-v0.2.2 Cargo.toml](https://github.com/zed-industries/zed/blob/gpui-v0.2.2/crates/gpui/Cargo.toml) and the [Zed-for-Windows post](https://zed.dev/blog/zed-for-windows-is-here). **On main, blade is gone**: the Linux renderer was reimplemented on **wgpu** by community contributor @zortax, merged 2026-02-13 ([PR #46758](https://github.com/zed-industries/zed/pull/46758), [HN thread](https://news.ycombinator.com/item?id=47002825)), living in the new `gpui_wgpu` crate. | Metal/D3D11: **in-house**; Linux: shared (blade → **wgpu**) |
| **Text system** | Platform shapers, one per OS: macOS **CoreText** (`core-text` crate) + a forked `font-kit` published as `zed-font-kit`; Windows **DirectWrite** ([direct_write.rs](https://github.com/zed-industries/zed/blob/gpui-v0.2.2/crates/gpui/src/platform/windows/direct_write.rs)); Linux **cosmic-text 0.14** (rustybuzz shaping) + fontconfig via zed-font-kit. The new `gpui_wgpu` path uses **cosmic-text 0.19 + swash** ([gpui_wgpu/Cargo.toml](https://github.com/zed-industries/zed/blob/main/crates/gpui_wgpu/Cargo.toml)). | **Mixed**: OS APIs on mac/win (in-house glue), shared `cosmic-text` on Linux/web |
| **Layout** | **taffy** (flexbox/grid) — `taffy 0.9.0` resolved in the published 0.2.2; main pins `taffy = "=0.10.1"` ([Cargo.toml on main](https://github.com/zed-industries/zed/blob/main/crates/gpui/Cargo.toml)). Same engine as Dioxus/Blitz/Bevy UI — genuine ecosystem sharing. | **Shared** (exact-pinned) |
| **Vector/SVG/paths** | `resvg`/`usvg` for SVG, `lyon` for path tessellation, `image` for rasters, `etagere` for atlas packing. | Shared |
| **Widgets/components** | gpui has low-level elements, including `uniform_list` virtualization, but **no first-party high-level widget or text-input library**: buttons are normally styled `div()`s and text editing requires `EntityInputHandler`. Zed's own component library (`ui`, `ui_input`) is **GPL-3.0-or-later and unpublished** ([ui/Cargo.toml](https://github.com/zed-industries/zed/blob/main/crates/ui/Cargo.toml)), so it is not reusable in a non-GPL app. The principal permissive alternative found is **[longbridge/gpui-component](https://github.com/longbridge/gpui-component)** — Apache-2.0, 60+ widgets including inputs, tables, docks and charts, published as `gpui-component 0.5.1` in February 2026. | gpui: low-level elements; high-level components: third-party |
| **Accessibility** | Nothing in 0.2.2; **AccessKit** integrated on main since 2026-05-27 (see §3). | Shared (AccessKit) — unreleased |
| **Async runtime** | Own executor on the platform event loop; `scheduler` crate on main; `gpui_tokio` bridge. | In-house |

## 3. Accessibility

- **Status quo for released versions: effectively nothing.** The tracking
  issue [#41138 "Windows: Screen reader accessibility missing completely"](https://github.com/zed-industries/zed/issues/41138)
  (Oct 2025, open) documents that Zed/gpui exposed no accessibility tree at
  all.
- **AccessKit landed in gpui on 2026-05-27**:
  [PR #56065 "gpui: Accesskit support"](https://github.com/zed-industries/zed/pull/56065)
  (author @cameron1024, replacing the broader
  [#51097](https://github.com/zed-industries/zed/pull/51097)). Scope is
  deliberately minimal: it adds AccessKit plumbing to gpui only — "Once this
  lands, we can start adding aria attributes to Zed's components." A follow-up
  added an [`Application::inaccessible()`](https://github.com/zed-industries/zed/pull/57954)
  opt-out. On main, `accesskit 0.24` plus platform adapters
  (`accesskit_macos`, `accesskit_unix`, `accesskit_windows`) are dependencies
  of the gpui platform crates ([workspace Cargo.toml](https://github.com/zed-industries/zed/blob/main/Cargo.toml)).
  **None of this is in any crates.io release** — external users on 0.2.2 have
  zero AccessKit integration today. Zed itself currently opts out with
  `Application::inaccessible()`: the follow-up PR says the adapter was
  temporarily disabled after nightly panics while the implementation gains
  confidence. Merged plumbing therefore does not yet mean accessible Zed
  components or current product accessibility.
- **Keyboard navigation:** solid primitives — focus handles, `tab_index()` /
  `tab_stop()` on interactive elements
  ([window.rs in 0.2.2](https://github.com/zed-industries/zed/blob/gpui-v0.2.2/crates/gpui/src/window.rs),
  [`tab_stop.rs` example](https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/tab_stop.rs)),
  plus the action/keybinding system. But since widgets are DIY, semantics are
  DIY too.
- **IME:** real support exists — macOS implements `NSTextInputClient`
  ([mac/window.rs](https://github.com/zed-industries/zed/blob/gpui-v0.2.2/crates/gpui/src/platform/mac/window.rs)),
  Wayland uses the text-input protocol, Windows handles WM_IME events —
  **but only for widgets that implement `EntityInputHandler`** (marked-text
  ranges etc.). The 746-line [`input.rs` example at the audited 0.2.2 tag](https://github.com/zed-industries/zed/blob/gpui-v0.2.2/crates/gpui/examples/input.rs)
  shows what that takes. Known rough edges remain, e.g.
  [#56149 IME candidate window position on Windows](https://github.com/zed-industries/zed/issues/56149).
- **RTL support remains incomplete.** [#31102 "RTL Right-to-Left Text Input/Rendering Support"](https://github.com/zed-industries/zed/issues/31102)
  remains open. The later Babel experiment found that the tested macOS/CoreText
  backend renders full BiDi lines correctly, but gpui's logical-to-visual caret
  and selection geometry breaks inside reordered RTL runs. That macOS result
  does not establish equivalent behavior on DirectWrite or cosmic-text.

## 4. OS shell integration

Verified against the `Platform` trait in the published 0.2.2
([src/platform.rs](https://github.com/zed-industries/zed/blob/gpui-v0.2.2/crates/gpui/src/platform.rs)):

**gpui provides:** native app menus + macOS dock menu (`set_menus`,
`set_dock_menu`, [`set_menus.rs` example](https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/set_menus.rs));
native open/save dialogs (`prompt_for_paths`, `prompt_for_new_path`);
`reveal_path` (Finder/Explorer), `open_with_system`, `open_url`,
`register_url_scheme`, `add_recent_document`; dark-mode detection
(`window_appearance` + change observers); OS keychain credentials
(`write_credentials` etc.); cursor styles; drag & drop both in-app and from
the OS (`on_drag`/`on_drop`, `ExternalPaths`,
[`drag_drop.rs` example](https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/drag_drop.rs));
multi-window (incl. popups/`WindowKind`), custom titlebars
(`appears_transparent`, traffic-light positioning — how Zed draws its own);
screen capture behind the `screen-capture` feature. The trait surface is not
uniform implementation coverage: in 0.2.2 `register_url_scheme` returns an
"unimplemented" error on Windows and Linux, `add_recent_document` is a no-op
on Linux, and screen capture reports unsupported on Wayland (while the X11,
Windows, and macOS paths implement it). These are source-verified platform
limitations in the published
[Windows](https://github.com/zed-industries/zed/blob/gpui-v0.2.2/crates/gpui/src/platform/windows/platform.rs) and
[Linux/Wayland](https://github.com/zed-industries/zed/blob/gpui-v0.2.2/crates/gpui/src/platform/linux/wayland/client.rs)
implementations, not locally exercised Linux/Windows results.

**gpui core does NOT provide:** system tray/status items, native toast
notifications, or printing. `liora-tray` exists, and this study's later macOS
shell app successfully assembled `tray-icon`/muda with gpui; that demonstrates
compatible ecosystem integration, not a first-party gpui API. Zed renders
notifications in-window and hand-rolls its titlebar/chrome on top of the
transparent-titlebar hooks.

## 5. Usability outside Zed (the key question)

- **crates.io:** gpui was finally published in Oct 2025 —
  0.2.0 (2025-10-09), 0.2.1 (10-14), 0.2.2 (10-22), announced by
  [@zeddotdev](https://x.com/zeddotdev/status/1976309201744937039); tags
  `gpui-v0.2.x` in the repo. **It is genuinely usable**: the package includes
  README, 30 example targets, and [docs.rs builds successfully](https://docs.rs/gpui/0.2.2/gpui/).
  Our mini-app pinned `=0.2.2` with no git dependency.
- **But cadence stalled immediately:** no release in the ~8.5 months since
  0.2.2, while main diverged massively (wgpu renderer Feb 2026, AccessKit May
  2026, the crate split, `taffy` 0.9→0.10.1, cosmic-text 0.19). The version on
  main is still `0.2.2`, i.e. not even bumped. Users wanting 2026 gpui must
  use a git dependency on the zed monorepo again.
- **Stability policy:** explicitly pre-1.0 — "There will often be breaking
  changes between versions" ([README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md)).
  No changelog is published for gpui itself.
- **Ecosystem snapshot (2026-07-07):** crates.io reported **100 reverse
  dependencies** of `gpui`; **98 targeted the 0.2 release line**, rather than
  all 100. Examples include [`gpui-component`](https://crates.io/crates/gpui-component)
  (0.5.1), `gpui-router`, `gpui-tokio-bridge`, `gpui-video-player`,
  `gpui-terminal`, theme/icon packs (`liora-*`), etc. The official
  [zed-industries/awesome-gpui](https://github.com/zed-industries/awesome-gpui)
  list had roughly 45 app entries and 20 libraries. **Zed** and **Longbridge
  Pro** (via gpui-component) are explicitly documented product uses. Projects
  such as [helix-gpui](https://github.com/polachok/helix-gpui),
  [Loungy](https://github.com/MatthiasGrandl/loungy),
  [Hummingbird](https://github.com/143mailliw/hummingbird),
  [coop](https://github.com/lumehq/coop), and
  [omarchist](https://github.com/tahayvr/omarchist) were showcase/repository
  entries; list membership alone was not treated as proof of a currently
  shipping production deployment. The ecosystem also has an official
  scaffolder [create-gpui-app](https://github.com/zed-industries/create-gpui-app)
  (373 stars, though untouched since Apr 2025).
- **Platform matrix (external users, 0.2.2):** macOS (Metal), Linux
  Wayland+X11 (blade/Vulkan), Windows (D3D11; first-class since
  [Zed's Windows GA, 2025-10-15](https://zed.dev/blog/zed-for-windows-is-here)),
  FreeBSD (best-effort cfg support). **Web/WASM: in active development on
  main** — `gpui_web` + `gpui_wgpu` (WebGPU) crates exist with wasm target
  sections ([gpui_web/Cargo.toml](https://github.com/zed-industries/zed/blob/main/crates/gpui_web/Cargo.toml),
  longstanding ask in [discussion #8203](https://github.com/zed-industries/zed/discussions/8203)) —
  unreleased. **Mobile: none.**

## 6. License, governance, and concentration

- **License split (verified):** the `gpui` crate and all new `gpui_*`platform
  crates are **Apache-2.0** (crate metadata on crates.io and each
  `Cargo.toml`); the Zed app (`crates/zed`) and Zed's component library
  (`crates/ui`, `crates/ui_input`) are **GPL-3.0-or-later**. Repo root carries
  [LICENSE-APACHE](https://github.com/zed-industries/zed/blob/main/LICENSE-APACHE)
  and [LICENSE-GPL](https://github.com/zed-industries/zed/blob/main/LICENSE-GPL).
  Practical consequence: the framework is permissive, but the only first-party
  widget set is GPL — the permissive widget layer comes from a third party
  (Longbridge).
- **Governance/backing:** developed by **Zed Industries** inside the zed
  monorepo; external contributions require the [Zed CLA](https://zed.dev/cla)
  and, per the README, "for the near future gpui is tied to Zed, so
  contributions will need to be made there and kept in sync"
  ([gpui.rs](https://www.gpui.rs)). Zed officially announced a **$32M Series B
  led by Sequoia on 2025-08-20**, bringing publicly announced total funding to
  **over $42M** ([Zed announcement](https://zed.dev/blog/sequoia-backs-zed)).
- **Vendor concentration:** the roadmap is driven by one vendor's editor
  product, and gpui has no separately documented release process. The
  eight-month crates.io gap while main changed substantially is observable;
  calling it a numerical "bus factor" would require a defined authorship
  calculation that this report does not provide.
  Mitigations: Apache-2.0 forkability, large community (awesome-gpui, 100
  reverse deps), and demonstrated willingness to merge huge community PRs
  (the entire Linux wgpu renderer, AccessKit support).

## 7. Docs & learning resources

- [gpui.rs](https://www.gpui.rs) — official site: hello-world, docs index,
  examples index; honest note that "the best way to learn about these APIs is
  to read the Zed source code… or drop a question in the Zed Discord."
- [docs.rs/gpui](https://docs.rs/gpui) — full API reference, builds cleanly.
- 30 example targets ship **inside the published 0.2.2 crate**; the audited
  current `main` tree has 41 example entrypoints. They cover input, drag&drop,
  uniform lists, window management, and menus.
- Two conceptual docs in-tree: [contexts.md](https://github.com/zed-industries/zed/blob/main/crates/gpui/docs/contexts.md),
  [key_dispatch.md](https://github.com/zed-industries/zed/blob/main/crates/gpui/docs/key_dispatch.md);
  Zed blog posts (e.g. ownership & data flow, ["GPUI 2 is now in production"](https://zed.dev/blog/gpui-2-on-preview)).
- Community: [awesome-gpui](https://github.com/zed-industries/awesome-gpui),
  [gpui-component docs site](https://longbridge.github.io/gpui-component/),
  assorted tutorials, and the independent
  [GPUI Book](https://github.com/MatinAniss/gpui-book). There is **no official
  book**; the community book is a real guide-level resource but is not a Zed
  support commitment. Rating: **3/5** — good reference + examples, thinner
  official conceptual on-ramp than iced/Slint.

## 8. Friction log (from building `apps/gpui-app`)

Full detail in [apps/gpui-app/GAPS.md](../apps/gpui-app/GAPS.md).

1. **Metal Toolchain trap (macOS):** with default features, `gpui`'s build.rs
   fails on Xcode 26: `cannot execute tool 'metal' due to missing Metal
   Toolchain; use: xcodebuild -downloadComponent MetalToolchain` (reproduced).
   Fix: the `runtime_shaders` feature (compiles shaders at startup) or a
   multi-GB toolchain download. This reproduced on this Xcode 26 installation;
   machines with the Metal toolchain installed will not hit it.
2. **No first-party high-level widget library:** the text input had to be
   hand-rolled from raw `on_key_down` events (no IME/selection); the official path is implementing
   `EntityInputHandler` — 746 lines in the bundled example. Buttons are styled
   divs. `uniform_list` exists in core and gpui-component is a third-party
   alternative, but neither supplies a first-party text field in gpui itself.
3. **Canonical build result:** clean `cargo build --release` completed in
   **55.16 s (reported as 56 s)** on the M4 Pro with **391 unique crate names**;
   the old 525 figure counted repeated/name-version tree rows, not unique names.
   The unstripped binary is **5.0 MiB** and app source is 230 LoC.
4. **API discoverability is decent**: docs.rs + bundled examples answered
   everything (window options, focus, scrolling via `.id()` +
   `.overflow_y_scroll()`); the subtlety of `Keystroke::key_char` vs `key`
   required reading gpui source.
5. **Apps don't quit when the last window closes** unless you wire
   `cx.on_window_closed(… cx.quit())` yourself; plus a
   `block v0.1.6` future-incompat warning from the Objective-C bridge.
   Runtime: zero warnings on stdout/stderr; window rendered and survived a
   12 s background run before being killed.
