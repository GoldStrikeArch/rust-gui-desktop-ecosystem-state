# egui (with eframe) — Deep Dive

*RCN "Cross-Platform GUI Desktop Apps" initiative — framework report 02.*

*Core research was collected 2026-07-07 and reconciled through 2026-07-09 on
macOS 26.5.2 (Apple M4 Pro), rustc 1.96.1. Version tested: **egui / eframe
0.35.0** (published on crates.io 2026-06-25: [egui](https://crates.io/crates/egui),
[eframe](https://crates.io/crates/eframe)). Local execution claims apply only
to that reference machine; other-platform claims are source/API-path verified.*

---

## 1. Architecture & paradigm: immediate mode, for real

egui is the canonical Rust **immediate-mode** GUI: there is no retained widget
tree that you construct and then mutate. Every frame, your code walks over your
*own* application state and re-emits the entire UI; widgets are functions that
return interaction results in the same call
(`if ui.button("Add").clicked() { … }`). The README is explicit about the
consequence: "The application code becomes vastly simpler" and "you don't have
to worry about app state and GUI state being out-of-sync"
([README, "Why immediate mode"](https://github.com/emilk/egui/blob/0.35.0/README.md#why-immediate-mode)).

**State.** All durable state lives in your struct; egui keeps only small
per-widget memory (scroll offsets, window positions, focus) keyed by
widget `Id`s — which is why unstable/duplicated IDs are a classic egui bug
class ([README](https://github.com/emilk/egui/blob/0.35.0/README.md#ids)).

**Repainting & power draw.** eframe is *reactive*, not a game loop: "egui only
repaints when there is interaction (e.g. mouse movement) or an animation, so
if your app is idle, no CPU is wasted"
([README](https://github.com/emilk/egui/blob/0.35.0/README.md#cpu-usage)).
Continuous repaint is opt-in via
[`Context::request_repaint`](https://docs.rs/egui/0.35.0/egui/struct.Context.html#method.request_repaint)
(or `request_repaint_after` for timed updates). When it *does* repaint, it
re-lays-out everything; the README budgets "1-2 ms per frame" for typical UIs
and warns that layout code must stay fast since immediate mode "does a full
layout each frame". The local Tasks run appeared idle in a quick observation,
but no raw CPU samples were retained for that run, so it is not treated as a
controlled measurement.

**App structure / eframe as shell.** [eframe](https://docs.rs/eframe/0.35.0/eframe/)
is the official app shell: it owns the winit event loop, window creation,
renderer setup, app icon, and the wasm bootstrap. It also offers persistence,
but only behind eframe's non-default `persistence` feature; a default eframe
build does not enable it
([eframe feature list](https://github.com/emilk/egui/blob/0.35.0/crates/eframe/Cargo.toml)).
You implement
[`eframe::App`](https://docs.rs/eframe/0.35.0/eframe/trait.App.html). **Breaking
change to know about:** since 0.34 the required method is
`fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)` — it replaced the
long-standing `update(&mut self, ctx: &Context, …)` as part of the 0.34 "more
`Ui`, less `Context`" refactor ([CHANGELOG 0.34](https://github.com/emilk/egui/blob/master/CHANGELOG.md)).
The `Ui` you get is the raw viewport root ("no margin or background color");
you wrap it in `egui::CentralPanel` yourself
([App trait docs](https://docs.rs/eframe/0.35.0/eframe/trait.App.html)). A
separate provided `logic()` hook runs before `ui` for non-visual work. Almost
all pre-2026 tutorials show the old `update()` API.

---

## 2. Full stack table — shared vs re-implemented

Versions are what `cargo tree` resolved for the mini-app (pinned by
[egui's workspace Cargo.toml at tag 0.35.0](https://github.com/emilk/egui/blob/0.35.0/Cargo.toml)).

| Layer | Crate(s) | Shared or in-house? |
|---|---|---|
| Windowing / input | **winit 0.30.13** via [egui-winit](https://crates.io/crates/egui-winit) (pulled in by eframe) | **Shared** ecosystem crate |
| Rendering backend | **wgpu 29.0.4** via [egui-wgpu](https://crates.io/crates/egui-wgpu) — the **default renderer** in eframe 0.35 (default features: `accesskit, default_fonts, wayland, web_screen_reader, wgpu, x11`); eframe supports a Glow/OpenGL renderer behind the opt-in `glow` cargo feature (then selectable via `NativeOptions::renderer`); this measured default 0.35 build compiled only the wgpu renderer — **glow 0.17** in the lockfile arrives via wgpu's GL backend, not an enabled eframe feature ([eframe docs](https://docs.rs/eframe/0.35.0/eframe/), [eframe Cargo.toml](https://github.com/emilk/egui/blob/0.35.0/crates/eframe/Cargo.toml)). Glow 0.18 was released after the July 7 snapshot, so 0.17 is not a claim about the ecosystem's current latest release. | **Shared** (wgpu/glow); the thin egui integrations are in-house |
| Tessellation / painting | **epaint 0.35** — CPU tessellates shapes into anti-aliased (feathered) triangle meshes + a texture atlas; backends just upload vertex buffers | **In-house** (egui project crate) |
| Text **shaping** | **harfrust 0.7.0** — pure-Rust HarfBuzz port, adopted in **0.35** (PR [#8031](https://github.com/emilk/egui/pull/8031), merged 2026-04-06): replaced "character-by-character glyph positioning with proper OpenType text shaping" — GPOS kerning, GSUB ligatures, combining marks/diacritics via anchor tables | **Shared** (harfbuzz org) — *new*; ab_glyph is gone |
| Glyph loading / hinting / variable fonts | **skrifa 0.42.1** (Google Fonts' Rust font stack), adopted in **0.34** together with **vello_cpu 0.0.9** (Linebender) for glyph **rasterization**, replacing ab_glyph and enabling font hinting and a font-variations API (PRs [#7694](https://github.com/emilk/egui/pull/7694), [#7859](https://github.com/emilk/egui/pull/7859)) | **Shared** (Google Fonts + Linebender) |
| Text layout (runs, wrapping, galleys) | `epaint::text` — segments text into font-face runs (grapheme-aware), shapes each with harfrust, per-glyph NOTDEF fallback across *registered* fonts ([#8031 description](https://github.com/emilk/egui/pull/8031)) | **In-house** |
| BiDi / RTL paragraph handling | **Paragraph-level BiDi/run reordering is missing.** HarfRust shapes individual Arabic/Hebrew word runs directionally, but epaint does not apply the Unicode BiDi algorithm across words or mixed-direction paragraphs. [#1016 "Bidirectional text support"](https://github.com/emilk/egui/issues/1016) and [#5069](https://github.com/emilk/egui/issues/5069) remain open; only a UI-layout RTL fix shipped ([CHANGELOG](https://github.com/emilk/egui/blob/master/CHANGELOG.md)). | **Incomplete** |
| System font discovery / fallback | **Missing.** Fallback only iterates fonts *you* register; for non-Latin text "you need to install your own font (.ttf or .otf) using `Context::set_fonts`" ([README](https://github.com/emilk/egui/blob/0.35.0/README.md)). No fontique/fontdb. [#3378 "Cosmic Text for font rendering"](https://github.com/emilk/egui/issues/3378) is still open; a [Parley integration PR #5784](https://github.com/emilk/egui/pull/5784) stalled in favor of the direct skrifa+harfrust wiring | **Absent** (manual) |
| Layout | egui's own single-pass immediate-mode layout (`Ui`, `Layout`, since 0.32 also [`Atom`/`AtomLayout`](https://github.com/emilk/egui/blob/master/CHANGELOG.md) building blocks) | **In-house** |
| Widget library | egui built-ins (buttons, TextEdit, ScrollArea, unified `Panel` since 0.34, `Popup`/`Modal` since 0.32, plots via separate [egui_plot](https://crates.io/crates/egui_plot), tables via [egui_extras](https://crates.io/crates/egui_extras)) | **In-house** |
| Accessibility | **accesskit 0.24.1** schema types are a required dependency of core egui since 0.34 ([CHANGELOG](https://github.com/emilk/egui/blob/master/CHANGELOG.md)). The native `accesskit_winit 0.32.2` adapter is wired through eframe's optional `accesskit` feature, which eframe 0.35 enables by default and this app retained. | **Shared** |
| Clipboard / misc | arboard 3.6.1, webbrowser 1.2, image 0.25 (in egui-winit/eframe) | **Shared** |
| App shell | eframe 0.35 | **In-house** |

The 0.34/0.35 text-stack replacement is the single biggest architectural shift
since the AccessKit pilot: egui went from a hand-rolled ab_glyph pipeline
(no shaping, no hinting) to shared, best-of-breed crates from the Google
Fonts (skrifa), Linebender (vello_cpu) and HarfBuzz (harfrust) ecosystems.
Notably, the harfrust PR was a community contribution whose author openly
credited Claude Code with "most of the heavy lifting"
([#8031](https://github.com/emilk/egui/pull/8031)).

**Remaining text limitations (verified July 2026):** no paragraph-level BiDi
or run reordering (#1016/#5069 open) and no automatic system-font fallback
(bundle-your-own fonts, README). Individual Arabic/Hebrew word runs shape,
but multi-word RTL and mixed-direction paragraphs are ordered incorrectly.
CJK works if you register a CJK font.

---

## 3. Accessibility

- **Pilot status confirmed.** egui was AccessKit's first integration —
  PR [#2294](https://github.com/emilk/egui/pull/2294) landed in egui 0.20
  (Dec 2022). As of 0.34, `accesskit` is **no longer optional in egui** (it
  became a required dependency, [CHANGELOG](https://github.com/emilk/egui/blob/master/CHANGELOG.md)),
  while the native adapter is controlled by an **eframe feature enabled by
  default in 0.35**
  ([eframe Cargo.toml](https://github.com/emilk/egui/blob/0.35.0/crates/eframe/Cargo.toml):
  `default = ["accesskit", …]`). Thus the schema is always present in core;
  native adapter delivery is on by default in the tested eframe configuration,
  not mandatory for every possible egui integration.
- **Adapter scope:** AccessKit itself provides Windows UI Automation, macOS
  NSAccessibility, Linux/Unix AT-SPI, Android, and iOS adapters
  ([AccessKit README](https://github.com/AccessKit/accesskit)). That library
  list is not the same as eframe's official platform matrix, and no web canvas
  adapter is available. Egui's wasm builds instead have an experimental
  built-in screen reader (`web_screen_reader` feature,
  [README](https://github.com/emilk/egui/blob/0.35.0/README.md)).
- **What this project established:** the official test harness
  [egui_kittest](https://docs.rs/egui_kittest/0.35.0/egui_kittest/) drives apps
  *through the AccessKit tree*, and our mini-app's input (Role::TextInput),
  "Add"/"Delete" buttons and live counter label were all queryable and
  operable that way (see §9). This proves those semantic nodes and actions
  exist; it does not by itself prove complete Narrator, VoiceOver, or other
  real screen-reader behavior.
- **Known gaps:** live regions are not exposed
  ([#2647](https://github.com/emilk/egui/issues/2647)) — so dynamic updates
  (like our task counter) won't be announced automatically;
  `Response::labelled_by` can panic with invalid ids
  ([#3647](https://github.com/emilk/egui/issues/3647)); menus lack full
  keyboard-shortcut plumbing ([#2831](https://github.com/emilk/egui/issues/2831)).
- **Keyboard actions tested locally:** programmatic focus and text entry,
  button clicks, and Enter-to-submit worked through `egui_kittest`. The suite
  did not exercise Tab/Shift-Tab traversal, arrow-key navigation, or Space
  activation, so those are not claimed as local verification.
- **IME:** actively maintained; 0.35 shipped "proper visuals for IME
  composition" ([#8083](https://github.com/emilk/egui/pull/8083)) and improved
  IME event handling ([#7983](https://github.com/emilk/egui/pull/7983)).
- **RTL:** still the weakest axis — see text-stack table above (bidi issues
  [#1016](https://github.com/emilk/egui/issues/1016) /
  [#5069](https://github.com/emilk/egui/issues/5069) open).

---

## 4. OS shell integration

| Capability | Status | Source |
|---|---|---|
| Native menu bar | **Not possible built-in** — open feature request [#3411 "eFrame: Native system menubar"](https://github.com/emilk/egui/issues/3411); egui draws its own in-window menu bar. Workarounds: [muda](https://crates.io/crates/muda) + custom winit glue | issue tracker |
| System tray | **Not built-in** — long-standing asks ([discussion #737](https://github.com/emilk/egui/discussions/737), [#1388](https://github.com/emilk/egui/discussions/1388)); third-party [tray-icon](https://crates.io/crates/tray-icon) can be combined with eframe's newer **external event loop** support (official [`external_eventloop` example](https://github.com/emilk/egui/tree/0.35.0/examples/external_eventloop)) | issue tracker / examples |
| Notifications | **Third-party.** `notify-rust` is a general OS notification crate, but in this macOS/eframe experiment it was followed by a frame-scheduling freeze in two of three full-app runs. The shipped experiment uses an `osascript` subprocess instead. Delegate replacement is a suspected cause, not proven without a minimized reproduction. In-app toasts are available via [egui-notify](https://crates.io/crates/egui-notify). | crates.io and local `apps/egui-tray` experiment |
| File dialogs | **Third-party but blessed**: the official eframe example uses [rfd](https://crates.io/crates/rfd) ([examples/file_dialog](https://github.com/emilk/egui/tree/0.35.0/examples/file_dialog)); pure-egui alternative: [egui-file-dialog](https://crates.io/crates/egui-file-dialog) | repo examples |
| Drag & drop (files in) | **Built-in**: `RawInput::hovered_files` / `dropped_files` surfaced through eframe/winit ([docs](https://docs.rs/egui/0.35.0/egui/struct.RawInput.html)) | docs.rs |
| Multi-window | **Built-in since 0.24** (Nov 2023): the viewport API, immediate & deferred viewports ([release 0.24.0 "Multi-viewport"](https://github.com/emilk/egui/releases/tag/0.24.0), PR [#3172](https://github.com/emilk/egui/pull/3172); [viewport docs](https://docs.rs/egui/0.35.0/egui/viewport/index.html)). Not available on web (falls back to embedded) | release notes |
| Dark mode | **Built-in**: [`ThemePreference`](https://docs.rs/egui/0.35.0/egui/enum.ThemePreference.html) with `System` variant follows OS light/dark; per-theme visuals customizable | docs.rs |
| Clipboard, open-URL | **Built-in** via arboard / webbrowser (bundled by egui-winit) | [workspace Cargo.toml](https://github.com/emilk/egui/blob/0.35.0/Cargo.toml) |

Net: egui deliberately draws **everything** itself; the OS shell surface is
thin. Anything that must look/behave native (menus, tray) is DIY or
third-party — the recurring complaint that eframe owned the event loop has,
however, been addressed via the external-eventloop examples.

---

## 5. Platform matrix (verified 0.35)

| Platform | Status |
|---|---|
| Windows | First-class (eframe official target; UIA a11y) — [README](https://github.com/emilk/egui/blob/0.35.0/README.md) |
| macOS | Official target. Locally, the app compiled and exposed an on-screen window; the NSAccessibility adapter is present in the dependency path, but real VoiceOver speech was not tested. |
| Linux X11 | First-class — `x11` default feature ([eframe Cargo.toml](https://github.com/emilk/egui/blob/0.35.0/crates/eframe/Cargo.toml)) |
| Linux Wayland | First-class — `wayland` default feature (same source) |
| Web / wasm | First-class: egui runs on canvas via WebGL2 (glow) or WebGPU (wgpu); [egui.rs](https://www.egui.rs/) itself is the wasm demo. No AccessKit on web (adapter "planned", [AccessKit README](https://github.com/AccessKit/accesskit)); no multi-viewport |
| Android | Supported: `android-game-activity` / `android-native-activity` eframe features + official [`hello_android` example](https://github.com/emilk/egui/tree/0.35.0/examples/hello_android); README lists Android as an eframe target |
| iOS | **Not an official eframe target** (README lists Web, Linux, macOS, Windows, and Android). The presence of iOS support in lower-level winit and AccessKit does not establish an eframe port. |

---

## 6. License, governance, backing, cadence, author concentration, production users

- **License:** dual **MIT OR Apache-2.0**
  ([README](https://github.com/emilk/egui/blob/0.35.0/README.md#license)).
- **Governance/backing:** BDFL model. Creator **Emil Ernerfeldt** (@emilk) is
  co-founder & CTO of **Rerun** ([his profile](https://github.com/emilk/emilk),
  [The Org](https://theorg.com/org/rerun/org-chart/emil-ernerfeldt)); the README
  states plainly: "egui development is sponsored by Rerun"
  ([README](https://github.com/emilk/egui/blob/0.35.0/README.md)). The #2
  maintainer, **Lucas Meurer** (@lucasmerlin), is *also* "sponsored by Rerun to
  help maintain egui" ([his GitHub profile / hello_egui](https://github.com/lucasmerlin)).
  The public direct-maintainer funding identified in this audit therefore
  comes from one organization, Rerun. That is a project-funding concentration
  observation, not a claim about Rerun's private runway or either maintainer's
  complete employment/income. The project also had 29.6k stars and 2.0k forks
  as of the dated 2026-07-07 GitHub API snapshot.
- **Commit-authorship concentration heuristic:** GitHub's repository-lifetime
  contributor totals on 2026-07-07 were emilk 2,995, lucasmerlin 179, and the
  next contributors ≤50 each. This supports an interpretive maintenance-risk
  rating of roughly 1–2 lead maintainers; it is not a measured probability of
  project collapse or a statement about private availability.
- **Release cadence:** roughly 2–4 minor releases/year with patch follow-ups —
  0.33.0 2025-10-09, 0.34.0 2026-03-26 (patches through 0.34.3 2026-05-27),
  0.35.0 2026-06-25 ([releases](https://github.com/emilk/egui/releases)).
  Every minor release **breaks API** (README: "New releases will have breaking
  changes"), and 0.34's `App::ui` rename proved it.
- **Adoption metrics (crates.io API, 2026-07-07):** egui 19.3M all-time
  downloads (4.37M in the trailing 90 days), eframe 14.8M; **1,032 crates
  depend on egui** on crates.io ([reverse deps](https://crates.io/crates/egui/reverse_dependencies)).
- **Verified production users:** **Rerun Viewer** ([rerun.io](https://rerun.io),
  [repo](https://github.com/rerun-io/rerun)) is the flagship — a commercial,
  cross-platform (desktop + web) data-visualization product built on
  egui+wgpu, with Rerun maintaining public egui infrastructure like
  [egui_tiles](https://github.com/rerun-io/egui_tiles). Beyond Rerun, the
  [bevy_egui](https://crates.io/crates/bevy_egui) and the 1,032 reverse
  dependencies recorded on 2026-07-07 provide dated evidence of substantial
  adoption in game/debug/editor tooling and a long open-source tail. This
  audit verified Rerun as the clearest company-backed desktop product; it did
  not exhaustively prove that no other such product exists.

---

## 7. Docs & learning resources

- **API docs:** [docs.rs/egui](https://docs.rs/egui) and
  [docs.rs/eframe](https://docs.rs/eframe) are thorough, with doc examples on
  most widgets; feature flags are documented via `document-features`.
- **The killer resource is the live demo:** [egui.rs](https://www.egui.rs/)
  runs the full widget gallery in wasm with links from every demo to its
  source — immediate-mode code reads top-to-bottom, so this works unusually
  well as documentation.
- **Official examples:** ~24 in-repo ([examples/](https://github.com/emilk/egui/tree/0.35.0/examples))
  covering custom fonts, file dialogs, multiple viewports, external event
  loops, Android, screenshots.
- **Testing story:** [egui_kittest](https://docs.rs/egui_kittest/0.35.0/egui_kittest/)
  is an *official, in-tree* AccessKit-based UI test harness — rare in the Rust
  GUI space.
- **Gaps:** no official book (unlike some competitors). The current official
  eframe template uses `App::ui`, while many older examples and tutorials
  still show the pre-0.34 `update()` API; deeper topics (custom
  widgets, painters, layout internals) rely on reading source. Community:
  active GitHub Discussions and Discord.
- **Rating: 4/5** — excellent reference + demo + tests, no book, churn-prone
  tutorials.

---

## 8. Immediate-mode tradeoffs (honest ledger)

**Strengths**

- *Simplicity & state coherence:* one source of truth; UI is a pure-ish
  function of `&mut self`. Our task counter can never be stale — it is
  recomputed every frame. No callbacks, no message plumbing, no diffing.
- *Tooling & debug UIs:* dropping a slider/inspector next to any live value is
  one line. Rerun, `bevy_egui`, and the dated reverse-dependency count show
  strong adoption in game, robotics, and data/ML tooling.
- *Iteration speed:* the whole mini-app is one 105-line file; UI logic and
  event handling are the same code path.
- *Testability:* immediate mode + AccessKit = headless full-app tests
  (egui_kittest) without any test-specific hooks in app code.

**Costs**

- *Layout:* single-pass layout means "to know the size… we must do the layout,
  but the layout code also checks for interaction… *before* we know its size"
  ([README](https://github.com/emilk/egui/blob/0.35.0/README.md)) → first-frame
  flicker for auto-sized windows, and no general constraint solving. Anything
  like "input stretches, button hugs" is manual arithmetic or
  `Layout::right_to_left` tricks (felt directly in this 100-line app, §9).
  Non-trivial responsive layouts (tables that fill, aligned forms) need
  egui_extras or hand-rolling.
- *CPU/power:* full re-layout every repaint. Reactive mode hides this while
  idle, but big scroll areas / complex UIs must be manually virtualized
  (`ScrollArea::show_rows`) — the framework won't save you.
- *A11y tree stability:* the accessibility tree is rebuilt from whatever code
  ran this frame; nodes keep identity only via egui's `Id` system, so
  conditional UI and ID clashes translate directly into a11y-tree churn — and
  screen-reader features that need durable semantics (live regions,
  [#2647](https://github.com/emilk/egui/issues/2647)) remain unimplemented.
- *Styling/theming:* no styling DSL; theming = mutating `Style`/`Visuals`
  structs in code. Fine for tools, weak for brand-heavy consumer apps.
- *Text editing depth:* `TextEdit` is a good basic editor, but IME polish only
  landed in 0.35 and paragraph-level BiDi is absent. A correctness-critical
  mixed-direction editor would need a different text pipeline or substantial
  additional work.

---

## 9. Friction log — building the Tasks mini-app (deliverable B)

App: [`apps/egui-app/`](../apps/egui-app/) — eframe pinned `=0.35.0`, default
features only, single 168-line `main.rs` (105 lines of app, the rest an
egui_kittest test module). Full spec compliance; `GAPS.md` records **no spec
gaps**. Canonical clean `cargo build --release`: **26.39 s → 27 s rounded**,
with a **1 s** incremental rebuild. The flat list has 164 name-version rows
including the app root, 163 external rows, and **156 unique crate names**;
the unstripped wgpu binary is **12,531,728 B** (12.53 MB / 11.95 MiB);
launched, survived more than 10 s, exposed an on-screen window, printed no
runtime warnings, and was killed cleanly. The
kittest tests drive the real UI through AccessKit
(`get_by_role(TextInput)`, `get_by_label("Add")`) and pass.

1. **API churn bit immediately.** Many older online examples say
   `impl App { fn update(&mut self, ctx, frame) }`; 0.34+ requires
   `fn ui(&mut self, ui: &mut Ui, frame)` and hands you a bare, margin-less
   root `Ui` that you must wrap in `CentralPanel` yourself
   ([App docs](https://docs.rs/eframe/0.35.0/eframe/trait.App.html)). Cost:
   one docs round-trip; would be a compile-error scavenger hunt if porting an
   old codebase.
2. **Enter-to-submit is a folklore pattern.** There is no submit event on
   `TextEdit`; the idiom is
   `response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))` plus
   `response.request_focus()` to restore focus — non-obvious, undocumented on
   the widget itself, and subtly wrong variants (checking `changed()`, or
   forgetting refocus) abound in the wild.
3. **"Fill remaining width" is manual.** Making the input stretch while the
   Add button hugs required `desired_width(ui.available_width() - 50.0)`;
   right-aligning each row's Delete button required a nested
   `Layout::right_to_left` containing a re-reversed `left_to_right` for the
   label. Flexbox-class layouts are where immediate mode taxes you.
4. **Delete-while-iterating needs the deferred-index idiom**
   (`Option<usize>` collected in the loop, applied after) — standard Rust
   borrow discipline, but boilerplate every list UI pays.
5. **kittest gotcha:** `Node::type_text` emits `egui::Event::Text`, which is
   delivered to the *focused* widget — without an explicit `node.focus()`
   first, typing silently vanishes (found by reading
   [egui's own demo tests](https://github.com/emilk/egui/blob/0.35.0/crates/egui_demo_lib/src/demo/text_edit.rs)).

Positives worth logging: zero `unsafe`, zero build-script/system-dependency
pain on macOS (no cmake, no pkg-config drama), compile-run loop after the
first build was ~1 s, and the framework's defaults (resizable window, dark
mode following the OS, scroll physics, focus ring) were all correct without
configuration.

---

## Appendix: measurements (Apple M4 Pro, macOS, rustc 1.96.1)

| Metric | Value |
|---|---|
| eframe/egui version | 0.35.0 (pinned `=0.35.0`) |
| App LoC (`src/main.rs`) | 105 app / 168 incl. kittest tests |
| Clean release build | 26.39 s wall → 27 s rounded |
| Incremental rebuild | 1 s |
| Binary size (release, unstripped) | 12,531,728 B = 12.53 MB / 11.95 MiB |
| Dependency rows / unique names | 164 incl. root / 163 external / 156 unique names |
| Launch evidence | process survived >10 s; on-screen window observed; no warnings; clean exit |
| Headless UI tests (AccessKit-driven) | 2/2 pass |
