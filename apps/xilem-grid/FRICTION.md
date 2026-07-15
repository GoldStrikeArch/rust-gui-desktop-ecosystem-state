# FRICTION — xilem-grid ("Grid", xilem 0.4.0 from crates.io)

App: `apps/xilem-grid/` · package `xilem-grid` · `cargo run --release`.
Build: release, clean (no app warnings; only the ecosystem-wide `block v0.1.6`
future-incompat note from objc transitive deps). Launch verified on macOS
(M4 Pro): window up, full scripted CGEvent interaction pass (filter typing,
sort toggling, click/shift/cmd selection, divider drag, deep scrolls), alive
after 10 s. Evidence retained in `verify/` — `run7-stdout.log` (BUILD_MS /
FILTER_MS / SORT_MS), `run7-runtime.log` (RSS samples + the exact `ps`
commands), `run7-shot*.png` (inspected screenshots), plus an earlier agent's
run1–run6 logs/screenshots. Note: the original build transcript was lost;
ratings below are from code audit + a fresh observed run.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| table_widget | **hand-rolled** | observed | xilem/masonry 0.4 has no table/grid-with-headers widget (masonry's `Grid` is a layout, retains all children — useless at 100k). The table is built from scratch: header = `flex_row` of custom `RowFrame` cells + `DragHandle` dividers; body rows = `flex_row` of fixed-width `sized_box(label)` cells inside `RowFrame`. The two custom masonry widgets + their xilem `View` plumbing are 536 LoC (widgets.rs) — half the app. |
| virtualization | **built-in** | observed (scripted deep scrolls + RSS) | masonry 0.4 ships `VirtualScroll` exposed as the stock `virtual_scroll(0..n, row_view)` view — anchor-based, only on-screen rows exist as widgets; the driver rebuilds rows as the active range changes. Scrolled ~130k px deep (scripted, 2 passes) with no visible degradation, 0.1–0.3% CPU during idle sampling; RSS 130.4 → 151.1 MiB across the whole scroll (see below). Caveats coded around: ids can transiently be outside the valid range (placeholder branch), and an empty range is documented jank (swapped for a placeholder label via `Either`). |
| sort | **assembled** | synthetic-input + observed (stdout, screenshots) | No header/sort support anywhere, but once header cells are clickable (custom `RowFrame` — see row_selection) sorting is plain model code: `sort_unstable_by` on a `Vec<u32>` index vector, tie-break by id, `^`/`v` text indicator. Verified: clicking "Value" gives `SORT_MS Value asc 18.611`, second click `desc 1.583` (reversing an already-sorted vector is near-free); screenshot shows `Value v` + descending values. Unicode ▲/▼ render as tofu with masonry's default fonts (known from xilem-board), hence ASCII. |
| filter_latency | **assembled** | synthetic-input + observed (stdout) | Substring scan over 100k cached-lowercase names rebuilding the visible index vector, self-timed. This run (typing "amber", then deleting): `FILTER_MS 1 1.495` … `FILTER_MS 4 6.098`, `FILTER_MS 5 4.432`; deletions 3.1/4.7/2.8/1.8 ms; clear 0.010 ms. Earlier run: 4.1 ms @ 1 char, 4.3 ms @ 4 chars. Typical: **1-char ~1.5–4 ms, 4-char ~3–6 ms** — well under a frame. The subsequent view diff/paint is excluded from the number but visually instant; `virtual_scroll` re-anchors to the shrunken range without fuss. |
| column_resize | **hand-rolled** | synthetic-input + observed (screenshot) | The predicted weakest cell — and it was: no drag primitive, no resizable-panel widget usable inside a header row (`split` is a two-pane container). Custom `DragHandle` masonry widget: `capture_pointer()` on press, converts pointer positions to window coords, reports cumulative dx; app clamps width 60–480 px and the rebuild relayouts header + all loaded rows. Verified by CGEvent drag of the Name/Category divider 80 px left — screenshot shows Name shrunk and all columns reflowed while dragging. Real drag-on-divider, not an approximation. |
| row_selection | **hand-rolled** | synthetic-input + observed (screenshot) | Stock views expose no click-with-modifiers (button gives `()` and draws chrome). `RowFrame` reads `PointerState.modifiers` on primary-down and reports `{shift, cmd}`; model keeps `HashSet<u32>` of ids + anchor for ranges. Verified: click row 1, shift-click row 5, cmd-click row 7 → toolbar reads "6 selected", 5-row contiguous highlight + 1 toggled row, accent bar on each (screenshot run7-shot3). Selection is by row id, so it survives re-sort/filter. |
| cell_custom_render | **assembled** | observed (screenshots) | Status chips need no custom widget: `sized_box(label).background_color(bg).corner_radius(8)` + padding hugs the text — stock views compose into a colored chip (Ok green / Warn amber / Err red, verified in every screenshot). Right-aligned numeric cells via `label.text_alignment(TextAlign::End)` + `LineBreaking::Clip`. The gap: any cell needing *interaction* or owner-draw drops to a masonry widget (see above). |

## Helper crates & why

None. Dependencies: `xilem =0.4.0` only (403 `name =` entries in Cargo.lock
including the app). PRNG is a 12-line xorshift64* to avoid `rand`; there is
no xilem ecosystem crate for tables/virtual lists to lean on.
Verification tooling is outside the crate: `verify/*.swift` CGEvent driver +
window-locator scripts and `verify/batch.sh` (occlusion-checked, window-
relative synthetic input + `screencapture`).

## Measurements (Apple M4 Pro, release build, MiB = 1024²)

- `BUILD_MS 27.611` this run (21.7 and 76.6 in earlier runs — first-launch
  variance; data gen + model build for 100k rows).
- Filter: see table. Sort: 18.6 ms cold asc, 1.6 ms desc flip.
- RSS via `ps -o rss= -p <pid>` (KiB → MiB): **130.4 MiB** after load (window
  up, idle); **150.4 MiB** after a scripted 40×1200 px scroll;
  **151.1 MiB** after a further 60×2000 px scroll (~8k rows deep in desc
  sort, screenshot verified). Earlier run: 107.2 MiB after load → 129.2 MiB
  after comparable scrolling. Deltas plateau — the ~20 MiB step is glyph/
  scene caches warming, not per-row leakage; virtualization holds at 100k.
- Binary 12,201,808 bytes unstripped; no-op incremental rebuild 0.35 s.

## LoC / time

Production **1022** (main.rs 486 + widgets.rs 536 — 52% of the app is the
two custom widgets + xilem View plumbing, not table logic) · verification
**217** (external `verify/` swift/sh drivers; nothing test-shaped ships in
the binary — the only in-binary aid is the `GRID_TOPMOST=1` window-level
gate, 4 LoC).
Where the time went (reconstructed — original transcript lost; inferred
from code shape and prior xilem apps): the dominant cost is widgets.rs —
every interactive gap (modifier-aware clicks, drag) needs a full masonry
`Widget` + ~120 LoC hand-written xilem `View` impl, boilerplate identical
in shape to xilem-dash/board; model-side sort/filter/selection is trivial
plain Rust; plus scripted-verification setup (window location, occlusion
checks, RSS gating on stdout markers).

## Surprises

- Good: `virtual_scroll` exists as a stock view in 0.4.0 and simply works at
  100k rows — the single biggest expected risk was free (unlike table,
  sort, resize, selection, which all required custom work).
- Good: 100k-row filter is single-digit ms; no incremental filtering needed.
- Bad: the stock-view boundary bites exactly as in xilem-dash: *any*
  pointer nuance (modifiers, drag deltas, capture) means a hand-rolled
  masonry widget + View plumbing; there is no generic gesture/click view.
- Bad: masonry's default font stack renders ▲/▼ (and ✕) as tofu — sort
  indicators are ASCII `^`/`v`.
