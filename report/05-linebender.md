# 05 — The Linebender Stack (xilem / masonry / vello / parley / kurbo / peniko + AccessKit)

*RCN "Cross-Platform GUI Desktop Apps" initiative — deep-dive, researched 2026-07-07.
All versions/dates verified against crates.io and primary sources on that date.*

Linebender is not one framework but a **layered stack of independently versioned
crates**, hosted by a community organization ([linebender.org](https://linebender.org/))
that describes itself as "a friendly group of people who share an interest in 2D
graphics and user interface design." That layering is exactly why it is the
leading candidate for a *shared foundation*: every layer below the widget set is
consumed today by non-Linebender projects — including three of this initiative's
other subjects (Slint, Blitz/Dioxus, Bevy) plus Servo.

---

## 1. The stack, layer by layer

| Layer | Crate | Version (date) | Downloads | License | Maturity |
| --- | --- | --- | --- | --- | --- |
| Geometry | [kurbo](https://crates.io/crates/kurbo) | 0.13.1 (2026-05-13) | 30.3 M | MIT/Apache-2.0 | **Mature.** De-facto standard 2D curves/paths crate |
| Paint types | [peniko](https://crates.io/crates/peniko) | 0.6.1 (2026-05-15) | 2.4 M | MIT/Apache-2.0 | Stable-ish vocabulary types (Brush, Gradient, Image); still 0.x churn |
| GPU renderer | [vello](https://crates.io/crates/vello) | 0.9.0 (2026-05-15) | 531 k | MIT/Apache-2.0 | Self-declared **alpha**; usable, not yet artifact-free |
| CPU/hybrid renderers | [vello_cpu / vello_hybrid / vello_common](https://crates.io/crates/vello_cpu) | 0.0.9 (2026-05-30) | 1.29 M (cpu) | MIT/Apache-2.0 | "Sparse strips" rewrite; hybrid "roughly beta quality" per Linebender |
| Font enumeration/fallback | [fontique](https://crates.io/crates/fontique) | 0.11.0 (2026-06-26) | 1.69 M | MIT/Apache-2.0 | Actively maturing in the Parley workspace; now also used by Slint |
| Text layout | [parley](https://crates.io/crates/parley) | 0.11.0 (2026-06-26) | 1.67 M | MIT/Apache-2.0 | Rapid releases; adopted by Bevy, Slint, Blitz |
| Widget layer (retained) | [masonry](https://crates.io/crates/masonry) | 0.4.0 (2025-10-29) | 19 k | **Apache-2.0 only** | Alpha; split into masonry_core / masonry_winit |
| Reactive layer | [xilem](https://crates.io/crates/xilem) | 0.4.0 (2025-10-29) | 8.8 k | **Apache-2.0 only** | Explicitly "alpha-quality software" |
| Accessibility (allied org) | [accesskit](https://crates.io/crates/accesskit) | 0.24.1 (2026-06-12) | 20.6 M | MIT/Apache-2.0 | Production: shipped in egui, Bevy, Slint, more |

MSRV across the stack: 1.85–1.88 ([crates.io metadata](https://crates.io/crates/xilem)).

### kurbo & peniko
[kurbo](https://github.com/linebender/kurbo) (Bézier paths, shapes, affine
transforms) is the most broadly adopted Linebender crate — 30M+ downloads, used
far beyond GUI (druid/piet heritage). The Nov-2025 update reports a 3000×
high-accuracy speedup for cubic-Bézier nearest-point work, but explicitly says
the change **missed the kurbo 0.13 release train**
([tmil-23](https://linebender.org/blog/tmil-23/)). peniko supplies renderer-agnostic
paint types (brushes, gradients, color via the `color` crate) shared by vello
and downstream renderers.

### vello — GPU renderer, and the sparse-strips pivot
Classic [vello](https://github.com/linebender/vello) is a compute-shader
renderer on wgpu (0.9.0 uses wgpu 29; 0.8.0 was essentially "update wgpu 27→28"
([releases](https://github.com/linebender/vello/releases))). The README still
declares it **alpha**, with known work on conflation artifacts, blur/filters,
GPU memory strategy, and glyph caching.

Since 2025 the center of gravity moved to the **sparse strips** architecture in
the same repo: `vello_common` + [`vello_cpu`](https://crates.io/crates/vello_cpu)
(pure-CPU, SIMD via Linebender's `fearless_simd`) and `vello_hybrid` (CPU
geometry + GPU rasterization), all at 0.0.9 with explicitly **no API-stability
guarantees**. The [Q1 2026 report](https://linebender.org/blog/tmil-25/) says
Vello Hybrid has reached "roughly beta quality" after major performance work,
and — a significant strategy change — the planned unified "Vello API"
abstraction was **abandoned as too complex**, in favor of two community
abstraction layers: **AnyRender** (from the Blitz/Dioxus ecosystem, ergonomics-first;
[anyrender 0.11.1](https://crates.io/crates/anyrender)) and **imaging**
(performance-first; Masonry has migrated to it, making Masonry GPU-agnostic and
able to render via Vello CPU).

**Are the Vello renderers production-ready?** They have different maturity
levels. Classic GPU Vello still calls itself **alpha**. Vello CPU is being used
for production-oriented use cases but remains API-unstable at 0.0.x. Vello
Hybrid is described by Linebender as **roughly beta quality**. Servo is adopting
vello/vello_cpu as its 2D canvas backend because raqote is unmaintained
([servo#38282](https://github.com/servo/servo/pull/38282),
[servo#36821](https://github.com/servo/servo/pull/36821),
[tracking servo#38345](https://github.com/servo/servo/issues/38345)), but its
tracking issue lists live defects (stroking performance, blend clipping, line
caps). The 0.0.x versioning of the sparse-strips crates is itself an API-
stability warning. The planned unified low-level Vello API was abandoned;
AnyRender and imaging are the two current renderer-abstraction directions.

### parley + fontique — text layout
[parley](https://github.com/linebender/parley) does rich-text layout: styled
runs, bidi, line breaking, alignment, inline boxes, plus an editing layer
(`PlainEditor`) used by Masonry's text input. Release history
([CHANGELOG](https://github.com/linebender/parley/blob/main/CHANGELOG.md),
dates from [crates.io](https://crates.io/crates/parley/versions)):

- **0.6.0 (2025-10-06)** — switched shaping from **swash to HarfRust**, the
  HarfBuzz project's official Rust port: "production-quality shaping for all
  scripts."
- **0.9.0 (2026-04-21)** — floats/excluded regions ("floating boxes and other
  advanced layouts").
- **0.10.0 (2026-06-01)** — dictionary-based line/word breaking for CJK, Thai,
  Khmer, Lao, Myanmar (opt-in `complex-scripts`).
- **0.11.0 (2026-06-26)** — HarfRust 0.10, line-break customization.

parley 0.11's dependency list confirms the modern text pipeline: `harfrust`
(shaping), `skrifa` (font parsing — Google Fonts' oxidize work), `fontique`
(system font enumeration incl. CoreText on macOS + fallback), and **ICU4X**
components (`icu_segmenter`, `icu_properties`) — **swash is gone entirely**
(crates.io [deps](https://crates.io/crates/parley/0.11.0/dependencies)).
AccessKit text properties are an optional feature wired in since 0.3/0.8.

### masonry — the retained widget layer
[masonry](https://crates.io/crates/masonry) is a *foundational*, non-reactive
retained widget tree — deliberately positioned as something **other frontends
can target**. Since 0.4.0 it is split into `masonry_core` (windowing-agnostic)
plus `masonry_winit` (driver), and per the
[Q1 2026 report](https://linebender.org/blog/tmil-25/) it now consumes
`ui-events` instead of winit types directly. That decoupling prompted a
discussion and prototype direction for embedding Masonry in VST audio-plugin
windows via `baseview`; the cited report does **not** document a completed VST
embedding. Layout is Masonry's own box-constraint system (druid
lineage; a new layout system landed in Q1 2026) — it does *not* use taffy.
Downside: crates.io shows only 3 reverse dependencies (xilem plus two small
projects); this audit found no major documented Masonry adopter independent of Xilem.

### xilem — the reactive layer
Releases: 0.1.0 (2024-05-07), 0.3.0 (2025-05-10), **0.4.0 (2025-10-29)** — a
~6-month cadence ([crates.io](https://crates.io/crates/xilem/versions)).
v0.4.0 brought initial **multi-window support**, styling properties, new
widgets (slider), blinking caret, clipboard basics, a layer system, and better
keyboard navigation; the notes state plainly this is "alpha-quality software"
with "plenty of missing features," and that a changelog will only be kept
*from* 0.4.0 onward ([releases](https://github.com/linebender/xilem/releases)).

### AccessKit — governance and role
[AccessKit](https://github.com/AccessKit/accesskit) is **not a Linebender
project** — it lives in its own GitHub org, led by Matt Campbell — but it is
tightly allied: Matt was one of the four contributors whose Xilem work Google
Fonts funded in 2024 ("Matt on accessibility",
[Xilem 2024 plans](https://linebender.org/blog/xilem-2024/)), and the
[linebender.org](https://linebender.org/) project list does *not* include it.
It is the clearest de-fragmentation success in the whole ecosystem: platform
adapters for **Windows (UIA), macOS (NSAccessibility), Linux (AT-SPI),
Android, and iOS** (web planned), with 115 reverse dependencies including
egui, Bevy (`bevy_a11y`), Slint's winit backend, Blitz, Servo, zng, Freya —
and parley itself
([crates.io reverse deps](https://crates.io/crates/accesskit/reverse_dependencies)).

---

## 2. The strategic question: is Linebender becoming the shared foundation?

### Evidence FOR (all verified July 2026)

1. **Bevy switched its text stack to parley.** Bevy 0.19 migrated `bevy_text`
   from cosmic-text to parley, citing "meaningfully better documentation and
   … somewhat nicer to use"
   ([Bevy 0.19 notes](https://bevy.org/news/bevy-0-19/),
   [bevy#21765](https://github.com/bevyengine/bevy/issues/21765),
   [migration guide](https://bevy.org/learn/migration-guides/0-18-to-0-19/));
   `bevy_text`/`bevy_ui` now require parley ^0.9.
2. **Slint unified on fontique + parley.** Slint 1.14 "unified everything
   behind the Fontique and Parley crates from the Linebender organization"
   for consistent cross-platform behavior and future rich text
   ([Slint 1.14 blog](https://slint.dev/blog/slint-1.14-released),
   [slint#9564](https://github.com/slint-ui/slint/pull/9564),
   [slint#9466](https://github.com/slint-ui/slint/pull/9466)); `i-slint-core`
   requires parley ^0.10 and fontique. femtovg (Slint's GL renderer) also moved
   to parley 0.9+.
3. **Servo is adopting vello for 2D canvas** (vello and vello_cpu backends;
   `servo-canvas` requires vello ^0.9) to replace unmaintained raqote
   ([servo#38345](https://github.com/servo/servo/issues/38345)).
4. **Blitz / Dioxus Native builds on parley + vello** (via the AnyRender
   abstraction; Stylo for CSS, Taffy for box layout)
   ([Blitz README](https://github.com/DioxusLabs/blitz)) — so the
   "HTML-renderer" wing of the ecosystem shares Linebender's text and paint
   layers.
5. **Shaper convergence inside the principal reusable pure-Rust text stacks:**
   parley (since 0.6) and cosmic-text both now shape with
   [HarfRust](https://crates.io/crates/harfrust) (0.12.0, 2026-07-03,
   maintained in the HarfBuzz org). Rustybuzz itself moved to
   [`harfbuzz/rustybuzz`](https://github.com/harfbuzz/rustybuzz); its
   [maintenance/deprecation discussion](https://github.com/harfbuzz/rustybuzz/issues/74)
   is closed, and the active repository is not archived. Platform text APIs
   and browser engines remain separate shapers.
6. **AccessKit** (allied, above) already de-fragmented accessibility across
   egui/Bevy/Slint/Servo/Blitz/Masonry.
7. **Reverse-dep breadth:** vello has 52 dependent crates (bevy_vello,
   vello_svg, velato, floem's vello renderer, cartography, craft…), parley 65
   ([crates.io](https://crates.io/crates/parley/reverse_dependencies)).

### Evidence AGAINST / caveats

1. **The competing text stack is still bigger.** cosmic-text has **137**
   reverse dependencies vs parley's 65, and its consumers include **iced**
   (`iced_graphics`/`iced_tiny_skia`), **Zed's gpui** (gpui on crates.io
   requires cosmic-text ^0.14 — Zed does **not** use parley), glyphon (the
   standard wgpu text renderer), and Cushy/kludgine
   ([crates.io](https://crates.io/crates/cosmic-text/reverse_dependencies)).
   The dated reverse-dependency list can include published Floem releases, but
   current Floem `main` switched from cosmic-text to Parley in March 2026
   ([PR #1034](https://github.com/lapce/floem/pull/1034)).
2. **The top of the stack has little documented external adoption.** Masonry:
   3 reverse deps. No publicly documented shipping production Xilem app was
   found in this audit; visible ecosystem apps include a Runebender port and
   the Scrolled Quran project
   ([tmil-25](https://linebender.org/blog/tmil-25/)).
3. **Renderer API instability:** the unified "Vello API" was abandoned; two
   competing abstractions (AnyRender vs imaging) now sit atop vello — mini
   fragmentation inside the de-fragmentation candidate
   ([tmil-25](https://linebender.org/blog/tmil-25/)).
4. **Funding transition risk:** Google Fonts funded Raph Levien's role and
   four Xilem contributors in 2024
   ([Xilem 2024 plans](https://linebender.org/blog/xilem-2024/)); Raph took a
   voluntary exit from Google (last day 2025-10-12) and joined **Canva** in
   Jan 2026, stating he will continue Linebender work
   ([tmil-19](https://linebender.org/blog/tmil-19/)). Continued Google funding
   post-departure is not publicly confirmed. Parley has two **NLnet grants for
   2026** (internationalization + rich-text copy/paste; modularity + font
   fallback) ([tmil-23](https://linebender.org/blog/tmil-23/),
   [nlnet.nl/project/Parley](https://nlnet.nl/project/Parley/),
   [Parley-copypaste](https://nlnet.nl/project/Parley-copypaste/)).
5. **Governance is informal.** Zulip + weekly office hours; an
   [RFC repo](https://github.com/linebender/rfcs) exists with a defined process
   (final call: Raph Levien) but minimal activity. Blog cadence slipped from
   monthly to quarterly in 2026 (the Q1 post covers three months).

**Net assessment:** the *middle* of the Linebender stack (kurbo, peniko,
fontique, parley, vello, plus allied AccessKit and HarfRust/skrifa/ICU4X) is
measurably becoming shared infrastructure — Bevy, Slint, Servo, Blitz all chose
it in 2025–2026. The *top* (masonry/xilem) remains an experiment with a single
consumer.

---

## 3. parley vs cosmic-text — the core text-stack duplication

| | **parley 0.11** (Linebender) | **cosmic-text 0.19** (System76/pop-os) |
| --- | --- | --- |
| Released | 2026-06-26 | 2026-04-22 |
| Shaping | HarfRust (since 0.6, 2025-10) | HarfRust (recently migrated from rustybuzz) |
| Font parsing | skrifa | swash (rasterization) + fontdb |
| Font loading/fallback | **fontique** (system enumeration incl. CoreText; scriptaware fallback) | **fontdb** + per-character fallback lists |
| Segmentation | ICU4X (`icu_segmenter`), dictionary-based CJK/Thai/Khmer/Lao/Myanmar breaking (0.10) | unicode crates; UDHR-in-500-languages test corpus |
| Rich text | styled runs, inline boxes, floats/excluded regions (0.9), alignment, text-indent | "rich text styling (bold, italic)" per run |
| Editing | `PlainEditor` (used by Masonry) | full editor buffer w/ selection, copy/paste, click detection |
| A11y | optional AccessKit text properties | none built-in |
| Rasterization story | none (renderer's job — vello/glifo) | swash rasterization built-in; glyphon for wgpu |
| Reverse deps (crates.io) | **65** — Bevy, Slint, Blitz, femtovg, krilla (Typst's PDF layer), bevy_vello | **137** — iced, Zed's gpui, glyphon, published Floem releases (current main uses Parley), Cushy, uiua |
| License | MIT/Apache-2.0 | MIT/Apache-2.0 |

Sources: [parley CHANGELOG](https://github.com/linebender/parley/blob/main/CHANGELOG.md),
[parley deps](https://crates.io/crates/parley/0.11.0/dependencies),
[cosmic-text README](https://github.com/pop-os/cosmic-text),
crates.io reverse-dependency APIs (2026-07-07).

**Finding:** the duplication is real but *narrowing from below*. Both stacks
now share HarfRust for shaping, so the remaining fork is in layout, font
database/fallback (fontique vs fontdb) and editing. Momentum flipped in
2025-2026: Bevy defected from cosmic-text to parley, Slint consolidated on
parley/fontique — but cosmic-text retains the larger installed base via iced,
Zed, and glyphon. Two well-maintained text stacks will likely persist
medium-term.

---

## 4. Xilem architecture (vs Elm/iced and immediate mode/egui)

Xilem implements the **reactive view-tree** pattern
([ARCHITECTURE.md](https://github.com/linebender/xilem/blob/main/ARCHITECTURE.md),
[xilem_core](https://crates.io/crates/xilem_core)):

- App state is a plain Rust struct. An `app_logic(&mut State) -> impl WidgetView<State>`
  function runs after **every** state mutation, producing a lightweight,
  short-lived **view tree** (cheap value types like `flex_col((...))`,
  `text_button("Add", callback)`).
- The new view tree is **diffed against the previous one** (`View::rebuild`),
  and only the differences are applied to the long-lived **Masonry widget
  tree** — retained widgets keep layout, text, focus and accessibility state.
- Callbacks receive `&mut State` directly — no message enum, no channels.
- **Lens/map_state/map_action:** `lens` and `map_state` let a child view
  operate on a sub-slice of parent state, giving Elm-style composition without
  Elm-style message plumbing; `map_action` covers the message-passing case
  when wanted. Some architecture prose calls this general pattern "adapt," but
  Xilem 0.4 does not expose a public `adapt` constructor.

vs **iced (Elm)**: iced separates `view(&State) -> Element<Message>` from
`update(&mut State, Message)`; all interaction round-trips through a message
enum. Xilem fuses these — the "message" is a closure bound into the view.
vs **egui (immediate mode)**: egui re-runs immediate widget construction each
frame, but it still retains memory, text/layout caches, interaction state and
accessibility identity keyed by widget IDs. Xilem instead rebuilds cheap *view
descriptions* and retains concrete Masonry widgets, so accessibility trees,
text selections, and animations live directly in stable widget objects.

**Masonry as a standalone target** is an explicit goal — non-reactive API,
now windowing-agnostic (`masonry_core` + `ui-events`). A VST/baseview embedding
was discussed as a motivating possibility, not demonstrated as complete
([tmil-25](https://linebender.org/blog/tmil-25/)). In practice no other major
frontend has adopted it yet (3 reverse deps).

---

## 5. Production readiness — honestly

- **Self-description:** "An experimental Rust architecture for reactive UI"
  ([repo](https://github.com/linebender/xilem)); v0.4.0 notes: "alpha-quality
  software … plenty of missing features … expect major breaking changes"
  ([releases](https://github.com/linebender/xilem/releases)).
- **Release lag / internal skew:** xilem 0.4.0 (Oct 2025) pins vello 0.6 and
  parley 0.6, while standalone vello is 0.9 and parley 0.11 — eight months of
  renderer/text work (including the imaging/GPU-agnostic Masonry migration)
  exists **only on git main**. Our build tree even contains two skrifa
  versions because of this skew.
- **Changelog only since 0.4.0**; docs are docs.rs-level plus examples — no
  book. Docs quality: adequate for the demo path, thin beyond it (~2.5/5).
- **What exists (0.4.0):** flex/grid/zstack/split/indexed_stack layouts,
  portal (scroll), virtual_scroll, label/prose/text_input, button, checkbox,
  slider, progress_bar, spinner, image, initial multi-window, styling
  properties, clipboard basics, layers
  ([view module docs](https://docs.rs/xilem/0.4.0/xilem/view/index.html)).
  Git main adds Svg, Divider, CollapsePanel, a new layout system
  ([tmil-25](https://linebender.org/blog/tmil-25/)).
- **What's missing:** native/context menus (open
  [tracking issue #1343](https://github.com/linebender/xilem/issues/1343)),
  combobox/select, tables/trees, file dialogs and OS integration (tray, global
  shortcuts, notifications), theming beyond the young property system (dark
  default only), localization. Multi-window is "initial." No stable-identity
  keyed list view.
- **Trajectory (2024→2026 reports):** 2024 = Google-funded foundation year
  (four funded devs; [xilem-2024](https://linebender.org/blog/xilem-2024/),
  [backend roadmap](https://linebender.org/blog/xilem-backend-roadmap/));
  2025 = masonry_core split, pass-throughs, multi-window, releases 0.3/0.4;
  2026 Q1 = imaging migration (GPU-agnostic, vello_cpu rendering), ui-events,
  new widgets, and discussion of VST embedding
  ([tmil-25](https://linebender.org/blog/tmil-25/)).
  Direction is steady and the public pace is deliberate: 0.3→0.4 took about
  six months, and the 2026 Q1 post covered a quarter. The sources do not prove
  that the cadence change was caused by the Google funding transition.

**Verdict:** the foundation layers are adoptable today (and are being
adopted); xilem itself is not yet a sensible choice for shipping a commercial
desktop app in 2026 — it is a bet on 2027+.

## 6. Accessibility depth

Integration is structural, not bolted on: `masonry_core` depends on
`accesskit` directly and every Masonry widget implements an accessibility pass
building the AccessKit tree. Parley can emit the full set of AccessKit text
properties, including selection and caret semantics;
`accesskit_winit`/`accesskit_macos` then wire the tree to platform adapters.
`accesskit_consumer` belongs on that adapter/consumer side, not inside Parley's
selection or caret implementation. These dependencies are visible in our
app's tree (AccessKit 0.21 under masonry_core, Parley and the winit adapter).
Retained widgets give AccessKit stable node identity. Caveats: the stack's
accesskit version (0.21 in the 0.4 release) trails standalone AccessKit
(0.24.1), and end-to-end screen-reader polish (VoiceOver/NVDA behavior of
every widget) is not systematically documented or tested publicly.

## 7. Platform matrix

| Platform | Status | Evidence |
| --- | --- | --- |
| Windows / macOS / Linux | Primary targets (winit + wgpu/vello) | [repo](https://github.com/linebender/xilem); this report's app ran on macOS 26.5 |
| Android | Real but demo-grade: of 21 audited example source files, 8 contain an Android entrypoint and 7 declare explicit Android targets; this is substantial example coverage, not every example. A 2024 Google-funded workstream covered Android platform integration. | [to_do_mvc example](https://github.com/linebender/xilem/blob/main/xilem/examples/to_do_mvc.rs), [xilem-2024](https://linebender.org/blog/xilem-2024/) |
| iOS | No support in xilem (AccessKit has an iOS adapter, winit iOS exists, but no xilem story) | — |
| Web/wasm | Two distinct answers: (a) Vello-on-WebGPU, whose effective platform matrix follows browser WebGPU support. The Vello README's older “Firefox/Safari experimental” shorthand is stale: [Safari 26 ships WebGPU](https://webkit.org/blog/17333/webkit-features-in-safari-26-0/), while [Firefox enables it on Windows and Apple-silicon macOS](https://developer.mozilla.org/en-US/docs/Mozilla/Firefox/Experimental_features) and still limits Linux/Intel-macOS support to Nightly in the audited status; (b) **xilem_web**, a separate sibling crate targeting the DOM directly (shares xilem_core, *not* masonry/vello). | [xilem repo](https://github.com/linebender/xilem) |

## 8. License, backing, cadence, governance and maintainer concentration

- **License:** vello/parley/kurbo/peniko/fontique MIT OR Apache-2.0;
  **xilem and masonry are Apache-2.0 only** (crates.io) — a nonstandard choice
  for Rust that some dual-license-requiring consumers flag.
- **Backing history:** Google Fonts funded Raph Levien's research role
  (2020–2025) and four Xilem contributors in 2024 (Hamilton, McNab, Campbell,
  Faure) ([xilem-2024](https://linebender.org/blog/xilem-2024/)). Raph left
  Google 2025-10-12 → **Canva** (Jan 2026), continuing Linebender; Canva
  developers contribute to Vello ([tmil-19](https://linebender.org/blog/tmil-19/)).
  **Two NLnet/NGI0 grants fund Parley work in 2026**
  ([NLnet: Parley](https://nlnet.nl/project/Parley/),
  [NLnet: Parley-copypaste](https://nlnet.nl/project/Parley-copypaste/)).
- **Cadence:** parley and Vello release several times per year. Xilem's cadence
  is slower and uneven: 0.3 arrived in May 2025 and 0.4 in October 2025, while
  the preceding gap was longer. The public update cadence moved from monthly
  posts to a Q1 2026 roundup.
- **Governance/maintainer concentration:** informal community org — Zulip, weekly office
  hours, an RFC process with Raph as final decision-maker
  ([rfcs repo](https://github.com/linebender/rfcs),
  [contributor guidelines](https://linebender.org/contributor-guidelines/)).
  Public contributions show several recurring per-crate leads. Any precise
  maintainer count or description of one person as the singular architectural
  authority is an analytical judgment, not a published governance fact.

## 9. Friction log (from building `apps/xilem-app/`, xilem =0.4.0)

Full details in [`apps/xilem-app/GAPS.md`](../apps/xilem-app/GAPS.md).

1. **Spec fully expressible — zero functional gaps.** Placeholder, Enter-to-submit
   (`.on_enter` + `InsertNewline::Never`), per-row delete, live counter, portal
   scrolling all exist as first-class APIs. Caveat: the spec app is nearly
   identical to xilem's own `to_do_mvc` example — we measured its best-tested path.
2. **One compile-fix iteration:** `FlexSpacer::Fixed` takes a `Length`, not
   `f64` (circulating examples show the old signature). Compiler error was
   excellent (pointed into xilem's source).
3. **Version skew is the real friction:** the latest release pins vello 0.6 /
   parley 0.6 (8 months old); duplicate skrifa versions in-tree; everything
   interesting from Q1 2026 requires git main.
4. **Canonical measurements** (M4 Pro, rustc 1.96.1): clean release build
   **28 s**, forced incremental rebuild (after touch main.rs) **1 s**, **143 unique crate names / 154
   name-version entries including the app**, binary 11,944,000 bytes raw and
   **9.7 MiB stripped**, app 81 LoC. The app launched and stayed alive with no
   runtime warnings; title/placeholder/counter were manually observed. The
   original SPEC-1 screenshot artifact was not retained. One future-incompat
   warning (`block v0.1.6` via copypasta) was emitted.
5. **Defaults are spartan:** dark-only theme, no menu bar, no macOS
   app-nap/dock niceties — the "OS integration" layer is simply not there yet.
