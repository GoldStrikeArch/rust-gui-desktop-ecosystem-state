# Data density: "Grid" (100k rows) in seven frameworks (macOS)

**Run dates:** 2026-07-09..10. Evidence labels per cell in per-app
FRICTION.md; raw rows in [data/iter4-rows.md](data/iter4-rows.md). Each app
prints `BUILD_MS`/`FILTER_MS`. Tauri's original Grid log was not retained; a
[fresh audit rerun](../measurements/verification-iter4-rerun/tauri-grid-20260710.log)
now records 14/14 self-test checks, but is not a reconstruction of the original
timing environment. The earlier GPUI values in the raw row disagreed with its
retained `grid-stdout.log`; the table below uses that retained GPUI log.

Iteration 4, SPEC-7: 100,000 deterministic rows, virtualized table, sort by
header click, filter-as-you-type (self-timed), column resize, row selection,
colored status chips. All seven built, launched, and handled a 100k-row model.

## The headline: computation was not the bottleneck; widgets were

Filter-as-you-type latency over 100k rows, full rescan per keystroke
(self-timed; backed by original retained logs except the historical Tauri row):

| | iced | egui | gpui | tauri | xilem | slint | dioxus |
|---|---:|---:|---:|---:|---:|---:|---:|
| 1-char filter (ms) | 4.0 | 5.7 | 1.7 | 3.4 | 1.5 | 2.3 | 2.6 |
| 4-char filter (ms) | 4.0 | 3.6 | 4.3 | 4.2 | 6.1 | 3.4 | 4.0 |
| 100k gen+build (ms) | 36 | 18 | 31 | 15 | 28 | 135 | 19 |
| RSS after load (MiB) | 88 | 107 | 97 | 121¹ | 130 | 120 | 106¹ |

¹ main process only; WebKit XPC helpers excluded.

The fresh Tauri rerun corroborated the app path and timing scale: 3.111 ms at
one character, 4.130 ms at four characters, 10.814 ms to clear the filter, and
16.118 ms to build. Those rerun values do not replace the historical row.

Every framework's reported filter pass stayed within roughly 1.5–6.1 ms in
these implementations, so computation was not the bottleneck for the tested
queries. Sorting was not uniformly single-digit: GPUI name sorts took about
11–12 ms in its retained log, Xilem recorded an 18.6 ms ascending value sort,
and other runs varied by column/order. No implementation added debounce. The
virtualization paths kept the working set bounded, but RSS was not literally
flat everywhere: Xilem rose from about 130.4 to 151.1 MiB while warming caches,
and its cited run scrolled about 130k pixels/~8k rows rather than the full
100,000-row range.

## The table-widget matrix (where the differences actually live)

| Capability | iced | egui | gpui | tauri | xilem | slint | dioxus |
|---|---|---|---|---|---|---|---|
| Table widget | hand-rolled¹ | **built-in** (egui_extras) | hand-rolled | hand-rolled² | hand-rolled³ | assembled⁴ | hand-rolled |
| Virtualization | hand-rolled | **built-in** | **built-in** (uniform_list) | hand-rolled (IPC windows) | **built-in** (virtual_scroll) | **built-in** (Model/ListView) | hand-rolled (DOM windows) |
| Sort | assembled | assembled | assembled | hand-rolled | assembled | assembled | assembled |
| Column resize | hand-rolled | **built-in** | assembled (DnD system) | hand-rolled | hand-rolled (custom widget) | assembled | hand-rolled |
| Row selection (shift-range) | assembled⁵ | assembled⁶ | assembled | hand-rolled | hand-rolled⁵ | assembled | assembled |
| Custom cells (chips) | built-in | built-in | assembled | built-in | assembled | assembled⁴ | built-in |

¹ iced 0.14's new `table` widget was REJECTED after a source read: it eagerly
materializes one Element per cell for ALL rows (600k widgets at 100k rows) —
no virtualization, sort, or resize. "Has a table widget" ≠ "has a data grid."
² a real Tauri app would npm-install a JS grid (out of bounds under the
no-external-JS rule); notable architecture: rows stayed in Rust with the
viewport fetching windows over IPC — every keystroke 3.4–10.8 ms, no
placeholder flashes. IPC works fine as a virtualization backplane.
³ xilem is the inverse of egui: its riskiest cell (100k virtualization) is a
free stock view (`virtual_scroll`, first real test — it held), while
everything table-shaped around it (headers, selection, resize) needed
hand-written masonry widgets (536 LoC, half the app). Side-effect sighting:
sort arrows ▲/▼ rendered as tofu (the fontique Han/symbol fallback bug
reaching UI chrome).
⁴ Slint's `StandardTableView` was REJECTED after a source audit: rows are
text-only `StandardListViewItem`s and selection is a single current-row —
status chips and range-select cannot even be rendered inside it. The custom
Rust `Model` + `ListView` path is the real (and good) story: 20 of 100k rows
materialized at first paint. Also load-bearing and undocumented: the DSL can
assign into model row fields and it write-backs via `set_row_data`.
⁵ winit delivers no modifiers on mouse events — iced needs a persistent
ModifiersChanged subscription; xilem needs a custom widget reading
PointerState.
⁶ egui trap: selectable label text silently eats row clicks in sense()-
enabled tables — fixed via `style.interaction.selectable_labels = false`.

## Verdict for the initiative

Only **egui** (via first-party `egui_extras`) supplied a table basis that met
this tested 100k/custom-cell specification without replacing the body widget;
sorting and shift-selection were still application code. Its reported
production-only split was compact, but the canonical total including
verification was not the round's lowest. Two frameworks ship table
widgets that fail on contact (iced's non-virtualizing `table`, Slint's
text-only `StandardTableView`) — a "widget exists" checkbox on a comparison
matrix would mislead; capability testing was necessary. Virtualization
primitives, however, worked in 4 of 7 (egui, gpui, xilem, slint); the other
three implementations assembled their own windowing, without a controlled
developer-time measurement. The business-app gap is
the *table furniture* (sortable headers, resize, selection models), not
performance.

## Caveats

One implementation per framework; latencies are self-timed in-app but are
descriptive outcomes, not a controlled ranking. Data generators, query strings
and selectivity, sort state/order, warm-up, and timed work differ; for example,
Tauri's reported filter path also re-sorted an already sorted view. Column
resize and selection were verified by synthetic input on a shared desktop;
macOS/M4 Pro only.
