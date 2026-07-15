# FRICTION — dioxus-grid ("Grid", Dioxus 0.7.9 desktop/webview)

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| table_widget | **hand-rolled** | observed | Nothing exists: no table/grid widget in Dioxus and no desktop-compatible datagrid crate found (ecosystem table efforts target dioxus-web/wasm or are pre-0.7). Built the whole thing — header, cells, sort, selection — from `div`s with `display: grid` rows, column widths as a `grid-template-columns` string from a `Signal<[f64; 6]>`. The webview gives free text ellipsis, tabular-nums, hover styling. |
| virtualization | **hand-rolled** | self-test | Classic DOM windowing: scroll container > sticky header + a `rows*28px`-tall spacer; only viewport±8 rows rendered, absolutely positioned at `view_index*ROW_H`. Enabler new since iteration 2: **0.7's `ScrollData` carries `scroll_top()`/`client_height()`**, so `onscroll` needs no eval/JS. Guarded write (only when `scroll_top/ROW_H` crosses a row) keeps pixel-level scrolling on the compositor; Rust re-renders ~36 rows only when the window slides. Viewport height via `onresize` (ResizeObserver-backed, fires on mount). Self-test scrubbed the full 2.8M-px range via real DOM `scrollTop` writes (`document::eval`) firing real onscroll events; signal tracked to 2,799,460. Hand-feel of trackpad scrolling not machine-verified (WKWebView automation needs Accessibility perms). |
| sort | **assembled** | self-test | Header `onclick` toggles asc/desc with ▲/▼ indicator; comparator over the full 100k `Vec` with id tiebreak, view = `Vec<u32>` of indices. Sorting f64 over 100k rows: `SORT_MS Value asc 8.67`, `desc 7.39` (name/String sort ~30 ms when tried interactively during dev — unlogged, synthetic-input). All logic is mine; the framework contributes only the click. |
| filter_latency | **assembled** | self-test | Substring filter over all 100k names per keystroke, then re-apply active sort. Captured (run-stdout.log): `FILTER_MS 1 2.61`, `FILTER_MS 2 2.28`, `FILTER_MS 3 4.36`, `FILTER_MS 4 4.00`, clear `FILTER_MS 0 0.04` (ms). Rebuild is data-side only; the DOM diff after it touches ~36 rows so typing feels instant. Timing excludes the webview patch/paint. |
| column_resize | **hand-rolled** | self-test | Real divider drag, not the expected weak cell: divider `onmousedown` → root `onmousemove` applies clientX delta → root `onmouseup`/`onmouseleave` ends. Works cleanly *because resize only needs deltas* — the "events carry no element geometry" trap (iterations 2/3) never triggers; no overlay trick needed. Self-test drove the same drag path (arm signal → move → release): 230→300 px. Live-drag smoothness not machine-verified. |
| row_selection | **built-in-ish / assembled** | self-test | `onclick` + `evt.modifiers().contains(Modifiers::SHIFT)` — modifiers ride on every MouseEvent, so Shift-range costs ~10 LoC (anchor = last plain-clicked view index, range over current view, ids in `HashSet<u32>`). Self-test: click row 5 then Shift-click row 25 → 21 selected. Rated assembled: the event affordance is built-in, the selection model is mine. |
| cell_custom_render | **built-in** | observed | Any RSX is a cell. The status chip is a `span` + 3 CSS classes; right-aligned numerics are `text-align: right`. This is the webview model's core strength — zero friction, nothing to fight. |

## Helper crates

- `tokio` (features = ["time"]) — **verification only**: paces the
  GRID_SELFTEST script. Production paths never use it (no timers needed —
  the grid is fully event-driven). Same "framework runs on tokio but
  re-exports no timer" finding as iteration 2.
- No datagrid/virtual-list helper exists for Dioxus desktop; searched before
  hand-rolling (options found target web/wasm or are unmaintained pre-0.7).

## Design notes

- Rows are immutable → generated once into a `static OnceLock<Vec<Row>>`, so
  RSX borrows `&'static str` names/dates instead of cloning Strings per
  frame; only the formatted `value` allocates per rendered row.
- View = `Signal<Vec<u32>>` of row indices (400 KB), rebuilt eagerly on
  filter/sort; selection stores row *ids* so it survives re-filtering.
- Header lives *inside* the scroller as `position: sticky` so horizontal
  scrolling (after widening columns) keeps header and body aligned for free.
- Self-test drives the SAME `use_callback`s the UI handlers call
  (apply_filter/apply_sort/do_select/apply_resize), so code paths — not
  copies — are exercised; only the webview event plumbing is bypassed.

## Cross-reference: the occlusion freeze (found in dioxus-fetch)

dioxus-fetch (same iteration, same pinned version) hit a serious wart this
app's self-test escaped only by window-stacking luck: while the window is
occluded/unactivated, WKWebView throttling plus dioxus-desktop's
`edits_in_progress` gate parks the ENTIRE VirtualDom + task loop after the
next signal write — timers, futures, everything, not just painting. Any
background work a grid app schedules would stall the same way. Details,
source anchors, and the always-on-top workaround: apps/dioxus-fetch/FRICTION.md.

## Where the time went

- ~30% virtualization geometry: sticky-header offset inside scroll_top
  (absorbed by overscan=8), bucket-guarded scroll writes, empty-view clamps.
- ~25% pre-flight API verification in the vendored 0.7.9 sources (does
  ScrollData carry scroll_top? does eval resolve without dioxus.send()?) —
  both yes; this avoided the iteration-2 blind alleys entirely.
- ~20% header/divider event choreography: divider as sibling of the sort
  label (not child) so a resize drag can't fire a sort click.
- ~15% CSS (chips, sticky header, grid rows).
- ~10% self-test script.

## Measurements

- `src/main.rs`: **553 lines total = 507 production** (incl. ~64 CSS-in-Rust,
  ~36 module doc) **+ 46 verification** (GRID_SELFTEST block).
- First `cargo check` after writing the code: **0 errors, 0 warnings**.
- Release build (cold, parallel, deps shared-cache): **114.9 s**; touch
  rebuild: **3.6 s**. Dependency graph ~291 unique crate names (tokio adds a
  few over the 279 of iteration 1). Binary **6,249,328 bytes (6.0 MiB) raw**.
- Startup: `BUILD_MS 19.1` / `20.9` (two runs) for 100k-row generation +
  identity view.
- Filter: `FILTER_MS 1 2.61` … `FILTER_MS 4 4.00` (captured in
  run-stdout.log; also SORT_MS lines). Full log retained.
- RSS (main process, `ps -o rss= -p <pid>`): **106.4 MiB after load**,
  **100.8 MiB after the scripted full-range scroll scrub** (40 jumps × 60 ms
  across 2.8M px), i.e. windowing holds memory flat — the after-scroll dip is
  WebKit housekeeping. Caveat (observed): WKWebView renders in a separate
  out-of-process `com.apple.WebKit.WebContent` not parented to the app PID,
  so these numbers are the Rust/app process only; on a shared desktop the
  WebContent helper can't be attributed safely. Plain (non-selftest) 10 s
  launch: window up, `BUILD_MS 20.9` only output, RSS 87.0 MiB, clean
  SIGTERM. Scrolling-under-finger smoothness and drag feel: verified by
  construction + real-event self-test, not by hand (evidence: self-test).
