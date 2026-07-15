# A primer: the load-bearing crates, the four fragmentations, and the sustainability risk

Companion to [00-ecosystem-map.md](00-ecosystem-map.md), written for readers
who don't live inside the GUI stack. Everything here is sourced in the map;
this document trades citations for clarity.

## Part 1 — What these crates actually are (and why you should care)

A GUI framework is not one program — it's a tower of specialized layers. In
Rust, most frameworks share *some* of these layers and re-implement others.
These are the layers, bottom to top, with the crates that occupy them:

### The window layer

- **winit** — *the window opener.* Creates windows on Windows/macOS/Linux
  (X11 and Wayland)/web/Android/iOS and feeds your app the raw input stream:
  keyboard, mouse, touch, DPI changes, monitor hotplug, IME composition
  events. Most framework-drawn native stacks in this sample sit on it (iced,
  egui, Xilem, and Slint's default Rust path; also floem and Bevy). GPUI has
  its own platform layer, Tauri/Dioxus use Tao, and Makepad plus GTK/Qt stacks
  are other important exceptions. **Why you care:** a winit stall or platform
  fix propagates across many Rust GUI users, though not every framework.
- **tao** — *Tauri's fork of winit* (2021), created because Tauri needed
  menus, tray icons, and — crucially — GTK-hosted windows so WebKitGTK could
  render into them on Linux. Used by Tauri and (still, for now) Dioxus.
- **raw-window-handle** — *the adapter plug between window and GPU.* A tiny
  crate that says "here is an OS window handle" in a way any renderer can
  consume. It is the only **cross-platform GUI-interoperability abstraction**
  present in all seven test dependency trees. Other universal names include
  platform-specific GUI crates such as `objc2-app-kit` and Core Graphics, so
  it is not literally the only GUI-related crate in the overlap.
- **softbuffer** — *draw pixels without a GPU:* puts a plain CPU pixel buffer
  on a winit window (used for software-rendering fallbacks).

### The GPU layer

- **wgpu** — *the GPU translator.* One modern API that becomes Metal on
  macOS, Vulkan on Linux/Android, DirectX 12 on Windows, WebGPU in browsers.
  Firefox's WebGPU implementation *is* wgpu, which is why Mozilla helps
  maintain it. Iced and egui default to it, Vello/Xilem use it, Slint offers
  a wgpu path, and Zed's GPUI renderer uses it on Linux. GPUI still uses
  platform GPU paths elsewhere, Slint has several other backends, and webview
  apps delegate drawing to browser engines. **Why you care:** it is the
  leading shared GPU API in this sample, not a universal renderer.
- **The framework-side renderer families** are where sharing falls off (see
  Part 2). Depending on how multi-backend frameworks are grouped, this sample
  has at least six maintained families, including **Vello**, **epaint/egui**,
  Iced's renderer, Slint's **FemtoVG**, **Skia**, Qt/software paths, and
  GPUI's platform pipeline. They target a mix of wgpu, direct platform GPU APIs,
  OpenGL/Skia, Qt, and CPU software; they do **not** all sit above wgpu.
  These paths own or integrate their own choices for clipping, gradients,
  anti-aliasing, caching, and text rasterization.

### The text layer (the deepest, most duplicated layer)

Text is a pipeline, and each stage is its own hard problem:

1. **Font discovery/fallback** — find fonts on the system; when a glyph is
   missing (an emoji, a Chinese character), pick a substitute font.
   **fontique** (Linebender, active) and **fontdb** (dormant since Oct 2024,
   still used by iced's stack and others) are the main reusable Rust discovery
   stacks represented here. GPUI also uses platform APIs, egui bundles fonts
   by default, and webviews delegate fallback to the browser engine.
2. **Shaping** — turn a Unicode string + font into positioned glyphs:
   ligatures, kerning, and the complex rules of Arabic, Devanagari, etc.
   Crate: **harfrust** — the HarfBuzz organization's official Rust port
   (HarfBuzz is the shaper inside Chrome/Android/Linux). In under a year it
   became the shared shaper across the three principal reusable pure-Rust
   stacks in this sample: cosmic-text, Parley, and epaint 0.35. GPUI's
   macOS/Windows paths and browser/webview engines remain outside that scope.
   **rustybuzz** remains released in the `harfbuzz/rustybuzz` repository; its
   deprecation/migration discussion is closed and the repository is active,
   not archived. **swash** still does
   *rasterization* (turning shaped glyphs into pixels) in several stacks,
   alongside **skrifa** (Google Fonts' glyph loader).
3. **Layout** — line breaking, wrapping, bidirectional text (mixing
   English + Hebrew), rich text spans, text editing semantics (where does
   the caret go when you press ↑?). Crates: **cosmic-text** (System76) vs
   **parley** (Linebender) vs two in-house implementations (egui's galley,
   gpui's line layout). This is the fragmented stage — see Part 2.

**Why you care:** correct text is one of the most expensive parts of a GUI
toolkit — mature browser and desktop stacks embody decades of engineering.
Each independent layout stack must address emoji, BiDi, IME, and fallback
behavior on its own schedule.

### The structure layer

- **taffy** — *the layout calculator.* You give it a tree of boxes with CSS
  flexbox/grid rules; it returns their positions and sizes. Maintained under
  DioxusLabs (primary maintainer: nicoburns) but deliberately cross-team.
  Used by Zed's gpui, Blitz, bevy_ui, floem, Servo (for CSS Grid!), and
  experimentally Slint. **Why you care:** it's the proof that frameworks
  *will* share a hard component when its API is neutral and its quality is
  high — an example the initiative can evaluate for other layers.
- **AccessKit** — *the accessibility adapter.* Screen readers (VoiceOver,
  NVDA, Orca) don't see pixels; they need a semantic tree ("this is a
  button named Add, it's focused"). Each OS has a different, ancient,
  fiddly API for that tree. AccessKit defines ONE Rust tree format and
  ships adapters for Windows UI Automation, macOS NSAccessibility, Linux
  AT-SPI, Android, and iOS. The toolkit pushes tree updates; AccessKit
  translates. egui, Slint, Xilem/Masonry, Bevy, Vizia, Blitz, GPUI (merged in
  May 2026, though Zed currently disables it), and even GTK 4.18 use it;
  Iced and Floem remain gaps. **Why you care:** it is the dominant shared
  accessibility path in this sample, and recent public authorship is highly
  concentrated (Part 3). It still requires toolkit semantics and real
  assistive-technology testing.

### The shell-integration layer (the "everything around the window" bits)

- **rfd** — native open/save file dialogs.
- **muda** — native menu bars and context menus (tauri-apps).
- **tray-icon** — system tray icons (tauri-apps).
- **global-hotkey** — system-wide keyboard shortcuts (tauri-apps).
- **arboard** — clipboard read/write (repository hosted by 1Password).
- **notify-rust** — desktop notifications.
- **window-vibrancy** — translucent/blurred window backgrounds (tauri-apps).

**Why you care:** "desktop app" quality lives here. On Linux, specific paths
in muda and tray-icon require GTK integration, while global-hotkey is
X11-only; that is not a claim that every crate in this list assumes GTK (Part
2, story 4).

### The ship-it layer

- **rcodesign (apple-codesign)** — a principal open-source Rust-native tool
  for signing and notarizing macOS apps from non-macOS CI. Apple's own tools
  remain the standard option on macOS, and Tauri/cargo-packager can drive
  configured signing workflows. Its concentrated maintenance and long gap
  between crates.io releases are risk signals, not proof that every Rust
  release pipeline uses it.
- **tauri-bundler / cargo-packager / cargo-bundle** are bundlers with different
  format and integration coverage. `cargo-bundle` is a basic bundle producer,
  not an auto-updater; Tauri and cargo-packager can also coordinate configured
  signing and distribution artifact production.
- **Velopack** is a framework-independent installer and delta-update toolchain
  with a Rust runtime client. Updater metadata/artifact production, signing
  those artifacts, and the in-app update client are distinct responsibilities.
  Tauri integrates more of this lifecycle, but it is not the only maintained
  update option.

## Part 2 — The four fragmentation stories, explained

### Story 1: four text-layout stacks

Shaping converged on HarfRust across cosmic-text, Parley, and epaint 0.35 —
the three principal reusable pure-Rust stacks in this sample, not every
platform and browser path. The *layout* stage above it still exists four ways:

| Stack | Owner | Who uses it |
|---|---|---|
| cosmic-text | System76 (for COSMIC desktop) | iced, COSMIC, gpui-on-Linux, Cushy |
| parley | Linebender | xilem, Slint, Blitz, Bevy 0.19, floem |
| epaint "galley" | egui, in-house | egui only — no paragraph-level BiDi, no color emoji |
| line_layout | Zed, in-house (platform shapers) | gpui only |

Why it persists: cosmic-text development is tied to System76's COSMIC work,
while Parley has received grants and contributions from people whose employers
include Canva. Those are different, dated forms of support; public evidence
does not establish one uniform current staffing or funding model. egui *tried*
to adopt Parley and the attempt stalled on a genuine architecture mismatch:
Parley wants to own "a rectangle to lay text into," while immediate-mode egui
re-derives layout every frame and wants incremental caching. Iced adopted
cosmic-text in 2023; the cited adoption PR praises cosmic-text but does not
document a comparative decision that Parley was not ready.

Cost in practice: four different sets of text behavior. In the Babel run,
egui shaped individual Arabic/Hebrew words but lacked paragraph-level BiDi,
so multi-word and mixed-direction ordering was wrong. Slint correctly reordered
BiDi runs and passed the tested selection path; its remaining gaps include no
explicit base-direction/default-alignment control, no automatic RTL UI
mirroring, and codepoint-oriented Backspace. GPUI's macOS renderer displayed
the tested BiDi text correctly through CoreText, while the sample editor's
caret/selection geometry failed inside visual-order RTL runs; other GPUI
platforms use different shaping paths. Momentum favors Parley (Slint, Floem,
and Bevy migrated within roughly 12 months), while COSMIC gives cosmic-text a
continuing production reason to exist.

### Story 2: at least six framework-side renderer families

wgpu is the leading common GPU API in this sample, but it is not the only backend and it
does not define a toolkit's 2D renderer. The sample contains at least six
maintained framework-side families, depending on whether Slint's FemtoVG,
Skia, Qt, and software paths are grouped or counted separately. The major
buckets include Vello, epaint, Iced's renderer, Slint's multi-backend renderer
set, and GPUI's platform pipeline. They target a mix of wgpu, direct Metal/D3D paths,
OpenGL/Skia, Qt, and CPU software.

Reasons are philosophical as much as historical: Vello explores GPU-compute
rendering; epaint and Iced include CPU tessellation approaches; GPUI draws
with an in-house primitive pipeline; and Slint needs paths that reach devices
with no desktop GPU. Classic Vello remains alpha-quality; only the newer
Vello Hybrid path has been described as roughly beta. This layer may not need
one universal implementation, but each family still carries integration work
for clipping, gradients, anti-aliasing, caching, text, and driver behavior.

### Story 3: the widening tao/winit fork

In 2021 Tauri forked winit into tao because it needed (a) native menus and
tray, (b) on Linux, windows that are GTK containers — because WebKitGTK (the
system webview used by this stack on Linux) renders inside GTK. Since then,
menus and tray
were extracted into separate winit-compatible crates (muda, tray-icon), so
part of the fork rationale is gone. But the GTK requirement remains, so tao
stays — retaining winit's *pre-0.30* API while winit itself is mid-redesign
toward 0.31. Shared Wayland or macOS changes can therefore require separate
work in the diverging codebases. Dioxus, which inherited Tao via its
Tauri-derived stack, still uses Tao/Wry in 0.7.9; open issue #2706 **proposes**
a winit migration but does not establish that it is underway. Tauri v3
planning targets GTK4 — not an un-fork. The structural options would include
making Wry windowing-agnostic or giving winit a supported GTK embedding mode;
the audit found discussions, not a shipped solution.

### Story 4: the GTK fault line in Linux shell integration

The principal cross-platform shell-integration crates here were built in the
Tauri orbit, but their Linux constraints differ:

- **muda's Linux menubar path requires a `gtk::Window`** — it cannot attach a
  native menubar directly to a plain winit window (iced, egui, Slint,
  Xilem…).
- **tray-icon's official winit example runs a second GTK event loop** on a
  parallel thread; applications need to account for that integration.
- **global-hotkey is X11-only** — on Wayland, global shortcuts require the
  XDG Desktop Portal protocol; the Rust `ashpd` crate exposes that portal, but
  global-hotkey does not integrate it.

Protocol-specific Rust building blocks already exist: `ksni` implements
StatusNotifierItem trays, Rust DBusMenu implementations exist, and `ashpd`
exposes GlobalShortcuts. Native apps can and do integrate shell features, and
the macOS sample in this audit did so. The remaining ecosystem gap is a single
maintained, framework-neutral facade with the tauri-apps crates' cross-platform
coverage and direct winit/Wayland paths. Improving that facade would benefit
native winit apps; it would not remove Tao's separate WebKitGTK embedding
constraint.

## Part 3 — Why AccessKit is a concentrated strategic dependency

- **What AccessKit is:** the dominant shared adapter between Rust toolkits and
  OS screen-reader APIs in this sample (see Part 1). Egui, Slint, Xilem,
  Bevy, Vizia, Blitz, GPUI, and GTK's optional backend use it; Iced and Floem
  remain gaps, web/canvas has no shipped adapter, and Zed currently disables
  GPUI's merged path. AccessKit translates a semantic tree; it cannot author
  correct widget semantics or replace VoiceOver/NVDA/Orca testing.
- **What the authorship snapshot says:** the dated public history counted 363
  all-time commits from founder Matt Campbell and 174 from Arnold Loubriat,
  versus 14 for the next contributor. Recent work was also concentrated around
  those two. This is a continuity-risk signal; it does **not** prove that only
  two people understand the project, that maintainers are unpaid, or that the
  project is inactive.
- **What the STF evidence says:** the **Sovereign Tech Fund** supported a
  2023–24 GNOME Foundation contract covering related accessibility work,
  including AccessKit/AT-SPI plumbing, and GNOME reported that effort mostly
  wrapped up by 2025. This audit found no later direct institutional AccessKit
  grant; personal GitHub Sponsors links remain visible. That is narrower than
  saying all project funding ended.
- **Why this matters strategically:** AccessKit is active and widely adopted,
  while open work remains around web/canvas support, deeper text-editing
  semantics, and the Linux accessibility substrate (the Wayland-oriented
  "Newton" effort remains a prototype). A recurring grant could therefore
  have cross-framework leverage. That is an investment argument based on the
  observed adoption and concentration, not a claim about private finances or
  what work can happen only when paid.

## Part 4 — How to read the sustainability table

The full [load-bearing table](data/load-bearing-crates.md) combines dated
public indicators: release cadence, recent and all-time commit concentration,
documented departures or handoffs, reverse-dependency reach, and funding links
that the audit could find. Examples include a public winit contributor burnout
hiatus during an API redesign, Raph Levien's move from Google to Canva,
completion of the audited GNOME/STF work, RazrFalcon's public handoff, and a
long apple-codesign crates.io release gap.

Those observations justify a **risk assessment**, not certainty about private
employment, current budgets, maintainers' knowledge, or future abandonment.
The useful reading is impact × concentration × succession evidence: a quiet,
stable adapter is not automatically unhealthy, while an active project can
still merit continuity investment. On that basis, an industry consortium such
as RCN could fund shared infrastructure without choosing a framework winner;
the cost and effect would need a project-specific proposal rather than the
blanket label "one-salary problem."
