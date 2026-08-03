# The interactivity difficulty curve: two interaction-heavy apps, seven frameworks

**Run date:** 2026-07-07 for the retained serial builds; evidence and prose
reconciled through 2026-07-09.

Iteration 2 of the experiment. Where the todo app (SPEC.md,
[10-empirical-results.md](10-empirical-results.md)) tested forms and lists,
this round tested what production desktop apps are actually made of:
**drag-and-drop, live data, custom chart drawing, hover interactions, inline
editing, and animation.** Two apps per framework, same pinned versions as
iteration 1, same machine (Apple M4 Pro, macOS 26.5.2, rustc 1.96.1):

- **"Pulse"** (`apps/SPEC-2.md` → `apps/<fw>-dash/`): 6 metric cards with
  sparklines, drag-to-reorder grid, 10–60 Hz live data with a rate slider,
  a big scrolling line chart with hover crosshair + tooltip, animations.
- **"Board"** (`apps/SPEC-3.md` → `apps/<fw>-board/`): 3-column kanban with
  cross-column drag-and-drop, drop indicators, drag ghosts, double-click
  inline editing, per-column scrolling, drop animations.

All 14 apps built and survived the scripted launch check; a later audit saw an
on-screen window for every current binary. Capability evidence ranges from
source/API-path review to app hooks and synthetic input, with approximations
recorded in each `FRICTION.md`:
**built-in** (a widget/API does it) → **assembled** (composed from framework
primitives, small glue) → **hand-rolled** (you implement the mechanics) →
**not-achievable** (documented approximation). Raw agent data:
[data/interactive-rows.md](data/interactive-rows.md).

## The capability-difficulty matrix

**Pulse (dashboard):**

| Capability | iced | egui | gpui | tauri | xilem | slint | dioxus |
|---|---|---|---|---|---|---|---|
| DnD card reorder | hand-rolled | **assembled**¹ | **assembled** | hand-rolled | hand-rolled | hand-rolled | hand-rolled |
| Live data (10–60 Hz) | built-in² | assembled | assembled | assembled | built-in | **built-in** | assembled³ |
| Charts (sparklines + line) | hand-rolled | **assembled**¹ | hand-rolled | hand-rolled | hand-rolled | hand-rolled | hand-rolled |
| Hover crosshair + tooltip | hand-rolled | **built-in**¹ | hand-rolled | hand-rolled | hand-rolled | assembled | assembled |
| Slider | built-in | built-in | **hand-rolled** | built-in | built-in | built-in | built-in |
| Click-to-select | built-in | built-in | built-in | built-in | hand-rolled | assembled | built-in |
| Animation | assembled | assembled | built-in | assembled | hand-rolled | **built-in** | assembled |

**Board (kanban):**

| Capability | iced | egui | gpui | tauri | xilem | slint | dioxus |
|---|---|---|---|---|---|---|---|
| Cross-column DnD | hand-rolled | hand-rolled | **assembled** | hand-rolled | hand-rolled | **assembled**⁴ | hand-rolled |
| Within-column reorder | hand-rolled | hand-rolled | assembled | hand-rolled | hand-rolled | assembled | hand-rolled |
| Drop indicator | assembled | hand-rolled | assembled | assembled | assembled | assembled | hand-rolled |
| Drag ghost/preview | assembled | assembled | **built-in** | **built-in** | assembled | hand-rolled | hand-rolled |
| Inline edit (dbl-click) | assembled | assembled | hand-rolled⁵ | assembled | hand-rolled⁵ | assembled | assembled |
| Add/delete cards | built-in | assembled | hand-rolled⁵ | assembled | assembled | assembled | assembled |
| Drop/reorder animation | hand-rolled | assembled | assembled | hand-rolled⁶ | hand-rolled | assembled | assembled |
| Column scrolling | built-in | built-in | built-in | built-in | built-in | built-in | built-in |

¹ via helper crates `egui_dnd` + `egui_plot` — the only implementation in this
experiment that chose interaction-specific chart/DnD helpers. Dioxus added
Tokio for its timer, and current Iced- and GPUI-compatible chart/DnD helpers
also existed, with uneven scope and maturity.
² iced's `time::every` is right but doesn't compile under default features
(needs the `tokio`/`smol` executor feature; the error gives no hint).
³ dioxus runs on tokio internally but re-exports no timer — you must add
tokio yourself.
⁴ Slint 1.17 ships first-class `DragArea`/`DropArea` elements — new since
iteration 1's research; they model *data transfer*, so ghost/indicator/index
math is still yours.
⁵ GPUI 0.2.2 ships no reusable first-party high-level text-input widget, so
this app hand-rolled a minimal editor without IME/selection/clipboard. Xilem
does ship TextArea/TextInput views, but this inline-edit path still needed a
custom wrapper for double-click, autofocus, and Escape behavior.
⁶ tauri = hand-scheduled FLIP transforms tweened by CSS.

Crude difficulty score (built-in=0, assembled=1, hand-rolled=2, summed over
both apps — treat as ordering, not measurement):
**egui 14 ≈ slint 14 < iced 16 = gpui 16 < tauri 17 < dioxus 18 < xilem 21.**

## Code volume for the same two specs

| Framework | Pulse LoC | Board LoC | Total | Helper crates |
|---|---:|---:|---:|---|
| egui | 431 | 607 | 1,038 | egui_plot, egui_dnd |
| dioxus | 524 | 412 | **936** | tokio (timer only) |
| slint | 667 | 499 | 1,166 | — |
| gpui | 733 | 515 | 1,248 | — |
| tauri | 733 | 466 | 1,199 | — |
| iced | 777 | 617 | 1,394 | — |
| xilem | 1,233 | 1,160 | **2,393** | — |

These are the canonical `.rs` + `.slint`/HTML/JS/CSS counts from the measured
trees, excluding JSON/TOML config but including verification hooks stored in
source. They are not production-only LoC; future runs should report production
and harness code separately. Dioxus was the lowest total here; egui was second
overall and lowest among the five framework-drawn implementations.

## Runtime cost at 10 Hz (the repaint-model comparison)

30-second sample of the running Pulse apps, live data at the default 10 Hz
(`scripts/runtime-sample.sh`; %CPU is per-core; webview figures include the
WebKit helper processes spawned at launch):

| Framework | avg CPU | peak CPU | max RSS | Repaint model (as implemented) |
|---|---:|---:|---:|---|
| iced | **3.5%** | 4.6% | 95 MiB | message-driven; canvas caches cleared once per tick |
| gpui | 6.8% | 9.5% | **79 MiB** | event-driven notify; whole-scene GPU repaint per dirty frame |
| slint | 9.2% | 10.0% | 95 MiB | property-dirty full redraw; Path re-tessellation is the cost |
| egui | 9.6% | 11.7% | 109 MiB | reactive `request_repaint_after` (frame rate ≈ tick rate) |
| xilem | 9.9% | 12.4% | 106 MiB | rebuild+diff → vello full-scene re-render per tick |
| dioxus | 11.9% | 14.2% | 208 MiB | signal write → VDOM diff → DOM edits over webview IPC |
| tauri | 14.0% | 17.4% | 211 MiB | Rust events → rAF-coalesced Canvas2D redraw |

Two clean tiers in these implementations: native-GPU frameworks ran a ticking dashboard at 3–10% of
one core in ~80–110 MiB, webviews at 12–14% in ~210 MiB. Iced had the lowest
measured CPU in this sample; the immediate-mode CPU-tax
folklore about egui did not materialize (its reactive scheduler repaints at
the tick rate, not continuously). No controlled board-idle dataset was retained.

## Build metrics (all 14 apps)

Clean release build (cargo clean → build, warm registry), incremental rebuild
after touching main.rs, stripped binary size, unique crate names
(`measurements/results-iter2.csv`; the retained xilem-board build log is the
authoritative 26.37-second observation):

<!-- BEGIN GENERATED: iter2-builds -->
| App | Clean | Incr | Binary (stripped MiB) | Unique names | | App | Clean | Incr | Binary (stripped MiB) | Unique names |
|---|---:|---:|---:|---:|---|---|---:|---:|---:|---:|
| iced-dash | 25 s | 2 s | 8.8 | 170 | | iced-board | 23 s | 2 s | 8.5 | 140 |
| egui-dash | 26 s | 1 s | 10.9 | 162 | | egui-board | 24 s | 1 s | 10.6 | 156 |
| gpui-dash | 54 s | 2 s | 4.5 | 391 | | gpui-board | 54 s | 2 s | 4.3 | 391 |
| tauri-dash | 35 s | 9 s | 6.3 | 204 | | tauri-board | 36 s | 9 s | 6.4 | 204 |
| xilem-dash | 28 s | 2 s | 9.7 | 143 | | xilem-board | 34 s | 2 s | 9.9 | 143 |
| slint-dash | 40 s | 3 s | 11.2 | 302 | | slint-board | 48 s | 7 s | 13.6 | 302 |
| dioxus-dash | 33 s | 1 s | 5.0 | 279 | | dioxus-board | 31 s | 1 s | 5.0 | 279 |
| freya-dash | 29 s | 1 s | 18.0 | 192 | | freya-board | 30 s | 1 s | 18.5 | 192 |
| vizia-dash | 19 s | 1 s | 19.5 | 128 | | vizia-board | 17 s | 1 s | 19.6 | 128 |
| floem-dash | 42 s | 1 s | 14.4 | 226 | | floem-board | 42 s | 1 s | 14.3 | 226 |
<!-- END GENERATED: iter2-builds -->

Moving from the todo app to these interaction-heavy implementations moved most
clean builds by seconds (iced 22→25 s, egui 27→26 s even with two extra helper
crates), consistent with dependency compilation dominating at this scale.
Incremental rebuilds stay in the 1–2 s loop everywhere except Tauri's 9 s
build-script tax and the observed 7 s slint-board rebuild. The latter cannot be
explained by DSL source size alone: slint-board has 367 `.slint` lines versus
slint-dash's 393, yet the dashboard rebuilt in 3 s.

## What the round actually taught (cross-cutting)

1. **Six implementations hand-rolled their charts.** Egui alone used
   `egui_plot` in this experiment, but current compatible options also include
   `plotters-iced2`, `iced_plot`, gpui-component charts, gpui-d3rs, gpui-px,
   and plotters-gpui. Coverage and maturity remain uneven; implementation
   choice is not evidence that no ecosystem crate exists.
2. **The tested built-in DnD surfaces differed substantially.** GPUI supplied
   typed payloads, a framework-painted ghost, and per-listener bounds on every
   drag move. Slint 1.17 shipped `DragArea`/`DropArea`, and egui supplied a
   payload store plus a list-reorder helper crate. The other implementations
   in this experiment built pointer state machines themselves; their code was
   not isolated into a comparable DnD-only LoC measure. The webviews' HTML5
   advantage is real but partial: a free native ghost, but wry requires
   `dragDropEnabled: false` (or all HTML5 drag events are silently
   swallowed) and Dioxus's Rust-side events expose no element geometry.
3. **There is no shared automatic layout-transition abstraction.** Stock
   Iced/Xilem layouts snapped; other implementations used egui tweens,
   hand-built CSS FLIP, or explicit Slint x/y animation after moving away from
   normal layout. The implementation paths ranged from CSS transitions in the
   webviews, through Slint declarative `animate`, Iced 0.14 `Animation`, GPUI
   `with_animation`, and egui id-keyed helpers, to hand-built frame logic in
   Xilem/Dioxus-Rust. This experiment defined no metric that would rank those
   primitives, and used no springs.
4. **The text-input widget gap taxes everything above it.** Inline editing
   was "assembled" in five frameworks. GPUI needed a hand-rolled editor; Xilem
   supplied a text area but needed custom interaction/focus plumbing around it.
   Missing high-level editor behavior compounds in every feature that touches
   text.
5. **Timers/live data are a solved problem everywhere.** Slint and Xilem expose
   direct defaults; Iced also has a framework API, feature-gated behind an
   misleading compile error, egui's is an idiom (deadline math), dioxus
   makes you add tokio yourself.
6. **Every implementation recorded at least one framework-specific trap**:
   wry's
   `dragDropEnabled` default eating drag events; egui's hit-test making
   buttons inside stock drag sources silently inert (found via a failing
   kittest test); masonry's last-wins pointer capture breaking stock buttons
   inside draggable containers; Dioxus mouse events lacking target geometry;
   iced's `text_input` capturing Escape from `keyboard::listen()`; Slint's
   `drag-image` being bitmap-only; and GPUI's lack of a general element-bounds
   query outside drag events, which forced paint-time bounds plumbing for
   geometry-aware interactions. Framework choice is partly about which such
   integration costs a team is equipped to absorb.
7. **Version-matched documentation was a tax in these implementations.** Every
   agent ended up reading vendored crate sources or version-pinned examples
   because hosted docs lagged or 404'd (iced 0.14 thin docs.rs, xilem 0.4
   docs matching git-main not the release, slint docs restructure, egui
   0.34/0.35 renames, dioxus dx-centric guides). This does not establish a
   universal one-minor-version expiry rule.
8. **Verification quality note:** egui retains executable `kittest`
   assertions. Xilem and Tauri record synthetic-input/app-hook verification in
   source and FRICTION narratives, but standalone reusable harnesses and raw
   output were not retained; those claims should not be presented as equally
   reproducible.

## Caveats

- One implementation per framework per app, written by an AI agent in
  framework-idiomatic style — effort ratings reflect *framework surface*,
  not developer skill variance. Gaps and approximations are documented
  per-app in `FRICTION.md`.
- CPU/RSS sampled on one machine at one tick rate; webview helper-process
  attribution is best-effort (new `com.apple.WebKit` processes during the
  sampling window).
- The difficulty score is an unweighted sum of ordinal ratings — use it as
  an ordering; read the per-capability rows for decisions.
