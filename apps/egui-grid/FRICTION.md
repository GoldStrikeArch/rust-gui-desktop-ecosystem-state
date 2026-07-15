# FRICTION.md — Grid (egui 0.35 + eframe + egui_extras)

App: `apps/egui-grid/` · package `egui-grid` · `cargo run --release`
Build: release, clean (no warnings). Launch verified on macOS: window up,
self-test script ran to completion, alive after 10 s, killed. 6 egui_kittest
tests (filter typing, sort toggling, row click + shift-range click through
the pointer path, divider-drag column resize, selection-model unit test,
offscreen wgpu render). Evidence retained in `verify-stdout.log` (stdout of
the verified launch) and `screenshot-kittest.png` (rasterized UI: grid,
chips, counter — inspected).

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| table_widget | **built-in** (helper crate from the egui org) | observed (live launch + inspected kittest render) | Core egui has `Grid`, which retains nothing and lays out everything — useless at 100k. The ecosystem path is `egui_extras::TableBuilder` (first-party, lives in the egui repo, version NOT offset: 0.35.0 ↔ egui 0.35.0 — unlike egui_plot/egui_dnd). Declarative columns + header row + body closure; ~40 LoC for the whole table. |
| virtualization | **built-in** | synthetic-input (scripted scroll) + observed (render shows ~25 rows laid out) | `TableBody::rows(height, 100_000, closure)` lays out only on-screen rows (fixed row height ⇒ O(1) offset math). Self-test jump-scrolled the full 100k range in ~4 s of frames with no degradation; RSS stable (see below). `heterogeneous_rows` exists for variable heights. |
| sort | **assembled** | self-test (kittest pointer clicks) | No sorting support in the widget at all — headers are just cells. Hand-assembled in ~30 LoC: clickable `selectable_label` in each header cell, `▲/▼` suffix as the indicator, `sort_unstable_by` on a `Vec<u32>` index vector (typed per-column comparators, `f64::total_cmp` for values). kittest asserts asc→desc toggle. |
| filter_latency | **assembled** | self-test (kittest typing) + synthetic-input (scripted queries; stdout log) | Substring scan over 100k names + index-vector rebuild each keystroke. From `verify-stdout.log`: `FILTER_MS 1 5.70`, `FILTER_MS 4 3.59` (earlier run: 2.06/3.92 — all well under a frame). Immediate mode means zero extra plumbing: rebuild the index vector, done. |
| column_resize | **built-in** | self-test | `TableBuilder::resizable(true)` + per-`Column` initial/min widths gives drag-on-divider resize (plus double-click autosize). Proven by a kittest pointer-drag on the ID/Name divider: press, 3 moves, release — Name column shifted ~40 px. Expected the weakest cell; was actually free. |
| row_selection | **assembled** | self-test (kittest) | `TableBuilder::sense(Sense::click())` + `TableRow::response()` (union of cell responses) + `set_selected(bool)` for the highlight are built in; the selection *model* (HashSet by data index, shift-range from anchor, cmd-toggle) is mine (~25 LoC). **Trap:** cell text stole all clicks until `style.interaction.selectable_labels = false` — selectable label text senses click+drag for text selection, so clicks on the text never reach the row response. Found via kittest, fixed in the table scope. |
| cell_custom_render | **built-in** | observed (inspected kittest render: green/amber/red chips) | Each cell is a closure over `&mut Ui`, so a colored status chip is just `Frame::new().fill(color).corner_radius(8).inner_margin(..)` around a `RichText` label. Right-aligned numeric cells via `with_layout(right_to_left)`. No cell "renderer registry" needed — immediate mode's best case. |

## Helper crates & why

- `egui_extras = "=0.35.0"` — the table widget (virtualized rows, resizable
  columns, striped rows, row sense/selection). First-party (egui repo), so
  the version tracks egui exactly — the offset trap from iteration 2
  (egui_plot 0.36 → egui 0.35) does **not** apply here; verified via
  crates.io dependency metadata (egui_extras 0.35.0 → egui ^0.35.0).
- Seeded PRNG hand-rolled (SplitMix64, 10 LoC) instead of the `rand` stack —
  deterministic data per SPEC with zero extra deps.
- Dev-only: `egui_kittest = "=0.35.0"` (feature `wgpu` for offscreen
  rasterization) + `image 0.25` to save the evidence PNG.

## Measurements (Apple M4 Pro, release build)

- `BUILD_MS 18.35` (100k rows generated + index vector built; stdout).
- `FILTER_MS 1 5.70` / `FILTER_MS 4 3.59` (also 2.06/3.92 in a prior run).
- RSS via `ps -o rss= -p <pid>` (KiB, /1024 → MiB): ~30.7 MiB data-only
  before the window; **~106.5 MiB** once the wgpu surface is up (after
  load, idle); **~112.6 MiB** right after the scripted full-range scroll;
  settles back to ~88 MiB idle afterwards. Virtualization holds: scrolling
  the whole 100k range moves RSS by only a few MiB.
- Row/self-test evidence: `verify-stdout.log` in this directory.

## LoC / time

Production 391 (src/main.rs above the test module) · verification 225
(154 kittest/unit tests + 71 src/selftest.rs scripted driver; the driver
ships in the binary but is inert without `GRID_SELFTEST=1`).
Where the time went: ~40% debugging the selectable-labels click-stealing
(minimal-repro kittest harness, then reading egui_extras/egui source —
`StripLayout` builds each cell as a `UiBuilder::sense` child Ui and
`TableRow::response()` is the union of cell responses); ~25% self-test
driver + RSS measurement scripting (first `ps` samples land during the
slow first launch and read garbage — gate on stdout markers, not sleeps);
~20% table/sort/filter wiring (easy); ~15% FRICTION/verification.

## Surprises

- Bad: default-selectable label text silently eats row clicks in
  `sense()`-enabled tables; nothing in the TableBuilder docs warns about it.
- Good: column resize (the predicted weakest cell) is a one-liner including
  double-click autosize.
- Good: kittest `Node::click` is real pointer simulation (press+release at
  node center), and `Harness::input_mut().modifiers` lets you shift-click —
  so range selection is testable through the UI.
- Neutral: 100k-row sort/filter each land in single-digit ms, so no
  incremental/streaming model was needed — immediate mode just rebuilds.
