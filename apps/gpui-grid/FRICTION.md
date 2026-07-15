# FRICTION — Grid (gpui =0.2.2)

Reference: SPEC-7.md. Built + verified on macOS (M4 Pro, rustc 1.96.1):
`cargo build --release` clean in 145 s wall (cold — all deps; crates.io gpui
with `runtime_shaders`, see apps/gpui-app/GAPS.md; the known `block v0.1.6`
future-incompat warning is the only noise). Binary 5.13 MiB unstripped
(5,378,816 B). Launched several times; alive ≥ 10 s each; killed cleanly;
empty stderr.

Verification method: OS-level *keystroke* injection is unreliable on this
shared desktop (other agents' windows churn the z-order — one injection round
landed on a sibling app's window; injection was gated on a frontmost check
after that and abandoned when the check failed). Evidence used instead:
- **synthetic-input**: real CGEvent mouse events (AXIsProcessTrusted()=true)
  posted at window-relative coordinates while the Grid window was topmost —
  column-resize drag and click/Shift-click selection (grid-window-interact.png).
- **self-test**: `GRID_SELFTEST=1` drives the *same functions the UI events
  call* (filter set → `apply_filter`, header toggle → `toggle_sort`, scroll
  handle) with screenshots taken during the run (grid-window.png filtered,
  grid-window-end.png sorted + mid-scroll). Retained stdout: grid-stdout.log.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| table_widget | **hand-rolled** | observed | gpui core has **no table/grid widget** — no header, column, or cell abstraction. The ecosystem has `gpui-component` (crates.io) with a Table, not evaluated/adopted here (core-only rule, same call as gpui-board). The table is a header row of styled `div()`s + a `uniform_list` of row `div()`s whose cells share a `widths: [f32; 6]` array (border-box sizing makes header/cell x-alignment exact with zero math). ~120 LoC for the table chrome. |
| virtualization | **built-in** | self-test | `uniform_list` (what Zed uses) renders only the visible range — the closure gets `Range<usize>` and returns ~24 rows regardless of the 100k count. RSS 97.3 MiB after load → 97.9 MiB after a full-range scripted scroll (121 × `scroll_to_item` over 6 s, painting confirmed by screenshots); a second run measured 91.1 MiB after scroll, i.e. flat within noise. Scrolling stayed fluid on screen. Caveat: uniform_list is vertical-only — no horizontal scrolling if columns are resized past the viewport (rows clip). |
| sort | **assembled** | self-test | Header `on_click` → app-side `sort_unstable_by` on the `Vec<u32>` index vector; ▲/▼ appended to the header label. `SORT_MS Name 12.2–15.9`, `SORT_MS Value 3.5–5.0` (ms, 100k rows). The framework contributes nothing but the click — which is fine; the indices-not-data model keeps it trivial. Toggle is same-column click (asc→desc), per spec. |
| filter_latency | **hand-rolled** | self-test | `FILTER_MS 1 3.87` / `FILTER_MS 4 4.65` (1-char / 4-char, ms; full range across the pass 3.6–6.5 ms) — substring scan over 100k precomputed-lowercase names + index-vector rebuild, re-applying any active sort. Filter-as-you-type is therefore ~2 orders of magnitude under one frame; no debounce needed. The *input box* is the minimal hand-rolled `on_key_down` field from gpui-app (no IME/selection/clipboard — gpui ships no text-input widget; the self-test calls the same `apply_filter` the key handler calls, key handler verified by code review). |
| column_resize | **assembled** | synthetic-input | A real divider drag, not an approximation: 7 px handle at each header cell's right edge starts a typed `on_drag` with an invisible ghost entity; `on_drag_move::<ColResize>` on the header row receives *its own bounds* every mouse move, so `new_width = cursor.x − (bounds.origin.x + Σ widths[..col])` — no manual bounds bookkeeping. Verified by CGEvent drag: ID column 70 → ~150 px on screen (grid-window-interact.png), live during the drag. `cursor_col_resize()` styling built in. ~25 LoC. |
| row_selection | **assembled** | synthetic-input | `on_click` per row; `ClickEvent::modifiers()` exposes shift/cmd natively, so plain-click = single select, Shift-click = range from anchor, Cmd-click = toggle — all real, none approximated. Selection stores row *ids* (survives re-sort/re-filter); anchor is a visible-index. Verified by CGEvent click row 0 + Shift-click row 4 → 5 rows highlighted, “5 selected” footer (grid-window-interact.png). |
| cell_custom_render | **assembled** | observed | Any element composes into a cell — the Status chip is a `rounded_full` div with per-variant bg/fg (Ok green / Warn amber / Err red), visible in every screenshot. Right-aligned Value is `.justify_end()` on the cell. There is no cell abstraction to fight because there is no table widget at all; “custom cell” is the *default* state of the world in gpui. |

## Instrumentation (retained: grid-stdout.log)

- `BUILD_MS 41` (28–41 across runs) — 100k-row generation (seeded xorshift64,
  hand-rolled civil-from-days ISO dates; no rand/chrono) + initial index build.
- `FILTER_MS <len> <ms>`: 1→3.87, 2→3.65, 3→6.45, 4→4.65, 0(clear)→0.03.
- `SORT_MS`: Name 15.29/15.93 (asc/desc), Value 5.03; second run 12.22/3.53.
- RSS via `ps -o rss= -p <pid>` (KiB): 99,616–99,632 after load (≈ 97.3 MiB);
  93,264–100,208 after the long scroll (≈ 91.1–97.9 MiB). ~100k rows of
  formatted strings cost ≈ +22 MiB over the gpui-board baseline (75 MiB).

## Helper crates

None — `gpui = "=0.2.2"` (runtime_shaders) only. PRNG and date formatting are
hand-rolled (~20 LoC) to keep the dependency set identical to prior gpui apps.

## LoC split

- Production: **621** (src/main.rs 655 total, single file)
- Verification: **34** (the `spawn_selftest` task + its env gate; the
  BUILD_MS/FILTER_MS/SORT_MS prints are spec-mandated and counted as
  production). External verification scripts (CGEvent injection, window-id,
  frontmost check) live in the session scratchpad, ~110 LoC Swift, not shipped.

## Where the time went

1. **Nothing table-shaped exists** — but assembling it was mostly mechanical;
   the real cost was *verification*, not construction: proving painting during
   the scripted scroll (a first screenshot captured a stale, occluded frame
   buffer — occluded gpui windows stop painting, so evidence had to be
   re-captured with the window frontmost), and the safe-injection dance on a
   desktop shared with other agents.
2. **uniform_list closure ergonomics** — the render closure gets `&mut App`,
   not `Context<Self>`, so row callbacks capture an `Entity<GridApp>` handle
   and call `entity.update(...)` (the gpui-babel pattern, plus per-row
   `entity.clone()`); `cx.listener` sugar is unavailable inside the list.
3. **Column-resize math** — one subtlety: the divider must
   `stop_propagation` on click or a resize-click also sorts the column.

## Surprises

- The whole SPEC-7 feature list needed **zero new framework capabilities**
  beyond what iterations 1–3 already used: uniform_list + typed DnD + styled
  divs cover a 100k-row sortable/filterable/selectable/resizable grid.
- 100k is nowhere near uniform_list's limit: filter 4–6 ms, sort 4–16 ms,
  flat RSS across a full scroll. The 11k-line Babel finding extrapolated
  cleanly (~10× rows ≈ same per-frame cost).
- `ClickEvent::modifiers()` making Shift/Cmd-click *native* was unexpected
  after frameworks where selection modifiers require keyboard-state tracking.
- Occluded windows stop painting (macOS display-link pause) — fine for users,
  a trap for screenshot-based verification.
