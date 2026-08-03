# FRICTION — Grid (vizia =0.4.0), SPEC-7

Reference: SPEC-7.md. Built + verified on macOS 26.5.2 (M4 Pro, rustc
1.96.1): `cargo build --release` and `cargo build --locked --release` clean
(no warnings), binary launched, window pixel-verified, alive well past the
10 s bar, killed cleanly. **No fallback was needed** — every SPEC-7
requirement is real, including a genuine header-divider drag for column
resize.

Evidence labels: **observed** (seen in the launched app), **self-test**
(the scripted `GRID_SELFTEST=1` run, captured in `selftest-log.txt` /
`selftest-err.txt`), **synthetic-input** (CGEvent clicks/drags/scrolls
scoped to this app's window, verified from window-scoped screenshots),
**source-only**, **unexercised**.

## Headline numbers (from `selftest-log.txt`, stdout retained)

- `BUILD_MS 10.67` — generate 100,000 deterministic rows + publish the first
  view (self-test).
- `FILTER_MS 1 1.64` / `FILTER_MS 4 7.70` — typical filter application at
  1-char and 4-char queries; full range seen across runs **1.50 – 7.70 ms**
  (self-test). No debounce needed at 100k rows.
- Sort 100k by name: **8.6 ms** cold ascending, **8.5 ms** to descending,
  **2.0 ms** by id (self-test `SORT …` lines). Under synthetic input a real
  header click measured 11.3–12.1 ms (includes the widget round-trip).
- Virtualization: at every scroll position the status-cell template ran for
  exactly **22 rows** out of 100,000 (`WINDOW … cells_built=22`).
- RSS: **139.6 MiB after load**, **290.0 MiB after a long scroll**
  (`ps -o rss= -p <pid>` / 1024; ~320 wheel events sweeping the whole
  100k range). Not flat — see the note under `virtualization`.
- `SELFTEST DONE pass=14 fail=0`.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| table_widget | **built-in** | observed + self-test | vizia 0.4 ships **`VirtualTable`** — a real data grid, not a layout helper: a `VirtualList` body plus a header row of `Resizable` cells, with `sort_state` / `sort_cycle` / `resizable_columns` / `selectable` / `selected_row_ids` modifiers and `on_sort` / `on_row_select` callbacks. It is the only framework in this cohort where SPEC-7's table is a *view constructor call*. Columns are `TableColumn::new(key, header_fn, cell_fn)` with per-column `width`/`min_width`/`sortable`/`resizable`/`hidden` signals. |
| virtualization | **built-in** | self-test | `VirtualList` under the hood: fixed `item_height`, a spacer VStack sized to `num_items * item_height`, and a recycled pool of `ceil(viewport/item_height) + 2` item views whose `top` is rebound as you scroll. Proven by instrumenting the *cell template itself* — it only runs for materialised rows, and it ran for exactly 22 row indices at each of 12 scroll ratios from 8 % to 100 % (`WINDOW ratio=1.00 first=99978 last=99999 cells_built=22 rows=100000`). Caveat worth recording: RSS is **not** flat across a long scroll (140 → 290 MiB). Row *views* are recycled, but Skia's paragraph/glyph caches and vizia's per-entity style stores grow as new text is shaped; it plateaus rather than leaking linearly. |
| sort | **built-in** | self-test + synthetic-input | Header press → `on_sort(cx, key, direction)`; the app owns the comparator, `TableSortCycle::BiState` gives asc↔desc, and `TableHeader` renders the indicator (`^` / `v` / `·`). Verified both ways: a real header click produced `SORT name asc first_id=40505 ms=11.28` with the `^` visible in the screenshot, and the scripted run asserted full-vector monotonicity in both directions plus restoration of generation order by id. |
| filter_latency | **assembled** | self-test | `Textbox::on_edit` → recompute from scratch (substring filter, then re-apply the active sort), self-timed with `std::time::Instant`. **1-char 1.64 ms, 4-char 7.70 ms** at 100,000 rows — an order of magnitude under a frame, so no debounce and no worker thread. The cheap part is publishing: rows go to the table as `Signal<Arc<[Row]>>` (`VirtualTable` accepts any `V: Deref<Target = [T]> + Clone`), so handing over a new 100k-row view is a refcount bump, and `Row` is clone-cheap (`Arc<str>` name, packed `yyyymmdd` date). |
| column_resize | **built-in** | synthetic-input + self-test | Real divider drag, not an approximation: `VirtualTable` wraps every non-final header cell in vizia's `Resizable` view, which owns the drag handle and writes the column's `width` signal. Verified with a real 110 px CGEvent drag on the Name/Category divider (screenshot before/after) and asserted in the scripted run through the same state the drag mutates (`RESIZE col=name width=220->310`). |
| row_selection | **built-in + assembled** | synthetic-input + self-test | Click selection is `Selectable::Multi` + `selected_row_ids` + `on_row_select(cx, id)`; **shift-click range is app logic** — the callback reads `cx.modifiers().shift()` and the model expands from a stored anchor over the *current* view order. Verified with real clicks: `SELECT count=1 clicked_id=31737` then shift-click → `SELECT count=6 clicked_id=33475`, with the six-row highlight visible in the screenshot. |
| cell_custom_render | **built-in** | observed | A cell is an arbitrary view tree built by a closure that receives `Memo<Row>`, so the status chip is a styled `Label` with `toggle_class("ok"/"warn"/"err", row.map(..))` and the value column is a right-aligned `Label`. Zero friction; screenshots show green/amber/red pills. |

## The one real trap

**`VirtualTable`'s own row and cell wrappers eat the click that selects the
row.** `VirtualTable` wraps each row in an HStack classed `table-row` and
each cell in a VStack classed `table-cell`; both are hoverable by default,
so the hover target is a *descendant* of the `ListItem` that carries
`on_press` → `ListEvent::Select`. vizia only fires an action when
`cx.current == meta.target`, so **row clicks silently did nothing**. The fix
is one CSS rule, `.table-row, .table-cell { pointer-events: none; }`
(pointer-events is inherited in vizia's hover system, so it hands the hover
back to the list item). This is the same class of trap as needing
`.hoverable(false)` on the children of any pressable container, and it hits
you inside a *built-in* widget, where you cannot reach the offending views
except through CSS.

Two smaller ones: the status chip is stretched to the column width unless
given an explicit `width` (auto-width plus padding under-measures and clips
the last glyph), and `Context::load_image`-style APIs aside, `VirtualTable`
exposes no scroll position or `on_scroll` — the virtualization evidence here
had to be gathered by instrumenting the cell template with atomics, because
`VirtualList`'s content closure must be `Copy` and therefore cannot capture
an `Rc`.

## Helper crates

**None.** `vizia = "=0.4.0"`, default features only. The PRNG is a 10-line
xorshift*, dates are packed integers (no `chrono`), and the table,
virtualization, sortable/resizable headers and selection are all core views.

## LoC split

- Production: **~500** (`src/main.rs` 672 minus ~172 lines of the
  `GRID_SELFTEST` script, its assertions and the virtualization probe).
- Verification: **~172**, all in-app — there is no external driver script.
  The scripted run is a `cx.add_timer` state machine that emits the same
  events the widgets emit, one step per 120 ms tick so the table really
  re-renders (and the virtualization probe really re-runs) between steps.
- Retained evidence: `selftest-log.txt` (stdout), `selftest-err.txt` (empty).

## Where the time went

1. **The `pointer-events` trap** — row selection failing silently inside a
   built-in widget, with no way to see why except reading `VirtualTable`'s
   source.
2. **Designing falsifiable virtualization evidence.** "It scrolls fast" is
   not evidence; instrumenting the cell template so the log can say *22 of
   100,000 cells were materialised* is.
3. The table itself was ~45 minutes. `VirtualTable::new(cx, rows, columns,
   28.0, |row| row.id)` plus six `TableColumn::new(..)` calls is most of
   SPEC-7.

## Surprises

- Good: a genuine virtualized, sortable, resizable, selectable table widget
  in core. iced's `table` is O(rows × cols) widgets; egui/xilem/gpui all
  hand-roll windowing. vizia just has one.
- Good: `V: Deref<Target = [T]> + Clone` on the rows parameter means
  `Arc<[Row]>` works, so re-publishing a filtered 100k view is free.
- Good: 100k-row synchronous filter+sort in `Model::event` is a non-issue
  (≤ 8 ms), so no async, no incremental index, no debounce.
- Bad: RSS grows 140 → 290 MiB over a full sweep of the dataset. Row views
  are recycled; the text/style caches behind them are not bounded to the
  window.
- Bad: no scroll observability on `VirtualTable`, and its internal wrappers
  are only reachable through CSS class names that are not documented as API.
