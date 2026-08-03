# FRICTION — Grid (freya =0.4.0), SPEC-7

Reference machine per spec. `cargo build --release` clean, `cargo build
--locked --release` reproduces. Evidence: `selftest-log.txt` /
`selftest-err.txt` (`GRID_SELFTEST=1 ./target/release/freya-grid`, exit 0,
`SELFTEST DONE pass=14 fail=0`), plus an interactive release run driven with
synthetic CGEvent scroll and read back from window-scoped screenshots.

The generated dataset is byte-identical to the other ports of SPEC-7 (same
xorshift* seed and draw order): the self-test reports `SORT name asc
first_id=70664`, `SORT name desc first_id=28613`, `SORT id asc first_id=0`,
matching the iced and floem logs exactly.

## Ratings

| capability | rating | evidence | note |
|---|---|---|---|
| table_widget | **assembled** | source-only + observed | Freya ships a `Table` family (`Table`/`TableHead`/`TableBody`/`TableRow`/`TableCell`, plus a `TableArrow` sort indicator and `column_widths`), but it is a **layout helper**: you hand it one element per cell, so at 100k×6 it would materialise 600k elements. Rejected after reading `freya-components/src/table.rs`. The grid here is a header `rect` row plus a `VirtualScrollView` body, with the same visual result. |
| virtualization | **built-in** | self-test + observed | `VirtualScrollView::new_controlled(builder, controller).length(n).item_size(26.)` calls the builder **only** for rows in the viewport. No spacer arithmetic, no scroll-offset bookkeeping — the iced port hand-rolled all of that. Self-test: programmatic scrolls to y=800 / 260 000 / 0 produced `WINDOW first=30 / 10000 / 0`, where `first` is recorded **inside the item builder** (so it is the row that was really built, not a computed guess). Interactive: 400 wheel ticks landed on row 10396 with no stutter; RSS 106.1 → 107.2 MiB. |
| sort | **built-in (interaction) / hand-rolled (logic)** | self-test | Header cells are ordinary `rect`s with `.on_press`; the comparator, the asc/desc toggle and the ▲/▼ indicator are app code (~40 LoC). `AccessibilityRole::ColumnHeader` is a one-liner on the same element. |
| filter_latency | **built-in** | self-test | `Input` writes a `State<String>`; a `use_side_effect` that reads it re-derives `visible` and prints `FILTER_MS`. Typical: **1-char 5.15 ms, 4-char 7.59 ms** over 100k rows (`FILTER_MS 1 5.15` … `FILTER_MS 4 7.59`), clearing back to 100k rows in 0.52 ms. Note the effect-based wiring means the initial run has to be skipped explicitly, or startup prints a spurious `FILTER_MS 0`. |
| column_resize | **hand-rolled** | self-test + source | A 7 px divider `rect` after each header takes `on_pointer_down` to arm the drag; a root-level `on_global_pointer_move` streams the cursor x while armed, and `on_global_pointer_press` commits. ~30 LoC. Freya has a `ResizableContainer` component, but it splits a container into panels — it is not a column-width mechanism. Self-test drives the same three functions the pointer handlers call: `RESIZE col=id width=125`. |
| row_selection | **assembled + workaround** | self-test | `.on_press` per row, with plain / shift-range / cmd-toggle semantics. The workaround: `PressEventData` carries **no modifier state** (`MouseEventData` has `global_location`, `element_location`, `button` and nothing else), so a root-level `on_global_key_down`/`on_global_key_up` pair mirrors the live `Modifiers` into a signal — the same shape iced needed. Self-test: `SELECT count=1 clicked_id=5`, `SELECT count=4 clicked_id=8` (shift range), `SELECT count=1 clicked_id=2`. |
| cell_custom_render | **built-in** | observed | A cell is just an element, so the status chip is `rect().background(..).rounded_full().child(label())` with per-status colours. Verified in the screenshots (green Ok / amber Warn / red Err pills). |

## Helper crates

- **async-io 2.6.0** — verification only: the scripted self-test must wait a
  frame between a programmatic scroll and reading back which rows the virtual
  list built, and Freya's executor exposes no timer.

No table, virtualization, or PRNG crate (10-line xorshift\*).

## LoC split

- 724 total in one `src/main.rs`
- ~120 of those are the `GRID_SELFTEST` scripted pass and its assertions
- ~600 production (data generation, model, header, virtual body, resize drag)

## Memory (self-observed)

`ps -o rss= -p <pid>` on the release binary:

- after load (100k rows built, first frame): **106.1 MiB**
- after 400 wheel ticks (scrolled to row ~10 400): **107.2 MiB**

`BUILD_MS` (data generation + initial model build) is **12.8–13.0 ms**.

## Where the time went

1. Reading `freya-components`' `Table` to decide it could not be used, and
   `VirtualScrollView`'s `get_render_range` to know that `first =
   floor(-scroll_y / item_size)` — needed to write a *meaningful* `WINDOW`
   assertion rather than restating my own arithmetic.
2. The modifier workaround for shift-click.
3. `Size::flex` requiring `Content::flex()` on the parent (again), and the fact
   that `rect` has no `cursor_icon` property — the col-resize cursor is set
   imperatively with `Cursor::set` from `on_pointer_enter`/`on_pointer_leave`.

## Surprises

- Good: **virtualization is a stock component**, and it takes a
  `ScrollController` so a test can scroll it programmatically. This is the
  single biggest structural advantage Freya showed in this round — the body of
  a 100k-row table is one component call.
- Good: `AccessibilityRole::ColumnHeader`/`Row` are inline attributes on the
  same elements, so the a11y tree is right without a second data model.
- Bad: no modifiers on press events; no per-element cursor property.
- Bad: `Table` exists and is *not* what a "table widget" implies at scale — a
  name that will mislead someone into a 600k-element layout.
