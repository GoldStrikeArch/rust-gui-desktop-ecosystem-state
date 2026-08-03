# FRICTION — Grid (floem git @ 778bb5f2), SPEC-7

Verified on macOS (M4 Pro, rustc 1.96.1): release build clean (locked too);
plain launch alive >8 s; `GRID_SELFTEST=1` scripted run prints the full
evidence trail (selftest-log.txt) and exits `SELFTEST DONE pass=14 fail=0`.
The generated data is byte-identical to the iced port (same xorshift* seed:
`SORT name asc first_id=70664` matches iced's log exactly).

## Capability ratings (rating + evidence + note)

| capability | rating | evidence | note |
|---|---|---|---|
| table_widget | **assembled** | observed | floem has NO table widget. The grid is taffy flex rows + a header row; column widths live in one `RwSignal<[f64;6]>` read by every cell's reactive style closure. |
| virtualization | **built-in*** | self-test + observed | `VirtualStack` (understory_virtual_list) is real windowed virtualization — only ~40 row views exist; 100k rows sit at ~116 MiB RSS, scrolling is instant. The asterisk is a HEADLINE TRAP: without `min_height(0)` on the scroll's flex chain, taffy sizes the scroll to its 2.6-million-px min-content height, the clip never applies, the VirtualStack sees viewport == content and **materializes all 100k rows — 16 GiB RSS, 100% CPU shaping labels for minutes, no window ever appears**. Nothing warns; the fix is one obscure style line. (The same silent pathology had already inflated floem-babel to 1.9 GiB before this diagnosis.) |
| sort | **assembled** | self-test | Header `Click` listeners toggle asc/desc with ▲/▼ indicator (derived label). Sorting 100k u32 indices: 12.2 ms first sort, ~1–2 ms subsequent (`SORT ... ms=` lines). |
| filter_latency | **assembled** | self-test | `FILTER_MS 1 4.60` / `FILTER_MS 4 7.02` (full recompute incl. re-sort; same order as iced's 3.9/4.0 ms). Filter-as-you-type is an `Effect` tracking the TextInput's buffer signal — no change-callback needed. |
| column_resize | **hand-rolled** | self-test + observed | 7 px divider strips: `PointerDown` + `cx.request_pointer_capture` keeps `PointerMove` flowing outside the strip; width = start + Δx into the widths signal; every cell re-styles reactively. Pointer capture made this notably cleaner than iced's global-subscription dance (~30 LoC). Self-test drives the same math (`RESIZE col=id width=125`). |
| row_selection | **assembled** | self-test | `PointerDown` carries the modifier state (`event.state.modifiers`), so shift-range/cmd-toggle need NO global modifier tracking (iced needed a permanent subscription). `SELECT count=4` = shift-range of 4. |
| cell_custom_render | **assembled** | observed | The status chip is just a Label styled with background/rounded corners — any view can be a cell, no cell-renderer API needed. |

## Numbers

- `BUILD_MS 13.49` (100k row generation + model)
- FILTER_MS: 1-char 4.6 ms · 4-char 7.0 ms (typical)
- RSS after load: **~116 MiB**; unchanged (±1 MiB) after the scripted
  260,000 px jump-scroll (`ps -o rss= -p <pid>` at 3 s and 8 s)
- LoC 667 total; ~120 of those are the scripted self-test / evidence hooks

## The min-content trap (headline finding, worth repeating)

`VirtualStack` computes its window from `visual_rect` (clipped) vs
`layout_rect` (unclipped). Any flex ancestor that lets the scroll grow to
content height (taffy's default `min_height: auto`) silently disables
virtualization *and* the event loop never goes idle while it shapes 100k
labels — observed as "no window, 100% CPU, 16 GiB RSS, timers never fire".
Diagnosis required `sample(1)` stack dumps + a view-creation counter. Cost:
~1.5 h, by far the biggest line item in this app.

## Where the time went

1. The min-content trap above.
2. Everything else was fast: sort/filter/selection are plain signal code;
   pointer-capture resize worked first try.

## Helper crates

None (xorshift* PRNG hand-rolled, as in the sibling apps).
