# FRICTION — tauri-dash ("Pulse", SPEC-2)

Tauri =2.11.5 / tauri-build =2.6.3 (same pins as ../tauri-app), same manual
no-Node setup (hand-written `tauri.conf.json`, `withGlobalTauri`, static
vanilla HTML/CSS/JS in `ui/`, hand-written capability, copied icons).
Frontend is vanilla JS + Canvas2D; **no external JS libraries**.

## Architecture: generator in Rust, pushed over the event bridge

The synthetic data generator is a Rust thread (mean-reverting random walk,
xorshift64* — not worth a `rand` dep) that `app.emit("tick", …)`s a batch of
6 values at the tick rate. The frontend `listen()`s, keeps 300-sample ring
buffers, and repaints. `set_rate` / `set_paused` / `get_config` commands
control the generator; `PULSE_HZ` env var sets the starting rate so a
headless run can exercise 60 Hz without scripting the slider.

### How the IPC event path held up (measured, stdout via `report_stats`)

The frontend timestamps every event arrival and reports 5 s windows back to
Rust, which prints them — so a headless launch verifies the full
Rust→webview→Rust loop:

- **10 Hz**: 49–50 ticks per window, inter-arrival mean **99.98 ms**
  (target 100), max 104 ms, sd 1.8 ms.
- **60 Hz**: 296–299 ticks per window (~59.8 Hz effective), mean **16.65 ms**
  (target 16.67), max 21 ms, sd 1.7–1.8 ms. No drift, no backlog, no
  missed batches over the run.
- **emit→listener latency**: sub-millisecond at both rates (measured mean
  |0.7| ms; the sign is clock-domain skew between Rust `SystemTime` and JS
  `performance.timeOrigin + now()`, so read it as "< 1 ms", not a precise
  value).

Verdict: at this payload size (~120-byte JSON), the event bridge is nowhere
near saturation at 60 Hz; jitter is bounded by thread scheduling, not IPC.

**Repaint strategy**: every tick pushes into ring buffers and requests a
redraw; redraws are coalesced to **one requestAnimationFrame per display
frame** (6 sparkline canvases + main chart + big numbers per repaint). At
60 Hz the event rate ≈ refresh rate, so coalescing keeps it at ≤1 full
Canvas2D repaint per frame — no observed jank; the 60 Hz stats above were
collected while repainting continuously.

## Capability ratings

| Capability | Rating | Note |
|---|---|---|
| DnD card reorder | **hand-rolled** | HTML5 drag events give dragstart/over/drop + a free native ghost, but nearest-slot hit-testing over the grid, the slot cue, the reorder, and the FLIP reflow animation are all hand math (~55 LoC). **Trap:** wry's native drag-drop handler swallows HTML5 DnD — `"dragDropEnabled": false` required in the window config. |
| Live data (timer @ 10 Hz) | **assembled** | `std::thread` + absolute-deadline pacing + `app.emit()` on the Rust side, one `listen()` in JS (~30 LoC total). No framework timer/subscription abstraction, but the event bridge is a first-class, well-documented API and it just worked. |
| Sparklines + main chart | **hand-rolled** | Canvas2D from scratch: extent/padding, right-anchored scroll mapping, gridlines + labels, gradient fill, devicePixelRatio scaling (~90 LoC). Nothing chart-shaped exists in the stack. |
| Hover crosshair + tooltip | **hand-rolled** | mousemove → nearest-sample index → dashed crosshair + marker drawn into the same canvas pass; tooltip is an absolutely-positioned DOM node with edge-flip (~45 LoC). |
| Slider control | **built-in** | `<input type=range>` does 1–60 Hz directly; wiring is one `input` listener + one `invoke("set_rate")`. |
| Click-to-select | **built-in** | DOM `click` listener + a CSS class + chart retarget; ~10 LoC. |
| Animation | **assembled** | CSS transitions are the built-in primitive (hover elevation, selection ring). The card-reorder animation is the FLIP technique — JS measures rects and sets transforms, CSS tweens them (~20 LoC helper). No tweening/spring API beyond CSS; rAF is the per-frame callback. |

## Helper crates

None beyond iteration 1's set (`tauri`, `tauri-build`, `serde`,
`serde_json` — the latter two required by `#[tauri::command]` and the
`Serialize` event payload). No DnD/chart/animation crate exists to reach
for in the Tauri model; equally, none was needed.

## LoC (733 source; 773 including config)

- Rust: **168** (162 `src/main.rs` + 6 `build.rs`)
- Frontend: **565** (34 HTML + 396 JS + 135 CSS)
- Config: 40 (`tauri.conf.json` 33 + capability 7)
- Release binary **7.9 MiB**; **204 unique crate names** (unchanged from
  iteration 1). The old 271 figure counted flattened tree/name-version rows.

## Where the time went

1. Main chart + crosshair: coordinate mapping (right-anchored scroll,
   extent padding, index↔pixel inversion for hover) is fiddly by hand.
2. DnD reorder + FLIP: the mechanics are known patterns but pure manual
   labor; nothing in the stack helps.
3. IPC measurement plumbing (`report_stats`, stats windows) — self-imposed
   but it is also the verification story.

## Verification

Built release; launched from the raw binary twice (`PULSE_HZ=10` and `=60`),
~11 s each, alive both times, killed cleanly; stats lines above prove the
event path, command path, and JS tick handler ran end to end. Interactions
(DnD, select, hover, slider, pause) can't be clicked headlessly; they were
smoke-run in JavaScriptCore (same engine as WKWebView) against a minimal DOM
stub — card build, 70 ticks, select, hover tooltip, drag-reorder to a new
slot, pause/slider round-trips all asserted green — plus verified by
construction. The harness source and output were not retained, so these are
collection-time narrative checks, not rerunnable executable evidence.

## Surprises

- The event bridge is *better* than expected: 60 Hz sustained with ~1.8 ms
  sd and sub-ms delivery, with zero tuning.
- `dragDropEnabled: false` is a genuine trap: HTML5 drag events silently
  never fire while the window config default (true) lets wry's file-drop
  handler claim drags. Documented for Windows; disabled defensively here.
- Negative measured latency was a useful reminder that `SystemTime` and the
  webview's `performance.timeOrigin` are different clock domains — same-box
  cross-process latency below ~1 ms can't be measured this way, only bounded.
