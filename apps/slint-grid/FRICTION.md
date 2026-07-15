# FRICTION — slint-grid (Grid, Slint =1.17.1)

Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
Default slint features (winit backend, femtovg GL renderer).

Headline: the 100k-row case is exactly what Slint's custom `Model` trait is
for, and it delivers — 20 of 100,000 rows materialized at first paint, flat
RSS over a full-height scroll, 2–4 ms filter keystrokes. The headline finding
is the **StandardTableView delta**: the std-widgets table looks like the
answer and was rejected for cause (see below); everything table-shaped beyond
virtualization is assembled by hand, but from good primitives.

## Evidence base

`GRID_SELFTEST=1` drives the app through the REAL input pipeline
(`Window::dispatch_event`: key events into the focused LineEdit, header
clicks, Shift+click on rows, an 8-step divider drag, wheel scrolling), then a
full-range programmatic viewport sweep, then quits. Retained artifacts in this
directory: `verify-stdout.log` (canonical run, snapshot disabled for clean
RSS), `verify-stdout-snapshot.log` (identical run + snapshot),
`verify-snapshot.png` (pixel evidence: sort indicator, selection band rows
2–9, resized Name column, colored chips), `launch-plain.log` (plain 10 s
launch). RSS self-observed via `ps -o rss= -p <pid>` from the harness script
at t=3 s (after load) and t=10.4 s (after the sweep).

Evidence labels: **observed** (pixel snapshot / logs of the production path),
**self-test** (scripted in-process probes), **synthetic-input** (dispatched
window events through real hit-testing), **source-only** (read from crate
sources, not executed), **unexercised**.

## The StandardTableView delta (source-only, crate sources)

`i-slint-compiler-1.17.1/widgets/cupertino/tableview.slint` (284 lines) is
the entire widget. What it promises vs. what you get:

| Promise | Reality (1.17.1) |
|---|---|
| `rows: [[StandardListViewItem]]` | Cells are **text-only** (`StandardListViewItem` = `{text}` rendered by a plain `Text`). A colored status chip is **not achievable** without abandoning the widget — this alone disqualified it for SPEC-7. |
| `sort-ascending/-descending(col)` callbacks + indicator | You still sort the data yourself in Rust; the widget only draws the chevron and remembers `current-sort-column`. Identical work to a custom header. |
| Column resize | Genuinely built-in: 1 px divider with a 10 px `ew-resize` TouchArea writing `column.width`. I reused the same pattern in my header. |
| Row selection | Single `current-row` only; the highlight binding is `idx == current-row`, so **range/multi selection cannot even be rendered**. `row-pointer-event` exposes modifiers, but there is nowhere to put a second selected row. |
| Virtualization | Inherits ListView's lazy instantiation — same as the custom route. |

Conclusion: for a data-dense grid you keep ~40 lines of StandardTableView
convenience (header chrome, arrow-key nav) and lose chips + range selection.
Custom table = ListView + ~150 lines of .slint. That is the trade.

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| table_widget | **assembled** | synthetic-input + source-only | `StandardTableView` exists but rejected (delta above). Built: `ListView` + hand-made `HeaderCell`/`GridRow`/`Cell`/`ChipCell` components. Header tracks `lv.viewport-x` for horizontal scroll alignment (pattern cribbed from the widget source). Column widths live in a `VecModel<ColDef>`; the DSL writes `col.width = ...` straight into the model from the drag handler (repeater write-back via `set_row_data` — a documented-nowhere but load-bearing feature the built-in table itself relies on). |
| virtualization | **built-in** | self-test | Custom `Model` impl (`row_count`/`row_data`/`ModelTracker` over `ModelNotify`); ListView instantiates visible items only. Probe counter: **ROWDATA_CALLS_INITIAL 20** of 100,000 after first frame; **3,092 total** after a full 2.8M-px sweep; RSS flat (119.5 → 121.1 MiB). 100k×28px viewport (~2.8M logical px) stays comfortably inside f32 precision. |
| sort | **assembled** | synthetic-input + observed | Real header clicks toggle asc→desc with a hand-drawn ▲/▼ (snapshot shows Value ▲ with ascending values). Rust sorts the `Vec<u32>` index view: name 7–12 ms, f64 value 2.3–3.0 ms (SORT_MS lines). No help from the framework beyond callbacks — same as StandardTableView would give. |
| filter_latency | **hand-rolled** | synthetic-input | Real keystrokes through the focused LineEdit. **FILTER_MS 1-char ≈ 1.4–3.6 ms, 4-char ≈ 3.3–3.9 ms** (case-insensitive substring over precomputed lowercase names, then re-sort, then `ModelNotify::reset()`). `slint::FilterModel`/`SortModel` adapters exist (source-only) but a single index-vector recompute inside the custom Model is simpler, faster to instrument, and keeps selection semantics in one place. |
| column_resize | **assembled** | synthetic-input + observed | 8-step synthetic pointer drag on the divider: COL1_WIDTH_AFTER_DRAG 260 (220+40). Snapshot shows the widened Name column. ~12 lines of .slint cribbed from the widget source; `mouse-cursor: ew-resize` included. Not the weakest cell in Slint — it's the same mechanism the built-in table uses. |
| row_selection | **assembled** | synthetic-input + observed | Click → single; **Shift+click → range** via `PointerEvent.modifiers.shift` (Shift held down as a real `KeyPressed` window event; SELECTION Some((2, 9)); snapshot shows the 8-row accent band). Selection state lives in Rust as (anchor, cursor) view indices; `row_changed` fired per row for spans ≤4096, else `reset()`. Selection keyed to view indices and cleared on filter/sort (documented approximation). |
| cell_custom_render | **assembled** | observed | Colored status chips (rounded Rectangle + Text, colors by status) are ~20 declarative lines in the hand-built row — trivial, *but only because the row is hand-built*. Inside StandardTableView: not achievable. Snapshot shows green Ok / amber Warn / red Err chips. |

## Helper crates

None. PRNG is a hand-rolled xorshift*; dates are a 20-line days→ISO
converter (no `rand`, no `chrono`).

## LoC (production vs verification)

- Production: **539** — `src/main.rs` 305 (data gen, GridModel, callbacks)
  + `build.rs` 3 + `ui/main.slint` 231.
- Verification: **217** — `src/main.rs` 211 (selftest harness, snapshot
  writer) + `ui/main.slint` 6 (viewport/geometry instrumentation props).
- Production carries ~4 more instrumentation lines (`row_data_calls` counter,
  SORT_MS print) counted as production above.

## Measurements

- `BUILD_MS 135` (canonical retained log; 92–225 across runs) — 100k-row
  generation incl. all pre-formatted display strings + model build.
- Clean release build **60 s** (serial; an earlier concurrent-with-fetch run
  read 400 s — CPU contention, not the build), no-op rebuild **0.6 s**.
- Binary: 15,687,600 bytes raw / 13,651,088 bytes (**13.0 MiB**) stripped.
- Dependencies: **416** unique name-version entries incl. the app
  (`cargo tree --prefix none -e normal,build | sort -u`).
- RSS (`ps -o rss= -p <pid>`): **119.5 MiB** after load (t=3 s),
  **121.1 MiB** after the full-height sweep — snapshot-free run. (The
  snapshot run read 168 MiB after-scroll: one 2000×1280 RGBA readback plus
  encode buffers; artifact `verify-stdout-snapshot.log`.)

## Where the time went

1. ~35% the StandardTableView investigation + the custom table replacement
   (header/viewport-x alignment, ColDef VecModel write-back resize).
2. ~30% verification harness: synthetic-input plumbing (`dispatch_event`
   coordinates, Shift-modifier keying), RSS timing races in the shell
   harness (first sample raced exec and read 32 KiB), snapshot-vs-RSS
   pollution needing a second run mode.
3. ~20% Model-trait plumbing (notify granularity for selection: row_changed
   vs reset; keeping `SharedString` clones O(1) by pre-formatting all cell
   text at build).
4. ~15% everything else (data gen, sorting, DSL name collision: a property
   named `row` collides with the built-in grid-placement `row` property on
   every element — renamed to `item`).
