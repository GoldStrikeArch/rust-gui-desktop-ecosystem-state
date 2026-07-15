# FRICTION — tauri-grid ("Grid", SPEC-7)

Tauri =2.11.5 / tauri-build =2.6.3 (same pins as ../tauri-app), same manual
no-Node setup (hand-written `tauri.conf.json`, `withGlobalTauri`, static
vanilla HTML/CSS/JS in `ui/`, hand-written capability, copied icons).
**No external JS libraries** — the grid is hand-rolled DOM.

## Boundary decision: rows stay in Rust; IPC is the virtualization backplane

The 100,000 rows are generated in Rust (seeded xorshift64*, no `rand` dep)
and never leave the Rust process wholesale. The webview holds only a fetched
window: virtualization math (fixed 28 px rows, spacer div, translateY slice)
runs in JS, and the slice asks `get_rows(start, count)` over IPC whenever the
cache (render range + 48-row pad each side) no longer covers the viewport.
Filter and sort run in Rust over an index vector (`view: Vec<u32>`), so
`FILTER_MS` is printed directly from where the filter runs — no console
piping needed.

Why this side of the boundary:
- It exercises the architectural question Tauri actually poses: is the IPC
  bridge fast enough to be a row-supplier for a scrolling grid? (Answer
  below: yes, comfortably.)
- Shipping 100k rows to JS once would serialize/deserialize a ~10 MiB JSON
  payload at startup and duplicate the dataset in webview memory; the
  windowed model keeps the webview allocation bounded (~150 row DTOs).
- It matches Tauri's own "business logic in Rust" guidance and the design of
  ../tauri-app.

The tradeoff, honestly: a pure-JS grid (rows shipped once) would filter/sort
with zero IPC latency and never show placeholder rows during a fast fling.
With the windowed model, every cache miss is an async hop; flinging the
scrollbar shows "…" placeholder rows for one round-trip (~a few ms here —
not perceptible at these payload sizes, but it is extra machinery: stale-
generation guards, in-flight dedupe, re-fetch on discard). A JS-side copy is
also the only option if you want offline column ops without command plumbing.
For 100k small rows either works; the Rust-side view wins on principle
(single source of truth) and on startup cost, and is what this app tests.

## Numbers (this machine, M4 Pro; MiB units)

- Clean serial `cargo build --release`: **116.9 s** wall (`/usr/bin/time -p`,
  built while other agents' builds ran concurrently — treat as upper bound;
  iteration-1 tauri-app measured 36 s on an idle machine with the same dep
  set). Incremental rebuild after a UI-only change (assets re-embed): 30.9 s.
- `BUILD_MS 14.5` (typical; 14.2–19.8 across three runs) — generating 100k
  rows + initial view build in Rust.
- `FILTER_MS` (printed from Rust; includes substring scan of 100k names +
  re-sort of matches under the active sort; observed with name-desc sort):
  1-char **3.4 ms**, 2-char 3.5 ms, 3-char 5.6 ms, 4-char **4.2 ms**,
  clearing back to empty (100k rows re-sorted) 10.8 ms — worst case.
  User-perceived latency adds one IPC round-trip + slice re-render (sub-ms
  to low-ms at this payload; not separately instrumented).
- RSS (`ps -o rss= -p <pid>`, main process only — WKWebView's web content
  runs in Apple's shared `com.apple.WebKit.WebContent` XPC helpers, so this
  understates true footprint; same caveat as iteration 1):
  after load **120.8 MiB**, after a 24-position random scroll burst
  **121.2 MiB** (run 2; run 1 measured 109.2 / 111.9 MiB — WebKit variance).
- Release binary 8.0 MiB; 204 unique crate names (same set as tauri-app —
  zero new dependencies for this app).

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| table_widget | **hand-rolled** | observed | Tauri ships no widgets at all — the "table" is flex-row divs + a sticky header in a scroll container. The honest ecosystem answer is that real Tauri apps npm-install a JS grid (AG Grid, TanStack) — out of bounds here (no external JS libs, no Node), so everything below is built from DOM primitives. |
| virtualization | **hand-rolled** | self-test | Fixed row height + scroll math: spacer div sets scrollHeight, absolute slice translateY'd to the first rendered row, ~35 rows rendered, windowed `get_rows` over IPC on cache miss. Self-test scrolls to view-index 50,000 and 99,999 and asserts real rows render; a 24-position scroll burst ran without error and +0.4 MiB RSS. Smoothness under continuous human dragging: unexercised (headless run; placeholders are the designed fallback during misses). |
| sort | **hand-rolled** | self-test | Header click → `set_sort` command → Rust `sort_unstable_by` over the index vector; asc/desc toggle + ▲/▼ indicator asserted via synthetic header clicks. Full 100k name sort inside the 10.8 ms worst-case FILTER_MS. |
| filter_latency | **hand-rolled** | self-test | Typed "p"→"pr"→"pri"→"prim" via synthetic input events through the real IPC path: FILTER_MS 3.4/3.5/5.6/4.2 ms (1→4 chars). Rust-side scan is nowhere near being the bottleneck; no debounce needed. |
| column_resize | **hand-rolled** | synthetic-input | Pointer events on an 8 px header divider write `--w-<i>` CSS custom properties; all cells size from the vars, so one style write resizes the column everywhere including the sticky header. Verified by dispatched PointerEvents (+80 px asserted); `setPointerCapture` throws on synthetic pointerIds, so move/up listeners go on `window` (try/catch around capture). Not human-dragged. |
| row_selection | **hand-rolled** | synthetic-input | Click delegation on the slice; Set of view indices + anchor; shift-click selects the contiguous range (asserted: click vi=2, shift-click vi=6 → 5 highlighted). Selection is view-relative and intentionally cleared on filter/sort change (documented approximation — id-keyed persistence would need an extra IPC lookup of ids in a view range). |
| cell_custom_render | **built-in** | self-test | The one place the webview stack genuinely shines: a cell is arbitrary HTML/CSS, so the status chip is a `<span>` + 6 lines of CSS. Chip presence asserted in the self-test. |

## Helper crates

None beyond iteration 1's set (`tauri`, `tauri-build`, `serde`, `serde_json`
— the latter two required by `#[tauri::command]` serialization). PRNG is a
12-line xorshift64*; dates avoid month-length logic by capping day at 28.

## LoC (812 physical; 852 including config)

- Production **690**: Rust **251** (245 of `src/main.rs` + 6 `build.rs`),
  frontend **439** (31 HTML + 270 JS + 138 CSS)
- Verification **122**: Rust 8 (`report` command + selftest flag), frontend
  114 (`ui/selftest.js` 110 + hooks/flag lines in main.js/index.html)
- Config: 40 (`tauri.conf.json` 33 + capability 7)

## Where the time went

1. Two real bugs found by the first self-test run, both in the async-window
   plumbing, neither in Tauri: (a) a `get_rows` response from a stale
   generation was discarded without re-triggering a fetch, stalling the
   viewport on placeholders until the next scroll event; (b) the self-test's
   own "filter applied" wait condition passed before the IPC response landed
   (fixed with an applied-sequence counter). The virtualization itself and
   the IPC bridge worked first try.
2. Sticky-header + two-axis scroll layout (`width: max-content; min-width:
   100%` on header/spacer/rows) — CSS fiddling, not framework friction.
3. Self-test harness (~110 LoC) — the price of headless, evidence-producing
   verification; it caught bug (a), so it paid for itself.

## Verification

Built release; three launches from the raw binary. (1) `GRID_SELFTEST=1`
run: 14/14 assertions PASS (initial window, chips, mid/bottom windowed
scroll, sort asc/desc + indicators, 4-step filter typing + count label,
clear-filter, shift-click range, pointer-event resize), FILTER_MS/BUILD_MS
lines on stdout, RSS checkpoints via `ps` — full log retained at
collection time (grid-run2.log). (2) Same run earlier caught 2 bugs (11/14).
(3) Plain launch, no selftest: alive at 10 s, only output `BUILD_MS 14.514`,
killed cleanly. All synthetic-input caveats: no human mouse/keyboard drove
the window; events were dispatched inside the real WKWebView DOM.
