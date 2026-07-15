# FRICTION — slint-dash (Pulse, Slint =1.17.1)

Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2. Built with default slint
features (winit backend, femtovg GL renderer).

## Capability ratings

| Capability | Rating | Notes |
|---|---|---|
| DnD card reorder | **hand-rolled** | No grid/list reorder widget. Built on `TouchArea` (`moved`, `pressed-x/y`, `pointer-event`) with my own 5px threshold, ghost overlay, slot hit-testing math, and a `reorder(from,to)` callback into Rust that permutes a `slot` field. Key trick: the dragged card never moves — a separate ghost element follows the cursor — so the TouchArea's local coordinate space stays stable (avoids the classic self-referential feedback loop when an element tracks its own drag). Slint 1.17 *does* have `DragArea`/`DropArea` (used in the sibling slint-board app), but they're payload/target-oriented; for pure visual reorder inside one grid the TouchArea route is more direct. |
| Live data (timer @ 10 Hz) | **built-in** | `slint::Timer` (`TimerMode::Repeated`) on the Rust side; restarted with a new `Duration` when the slider changes, `stop()` on pause. A declarative `Timer` element also exists in the DSL since 1.8. Zero friction. |
| Sparklines + main chart | **hand-rolled** | No chart widget at all. Expected path (and what I did): `Path` element with SVG-syntax `commands` strings rebuilt in Rust every tick (6 sparklines @ 60 pts + line & area-fill of 300 pts), normalized into a fixed viewbox. Ergonomics: string-building for charts feels regressive and costs CPU (commands are re-parsed + re-tessellated per frame), axes/ticks/labels/scaling are all on you, and there is no path hit-testing. It works and looks fine, but it's the weakest capability in this app — roughly a third of total effort went here. |
| Hover crosshair + tooltip | **assembled** | `TouchArea.has-hover` + `mouse-x` → sample index; value read from a `[float]` model mirror of the chart data; crosshair/marker/tooltip are conditional Rectangles (~35 lines of .slint). No tooltip primitive, but composition is straightforward and fully reactive. |
| Slider control | **built-in** | `std-widgets` `Slider { minimum; maximum; changed(v) }`. Trivial. |
| Click-to-select | **assembled** | Would be `TouchArea.clicked` (built-in), but because the same TouchArea also drives dragging I had to disambiguate click vs drag myself in `pointer-event(up)` — using `clicked` naively would fire after every drop. |
| Animation | **built-in** | Slint's home turf. Primitives: declarative `animate <props> { duration, delay, easing, iteration-count }` on any property, easing keywords + `cubic-bezier(...)`, plus `states` with `in`/`out` transitions. No springs and no per-frame callback API in the DSL (a 60 Hz `Timer` is the escape hatch). Used here: card reorder settle (`animate x, y` with cubic-bezier — reorder animates "for free" because cards are positioned from a `slot` field), hover elevation (animated `drop-shadow-*`), selection border fade, drop-indicator glide. |

## Helper crates

None. Even the random walk is a hand-rolled xorshift to avoid `rand`.

## Repaint strategy

Property writes mark the item tree dirty; the winit/femtovg (GL) backend then
redraws the **full window** once per tick (10–60 Hz). No repaint when paused or
idle. The controlled 30-second 10 Hz sample measured **9.2% CPU**, **10.0% peak
CPU** and **95 MiB total process-tree RSS**. The earlier ~19% lifetime reading
included startup and is not comparable; raw per-second samples were not
retained. Chart `Path` re-tessellation is the likely driver; partial repaint is
only available on the software renderer.

## LoC

- Rust: 271 (`src/main.rs`) + 3 (`build.rs`)
- Slint DSL: 393 (`ui/main.slint`)
- Total: 667

## Measurements

- Canonical clean release build **40 s**; no-op incremental build **3 s**.
- Dependency graph: **302 unique crate names / 311 name-version entries
  including the app**.
- Binary: **12,925,648 bytes raw / 11,709,576 bytes (11.2 MiB) stripped**.

## Where the time went

1. ~35% charts: Path command generation, normalization, viewbox/stroke
   behavior, area fill closure.
2. ~30% hand-rolled DnD: ghost-vs-move design decision, slot hit-testing,
   click/drag disambiguation.
3. ~15% hover tooltip math (index ↔ x mapping, clamping).
4. Rest: layout/styling/timer plumbing (near-zero friction).

## Surprises

- Good: positioning cards from a model `slot` field + `animate x, y` gives a
  fully animated grid reflow in 2 lines — the repeater keeps element instances
  stable, only data flows.
- Good: `pressed-x/pressed-y` + the "static card, floating ghost" pattern made
  hand-rolled dragging much less painful than expected (~60 lines total).
- Bad: charts really are the ecosystem gap — everything is strings into `Path`.
  No plot widget, no third-party Slint chart crate to reach for.
- Bad: naive `clicked` + drag on one TouchArea double-fires; you must own the
  whole pointer state machine once any dragging is involved.
