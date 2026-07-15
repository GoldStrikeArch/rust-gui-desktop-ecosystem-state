# FRICTION — Grid (iced =0.14.0)

Reference: SPEC-7.md. Built + verified on macOS (M4 Pro, rustc 1.96.1):
`cargo build --release` clean, binary launched, window pixel-verified,
alive well past the 10 s bar (ran ~30 min through the whole self-test),
killed cleanly. No fallback was needed — every SPEC-7 requirement was
implementable (column resize is a real header-divider drag, not an
approximation).

Evidence labels used below: **observed** (seen working in the launched app
without synthetic input), **self-test** (driven by synthetic HID input,
verified via the captured stdout log `selftest-log.txt` + window
screenshots in `selftest/`), **source-only** (concluded from reading vendored
crate source, not executed), **synthetic-input**, **unexercised**.

## Headline numbers (from selftest-log.txt, stdout retained)

- `BUILD_MS 35.74` — generate 100k rows + initial index build (observed).
- `FILTER_MS 1 3.96` / `FILTER_MS 4 4.03` — typical filter application at
  1-char and 4-char queries; full range seen 1.15–5.59 ms (self-test:
  keystrokes were synthetic, timing is the app's own).
- Sort 100k by name: 8.81 ms cold, 1.57 ms re-sort to desc (reverse of an
  already-sorted vec), 2.09 ms by id (self-test, `SORT …` lines).
- RSS after load: **87.9 MiB**; after a long scroll (~800 wheel events, 677
  re-window events, reaching row ~20,822 / offset y=541,600 px):
  **89.7 MiB** — flat, i.e. no widget accumulation. Command:
  `ps -o rss= -p <pid>` (KiB) / 1024.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| table_widget | **hand-rolled** | source-only (for the built-in's limits) + observed (for the replacement) | iced 0.14 *does* ship `widget::table` — but reading `iced_widget-0.14.2/src/table.rs` shows `Table::new` eagerly builds one `Element` per cell for **all** rows (100k×6 = 600k widgets), with no virtualization, sorting, selection, or resizing; it is a layout helper for small data. Not viable here, so the table is a hand-rolled `scrollable` whose content is `[top spacer, ~40 windowed rows, bottom spacer]`. |
| virtualization | **hand-rolled** | self-test | Windowing from `scrollable::on_scroll`'s `Viewport::absolute_offset()` + fixed 26 px row height; a `sensor` wrapper supplies the viewport height before the first scroll. Content height stays exact (spacers), so the native scrollbar is proportional and draggable. Proven: 677 `WINDOW first=…` re-window events during a long scroll, deep-scroll screenshot at row 20,831, flat RSS. Caveat: iced has no built-in lazy list (`lazy` is a diff-skipping wrapper, not a virtualizer). |
| sort | **assembled** | self-test | `mouse_area` around each header cell → `sort_unstable_by` on the `Vec<u32>` index (rows stay immutable), `▲/▼` appended to the header text. Asc→desc toggling per SPEC. 8.8 ms worst case at 100k means synchronous sorting in `update` is fine — no async ceremony needed. |
| filter_latency | **assembled** | self-test | Filter-as-you-type on `text_input::on_input`, substring on name, recompute-from-scratch each keystroke (filter + re-apply active sort), self-timed. **1-char: 3.96 ms, 4-char: 4.03 ms** — far below frame budget; no debounce required at 100k rows. After each filter the body snaps to top via `widget::operation::scroll_to` to keep the window/offset consistent. |
| column_resize | **hand-rolled** | self-test | Real drag on a 7 px divider strip after each header: `mouse_area::on_press` arms the drag, a global `event::listen_with` subscription (alive only while resizing — same pattern as iced-dash's card drag) streams `CursorMoved` deltas, release commits. Proven: synthetic 60 px drag → `RESIZE col=id width=125` + screenshot (70→125; ~5 px eaten because the origin is the first *move* after the press — mouse_area's on_press carries no position). |
| row_selection | **assembled** | self-test | `mouse_area` per row + `HashSet` + anchor. Shift-click range and Cmd-click toggle work for real: `mouse_area::on_press` carries no modifiers, so a permanent `event::listen_with` subscription tracks `keyboard::Event::ModifiersChanged` (the documented iced idiom). Proven: `SELECT count=1 clicked_id=2`, then shift-click → `SELECT count=4` (range anchor 13→10), highlight screenshot. Synthetic-input gotcha: winit only learns modifiers from flagsChanged key events, not from mouse-event flags. |
| cell_custom_render | **built-in** | observed | A cell is just an `Element`, so the status chip is a `container` (rounded, colored) around a `text` — zero friction, screenshots show Ok/Warn/Err chips. Right-aligned value column is `container::align_x(Right)`. |

## Helper crates

None beyond `iced = "=0.14.0"` (default features — no timers/async needed;
filtering/sorting run synchronously in `update`). PRNG is the same 10-line
xorshift* as apps/iced-dash; ISO dates are generated as y-m-d ints (no
`chrono`), so name/date sorting is plain string/int comparison.

## LoC split

- Production: **~663** (src/main.rs 700 minus ~37 lines of `GRID_SELFTEST`
  evidence instrumentation: `SORT/SELECT/RESIZE/WINDOW` prints + flags).
- Verification: **~278** = ~37 in-app instrumentation lines +
  `selftest/uihelper.swift` 172 (CGEvent input synthesis + window lookup +
  frontmost-at-point guard) + `selftest/drive.sh` 69 (guarded driver).
- Retained evidence: `selftest-log.txt` (stdout), `selftest/shot-*.png`.

## Where the time went

1. **Shared-desktop input synthesis, not the app.** The desktop hosts a
   fullscreen Zoom window and other agents' app windows that steal focus
   sub-second. One early AppleScript `click at` hit Zoom instead of this
   window; after that every input burst is gated on a
   frontmost-window-at-point check (`uihelper topat`) plus re-activation
   with retries. Two real synthetic-input traps cost debugging time:
   winit ignores modifier flags on mouse events (shift-click needs a real
   flagsChanged event first), and clicks on a non-key window only activate
   it (macOS acceptsFirstMouse).
2. **Windowing edge cases** — keeping `scroll_y`, the spacers and the
   scrollable's internal offset consistent when the filter shrinks content
   (fix: `operation::scroll_to` top on every filter change).
3. The table/virtualization core itself was quick (~an hour): `on_scroll` +
   two spacers is a well-trodden pattern and iced's `Viewport` exposes
   exactly the right numbers.

## Surprises

- Good: 100k-row synchronous filter+sort in `update` is a non-issue
  (≤9 ms); no worker threads, no incremental indexing needed.
- Good: `sensor` (new in 0.14) neatly solves "how tall is my viewport
  before the user scrolls".
- Bad: the new `table` widget looks like the headline feature for this SPEC
  but is O(rows × columns) widgets by construction — the gap between
  "has a table widget" and "has a data grid" is the whole finding.
- Bad: `scrollable`'s programmatic `scroll_to` does not emit `on_scroll`,
  so app windowing state and widget offset can drift silently.
