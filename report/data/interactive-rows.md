# Iteration-2 structured rows (Pulse dash + Board kanban) — raw agent returns

## tauri

```yaml
framework: tauri
dash:
  build_ok: true
  launch_ok: true   # raw release binary, 11-12 s runs at 10 Hz and 60 Hz, alive both times, clean kill
  loc: 773          # Rust 168 (main.rs 162 + build.rs 6), frontend 565 (34 HTML + 396 JS + 135 CSS), config 40
  helper_crates: [] # only iteration-1 deps: tauri, tauri-build, serde, serde_json
  repaint_strategy: >-
    Rust generator thread emits a tick event per sample batch; JS coalesces to one
    requestAnimationFrame repaint per display frame (6 sparklines + main chart, Canvas2D).
    Measured: 10 Hz = mean interval 99.98ms (sd 1.8ms, max 104ms);
    60 Hz = mean 16.65ms vs 16.67 target (sd 1.8ms, max 21ms, no drops/backlog);
    emit->JS latency sub-millisecond at both rates; zero observed jank.
  ratings:
    dnd_reorder: {rating: hand-rolled, note: "HTML5 drag events + free native ghost, but nearest-slot hit-testing, slot cue, reorder and FLIP reflow are ~55 LoC of hand math. Requires dragDropEnabled:false in window config or wry swallows HTML5 DnD."}
    live_data: {rating: assembled, note: "std::thread with absolute-deadline pacing + app.emit() in Rust, one listen() in JS (~30 LoC). Event bridge is first-class, held 60 Hz effortlessly."}
    charts: {rating: hand-rolled, note: "Canvas2D from scratch: extent/padding, right-anchored scroll mapping, gridlines/labels, gradient fill, DPR scaling (~90 LoC)."}
    hover_tooltip: {rating: hand-rolled, note: "mousemove -> nearest-sample index -> dashed crosshair + marker; DOM tooltip with edge-flip (~45 LoC)."}
    slider: {rating: built-in, note: "<input type=range> 1-60 Hz; one input listener + invoke(set_rate)."}
    click_select: {rating: built-in, note: "DOM click + CSS class + chart retarget, ~10 LoC."}
    animation: {rating: assembled, note: "CSS transitions built-in; reorder uses hand-scheduled FLIP transforms tweened by CSS (~20 LoC). Primitives: CSS transitions/keyframes + rAF; no tween/spring API."}
board:
  build_ok: true
  launch_ok: true
  loc: 506          # Rust 21, frontend 445 (13 HTML + 267 JS + 165 CSS), config 40
  helper_crates: []
  ratings:
    cross_column_dnd: {rating: hand-rolled, note: "HTML5 drag primitives; per-column wiring, midpoint insertion-index math, two-splice move ~50 LoC. Same dragDropEnabled:false trap."}
    within_column_reorder: {rating: hand-rolled, note: "Same machinery; index computed excluding the dragged card so same-column drops land where indicated."}
    drop_indicator: {rating: assembled, note: "One div moved with insertBefore during dragover (~15 LoC + CSS)."}
    drag_ghost: {rating: built-in, note: "Native HTML5 drag snapshot follows cursor for free; defer the dimmed class via setTimeout(0) so snapshot is undimmed."}
    inline_edit: {rating: assembled, note: "Span<->input swap on dblclick, Enter/Esc/blur (~25 LoC); must set draggable=false during edit."}
    add_delete: {rating: assembled, note: "Button<->input swap per footer; delete per card (~35 LoC)."}
    animation: {rating: hand-rolled, note: "FLIP over persistent elements (id->element map) ~25 LoC; CSS keyframe pop-in for adds."}
    column_scroll: {rating: built-in, note: "overflow-y:auto per column; zero logic."}
```

Board state kept in JS (dragover needs synchronous DOM feedback; iteration 1 already exercised Rust-owned state); Rust shell is 15 lines.

SURPRISES:
- Tauri's event bridge over-delivered: sustained 60 Hz with ~1.8 ms sd inter-arrival and sub-ms emit→JS latency, zero tuning — IPC was never the bottleneck; hand-rolling Canvas2D charts was.
- "dragDropEnabled": false is the load-bearing trap for both apps: default true makes wry's native file-drop handler silently eat HTML5 drag events.
- A jsc DOM-stub smoke harness caught a real shipped bug (cardEls.set missing → every render duplicates cards) that the "alive 10 s" check missed — webview apps can look healthy while the UI is broken.
- The native HTML5 drag ghost is the biggest webview freebie in the study — every non-webview framework must hand-build the cursor-following preview.

TIME_SINK:
- Main-chart coordinate math (right-anchored scrolling, hover pixel→index inversion) — pure manual Canvas2D labor.
- DnD edge cases on the board (insertion index excluding dragged card, dragleave vs child relatedTarget).
- Building headless verification (stdout stats loop; jsc DOM-stub harness) since the real window can't be scripted.

## dioxus

```yaml
framework: dioxus
dash:
  build_ok: true   # 103.7 s clean (parallel w/ board); first cargo check 0 errors/0 warnings
  launch_ok: true  # 11 s run, ticking, RSS ~96 MiB, clean SIGTERM
  loc: 524
  helper_crates: ['tokio (features=["time"]) — dioxus-desktop runs on tokio but re-exports no timer']
  repaint_strategy: "Write-driven, no frame loop: use_future loop sleeps 1/hz and writes the metrics signal -> App re-runs -> VDOM diff -> minimal DOM edits over webview IPC; paused = zero re-renders; mousemove handlers peek-guard writes so unchanged state never re-renders"
  ratings:
    dnd_reorder: {rating: hand-rolled, note: "Mouse-event state machine (mousedown arm, 6px threshold, per-card mousemove picks slot, root mouseup commits) with drag state in Signal<Option<Drag>>. Needs .card * { pointer-events:none }."}
    live_data: {rating: assembled, note: "use_future + tokio::time::sleep(1/hz) loop re-reads rate/paused signals each lap so slider retargets live. ~12 LoC, but tokio must be added yourself."}
    charts: {rating: hand-rolled, note: "SVG polyline/line/text/circle in RSX regenerated from signals each tick; all scaling/axis/scroll math manual. SVG over Canvas-via-eval: stays in the declarative diff model, zero JS strings."}
    hover_tooltip: {rating: assembled, note: "onmousemove element_coordinates() -> sample index -> crosshair + tooltip div, ~25 LoC. Gotcha: offset coords relative to event target — SVG children need pointer-events:none."}
    slider: {rating: built-in, note: "HTML input type=range; webview renders natively."}
    click_select: {rating: built-in, note: "press+release under 6px threshold (3 LoC) so click and drag coexist."}
    animation: {rating: assembled, note: "CSS transitions + keyframes only. Dioxus has NO animation primitives (no tween/spring/frame callback); grid reflow after reorder snaps — FLIP would be fully hand-rolled."}
board:
  build_ok: true   # 103.3 s clean
  launch_ok: true  # 11 s, RSS ~100 MiB
  loc: 412
  helper_crates: []
  ratings:
    cross_column_dnd: {rating: hand-rolled, note: "No DnD support in Dioxus; no desktop-compatible helper crate (existing are wasm-only or pre-0.7). Mouse-event state machine in a Signal."}
    within_column_reorder: {rating: hand-rolled, note: "Same machinery; ~5 LoC index fixup when source and target share a column."}
    drop_indicator: {rating: hand-rolled, note: "KEY FINDING: MouseEvent has offset coords but NOT target size — above/below-midpoint uncomputable from the event; no sync getBoundingClientRect. Workaround: invisible top/bottom half-overlays per card + flex-grow endzone per column."}
    drag_ghost: {rating: hand-rolled, note: "Fixed-position div following client coords from root onmousemove, pointer-events:none. ~10 LoC + CSS."}
    inline_edit: {rating: assembled, note: "dblclick swaps to input; Enter/Esc on onkeydown; focus needs onmounted + async set_focus(true); stop_propagation so editing never arms a drag."}
    add_delete: {rating: assembled, note: "Pure signal writes — the React-like tier where Dioxus is effortless."}
    animation: {rating: assembled, note: "CSS settle keyframe replayed via bump counter in rsx key (drop recreates element). Neighbors snap — FLIP would be hand-rolled (documented approximation)."}
    column_scroll: {rating: built-in, note: "overflow-y:auto per column. No autoscroll-while-dragging near edges."}
```

SURPRISES:
- Dioxus mouse events carry offset coordinates but NO target-element geometry, and no sync getBoundingClientRect — the single biggest DnD pain; solved with invisible half-overlays per card.
- key: attributes are silently ignored unless on the first node of a block — surfaced only as a deprecation-style warning; would have silently broken list diffing and the drop animation.
- onresize (ResizeObserver-backed, 0.6+) fires on mount too — exact chart pixel width with zero JS; both apps' full interactive surface worked on first successful compile.
- Framework ships tokio internally but re-exports no timer — 10 Hz ticker requires adding tokio yourself.

TIME_SINK:
- DnD mechanics from scratch in both apps (~40-45% each): threshold state machines, ghost/indicator plumbing, stop_propagation choreography — ~120 lines of board exist just to move a card between two Vecs.
- Chart geometry + coordinate-space reconciliation (viewBox-vs-pixel trap, pointer-events:none discipline).
- RSX borrow discipline: precomputing view-model structs before rsx! because .read() guards + nested closures fight the borrow checker.

## slint

```yaml
framework: slint
dash:
  build_ok: true
  launch_ok: true
  loc: 667          # 274 Rust, 393 .slint
  helper_crates: []
  repaint_strategy: property-dirty -> full-window redraw via winit/femtovg GL once per tick (10-60 Hz), zero repaint when paused; ~19% of one core at 10 Hz, Path re-tessellation the likely driver
  ratings:
    dnd_reorder: {rating: hand-rolled, note: "No reorder widget; TouchArea + own threshold, ghost overlay, slot hit-test math. Key pattern: dragged card never moves, a separate ghost does — keeps TouchArea coords stable."}
    live_data: {rating: built-in, note: "slint::Timer repeated, restarted on slider change, stop() on pause; DSL also has a Timer element. Zero friction."}
    charts: {rating: hand-rolled, note: "No chart widget; Path element with SVG commands strings rebuilt in Rust per tick. Works but stringly-typed, re-tessellated per frame, all axes/scaling on you — the framework's weak spot as predicted."}
    hover_tooltip: {rating: assembled, note: "TouchArea.has-hover + mouse-x -> index; crosshair/marker/tooltip as conditional Rectangles (~35 LoC)."}
    slider: {rating: built-in, note: "std-widgets Slider."}
    click_select: {rating: assembled, note: "clicked exists but sharing a TouchArea with drag forces manual click-vs-drag disambiguation in pointer-event(up)."}
    animation: {rating: built-in, note: "Declarative animate blocks (duration/easing incl. cubic-bezier) + states with transitions; no springs, no per-frame callback. Slot-field positioning + animate x,y = animated grid reflow for free."}
board:
  build_ok: true    # one trivial fix (rotation-angle not valid on Rectangle)
  launch_ok: true   # 0.0% idle CPU
  loc: 499          # 132 Rust, 367 .slint
  helper_crates: []
  ratings:
    cross_column_dnd: {rating: assembled, note: "Slint 1.17 has a real DnD API: DragArea per card + DropArea per column (threshold, dragging state, cancel built in). Payload is DSL-opaque DataTransfer built/parsed by Rust callbacks (stringly 'col:idx')."}
    within_column_reorder: {rating: assembled, note: "can-drop(DropEvent) streams column-local position -> round(y/row-h) insertion index; Rust does -1 same-column adjustment."}
    drop_indicator: {rating: assembled, note: "Accent slot highlight + a real animated gap (cards below shift one row)."}
    drag_ghost: {rating: hand-rolled, note: "DragArea.drag-image is bitmap-only, no element->image rendering in the DSL; shipped pseudo-ghost overlay driven by can-drop positions (tracks only over DropAreas). Biggest gap in the new API."}
    inline_edit: {rating: assembled, note: "double-clicked -> conditional LineEdit with focus/select-all; Enter=accepted; Esc has no LineEdit hook — caught by wrapping FocusScope via key bubbling (idiomatic but non-obvious)."}
    add_delete: {rating: assembled, note: "LineEdit accepted + VecModel push/remove; same FocusScope-for-Esc trick."}
    animation: {rating: assembled, note: "Cards absolutely positioned by index with animate y. A layout-based list could NOT animate reorder at all (repeater instances are index-bound)."}
    column_scroll: {rating: built-in, note: "Flickable{interactive:false} disables press-drag flicking (would fight DnD) but wheel still scrolls — verified in i-slint-core source, not docs."}
```

SURPRISES:
- Slint 1.17 ships first-class DragArea/DropArea elements (payload-based, copy/move/link actions, DropEvent.position) — cross-column DnD dropped from expected hand-rolled to assembled; but they model DATA transfer, not visual reorder: ghost/indicator/index math remain yours.
- The data-transfer payload is deliberately opaque in the DSL — even an internal card move round-trips through Rust via a stringly payload.
- "Static card + floating ghost" makes hand-rolled TouchArea dragging surprisingly tame (~60 LoC) by sidestepping the coordinate-feedback loop.
- Flickable{interactive:false} still handles wheel scroll — exactly what a drag-inside-scroll UI needs; confirmed only by reading i-slint-core source.

TIME_SINK:
- Charts: Path commands string generation + knowing string re-parse/re-tessellation is the CPU cost (~19% core at 10 Hz vs 0% idle board).
- Reverse-engineering the DragArea/DropArea contract from crate source (no drag-start callback — changed dragging is the signal; drag-image bitmap limitation); docs.slint.dev URLs unreliable post-restructure.
- Animating list reorder: repeaters bind rows to fixed instances, so both apps abandoned layouts and positioned elements from a slot/index field to get animate x,y reflow.

## iced

```yaml
framework: iced
dash:
  build_ok: true
  launch_ok: true   # ~1.5% CPU @ 10 Hz
  loc: 777
  helper_crates: []   # rand avoided via 10-line xorshift; no chart/DnD crate exists for 0.14
  repaint_strategy: message-driven only — 10 Hz tick clears canvas::Caches once per tick; crosshair uses canvas-local request_redraw; window::frames() subscribed only while a hover animation is in flight; drag listener subscribed only mid-drag
  ratings:
    dnd_reorder: {rating: hand-rolled, note: "No DnD API in iced 0.14. mouse_area on_press/on_enter per card + gated global event::listen_with + pin-in-stack ghost + 8px threshold. ~90 LoC but ZERO manual hit-testing — on_enter during drag does the targeting."}
    live_data: {rating: built-in, note: "iced::time::every is exactly right and rate changes re-key automatically, BUT doesn't compile under default features — needs the smol/tokio executor feature; the error gives no hint."}
    charts: {rating: hand-rolled, note: "canvas + Program trait is idiomatic; every polyline/gridline/label manual (~150 LoC). Pleasant API, no egui_plot equivalent for 0.14."}
    hover_tooltip: {rating: hand-rolled, note: "Canvas-internal State, request_redraw on CursorMoved. No canvas text measurement — tooltip width estimated from char count."}
    slider: {rating: built-in, note: "slider(1..=60, hz, Msg) — one line."}
    click_select: {rating: built-in, note: "mouse_area on_press + styled border."}
    animation: {rating: assembled, note: "0.14 ships first-party Animation<T> (lilt-based) + application::timed + window::frames() + float widget. You schedule frames yourself; no layout/FLIP animation, so drag reflow snaps."}
board:
  build_ok: true
  launch_ok: true   # 0.0% CPU idle
  loc: 617
  helper_crates: []   # iced_drop abandoned pre-0.13; nothing compiles against 0.14
  ratings:
    cross_column_dnd: {rating: hand-rolled, note: "~110 LoC: press arms, threshold activates, card removed from model into drag struct; per-card on_move + lane on_enter + tail zone; gated listen_with for cursor+drop."}
    within_column_reorder: {rating: hand-rolled, note: "Free once cross-column exists — same remove→retarget→insert path."}
    drop_indicator: {rating: assembled, note: "4px accent container injected at target index (~10 LoC)."}
    drag_ghost: {rating: assembled, note: "pin(ghost) in a root stack following cursor. Ghost is non-interactive so it never steals hover from drop targets."}
    inline_edit: {rating: assembled, note: "on_double_click built-in; Enter via on_submit; focus via operation::focus(id). SHARP EDGE: text_input CAPTURES Escape so keyboard::listen() never fires — needs raw event::listen_with."}
    add_delete: {rating: built-in, note: "Standard Elm CRUD."}
    animation: {rating: hand-rolled, note: "No layout/FLIP animation exists — reorder snaps; landed card plays 200ms settle pop via Animation + float (approximation per fallback rule)."}
    column_scroll: {rating: built-in, note: "One scrollable per lane; mouse_area coords stay correct inside scrolled viewports — DnD needed zero offset math."}
```

SURPRISES:
- time::every doesn't exist under default features — timers require the smol/tokio executor feature; compile error gives zero hint (found via stopwatch example).
- The classic drag-ghost problem (preview stealing hover) is a NON-issue: events pass through non-interactive overlay layers and on_enter/on_move keep firing mid-drag — no manual rectangle hit-testing at all.
- iced 0.14's animation story (Animation + timed + frames() + float) is coherent and power-sane but documented almost solely by the version-matched gallery example.
- text_input captures Escape, silently defeating keyboard::listen() — required source-diving ~/.cargo to diagnose.

TIME_SINK:
- API verification for a 7-month-old release: docs.rs thin; answers came from grepping vendored 0.14 source + version-matched examples.
- Chart plumbing in raw canvas (y-scaling, scrolling window, snapping, tooltip edge-flip) — mechanical but all manual.
- Designing the DnD message protocol (arm/threshold/remove-while-dragging/live-retarget) — design took longer than code; both apps reused the pattern.

## gpui

```yaml
framework: gpui
dash:
  build_ok: true   # only transitive block v0.1.6 future-incompat warning (known)
  launch_ok: true  # ~2.1% CPU @ 10 Hz, 75 MB RSS
  loc: 733
  helper_crates: []   # gpui =0.2.2 + runtime_shaders only
  repaint_strategy: "event-driven cx.notify() per 10 Hz tick / mouse event dirties window -> full re-render + whole-scene repaint per dirty frame; only the oneshot 320 ms drop-settle animation requests frames outside events"
  ratings:
    dnd_reorder: {rating: assembled, note: "Native typed DnD (on_drag/drag_over/on_drop, framework-painted ghost entity, no manual hit-testing); only the slot->metric reflow Vec is app code (~12 LoC)."}
    live_data: {rating: assembled, note: "No timer/subscription API; idiom is entity-owned cx.spawn loop on BackgroundExecutor::timer (~20 LoC), re-reading hz each lap."}
    charts: {rating: hand-rolled, note: "canvas() + PathBuilder + paint_path/paint_quad; all scaling/gridlines/scroll manual (~120 LoC). No chart crate exists for gpui."}
    hover_tooltip: {rating: hand-rolled, note: "on_mouse_move + manual snap; needs Rc<Cell<Bounds>> paint-time probe because handlers can't query element bounds."}
    slider: {rating: hand-rolled, note: "No widgets: track/fill/thumb divs, click-to-jump, drag via on_drag with an invisible ghost purely to get on_drag_move streams (~70 LoC). Cost more than the entire DnD feature."}
    click_select: {rating: built-in, note: "on_click + conditional border."}
    animation: {rating: built-in, note: "with_animation(id, Animation::new(dur).with_easing) per-frame tween, restarted by epoch-keyed id; duration+easing only — no springs, no layout/FLIP (reflow snaps)."}
board:
  build_ok: true
  launch_ok: true  # ~0.2% CPU idle (fully event-driven)
  loc: 515
  helper_crates: []
  ratings:
    cross_column_dnd: {rating: assembled, note: "Native typed DnD; on_drag_move fires per-listener with that listener's own bounds (capture phase), on_drop commits; app computes insertion index + model surgery (~45 LoC)."}
    within_column_reorder: {rating: assembled, note: "Same path; only extra is the from_ix<ix shift after removal. Verified interactively."}
    drop_indicator: {rating: assembled, note: "Card wrappers compare cursor to own midline -> (col,index); 3px accent line, dashed highlight for empty columns (~20 LoC)."}
    drag_ghost: {rating: built-in, note: "on_drag's closure returns a Render entity gpui paints under the cursor each frame, anchored at grab offset; never occludes drop targets."}
    inline_edit: {rating: hand-rolled, note: "Dbl-click free (ClickEvent::click_count()==2); the editor itself is iteration-1's raw on_key_down input (no IME/selection/clipboard) — gpui ships no text-input widget (sanctioned path ~750 lines)."}
    add_delete: {rating: hand-rolled, note: "Mutation trivial but rating follows the hand-rolled text input it depends on."}
    animation: {rating: assembled, note: "No layout/FLIP — cards snap; fallback: 260 ms opacity settle keyed by drop epoch (~8 LoC); gap documented."}
    column_scroll: {rating: built-in, note: ".overflow_y_scroll() per column; DnD works inside scrolled viewport with zero offset math (drag bounds are window-coords)."}
```

SURPRISES:
- The interrupted agent's work needed zero code changes — inline edit and drop animation were already complete on disk; remaining work was audit + launch verification + FRICTION.md files.
- A "no-widgets" framework has the BEST native DnD of the study: typed payloads, framework-painted ghost entities, per-listener bounds on every drag move — SPEC-3's headline feature took ~70 LoC while a plain slider took ~70 LoC of hand-rolled mechanics.
- gpui has no element-bounds query API: geometry-aware interaction outside drags (chart hover, slider) needs a paint-time Rc<Cell<Bounds>> "canvas probe"; on_drag_move is the only bounds-carrying event.
- Two-layer drop targeting leans on capture-phase on_drag_move dispatching parent-before-child — load-bearing ordering behavior not in the docs.

TIME_SINK:
- Manual chart math (gpui provides a path rasterizer and nothing above it).
- Hand-rolled text input (again): board's dbl-click-to-edit inherits the full editor cost; IME/selection/clipboard gaps documented.
- Bounds plumbing and Pixels ergonomics (private inner f32 in 0.2.2 forces f32::from(Pixels) helpers throughout).

## egui

```yaml
framework: egui
dash:
  build_ok: true
  launch_ok: true
  loc: 374          # app code (+57 kittest tests)
  helper_crates: [egui_plot =0.36.0, egui_dnd =0.16.0]  # version OFFSET from egui: these track egui 0.35 (egui_plot 0.35.0 targets egui 0.34!)
  repaint_strategy: "Reactive: each frame runs due ticks (wall-clock deadline math) then schedules exactly one wakeup via ctx.request_repaint_after(time-to-next-tick); paused = repaint only on input; animations self-request."
  ratings:
    dnd_reorder: {rating: assembled, note: "egui_dnd show_vec_sized in horizontal_wrapped = grid reorder with animated swaps + floating card in ~10 LoC glue; reorders Vec<usize> of stable identities. Core egui alone would be hand-rolled."}
    live_data: {rating: assembled, note: "No timer API; idiom is deadline math in the frame callback + request_repaint_after (~15 LoC). Frame rate ≈ tick rate, not display rate."}
    charts: {rating: assembled, note: "egui_plot for sparklines + main chart; scrolling window free via auto-bounds."}
    hover_tooltip: {rating: built-in, note: "Plot::show_crosshair(true) is one line; HoverPosition::NearDataPoint even carries the sample index."}
    slider: {rating: built-in, note: "egui::Slider one line."}
    click_select: {rating: built-in, note: "egui_dnd Handle::sense(click) makes card drag handle AND click target."}
    animation: {rating: assembled, note: "egui_dnd swap tweens + ctx.animate_bool hover shadow. egui offers id-keyed tween helpers only (no springs/keyframes); under-widget shadow needs the Shape::Noop placeholder + painter.set two-pass trick."}
board:
  build_ok: true
  launch_ok: true
  loc: 438          # (+91 kittest tests, +78 hit-test experiments)
  helper_crates: []  # deliberate: egui_dnd rejected (single-list only, no cross-container story); built on core egui::DragAndDrop
  ratings:
    cross_column_dnd: {rating: hand-rolled, note: "Core egui provides a typed payload store (auto-cleared on release/Esc) + a drag-source helper; hovered-column detection, insertion index from card rects, list surgery are yours (~60 LoC)."}
    within_column_reorder: {rating: hand-rolled, note: "Same path; decrement insert index when dropping below own old position (unit-tested)."}
    drop_indicator: {rating: hand-rolled, note: "Pointer.y vs card rects → index; accent hline painted as overlay into the gap — avoids immediate-mode chicken-and-egg."}
    drag_ghost: {rating: assembled, note: "Tooltip-layer render + transform_layer_shapes is built in, BUT stock dnd_drag_source is unusable for cards with buttons; forked ~25 LoC with UiBuilder::sense container pattern + is_decidedly_dragging gate."}
    inline_edit: {rating: assembled, note: "Label::sense(click) double_clicked() swaps in TextEdit; Enter/Esc; focus needs just-opened flag + request_focus."}
    add_delete: {rating: assembled, note: "All mutations deferred to end-of-frame to sidestep the borrow checker in per-column closures."}
    animation: {rating: assembled, note: "Drop-flash via animate_value_with_time; no way to animate layout position changes — cards teleport to their new slot (documented approximation)."}
    column_scroll: {rating: built-in, note: "ScrollArea::vertical().id_salt(col). Zero friction."}
```

SURPRISES:
- egui 0.35's hit test deliberately refuses to click through a topmost drag-only widget, and Ui::dnd_drag_source registers drag sense on top of children — every button inside a stock drag source is silently inert, and a drag-only widget "drags" from pointer PRESS (card teleports during a plain click). Caught only by a failing kittest test; fix = drag sense on the container (UiBuilder::sense) + is_decidedly_dragging. The single biggest egui DnD finding.
- Helper-crate versions are offset from egui's: egui_plot 0.36.0 and egui_dnd 0.16.0 are the egui-0.35-compatible releases — every pin verified against crates.io dep metadata.
- Chart hover interactions nearly free: show_crosshair(true) one line.
- egui 0.35 renamed TopBottomPanel/SidePanel into unified egui::Panel (on top of 0.34's App::update→App::ui) — pre-2026 examples miscompile in two independent ways.

TIME_SINK:
- ~40% of board time diagnosing the click-swallowing DnD trap, ending in reading egui's hit_test.rs/interaction.rs source.
- Version-matched API verification for the 0.35/0.36 generation — by reading crate sources in the cargo registry rather than docs.
- kittest ergonomics for a live-ticking app: Harness::run panics when the app never stops requesting repaints; dash tests switched to explicit step()s.

## xilem

```yaml
framework: xilem
dash:
  build_ok: true
  launch_ok: true   # ~3.5-9% CPU at 10 Hz
  loc: 1233         # main.rs 294 + widgets.rs 939
  helper_crates: []
  repaint_strategy: tick task -> app_logic rebuild + diff -> changed widgets request_paint_only -> vello full-scene GPU re-render; chart hover widget-local (no rebuild); on_anim_frame chains self-terminate at rest
  ratings:
    dnd_reorder: {rating: hand-rolled, note: "Custom masonry Widget (CardFrame) with pointer capture + threshold + window-coord slot math; cursor-follow via transformed().translate(). Render transform doesn't change paint order — ghost can pass under later siblings."}
    live_data: {rating: built-in, note: "task_raw view + bundled tokio (sleep loop + MessageProxy). Quirks: task closures aren't rebuilt, so mutable Hz travels via Arc<AtomicU64>; pause = removing the task from the tree via fork + bool.then()."}
    charts: {rating: hand-rolled, note: "No chart widget or ecosystem crate; custom widgets painting vello BezPaths. Text inside a custom widget means re-doing Label's parley ranged_builder/render_text dance by hand."}
    hover_tooltip: {rating: hand-rolled, note: "Widget-local hover + request_paint_only; tooltip text measured via a transparent parley dry-run. No tooltip/overlay primitive exists. Verified on screenshot."}
    slider: {rating: built-in, note: "slider() works incl. keyboard. Gotcha: masonry Slider clamps its own width to 100..200px and ignores flex — no API to widen."}
    click_select: {rating: hand-rolled, note: "No generic tap view for containers; button(child) adds chrome and its capture conflicts with dragging the same surface. Selection is the Clicked event of the hand-rolled CardFrame."}
    animation: {rating: hand-rolled, note: "Only primitive is per-widget on_anim_frame chains; no tween/spring/transition API at any level. Shipped eased hover-elevation; zero idle cost."}
board:
  build_ok: true
  launch_ok: true   # ~0% CPU idle
  loc: 1160         # main.rs 412 + widgets.rs 748
  helper_crates: []
  ratings:
    cross_column_dnd: {rating: hand-rolled, note: "CardFrame pointer capture + a compose-pass geometry registry (every card/column writes its window Rect into a shared HashMap, scroll-correct) + app-state hit-testing. Verified end-to-end with synthetic drags (CGEvent)."}
    within_column_reorder: {rating: hand-rolled, note: "Index = count of non-dragged cards whose registered center-y is above the pointer. Verified by scripted drag."}
    drop_indicator: {rating: assembled, note: "4px accent sized_box spliced into the column list at the computed index each rebuild; trivial once the (hand-rolled) index exists."}
    drag_ghost: {rating: assembled, note: "zstack top layer moved with transformed().translate(); source card dimmed via post_paint overlay."}
    inline_edit: {rating: hand-rolled, note: "Double-click via PointerState.count in a custom widget; Enter-commit built-in (on_enter). Esc-cancel has NO API — rescued by catching the unhandled Escape bubbling to the ancestor. Autofocus has NO API — hand-rolled AutoFocus widget grabs the inner TextArea's WidgetId and set_focus()es it; caret lands at position 0 (no caret/select-all API)."}
    add_delete: {rating: assembled, note: "Delete ✕ had to be a mini CardFrame, NOT a stock Button: masonry pointer capture is last-wins and Button doesn't mark Down handled — a stock button inside a draggable card loses its capture and never fires."}
    animation: {rating: hand-rolled, note: "Documented approximation: dropped card plays an on_anim_frame fade flash. Real position tweening would need hand-rolled FLIP on top of the geometry registry."}
    column_scroll: {rating: built-in, note: "portal(flex_col) per column. Caveat: Portal passes viewport max down as child constraints (known TODO) — initially produced viewport-height cards; no drag-edge auto-scroll."}
```

SURPRISES:
- xilem 0.4.0 ships more than expected — slider, task/task_raw, zstack, transformed, grid all exist and the timer/async story is genuinely good; but the cliff beyond the ~20 stock views is vertical: every gap (chart, tooltip, DnD, animation, autofocus, Esc) means a full masonry Widget impl + ~120-180 LoC of xilem View boilerplate.
- The raw materials one layer down are surprisingly adequate: events bubble to ancestors, PointerState.count gives double-click free, EventCtx::set_focus + public NewWidget.widget made the autofocus hack possible, compose-pass window_origin() enabled scroll-correct drop hit-testing.
- Masonry pointer capture is last-wins and stock widgets don't set "handled" on press → any hand-rolled interactive container silently breaks stock Buttons nested inside it.
- masonry Slider clamps its width to 100-200px ignoring flex; synthetic-input verification (CGEvent) worked on macOS where AppleScript keystrokes were denied — true end-to-end DnD/edit verification.

TIME_SINK:
- Layout-constraint semantics: Flex hands non-flex children its full loosened max in both axes and Portal passes viewport max down — "fill if bounded" sizing exploded cards to viewport height; flexed label doesn't push siblings (needs FlexSpacer::Flex).
- API archaeology: no hosted docs match the 0.4.0 release (git main differs); everything reverse-engineered from vendored sources and bundled examples.
- The slider goose chase: hours of "clicks do nothing" debugging that ended at the undocumented 200px width clamp — clicks were landing right of the widget.

## freya

(Added 2026-08-03 with the three-framework expansion cohort. Evidence labels
follow the newer files: observed / self-test / synthetic-input / source-only /
unexercised. Canonical build numbers are pending the next measurement pass.)

```yaml
framework: freya
version: "=0.4.0"
cohort: 2026-08-03-expansion
canonical_measurement: pending_new_cohort
dash:
  build_ok: true    # cargo build --release clean, --locked --release reproduces
  launch_ok: true   # alive >12 s; every interaction driven with synthetic CGEventPost mouse events and read back from window-scoped screenshots
  loc: 624          # single src/main.rs, heavily commented
  helper_crates: ["async-io 2.6.0", "freya feature `engine` (names the Skia types canvas() callbacks require)"]  # no rand (10-line xorshift*), no DnD crate, no plotting crate
  repaint_strategy: >-
    Signal-driven: Freya repaints when a signal a component read is mutated.
    The tick loop writes `metrics` at the configured rate, so at 10 Hz the whole
    tree is re-rendered and — per the `// TODO: Use incremental rendering` still
    in render_pipeline.rs — fully re-painted 10×/s. Hover crosshair writes the
    `cursor` signal and re-renders MainChart only; use_animation runs its own
    async-io task for ~160 ms per hover. Self-observed at 10 Hz:
    4.2–5.2% CPU, RSS ≈ 99.6 MiB.
  ratings:
    dnd_reorder: {rating: built-in, evidence: synthetic-input, note: "DragZone::new(payload, child).drag_element(ghost) + DropZone::new(child, on_drop) ship in freya::components. Payloads are TYPED (use_drag::<T>() keys a root-scope context by T), the 4 px threshold is configurable, and the drop handler just gets the payload back. Whole reorder feature ≈30 LoC including the Vec permutation. Verified: dragging the CPU card onto the Memory slot swapped them on screen."}
    live_data: {rating: assembled, evidence: observed, note: "spawn(async move { loop { … } }) is first-party and updates signals directly from the task (single-threaded reactivity, no channel plumbing). But Freya ships NO timer/interval primitive — the prelude exports nothing time-related — so the loop depends on async-io's Timer::after, which is exactly what freya-animation uses internally and is already in the tree. Rate changes are read with .peek() inside the loop, so re-rating needs no re-subscription."}
    charts: {rating: hand-rolled, evidence: observed, note: "canvas() hands you the raw skia_safe::Canvas, so scaling, gridlines, area fill and polyline are ~110 LoC of drawing. A first-party charting path EXISTS (the `plot` feature re-exports plotters plus freya-plotters-backend for this same canvas) and was deliberately not used, to measure the drawing story. skia-safe 0.98 moved the path API to PathBuilder + .snapshot()/.detach(); the compile error names Handle<SkPath> and offers no hint."}
    hover_tooltip: {rating: assembled, evidence: observed, note: "canvas().on_pointer_move(..) gives element_location already in element-local LOGICAL px — no hit-testing, no scroll-offset math. Crosshair and snapped marker are drawn in Skia, but the TOOLTIP IS A REAL ELEMENT (rect + two labels, Position::new_absolute(), Layer::Overlay, interactive(false)) because CanvasContext exposes a FontCollection but no text measurement. Screenshot shows `#131 / 57.05 %`."}
    slider: {rating: built-in, evidence: synthetic-input, note: "Slider::new(on_moved).value(percent) — one line. Caveat: hard-wired to a 0..100 PERCENTAGE, so a 1–60 Hz control needs its own two mapping expressions, and there are no steps/ticks."}
    click_select: {rating: built-in, evidence: synthetic-input, note: ".on_press(handler) is the high-level 'activate' event: it fires for left-click, touch AND keyboard activation on a focused element, so a11y comes free. Selection highlight is a Border swap."}
    animation: {rating: built-in, evidence: observed, note: "use_animation(|conf| { conf.on_change(OnChange::Rerun); AnimNum::new(0., target).time(160).ease(Ease::Out).function(Function::Quad) }) from freya::animation. It owns its own clock (an async-io task), re-runs when a signal read inside the closure changes, and you just read .get().value() — no frame subscription, no Instant threading. There is NO layout/FLIP animation, so the grid reflow on drop snaps (documented gap)."}
  sharpest_edge: "CANVASES THAT NEVER REPAINT. RenderCallback's PartialEq returns true unconditionally and CanvasElement::changed() is self != other, so a canvas() whose only changing input is data captured by its closure diffs as UNCHANGED: the old element stays in the tree and the pipeline keeps invoking the FIRST closure forever. Observed directly — six sparklines frozen at their seed values while the main chart animated, because the main chart's canvas also carried on_pointer_move/on_sized handlers and Callback::eq returns false unconditionally. Workaround: attach any handler (`canvas(on_render).on_wheel(|_| {})`). Nothing in the docs mentions this and there is no Canvas equivalent of iced's canvas::Cache::clear()."
board:
  build_ok: true
  launch_ok: true   # every capability exercised with synthetic CGEvent mouse events and System Events keystrokes, checked against window-scoped screenshots
  loc: 462          # single src/main.rs
  helper_crates: []  # freya "=0.4.0" default features covers DnD, scrolling, inputs, buttons and animation; freya::animation is compiled in unconditionally
  ratings:
    cross_column_dnd: {rating: built-in, evidence: synthetic-input, note: "DropZone::new(DragZone::new(card_id, body).drag_element(ghost), on_drop). The payload is typed (u64) and comes straight back in the handler; the whole reorder engine is ~35 LoC of Vec surgery. Observed: a card dragged from Todo into Doing, counts 3/2/1 → 2/3/1."}
    within_column_reorder: {rating: built-in, evidence: synthetic-input, note: "Same mechanism — every card is wrapped in a DropZone meaning 'insert before me', with an index correction when the source was earlier in the same column. Observed: card dragged from Doing index 2 to index 0."}
    drop_indicator: {rating: assembled, evidence: synthetic-input, note: "DropZone::on_drag_over(bool) fires on enter/leave ONLY while a drag of that payload type is in flight — exactly the signal you want. It drives a drop_slot signal and a 4→6 px pill between cards (~20 LoC)."}
    drag_ghost: {rating: built-in, evidence: synthetic-input, note: "DragZone::drag_element(..) renders any element at the cursor, offset by the grab point, on Layer::Overlay with interactive(false) — which correctly propagates to children, so the ghost never eats the drop. show_while_dragging(true) keeps the original in place. Zero code beyond describing the ghost."}
    inline_edit: {rating: hand-rolled, evidence: synthetic-input, note: "Freya 0.4 has NO double-click event — no on_double_click, no click count in MouseEventData — so it is emulated by remembering (card_id, Instant) of the last press against a 400 ms window. Enter is Input::on_submit; Escape needs Input::on_pre_key_down, which REPLACES the widget's stock key filter, so the app must re-implement the default arms (Enter/Shift pass, Tab skip, everything else stop_propagation + prevent_default) around its own Escape case."}
    add_delete: {rating: built-in, evidence: synthetic-input, note: "Button + a per-column adding: Option<usize> signal that swaps the button for an auto_focus(true) Input; delete is .on_press on a 20×20 rect carrying a11y_role(Button) + a11y_alt."}
    animation: {rating: built-in, evidence: observed, note: "use_animation_with_dependencies(&(column, index), ..) with OnChange::Rerun replays an AnimNum scale-in (0.94→1.0, 180 ms, Ease::Out/Function::Back) whenever a card's position changes — i.e. exactly on drop. No frame scheduling, no Instant threading. No layout/FLIP animation, so neighbours snap into their new slots instead of sliding; the moved card is the only thing that animates (documented gap)."}
    column_scroll: {rating: built-in, evidence: synthetic-input, note: "One ScrollView per column."}
  traps:
    - "`Ref` held across a write() ABORTS the app behind a modal dialog. State::peek()/read() return a Ref; writing to the same signal while one is alive panics, and a UI-thread panic surfaces as a blocking CFUserNotificationDisplayAlert 'Fatal Error' panel that freezes the app (window keeps its last frame, every later event queues behind the modal). The trap is `if let Some(x) = *state.peek() { state.write() … }` — the scrutinee temporary is still alive inside the body."
    - "DropZone cannot cover a container's slack space: it renders rect().width(auto).height(auto) around its child and exposes no size setters, so 'drop anywhere in the empty part of this column' cannot be expressed by sizing a zone (this app uses an explicit 140 px tail zone). The obvious alternative — use_drag::<T>() + on_mouse_up on the column rect — did NOT work: an on_mouse_up listener on an ancestor never fired for clicks in the empty area while on_pointer_down on the same rect fired every time."
    - "Size::flex silently no-ops: a child sized Size::flex(1.) collapses unless the PARENT opts into .content(Content::flex()). No warning; the first build rendered a single full-width column."
```

SURPRISES:
- Drag-and-drop is a first-party TYPED component pair (DragZone/DropZone) with a working ghost and an on_drag_over hook — the least code of any framework in this study for both a sortable dashboard and a cross-container kanban (~30-35 LoC each).
- use_animation needs no frame scheduling from the app at all, and use_animation_with_dependencies makes "animate when this thing moved" a two-line declaration.
- on_press unifies mouse/touch/keyboard activation and every element has AccessKit fields (a11y_role, a11y_alt) inline, so a11y is not a separate data model.
- Against that: canvases silently never repaint unless they carry an event handler, Slider is percentage-only, Input is single-line, there is no double-click event, and a trivial borrow mistake becomes a frozen app behind a system alert.
- Debug builds silently inject freya-performance-plugin's FPS overlay (cfg(debug_assertions) inside freya::prelude::launch), which is confusing the first time you see it.

TIME_SINK:
- Canvas staleness: diagnosing why sparklines froze while the main chart updated (the difference was the presence of event handlers).
- Skia API archaeology (PathBuilder vs Path, Color→SkColor, ctx.size already divided by the scale factor).
- The empty-column drop target — three attempts, including the Ref-across-write panic that hung the app behind a modal alert — plus double-click emulation and re-implementing Input's default key filter to get Escape.

## vizia

(Added 2026-08-03 with the three-framework expansion cohort.)

```yaml
framework: vizia
version: "=0.4.0"
cohort: 2026-08-03-expansion
canonical_measurement: pending_new_cohort
dash:
  build_ok: true    # cargo build --release and --locked --release clean, no warnings
  launch_ok: true   # alive well past the 10 s bar; interactions driven with CGEvent clicks/drags scoped to this app's window and verified from window-scoped screencapture -l shots
  loc: 743          # single src/main.rs, heavily commented; NO verification hooks compiled in (evidence came from external CGEvent synthesis + screenshots). Rough split: ~110 Skia drawing, ~90 CSS, ~150 model/event, ~250 view construction
  helper_crates: []  # vizia "=0.4.0" default features only; timers, DnD, custom Skia drawing, CSS transitions and the slider are all core; PRNG is a 10-line xorshift*
  release_binary_mib: 21.9   # Skia statically linked
  repaint_strategy: >-
    Fine-grained signal invalidation: every tick writes 6 Signal<VecDeque<f32>>
    buffers, invalidating 6 sparkline views plus the main chart. vizia repaints
    only the dirty views but still runs a full style/layout pass per event
    cycle. Measured 10 × 1 s ps samples at 10 Hz: 6.03% average CPU
    (range 3.5–6.0%), RSS 106–107 MiB steady.
  ratings:
    dnd_reorder: {rating: built-in, evidence: synthetic-input, note: "The standout finding: vizia core has real drag-and-drop. .on_drag(|ex| ex.set_drop_data(ex.current())) marks the view DRAGGABLE and fires when the pointer leaves a PRESSED card; .on_drop(|ex, data| ..) fires on release over any view; ex.has_drop_data() is available inside on_over for the slot highlight. Total DnD mechanics: ~14 lines of modifiers plus the model's order.update(remove/insert). Proven: a synthetic drag of the Memory card from slot 1 to slot 3 reflowed the grid. No helper crate, no hit-testing, no cursor-tracking subscription."}
    live_data: {rating: built-in, evidence: observed, note: "cx.add_timer(Duration, None, |cx, action| ..) + cx.start_timer/stop_timer is core API with NO feature flag and no executor choice (contrast iced 0.14, where time::every silently doesn't exist without the smol/tokio feature). Rate changes are in-place: cx.modify_timer(t, |s| s.set_interval(d)) — the timer is not torn down and rebuilt. TimerAction::{Start,Stop,Tick(delta)} even gives the elapsed delta."}
    charts: {rating: assembled, evidence: observed, note: "No plotting crate in the vizia ecosystem, but also no canvas LAYER to set up: vizia renders with Skia and vizia::vg re-exports skia-safe, so `impl View { fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) }` hands you the same skia_safe::Canvas the framework draws with, already transformed into the view's coordinate space. ~100 LoC for both chart types. Redraw is per-view and declarative — .bind(metric.samples, |mut h| h.needs_redraw()) — no cache-invalidation bookkeeping. Trap: skia-safe 0.93's Path is immutable, so geometry goes through vg::PathBuilder + snapshot()/detach(); vg::Path::new() compiles and then has no move_to."}
    hover_tooltip: {rating: assembled, evidence: synthetic-input, note: ".on_mouse_move(..) on the chart view plus .on_hover_out(..) to clear; index snapping is 3 lines; crosshair and marker are drawn in the same Skia draw(). The TOOLTIP IS A REAL VStack of Labels absolutely positioned over the canvas via left(Pixels(..))/top(Pixels(..)) bound to a signal, so it gets the framework's own text shaping — no measure_text, which is exactly what forces character-count estimates in canvas-only frameworks. Verified: tooltip reads '59.5 / sample 159' with crosshair and dot snapped to the line."}
    slider: {rating: built-in, evidence: synthetic-input, note: "Slider::new(cx, signal).on_change(..). One catch: vizia's Slider is ALWAYS normalised 0.0–1.0, so a 1–60 Hz range needs mapping in both directions. Verified: dragging the handle set the label to 45 Hz and the timer interval followed."}
    click_select: {rating: built-in, evidence: synthetic-input, note: ".on_press(..) + .toggle_class(\"selected\", signal.map(..)) and one CSS rule."}
    animation: {rating: built-in, evidence: observed, note: "vizia has a CSS ANIMATION SYSTEM: `transition: background-color 180ms, border-color 180ms, scale 180ms, shadow 180ms;` on .card, with :hover, .selected and .drop-target variants. No Instant threading, no per-frame subscription, no animation crate — the framework interpolates the style properties itself. @keyframes + cx.play_animation_for(..) exists for imperative one-shots (unused here). The gap, shared with the whole cohort: no layout/FLIP animation, so the grid reflow after a drop snaps."}
board:
  build_ok: true
  launch_ok: true   # every one of SPEC-3's requirements exercised with synthetic input; RSS after the full interaction sequence 99.4 MiB
  loc: 499          # single src/main.rs; ~150 CSS, ~150 model/events, ~180 view construction. No verification hooks compiled in, so production LoC == total LoC
  helper_crates: []
  ratings:
    cross_column_dnd: {rating: built-in, evidence: synthetic-input, note: "Three modifiers do the whole thing: .on_drag (marks DRAGGABLE, fires when the pointer leaves the pressed card), .on_over gated on ex.has_drop_data(), .on_drop. Proven: 'Draft the RFC' dragged from Todo into Doing, header counts 3/2/1 → 2/3/1."}
    within_column_reorder: {rating: built-in, evidence: synthetic-input, note: "Exactly the same handler; the model adjusts the target index by one when the source precedes the destination in the same column. Proven: 'Review PR #412' from index 2 to index 0 inside Doing."}
    drop_indicator: {rating: assembled, evidence: synthetic-input, note: "A zero-height Element between every pair of cards with .toggle_class(\"active\", drop_at.map(..)), plus a .column-active class on the whole column. The insertion POSITION is free because the drop target is the card you are over — no geometry maths."}
    drag_ghost: {rating: hand-rolled, evidence: synthetic-input, note: "The one genuinely manual part: vizia arms and routes the drag but renders NOTHING, so the ghost is an absolutely positioned Label whose left/top are bound to a cursor signal fed by the root's .on_mouse_move. ~12 lines including the physical→logical scale conversion."}
    inline_edit: {rating: built-in, evidence: synthetic-input, note: ".on_double_click(..) is a core action modifier; the card's Label is swapped for a Textbox by a Binding on the editing signal, and Textbox gives Enter and Esc directly: on_submit(|cx, text, enter|) where enter == true means the Enter key, and on_cancel is wired to Escape inside the widget. .on_build(|cx| { cx.focus(); cx.emit(TextEvent::StartEdit); }) puts the caret in it and selects the existing text."}
    add_delete: {rating: built-in, evidence: synthetic-input, note: "'+ Add card' swaps to an inline Textbox under the same Binding pattern; empty/whitespace rejected in the model. Proven: added a card to Done (1 → 2); Esc while typing left the count unchanged; the ✕ button removed a card (Todo 3 → 2)."}
    animation: {rating: built-in, evidence: observed, note: "CSS: `.drop-line { height: 0px; transition: height 140ms; }` / `.drop-line.active { height: 6px; }` — the framework interpolates a LAYOUT property, so the surrounding cards genuinely slide apart and back. Most retained/immediate-mode toolkits in this cohort animate paint only. The shared gap remains: no FLIP/layout-position animation, so a card that lands in a new column appears there instantly."}
    column_scroll: {rating: built-in, evidence: synthetic-input, note: "One stock ScrollView per column. Proven: after adding 10 filler cards, wheel scrolling over Todo moved only that column."}
  traps:
    - "on_press / on_drag / on_double_click need hoverable(false) children. vizia only runs an action when the acted-on view is the HOVERED entity (cx.current == meta.target), so a click landing on a card's Label or sparkline never reaches the card. Marking non-interactive children .hoverable(false) is the fix and is what vizia's own examples do. Silent failure: no warning, no compile error."
    - "on_drop ORDERING: on_drop runs while WindowEvent::MouseUp is still propagating to the root model, and it only QUEUES the app event. A root MouseUp handler that cleared drag state inline made every drop a silent no-op. Fix: queue a DragEnd event so the order stays Drop → DragEnd."
    - "Event coordinates are PHYSICAL pixels; layout units (Pixels(..)) are LOGICAL. On a 2× display the tooltip/ghost rendered at twice the cursor offset until divided by cx.scale_factor()."
```

SURPRISES:
- vizia is the only framework in this study where none of SPEC-2's three "hard" capabilities (DnD, live timer, animation) needed a helper crate or hand-rolled mechanics — roughly 30 lines between them.
- `transition: height` on a layout property actually animates LAYOUT, so the kanban's insertion gap really opens and closes; most of the cohort animates paint only.
- The same on_drag/on_over/on_drop core API also receives Finder file drops as DropData::File(PathBuf) — one drop API for both in-app and OS drags (used in apps/vizia-tray).
- Textbox::on_submit's second argument distinguishes Enter from focus loss and on_cancel is Escape, so SPEC-3's "Enter commits, Esc cancels" is two closures.
- Against that: 6% CPU at 10 Hz is high for what is redrawn (a full style/layout pass per event cycle), and the framework is unusually silent about mistakes — press handlers on containers, event coordinates used as layout coordinates, and drop ordering all compile and do nothing visible.

TIME_SINK:
- The three silent-failure traps (~60% of board time). Each one compiles, runs, and does nothing.
- Skia API archaeology: vizia pins skia-safe 0.93 whose Path is immutable, and the vizia examples only use vg::Path::rect(..), so the builder pattern had to be found in skia-safe's source.
- Coordinate spaces and the hover/target rule in dash — all three had to be found by printing WindowEvents from the model.

## floem

(Added 2026-08-03 with the three-framework expansion cohort. Version pin is the
git rev `778bb5f2` of lapce/floem main — see the version_deviation note in
iter4-rows.md and apps/floem-app/GAPS.md.)

```yaml
framework: floem
version: "git-778bb5f2"   # rev 778bb5f2aa08429e579ee2e6ac97e84fbf18b618 (2026-06-21); crates.io 0.2.0 is 20 months stale and `main` is unpublishable (forked floem-winit + understory_* git deps)
cohort: 2026-08-03-expansion
canonical_measurement: pending_new_cohort
dash:
  build_ok: true    # release build clean, locked rebuild too
  launch_ok: true   # alive after 10 s, killed cleanly
  loc: 421          # single src/main.rs
  helper_crates: []  # DnD, animation and slider are built-ins; charts are raw canvas; rand avoided with a 10-line xorshift*
  repaint_strategy: >-
    floem repaints only views whose TRACKED SIGNALS changed. Each tick writes 6
    value signals + 6 sample signals → 6 sparkline canvases + 1 chart canvas +
    7 labels repaint; everything else is untouched. Crosshair movement writes
    only the hover signal → only the chart canvas repaints. Hover/drag
    animations are driven internally by floem's transition system; no app-side
    frame scheduling exists anywhere in the file. Measured: 2.7% average CPU at
    10 Hz over a 12 s ps sample (the iced port measured 3.5% over 30 s).
  ratings:
    dnd_reorder: {rating: built-in, evidence: observed, note: "floem's standout cell. .draggable_with_config() on the card gives a drag with threshold, an AUTOMATIC GHOST (floem re-paints the dragged view at the cursor; dragging_style styles it), DragTargetEnter events on the other cards carrying the source's custom_data for live grid reflow (the reflowed grid IS the slot indicator), and a release animated with a configurable spring easing. ~25 LoC total, no manual hit-testing, no global pointer subscription — the iced port hand-rolled ~90 LoC for the same interaction."}
    live_data: {rating: assembled, evidence: observed, note: "floem has NO interval/subscription primitive — only one-shot exec_after(Duration, cb). The upstream `timer` example's own pattern is used: an Effect + exec_after + trigger-signal chain that re-arms itself (~15 LoC), re-reading the rate each round so slider changes just work. Slightly galling that the framework's own example must hand-build an interval."}
    charts: {rating: hand-rolled, evidence: observed, note: "No chart widget or ecosystem helper at this rev (crates.io chart helpers target floem 0.2). The canvas view + kurbo BezPath through the Renderer trait: scaling, gridlines, axis labels and polylines all manual (~110 LoC). BUT the paint closure is SIGNAL-TRACKED — reading samples.get() inside it makes repaint scheduling fully automatic, with no cache/damage management at all (iced needed 7 explicit canvas::Caches)."}
    hover_tooltip: {rating: hand-rolled, evidence: observed, note: "Pointer position lands in an RwSignal<Option<Point>> via on_event_cont(listener::PointerMove/PointerLeave); positions are already view-local. The paint closure reads the signal, so the crosshair repaints reactively. Genuine win over iced: TextLayout::new_with_text(..).size() gives REAL text measurement for the tooltip box (iced estimated width from char count). Snapping and edge-flipping are still manual."}
    slider: {rating: built-in, evidence: observed, note: "Slider::new_ranged(value_fn, 1.0..=60.0).step(1.0) — and unlike freya/vizia it takes a real range. Asterisk: changes arrive as a typed CUSTOM EVENT (on_event_stop(SliderChanged::listener(), ..) reading changed.value), not a plain callback, and the doc example IN THE SOURCE (event.state.value) does not compile against the same rev — the extractor already unwraps to &SliderState."}
    click_select: {rating: built-in, evidence: observed, note: "on_event_stop(listener::Click, ..) plus a reactive border style via a `selected` signal. (on_click_stop exists but is deprecated at this rev.)"}
    animation: {rating: built-in, evidence: observed, note: "Two primitives used deliberately: style transitions (transition_background(Transition::linear(200.millis())) + hover() styles gives tweened hover elevation with zero scheduling code) and the drag-release spring (easing::Spring::snappy()) that animates the ghost into place. floem also has a full keyframe Animation API (.animation(|a| a.keyframe(..)) with springs/bezier easings), unused here. Animation is clearly a first-class subsystem, unlike iced's schedule-your-own-frames model."}
board:
  build_ok: true
  launch_ok: true   # alive after 10 s, killed cleanly, nothing on stdout/stderr
  loc: 353          # single src/main.rs
  helper_crates: []  # everything is stock floem
  ratings:
    cross_column_dnd: {rating: built-in, evidence: observed, note: "Cards are .draggable_with_config() with the card id as custom_data; any card or column tail registered for DragTargetEnter receives that id and calls one move_card helper. Cross-container works EXACTLY like within-container — floem's drag system has no notion of container boundaries to fight. ~30 LoC including the model helper."}
    within_column_reorder: {rating: built-in, evidence: observed, note: "Same DragTargetEnter handler; hovering a sibling card moves the dragged card into its position (live reflow). No index math beyond a position() lookup."}
    drop_indicator: {rating: assembled, evidence: observed, note: "Live reflow makes the board itself the preview; the dragged card's slot is styled (accent border + tinted background) via a dragging: RwSignal<Option<u64>> set in DragStart/DragEnd/DragCancel listeners. No dedicated insertion-line API, but ~10 LoC of styling on top of the built-in events suffices."}
    drag_ghost: {rating: built-in, evidence: observed, note: "floem re-paints the dragged view at the cursor automatically; dragging_style styles that ghost (shadow, accent border, translucency). ZERO mechanics code — compare the iced port's pin-in-stack plus global cursor subscription."}
    inline_edit: {rating: assembled, evidence: observed, note: "dyn_container swaps a card row for a TextInput when editing == Some(id); DoubleClick is a FIRST-CLASS TYPED LISTENER; Enter is the TextInputEnter custom event; Esc is a KeyDown check. ViewId::request_focus() focuses the fresh input (pattern lifted from the upstream todo-complex example)."}
    add_delete: {rating: built-in, evidence: observed, note: "Buttons + signal updates; the reveal-on-demand add input is the same dyn_container + request_focus pattern, with identical Enter/Esc handling."}
    animation: {rating: built-in, evidence: observed, note: "On release floem animates the ghost into place with a configurable easing (Spring::snappy() here) — genuinely pleasant and free. Asterisk: the OTHER cards snap when the list reflows mid-drag; there is no FLIP/layout animation for reflow (same gap as every other framework tested)."}
    column_scroll: {rating: built-in, evidence: observed, note: "Each column body is .scroll() with flex_grow(1). One layout papercut: the inner stack needs min_height_full() so the tail drop target fills short columns."}
  design_caveats:
    - "LIVE-REFLOW COMMIT MODEL: the card is moved in the model while dragging, so releasing anywhere 'commits' the last hovered slot, and Esc (which floem treats as drag-cancel) does NOT restore the original order. Matches the iced port's behaviour and is acceptable per spec, but a transactional model would need to defer the move until DragTargetDrop."
    - "Card text inside a keyed dyn_stack row must be re-derived reactively (Label::derived + signal lookup) or an inline edit won't refresh the row, since keyed diffing keeps the old view — the classic fine-grained-reactivity gotcha."
```

SURPRISES:
- Built-in drag-and-drop with an automatic ghost and a spring release is unique among the frameworks measured: the spec's hardest capability was the easiest cell in both apps, and cross-container DnD costs exactly the same as within-container.
- Signal-tracked canvas paint closures give reactive repaint for custom drawing with zero bookkeeping — the whole cache/invalidation layer iced needed simply does not exist here.
- Real text measurement (TextLayout::size()) is usable inside canvas code, which is what lets the tooltip box be sized correctly rather than estimated from character counts.
- A typed DoubleClick listener exists (several frameworks make you hand-time double clicks).
- Against that: no interval timer primitive (the framework's own example hand-rolls a re-arming exec_after chain), no transactional drop or FLIP reflow animation, and heavy doc/example drift on `main` — deprecated constructors everywhere the docs still use them, and in-source doc examples that don't compile against the same rev.

TIME_SINK:
- API archaeology against a moving `main`: this rev deprecates the documented API surface (empty() → Empty::new(), on_click_stop → typed Click listener) and the slider's change-event shape had to be read from the source.
- Chart drawing — the usual manual-plotting work, though signal-tracked canvases removed the entire cache/invalidation layer.
- Editing/adding state choreography (which signal owns the buffer, commit vs cancel, focusing the freshly created input) and getting scroll + tail-drop-target + min_height_full to cooperate in taffy flexbox. DnD itself took minutes, not hours.
