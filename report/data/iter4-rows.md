# Iteration-4 structured rows (Peek / Grid / Fetcher) — raw agent returns

Run dates: 2026-07-09..10. Evidence labels: observed / self-test /
synthetic-input / source-only / unexercised (see FIXES.md conventions).
The SPEC-8 `/flaky` cycle is process-global; canonical probes reset it and run
serially so another client cannot shift its phase. Historical shared-server
runs below retain their original server attempt numbers.

The canonical build rows below are generated from
`measurements/results-iter4.csv`:

<!-- BEGIN GENERATED: iter4-canonical-builds -->
| App | Clean | Incremental | Binary (stripped MiB) | Unique crate names | AccessKit in tree | LoC (Rust) | LoC (other UI) | Process survived 8 s |
|---|---:|---:|---:|---:|---|---:|---:|---|
| iced-grid | 25 s | 2 s | 8.6 | 140 | no | 700 | 0 | yes |
| egui-grid | 27 s | 1 s | 10.6 | 162 | yes | 616 | 0 | yes |
| gpui-grid | 56 s | 1 s | 4.4 | 397 | no | 655 | 0 | yes |
| tauri-grid | 36 s | 10 s | 6.4 | 204 | no | 259 | 553 | yes |
| xilem-grid | 26 s | 2 s | 9.9 | 143 | yes | 1022 | 0 | yes |
| slint-grid | 47 s | 5 s | 13.4 | 306 | yes | 519 | 237 | yes |
| dioxus-grid | 31 s | 1 s | 5.1 | 284 | no | 553 | 0 | yes |
| iced-fetch | 28 s | 2 s | 10.0 | 197 | no | 584 | 0 | yes |
| egui-fetch | 30 s | 1 s | 12.5 | 180 | yes | 650 | 0 | yes |
| gpui-fetch | 56 s | 2 s | 6.3 | 399 | no | 739 | 0 | yes |
| tauri-fetch | 42 s | 11 s | 11.3 | 255 | no | 53 | 456 | yes |
| xilem-fetch | 31 s | 3 s | 11.1 | 198 | yes | 616 | 0 | yes |
| slint-fetch | 47 s | 5 s | 14.5 | 332 | yes | 474 | 142 | yes |
| dioxus-fetch | 33 s | 1 s | 5.9 | 302 | no | 493 | 0 | yes |
| iced-peek | 54 s | 7 s | 15.9 | 253 | no | 854 | 0 | yes |
| egui-peek | 32 s | 2 s | 11.9 | 191 | yes | 1177 | 0 | yes |
| gpui-peek | 56 s | 2 s | 6.8 | 407 | no | 1369 | 0 | yes |
| tauri-peek | 37 s | 9 s | 6.9 | 233 | no | 273 | 566 | yes |
| xilem-peek | 32 s | 2 s | 11.0 | 178 | yes | 1071 | 0 | yes |
| slint-peek | 50 s | 3 s | 14.0 | 336 | yes | 635 | 163 | yes |
| dioxus-peek | 37 s | 1 s | 5.8 | 305 | no | 983 | 0 | yes |
| freya-grid | 30 s | 1 s | 18.5 | 192 | yes | 724 | 0 | yes |
| freya-fetch | 31 s | 1 s | 19.8 | 245 | yes | 654 | 0 | yes |
| freya-peek | 32 s | 1 s | 20.0 | 242 | yes | 585 | 0 | yes |
| vizia-grid | 17 s | 2 s | 19.9 | 128 | yes | 672 | 0 | yes |
| vizia-fetch | 19 s | 2 s | 21.1 | 187 | yes | 800 | 0 | yes |
| vizia-peek | 22 s | 1 s | 20.0 | 189 | yes | 808 | 0 | yes |
| floem-grid | 44 s | 2 s | 14.5 | 226 | no | 667 | 0 | yes |
| floem-fetch | 48 s | 2 s | 16.0 | 271 | no | 711 | 0 | yes |
| floem-peek | 59 s | 2 s | 16.8 | 313 | no | 947 | 0 | yes |
<!-- END GENERATED: iter4-canonical-builds -->

Evidence-scope note: only Dioxus Peek retains raw per-sample CPU CSVs. Other
Peek CPU/FPS figures below are per-agent summaries rather than a controlled
cross-framework dataset. TCC observations all came from unbundled cargo
binaries on one host with a shared terminal grant; responsible-process
attribution is the leading interpretation of those observations, not an
independent or universal macOS result.

## gpui — grid + fetch

```yaml
framework: gpui
grid:
  build_ok: true          # 145 s cold; known block v0.1.6 noise only
  launch_ok: true         # alive ≥10 s + 15 s self-test; window screenshots retained
  loc_production: 621
  loc_verification: 34    # (+ ~110 LoC external injection scripts, not shipped)
  helper_crates: []       # gpui =0.2.2 (runtime_shaders) only; PRNG + civil-date hand-rolled
  build_ms: 31            # retained grid-stdout.log; earlier unretained summary said 28-41
  filter_ms_1char: 1.68
  filter_ms_4char: 4.32   # retained log; SORT_MS Name 10.80/12.22 ms, Value 3.53 ms
  rss_after_load_mib: 97.3  # 91.1-97.9 MiB after full-range scroll (flat)
  ratings:
    table_widget: {rating: hand-rolled, evidence: observed, note: "No table/grid widget in gpui core (gpui-component not adopted, core-only rule). Header row of divs + uniform_list of row divs sharing a widths array."}
    virtualization: {rating: built-in, evidence: self-test, note: "uniform_list renders only the visible range; RSS flat (+0.6 MiB) across a 121-step scripted full scroll. Vertical-only."}
    sort: {rating: assembled, evidence: self-test, note: "Header on_click + sort_unstable_by over a Vec<u32> index; ▲/▼ indicator. 100k rows: 10.8–12.2 ms (string), 3.5 ms (f64) in the retained log."}
    filter_latency: {rating: hand-rolled, evidence: self-test, note: "Retained log: 1.68 ms @1-char, 4.32 ms @4-char (substring over precomputed-lowercase + index rebuild + re-sort) — no debounce needed. Earlier agent-return values came from an unretained run. Input is the minimal hand-rolled field (no IME/selection)."}
    column_resize: {rating: assembled, evidence: synthetic-input, note: "Real divider drag: typed on_drag with invisible ghost + on_drag_move (listener gets own bounds). CGEvent-verified 70→~150 px."}
    row_selection: {rating: assembled, evidence: synthetic-input, note: "ClickEvent::modifiers() native — plain/Shift-range/Cmd-toggle all real. Selection stores ids, survives re-sort/filter."}
    cell_custom_render: {rating: assembled, evidence: observed, note: "Status chip = rounded div; custom cells are the default state of the world in gpui."}
fetch:
  build_ok: true          # 205 s cold (adds reqwest/tokio); binary 7.49 MiB vs grid 5.13 MiB
  launch_ok: true
  loc_production: 686
  loc_verification: 53
  helper_crates: [reqwest (default-features off), tokio (rt+time), futures, serde, serde_json]
  ratings:
    async_integration: {rating: assembled, evidence: self-test, note: "Dedicated parked current-thread tokio runtime on a std thread; tokio JoinHandles awaited directly inside cx.spawn (cross-thread wakers, zero glue); ~30 LoC bridge."}
    http_client_choice: {rating: assembled, evidence: self-test, note: "gpui 0.2.2 re-exports HttpClient trait + Application::with_http_client but the only shipped impl is NullHttpClient ('No HttpClient available'); Zed's reqwest_client unpublished. Chose plain reqwest — same architecture Zed uses internally."}
    debounce_stale: {rating: assembled, evidence: self-test, note: "250 ms background-executor timer as first await; replacing Option<Task> drop-cancels. 5 keystrokes → exactly 1 SEARCH_SENT; the superseded request produced no post-delay server SEARCH line. Because the server logs only after sleeping, that absence does not prove no request arrived."}
    progress_streaming: {rating: assembled, evidence: observed, note: "bytes_stream → mpsc → gpui drain task → live bar; screenshot at 31%; DL_DONE 8388608 + server DOWNLOAD complete."}
    cancellation_real: {rating: assembled, evidence: self-test, note: "Cancel drops drain task → AbortOnDrop aborts tokio task → TCP close. Server log: 'ABORT /download after 24/64 chunks' — byte-exact match to client DL_CANCEL 3145728."}
    error_retry_ux: {rating: assembled, evidence: self-test, note: "Red error + Retry until success; matches server FLAKY lines. Manual retry per spec."}
```

SURPRISES:
- gpui's Application::with_http_client API is an attractive nuisance: the trait ships, the only backend is NullHttpClient that errors unconditionally — yet BYO reqwest reduces to "await the tokio JoinHandle inside cx.spawn" plus two Drop impls; cancellation composed from drop semantics on both executors and worked first run.
- In this implementation, 100k rows remained well within `uniform_list`'s
  observed range: the retained filter pass was 1.7–6 ms, sorts were roughly
  3.5–12.2 ms, and RSS stayed flat in the scripted run.
- Superseded searches were cancelled before the handler reached its post-sleep
  `SEARCH` log point. That is consistent with axum dropping the sleeping
  handler on disconnect, but does not prove the request never arrived.
- Occluded gpui windows stop painting — a screenshot can capture a minutes-stale frame; injection thereafter gated on a frontmost check.

TIME_SINK:
- Verification, not construction: safe CGEvent injection on a shared desktop (one misfire into a sibling xilem window), stale-framebuffer screenshot trap.
- Discovering from registry source that the default HTTP client bails — the API surface strongly implies a working client exists.
- Attributing evidence in the shared fetcher-server log (sibling ABORT/FLAKY lines interleave) — byte-exact chunk matching required.

## egui — grid + fetch

```yaml
framework: egui
grid:
  build_ok: true          # no warnings (eframe =0.35.0, egui_extras =0.35.0)
  launch_ok: true         # stdout kept in apps/egui-grid/verify-stdout.log
  loc_production: 391
  loc_verification: 225   # 154 kittest/unit tests + 71 selftest.rs (inert without GRID_SELFTEST=1)
  helper_crates: ["egui_extras =0.35.0 (table; NOT version-offset — lives in the egui repo)", "dev-only: egui_kittest =0.35.0 (wgpu), image =0.25.10"]
  build_ms: 18.35         # 17.9-22.2 across runs
  filter_ms_1char: 5.70
  filter_ms_4char: 3.59
  rss_after_load_mib: 106.6   # 112.6 right after scripted full-100k scroll, settles ~88
  ratings:
    table_widget: {rating: built-in, evidence: observed, note: "egui_extras::TableBuilder (first-party) — declarative columns/header/body, ~40 LoC; verified live + offscreen wgpu render. Core egui alone would be hand-rolled."}
    virtualization: {rating: built-in, evidence: synthetic-input, note: "TableBody::rows lays out only on-screen rows (fixed height, O(1) offset). Scripted jump-scroll across all 100k rows: no degradation; ~25 rows materialized."}
    sort: {rating: assembled, evidence: self-test, note: "Headers are plain cells — no sort support. ~30 LoC: clickable labels + ▲/▼ + sort_unstable_by on an index vector."}
    filter_latency: {rating: assembled, evidence: self-test, note: "Full 100k substring rescan per keystroke: 5.7 ms @1-char, 3.6 ms @4-char — under a frame; no incremental model needed."}
    column_resize: {rating: built-in, evidence: self-test, note: "resizable(true) gives drag-on-divider + dbl-click autosize FREE. Proven by kittest pointer-drag. Predicted weakest cell; was the cheapest."}
    row_selection: {rating: assembled, evidence: self-test, note: "sense(click) + TableRow::response()/set_selected built in; selection model is app code. TRAP: cell text stole clicks until style.interaction.selectable_labels=false."}
    cell_custom_render: {rating: built-in, evidence: observed, note: "Cells are closures over &mut Ui — status chip = Frame fill + corner_radius. Immediate mode's best case."}
fetch:
  build_ok: true          # (eframe =0.35.0, ehttp =0.7.1)
  launch_ok: true         # full scripted pass against live server; stdout in apps/egui-fetch/verify-stdout.log
  loc_production: 487
  loc_verification: 163   # incl. end-to-end kittest search vs live server
  helper_crates: ["ehttp =0.7.1 (json+streaming; egui-org client, ureq-3 backend)", "serde =1.0.228", "dev-only: egui_kittest =0.35.0"]
  ratings:
    async_integration: {rating: assembled, evidence: observed, note: "No runtime at all: ehttp spawns a thread per request; callbacks write Arc<Mutex> and call ctx.request_repaint() (Context is Send+Sync — designed for this). ~15 LoC of glue."}
    http_client_choice: {rating: assembled, evidence: observed, note: "ehttp over reqwest+poll_promise: callback model fits immediate mode, has streaming. Trade-off: plain fetches can't be aborted mid-flight. Trap: Request::get sets a default timeout — must clear for the 8 s stream."}
    debounce_stale: {rating: assembled, evidence: self-test, note: "250 ms deadline armed on changed(). Stale = AtomicU64 generation guard (ehttp affords no abort — the core egui finding). Proven out-of-order: SEARCH_APPLY gen=3 then SEARCH_STALE_DROP gen=2."}
    progress_streaming: {rating: built-in, evidence: self-test, note: "ehttp::streaming::fetch delivers Response then per-chunk Parts; ProgressBar tracks the 8 MiB/8 s stream at ~8 Hz. No reader thread."}
    cancellation_real: {rating: built-in, evidence: observed, note: "Cancel → next chunk callback returns ControlFlow::Break, dropping the connection. Server proof: 'ABORT /download after 20/64 chunks'."}
    error_retry_ux: {rating: assembled, evidence: self-test, note: "Error + Retry + bonus auto-retry with exponential backoff (400 ms→4 s, countdown). Server counter is global, so retry-until-success is the only robust client logic."}
```

SURPRISES:
- Selectable label text silently eats row clicks in sense()-enabled tables; nothing in TableBuilder docs warns — found only because kittest clicks failed; fix: style.interaction.selectable_labels = false.
- egui_extras is NOT version-offset (0.35.0 ↔ egui 0.35.0, unlike egui_plot/egui_dnd) — the iteration-2 offset trap is repo-membership-dependent; ehttp isn't egui-versioned at all.
- Real download cancellation was the EASIEST SPEC-8 cell: ControlFlow::Break aborted the TCP connection first try.
- kittest is stronger than expected: real pointer simulation, shift-click via input_mut().modifiers, headless wgpu rasterization — visual evidence without touching the shared desktop.

TIME_SINK:
- ~40% of grid time on the selectable-labels click-stealing (minimal repro → egui_extras StripLayout / UiBuilder::sense source).
- RSS measurement scripting (ps samples during slow first launch read garbage; gate sampling on stdout markers).
- Fetch self-test determinism: reimplemented the server's FNV-1a latency function to pick a provably slow-then-fast query pair; kittest Harness::run-panics-while-spinner-animates trap (drive with step()).

## tauri — grid + fetch

```yaml
framework: tauri
grid:
  build_ok: true            # 116.9 s wall (loaded machine; same dep set was 36 s idle)
  launch_ok: true           # selftest 14/14 PASS inside real WKWebView
  loc_production: 690       # Rust 251 + frontend 439; +40 config
  loc_verification: 122
  helper_crates: []         # baseline tauri deps only; PRNG is 12-line xorshift64*
  build_ms: 14.5            # Rust-side 100k gen + view build (14.2-19.8)
  filter_ms_1char: 3.4      # filter runs in Rust — no console piping needed
  filter_ms_4char: 4.2      # worst observed 10.8 ms clearing back to 100k under active sort
  rss_after_load_mib: 120.8 # main process only (WebKit XPC helpers excluded)
  ratings:
    table_widget: {rating: hand-rolled, evidence: observed, note: "No widgets; flex-row divs + sticky header. Real Tauri apps would npm-install a JS grid — out of bounds here."}
    virtualization: {rating: hand-rolled, evidence: self-test, note: "Fixed 28px rows + spacer/translateY slice; rows stay in Rust, viewport fetches windows via get_rows over IPC (IPC as virtualization backplane). Asserted at vi=50,000 and 99,999."}
    sort: {rating: hand-rolled, evidence: self-test, note: "Header click → set_sort command → Rust sort over index vector."}
    filter_latency: {rating: hand-rolled, evidence: self-test, note: "Typed through real input events + IPC: 3.4/3.5/5.6/4.2 ms. Rust scan never the bottleneck."}
    column_resize: {rating: hand-rolled, evidence: synthetic-input, note: "Pointer events write --w-i CSS vars. setPointerCapture throws on synthetic pointerIds — listeners on window instead."}
    row_selection: {rating: hand-rolled, evidence: synthetic-input, note: "Click + shift-range (5 rows asserted). View-relative; cleared on filter/sort change (documented approximation)."}
    cell_custom_render: {rating: built-in, evidence: self-test, note: "Webview's genuine strength: chip = span + 6 lines CSS."}
fetch:
  build_ok: true            # 121.7 s
  launch_ok: true           # final selftest 10/10 PASS
  loc_production: 377       # Rust 46 + frontend 331; +49 config
  loc_verification: 132
  helper_crates: ["tauri-plugin-http =2.5.9 (reqwest-behind-IPC; FORCED — see http_client_choice)"]
  ratings:
    async_integration: {rating: built-in, evidence: observed, note: "The webview event loop drives user-authored code; there is zero user-authored Rust async in the app's 40-LoC production Rust side. tauri-plugin-http still executes Reqwest async work in Rust underneath."}
    http_client_choice: {rating: assembled, evidence: observed, note: "Native fetch CORS-BLOCKED from tauri://localhost origin against the ACAO-less server ('TypeError: Load failed'). tauri-plugin-http keeps fetch-shaped JS but executes via reqwest in Rust; +51 unique crates (204→255), binary 8.0→13.9 MiB."}
    debounce_stale: {rating: hand-rolled, evidence: self-test, note: "250 ms setTimeout + dual stale protection: real abort AND sequence guard. Demonstrated with deterministic-latency query pair."}
    progress_streaming: {rating: assembled, evidence: self-test, note: "Plugin body is a real ReadableStream (Tauri channel from reqwest); 83 incremental reads over 8 MiB."}
    cancellation_real: {rating: assembled, evidence: observed, note: "AbortController → plugin fetch_cancel → reqwest drop. Historical shared server: 'ABORT /download after 14/64 chunks'. The aborted search produced no post-delay SEARCH line; because that server logged only after sleeping, this does not prove no request arrived. The original client trace is missing; a fresh 10/10 audit rerun now retains the client-side cancel path against the old untagged server."}
    error_retry_ux: {rating: hand-rolled, evidence: self-test, note: "res.ok → error + Retry until 200. The historical attempt=21 came from a process-global counter shared with concurrent agents; current SPEC-8 keeps the global cycle and requires reset + serial probes."}
```

Retention update: the original Tauri Grid and Fetch client logs remain missing,
but fresh audit reruns retain [14/14 Grid checks](../../measurements/verification-iter4-rerun/tauri-grid-20260710.log)
and [10/10 Fetch checks](../../measurements/verification-iter4-rerun/tauri-fetch-20260710.log).
They verify the reconciled applications, not the original timing environment;
the Fetch rerun used the still-running historical server with its old untagged
log format.

SURPRISES:
- Browser-native fetch is unusable against any server you can't add CORS headers to: tauri://localhost custom-scheme origin makes WKWebView enforce CORS; the sanctioned fix (tauri-plugin-http) moves the security gate from CSP to the capability ACL — URL scopes baked at BUILD time, so a runtime FETCHER_PORT ≠ 7878 can't be allowed without a rebuild.
- The plugin's "fetch-compatible" shim isn't: aborting a controller whose request already completed raises a stray "resource id invalid" rejection and once froze the webview's JS entirely (n=1; fixed by nulling controllers on settle). Browser fetch treats late aborts as no-ops.
- IPC as a virtualization backplane is a non-event in the good sense: windowed get_rows over 100k Rust rows kept every keystroke at 3.4-10.8 ms with no placeholder flashes.
- Cancellation propagated into the server handler: aborting an in-flight
  `/search` prevented it reaching the post-sleep log/response point. The log
  placement cannot distinguish cancellation after arrival from no arrival.

TIME_SINK:
- Diagnosing the fetch abort wart from minified api-iife.js plus one wholly silent first launch (never reproduced).
- Async-race bugs in own code/tests — each fix costs a full cargo rebuild since UI assets embed into the binary.
- ~2 min clean builds per app on the loaded machine.

## iced — peek

```yaml
framework: iced
peek:
  build_ok: true          # 1 upstream future-incompat note (block v0.1.6 via nokhwa-bindings-macos)
  launch_ok: true         # observed alive 66 s with camera+mic+gallery live
  loc_production: 736
  loc_verification: 118   # env-gated hooks: selftest, PEEK_TAB, PEEK_DUMP_FRAME, PEEK_FAKE_CAMERA synthetic source
  helper_crates: [nokhwa 0.10.11 (input-avfoundation), cpal =0.17.3 (pinned to dedupe with rodio's), rodio 0.22.2, image 0.25.10, tokio (rt+sync+time)]
  camera_fps_observed: "30 captured / 30 presented steady-state at 1920x1080 YUYV (mode over 54 s; avg 27.3/25.7 with dips under shared-desktop load)"
  cpu_pct_at_preview: "27.8% avg / 34.4% peak of one core (1080p30 + mic; idle ~1%; RSS 97→229 MiB peak)"
  permission_outcome: "camera pre-authorized at launch; the persisted responsible-terminal grant is the leading interpretation on this host, but TCC attribution was not directly inspected. A mic prompt was inferred on the first run (stream built but delivered 0 callbacks ~5 s until Allow; even the rodio beep was held); denial branches unexercised"
  ratings:
    camera_pipeline: {rating: assembled, evidence: observed, note: "nokhwa on dedicated thread blocking in frame(), YUYV→RGBA there, latest-wins Mutex slot, 8 ms time::every poll → new Handle::from_rgba per frame; iced has no video/texture-stream primitive; bridge ~90 LoC"}
    camera_permission_behavior: {rating: assembled, evidence: observed, note: "nokhwa_check/nokhwa_initialize wrap AVAuthorizationStatus/requestAccess; app polls via AtomicI8 + 200 ms subscription; grant path observed, prompt/denial unexercised (already authorized)"}
    mic_meter: {rating: assembled, evidence: observed, note: "cpal Stream is !Send so it lives on its own thread; RMS to AtomicU32; 20 Hz subscription → dBFS bar with peak-hold; real ambient audio observed"}
    audio_playback: {rating: assembled, evidence: self-test, note: "rodio 0.22 DeviceSinkBuilder→mixer().add(SineWave) + 280 ms keep-alive (drop kills playback); audible output not independently heard"}
    thumbnail_grid: {rating: assembled, evidence: observed, note: "200 JPEGs via spawn_blocking + Semaphore(8), one Task per file into manual row-chunked grid (no grid widget); 256-624 ms total; iced_wgpu caches per handle-id, trims off-screen"}
    texture_upload_cost: {rating: built-in, evidence: self-test, note: "handles immutable + identity-keyed: no update-in-place — preview = full 7.9 MiB RGBA re-upload per frame (atlas allocate + write_texture); ~27 CPU points over idle at 1080p30"}
```

SURPRISES:
- A camera grant was already in place from another run. Every tested
  unbundled cargo binary launched from this terminal inherited access, which is
  consistent with responsible-process attribution on this host; it is not a
  universal guarantee for every binary/terminal. This app's prompt path never
  fired.
- A pending mic prompt is invisible at the cpal API level: stream builds and "plays" while delivering zero callbacks — and rodio OUTPUT was also held until the mic prompt resolved.
- Chased a "camera draws nothing" ghost for an hour; RGBA byte dump proved the pipeline perfect and the room was pitch black at 00:14 local.
- rodio 0.22 renamed core stream/sink types
  (`OutputStream`→`MixerDeviceSink`, `Sink`→`Player`) and still pins cpal
  0.17 while cpal is at 0.18. Many older examples need mechanical updates; the
  upstream changelog describes the renamed functionality as equivalent.

TIME_SINK:
- The false no-draw investigation (atlas source dive, alpha theory, synthetic-source bisection) — resolved by the PEEK_DUMP_FRAME hook.
- Audio-crate API archaeology: rodio 0.22 + cpal 0.17 don't match online examples; verified against vendored sources.
- TCC observation runs: multi-phase harness across 7 launches.

## dioxus — grid + fetch

```yaml
framework: dioxus
grid:
  build_ok: true          # first cargo check 0 errors
  launch_ok: true         # 10 s plain + full self-test; clean SIGTERM
  loc_production: 507
  loc_verification: 46
  helper_crates: [tokio(time, verification-only)]
  build_ms: 19.1
  filter_ms_1char: 2.61
  filter_ms_4char: 4.00
  rss_after_load_mib: 106.4   # main process; WebContent XPC out-of-process, unattributed
  ratings:
    table_widget: {rating: hand-rolled, evidence: observed, note: "No table widget for Dioxus desktop, no usable helper crate; divs with display:grid + grid-template-columns from a Signal<[f64;6]>."}
    virtualization: {rating: hand-rolled, evidence: self-test, note: "Sticky header + spacer + absolute rows for viewport±8; 0.7's ScrollData now carries scroll_top()/client_height() so onscroll needs NO JS. Full 2.8M-px scrub via real scrollTop writes; RSS flat."}
    sort: {rating: assembled, evidence: self-test, note: "SORT_MS Value 8.67/7.39 ms over full 100k Vec."}
    filter_latency: {rating: assembled, evidence: self-test, note: "FILTER_MS 2.3-4.4 ms across 1-4 chars; DOM diff is ~36 rows."}
    column_resize: {rating: hand-rolled, evidence: self-test, note: "Real divider drag via clientX deltas — the no-element-geometry trap doesn't bite because resize needs only deltas."}
    row_selection: {rating: assembled, evidence: self-test, note: "Shift-range ~10 LoC because modifiers ride on every MouseEvent. click 5, shift-click 25 → 21 selected."}
    cell_custom_render: {rating: built-in, evidence: observed, note: "Any RSX is a cell; chip = span + 3 CSS classes."}
fetch:
  build_ok: true          # 1 trivial first-check error (Task at dioxus::core::Task, not prelude)
  launch_ok: true
  loc_production: 402
  loc_verification: 91    # incl. ct_key cancellation-probe resource
  helper_crates: [reqwest 0.12 (no default features, json), serde, tokio(time — framework re-exports no timer)]
  ratings:
    async_integration: {rating: built-in, evidence: observed, note: "VirtualDom runs on multi-thread tokio; reqwest futures run directly in use_resource/spawn and write signals from async code — zero bridging. Caveat: the occlusion freeze gates ALL task servicing on webview visibility."}
    http_client_choice: {rating: assembled, evidence: observed, note: "reqwest drop-in because the runtime is tokio; per-call clients so per-connection cancellation is observable."}
    debounce_stale: {rating: built-in, evidence: self-test, note: "One use_resource = read query → sleep 250ms → GET; dependency change cancels the old task — debounce AND stale-protection need no sequence guard. Server: exactly one SEARCH q=\"amber\", none for the earlier \"am\"."}
    progress_streaming: {rating: assembled, evidence: self-test, note: "spawn + chunk() loop writing received-bytes signal per 128 KiB chunk."}
    cancellation_real: {rating: built-in, evidence: observed, note: "THE answer: yes, real. use_resource dep change mid-stream → 'ABORT /download after 12/64 chunks'; Task::cancel → 'ABORT after 16/64', exactly matching client 2,097,152 bytes. Dropped future → dropped Response → hyper closes TCP."}
    error_retry_ux: {rating: assembled, evidence: self-test, note: "Retry driven 3×: 500, 500, success attempt=24 correlating with server FLAKY lines. Auto-retry+backoff implemented, unexercised."}
```

SURPRISES:
- THE OCCLUSION FREEZE (the big one): an occluded/unactivated Dioxus window doesn't just skip painting — after the next signal write, edits_in_progress (dioxus-desktop edits.rs:117) parks the whole VirtualDom+task loop waiting for the throttled WKWebView to flush, so timers AND in-flight downloads stop. 3 of 6 verification runs froze; always-on-top made it deterministic-clean. wry 0.53 HAS with_background_throttling(Disabled) but dioxus 0.7.9's Config doesn't plumb it through. Upstream-actionable.
- use_resource cancellation is genuinely network-level for both restarts and Task::cancel. No sequence guard needed anywhere.
- ScrollData grew scroll_top/client_height in 0.7 → hand-rolled virtualization is pure-Rust (no eval, no overlay tricks); 100k rows on ~36 rendered divs, flat RSS.
- Shared-server log interleaving forced byte-exact correlation and re-runs for uncontaminated windows.

TIME_SINK:
- Diagnosing the occlusion freeze: five instrumented runs + source dive through dioxus-desktop edits/webview internals.
- dioxus-fetch cold release build 273 s (reqwest/hyper roughly doubles the dioxus-only 115 s).
- Cross-agent contention (contaminated flaky cycles, focus stealing) forced repeat evidence runs.

## slint — grid + fetch

```yaml
framework: slint
grid:
  build_ok: true          # 60 s serial (a 400 s reading was CPU contention)
  launch_ok: true
  loc_production: 539     # main.rs 305 + build.rs 3 + main.slint 231
  loc_verification: 217
  helper_crates: []       # xorshift PRNG + 20-line days→ISO instead of rand/chrono
  build_ms: 135           # 92-225 across runs
  filter_ms_1char: 2.3
  filter_ms_4char: 3.4
  rss_after_load_mib: 119.5   # flat after full 2.8M-px sweep
  ratings:
    table_widget: {rating: assembled, evidence: source-only, note: "StandardTableView REJECTED: cells are text-only StandardListViewItem and selection is single current-row — chips and range-select cannot even be rendered. Built ListView + custom header (header x tracks viewport-x, widths in a VecModel the DSL writes back into)."}
    virtualization: {rating: built-in, evidence: self-test, note: "Custom Rust Model impl — ListView materialized 20 of 100,000 rows at first paint, 3,092 total after full sweep; RSS flat."}
    sort: {rating: assembled, evidence: synthetic-input, note: "Real header clicks via dispatch_event; Rust sorts an index view — name 7-12 ms, value 2.3-3 ms. StandardTableView would give only the chevron; you sort yourself either way."}
    filter_latency: {rating: hand-rolled, evidence: synthetic-input, note: "Real keystrokes into LineEdit; substring + re-sort + ModelNotify::reset per keystroke. FilterModel/SortModel adapters exist (source-only) but index-vector recompute is simpler."}
    column_resize: {rating: assembled, evidence: synthetic-input, note: "8-step synthetic pointer drag on a 1px divider (pattern cribbed from widget source): 220→260 px, pixel-snapshot confirmed. Not the weak cell in Slint."}
    row_selection: {rating: assembled, evidence: synthetic-input, note: "Click + Shift-range via PointerEvent.modifiers.shift with a real Shift window event; row_changed per row (≤4096 span) else reset."}
    cell_custom_render: {rating: assembled, evidence: observed, note: "Chips are ~20 declarative lines in the hand-built row — trivial only because StandardTableView was abandoned; inside it, not achievable."}
fetch:
  build_ok: true          # 56 s serial
  launch_ok: true
  loc_production: 437     # main.rs 292 + main.slint 142
  loc_verification: 179   # incl. spawnlocal_probe.rs
  helper_crates: ["tokio =1.52.3 (1-worker; reactor for reqwest)", "reqwest =0.12.24 no-default-features", "serde", "serde_json"]
  ratings:
    async_integration: {rating: assembled, evidence: observed, note: "Background tokio + Weak::upgrade_in_event_loop for every UI hop. The alternative was DISPROVEN, not dismissed: spawnlocal_probe polling reqwest via slint::spawn_local panics 'there is no reactor running' — Slint ships an executor but no reactor."}
    http_client_choice: {rating: assembled, evidence: observed, note: "reqwest over ureq+thread: chunk() streaming, pooled keep-alive, abort-on-drop."}
    debounce_stale: {rating: assembled, evidence: self-test, note: "Debounce = restartable single-shot slint::Timer (2 keystrokes → 1 dispatch). Stale = AbortHandle + UI-thread sequence guard; guard demonstrated alone with slow-then-fast query pair → STALE_DROPPED seq=2."}
    progress_streaming: {rating: assembled, evidence: observed, note: "chunk() loop, ~64 UI updates over 8 s into built-in ProgressIndicator; snapshot frozen at 36% post-cancel."}
    cancellation_real: {rating: assembled, evidence: observed, note: "abort_handle().abort() → TCP close; private server log: 'ABORT /download after 23/64 chunks' (matches 3.0 s cancel point and UI 0.36)."}
    error_retry_ux: {rating: assembled, evidence: observed, note: "500 → Retry → success attempt:3 on ONE pooled connection (clean 1/2/3 only on a private server instance). Manual retry."}
```

SURPRISES:
- StandardTableView evaporates on contact: [[StandardListViewItem]] rows are text-only and its selection model can't RENDER more than one selected row — drag column-resize is the only convenience lost by hand-rolling (and its source hands you the pattern).
- The DSL can assign into model row fields inside a repeater and it write-backs via set_row_data — undocumented but load-bearing; the built-in table itself depends on it.
- Window::dispatch_event + Window::take_snapshot = complete synthetic-input + pixel-evidence harness with zero external tooling.
- The shared /flaky counter is provably global (interleaved attempts 10/12/15 from sibling traffic); clean cycles only on a private instance.

TIME_SINK:
- Verification attribution on shared infrastructure (private :7911 server instance; RSS sample racing exec; 6.7× build-time inflation from concurrent builds).
- Auditing StandardTableView's real 1.17.1 surface from crate sources before rejecting it.
- Designing the stale-response demo (replicated FNV-1a latency; abort_prev=false path to show the guard alone).

## iced — grid + fetch

```yaml
framework: iced
grid:
  build_ok: true
  launch_ok: true         # ~30 min through self-test; window pixel-verified
  loc_production: 663
  loc_verification: 278   # in-app prints + uihelper.swift 172 + drive.sh 69
  helper_crates: []       # std + iced only
  build_ms: 35.74
  filter_ms_1char: 3.96
  filter_ms_4char: 4.03
  rss_after_load_mib: 87.9   # 89.7 after long scroll (flat) — leanest of the round
  ratings:
    table_widget: {rating: hand-rolled, evidence: source-only, note: "iced 0.14 ships widget::table but source-read shows it eagerly builds one Element per cell for ALL rows (600k widgets at 100k rows) — no virtualization/sort/resize; replaced with hand-rolled scrollable + spacers + windowed rows."}
    virtualization: {rating: hand-rolled, evidence: self-test, note: "Windowing from on_scroll Viewport offset + fixed 26px rows; 677 re-window events during long scroll; RSS flat."}
    sort: {rating: assembled, evidence: self-test, note: "8.81 ms cold name-sort at 100k, 1.57 ms desc re-sort — synchronous in update is fine."}
    filter_latency: {rating: assembled, evidence: self-test, note: "~4 ms at 100k (1.15-5.59); scroll snapped to top via operation::scroll_to."}
    column_resize: {rating: hand-rolled, evidence: self-test, note: "mouse_area strip arms drag, conditional global listen_with streams deltas; synthetic 60px drag verified."}
    row_selection: {rating: assembled, evidence: self-test, note: "Shift-range works via permanent ModifiersChanged subscription (on_press carries no modifiers)."}
    cell_custom_render: {rating: built-in, evidence: observed, note: "Cells are arbitrary Elements; chip = rounded container — zero friction."}
fetch:
  build_ok: true          # first-try clean compile
  launch_ok: true
  loc_production: 552
  loc_verification: 253
  helper_crates: [reqwest =0.12.24 (json+stream, no TLS), tokio (time), serde]
  ratings:
    async_integration: {rating: built-in, evidence: observed, note: "features=[\"tokio\"] swaps the executor for the tokio runtime; Task::perform/sip poll off-thread. TRAP: default thread-pool executor has no reactor — reqwest panics at RUNTIME with no compile-time hint."}
    http_client_choice: {rating: assembled, evidence: observed, note: "reqwest for bytes_stream + Clone-able Client; zero friction since the app executor IS tokio."}
    debounce_stale: {rating: assembled, evidence: self-test, note: "ONE mechanism: Handle::abort of previous Task + sleep(250ms) prefix. 6 QUEUED → exactly 1 server SEARCH; in-flight abort proven. Generation guard kept but never fired."}
    progress_streaming: {rating: assembled, evidence: self-test, note: "First-party sipper feature: Task::sip streams (received,total) per chunk into progress_bar; 64 steps."}
    cancellation_real: {rating: assembled, evidence: observed, note: "abortable() sip task; Cancel → TCP close. 'ABORT /download after 34/64 chunks' = exactly client 4456448 bytes."}
    error_retry_ux: {rating: assembled, evidence: self-test, note: "State enum + Retry; FLAKY 1,2→500, 3→OK, screenshotted."}
```

SURPRISES:
- iced 0.14's new table widget is the anti-headline: it materializes every cell as a widget up front (source-verified) — "has a table widget" ≠ "has a data grid"; 100k rows still means hand-rolled windowing.
- 100k-row synchronous filter+sort inside update costs ≤9 ms — data scale is purely a rendering/windowing problem in iced, not compute.
- Task::abortable alone implements debounce, stale protection, AND protocol-level cancellation; the customary generation counter proved redundant.
- The shared desktop was actively hostile (fullscreen Zoom window, three agents stealing focus sub-second); every input gated on frontmost-window-at-point checks.

TIME_SINK:
- Synthetic input on a contested desktop (~half of total): winit ignores modifier flags on mouse events (shift-click needs a real flagsChanged event); occluded backgrounded window eventually stopped receiving events.
- Evidence attribution in the shared server log (line-count markers + byte↔chunk correlation).
- Windowing/scroll-offset drift when filters shrink content (programmatic scroll_to doesn't emit on_scroll).

## gpui — peek

```yaml
framework: gpui
peek:
  build_ok: true          # first try, 7m53s cold, 7.9 MiB binary; canonical dependency count is 407 unique crate names
  launch_ok: true         # run1 alive 418 s @30fps; launch-idle.png committed
  loc_production: 1274
  loc_verification: 95
  helper_crates: [objc, block, gpui_media, core-video, core-foundation, dispatch, image, smallvec, futures, cpal, rodio]
  camera_fps_observed: "30-31 fps presented, sustained (640×480 and 1920×1080 depending on default device); 0 fps presented when occluded while capture continues ~30/s"
  cpu_pct_at_preview: "1080p30: ZERO-COPY surface ~8% (5-10) vs CPU-upload ~22% (16-26, NV12→BGRA 2.9-5.0 ms/frame); idle 0.3%"
  permission_outcome: "No prompt fired; camera+mic were already authorized on this binary's first run and remained so across tested launches/rebuilds. Responsible-terminal attribution is inferred from this host's process ancestry/shared grant; denial branch coded but unexercised"
  ratings:
    camera_pipeline: {rating: assembled, evidence: observed, note: "gpui::surface(CVPixelBuffer) is a public zero-copy API (CVMetalTextureCache, exact 420f full-range NV12 assertion), but there is no camera API — ~370 LoC direct AVFoundation/objc glue cribbed from gpui's own screen_capture.rs. This implementation rejected nokhwa because its CPU-copy path could not feed GPUI's surface path; that is an architectural choice, not proof nokhwa can never be used with GPUI."}
    camera_permission_behavior: {rating: hand-rolled, evidence: observed, note: "AVCaptureDevice authorizationStatus/requestAccess via objc (~30 LoC); granted path observed incl. persistence; prompt/denial written but unexercisable."}
    mic_meter: {rating: assembled, evidence: observed, note: "cpal → RMS atomic → 20 Hz gpui timer → bar; live ambient tracked. TCC denial would yield silence, not error."}
    audio_playback: {rating: assembled, evidence: self-test, note: "rodio 0.22 (renamed API); 'BEEP ok' every trigger; audibility not human-confirmed."}
    thumbnail_grid: {rating: built-in, evidence: observed, note: "img(path) decodes async and caches decoded BGRA app-wide by path; uniform_list virtualizes so only visible thumbnails load; no built-in downscale — full-size decodes cached."}
    texture_upload_cost: {rating: built-in, evidence: self-test, note: "Zero-copy frame = CFRetain + scene node (IOSurface IS the Metal texture); CPU path = 2.9-5.0 ms convert + full 7.9 MiB/frame atlas upload and you must window.drop_image old frames or the atlas LEAKS — measured 8% vs 22% at identical 1080p30."}
```

SURPRISES:
- gpui 0.2.2 ships a public zero-copy video element
  (`surface()`/`paint_surface` via CVMetalTextureCache) that is undocumented,
  but the Metal renderer asserts the exact `420f` full-range NV12 format;
  `420v` and other formats panic during paint, so an app-side guard is needed.
- TCC never prompted. The app was already authorized on its first tested run
  and stayed authorized across rebuilds; responsible-terminal attribution is
  the leading local interpretation, not directly inspected TCC state.
- Presented-vs-captured fps directly observable: gpui stops presenting occluded windows (presented→0, capture ~30/s) — occluded camera apps keep burning capture CPU.
- macOS silently swapped the default camera between runs (640×480 vs 1920×1080), invalidating the first comparison; re-measured at matched resolution.

TIME_SINK:
- Pre-coding source archaeology (gpui/metal_renderer/core-video) to find the surface path, its NV12 assert, and the four crates whose types must version-unify — also why it compiled first try.
- AVFoundation delegate/objc glue, de-risked by cribbing gpui's screen-capture delegate.
- Re-running CPU/fps measurements three times for an honest same-resolution comparison.

## tauri — peek

```yaml
framework: tauri
peek:
  build_ok: true          # first attempt, 1m25s cold
  launch_ok: true         # multiple 25-80 s auto-driven runs + 10 s smoke
  loc_production: 663     # Rust ~243 + frontend 420
  loc_verification: 205
  helper_crates: ["nokhwa =0.10.11 (secondary Rust path only)", "serde/serde_json"]
  camera_fps_observed: "30.0 median (PRIMARY: JS getUserMedia in WKWebView, rVFC-counted, 640x480@30); 19.0 median (SECONDARY: nokhwa YUYV→RGBA→raw-IPC→canvas, capture/decode-bound; was 2.6 fps at 1080p with -O0 deps)"
  cpu_pct_at_preview: "~4-12% of one core TOTAL at 640x480@30 GUM: app 1.4-1.8% + WebKit.GPU 0.7-2.4% + WebContent 2-10%; RSS family 280-440 MiB"
  permission_outcome: "first run: camera granted in 2.6 s, mic prompt sat unanswered >120 s (graceful); later runs granted in 33-153 ms — persisted. Unbundled binary works because tauri-codegen EMBEDS ./Info.plist (__TEXT,__info_plist) in dev-context builds"
  ratings:
    camera_pipeline: {rating: built-in, evidence: observed, note: "getUserMedia is the real path — camera never touches Rust; tauri://localhost is a secure context; 30 fps. Rust-side nokhwa+raw-IPC secondary: hand-rolled, 19 fps, fragile under camera contention (Lock Rejected / one silent Camera::new hang) while GUM kept streaming against the same contention."}
    camera_permission_behavior: {rating: built-in, evidence: observed, note: "wry auto-grants WKWebView's media-capture delegate (source-only), so the only gate is TCC; Info.plist embedding for unbundled binaries is built into tauri-build dev context."}
    mic_meter: {rating: built-in, evidence: observed, note: "getUserMedia(audio) + AnalyserNode RMS→VU at 20 Hz, all JS, Rust bypassed. This is a recorded SPEC-6 deviation because the requirement called for cpal input."}
    audio_playback: {rating: built-in, evidence: observed, note: "WebAudio oscillator; ran with no user gesture (AudioContext running from the automated harness). This is a recorded SPEC-6 deviation because playback was required through rodio or cpal."}
    thumbnail_grid: {rating: assembled, evidence: observed, note: "asset: protocol needs 3 config pieces (protocol-asset feature + assetProtocol.enable + CSP img-src). loading=lazy: 200/200 in 3.9 s, zero jank, WebKit owns decode/cache."}
    texture_upload_cost: {rating: built-in, evidence: observed, note: "n/a for GUM — no app-visible upload; replaced by WebKit compositor cost (the 4-12% family CPU). Secondary path: 1.2 MiB RGBA + raw-IPC + putImageData per frame; poll loop alone ~9%."}
```

SURPRISES:
- tauri://localhost is a secure context and wry hard-codes WKPermissionDecision::Grant for media capture — getUserMedia needed literally ZERO Tauri config; the "camera permission dance" collapses to the OS TCC prompt, satisfied even unbundled by the embedded dev-mode Info.plist.
- Camera CONTENTION semantics, not speed, is the real path difference: WebKit's brokered capture streamed 30 fps while the user's video call held the camera; nokhwa's lockForConfiguration in the same process got Lock Rejected seconds later.
- Debug codegen is a benchmark trap: nokhwa YUYV→RGBA at -O0 = 2.6 fps at 1080p; [profile.dev.package."*"] opt-level=2 + 640x480 = 19 fps.
- Frontend assets embed at proc-macro expansion — editing ui/*.js leaves cargo build a no-op; touch src/main.rs required.

TIME_SINK:
- Multi-session camera-contention forensics to avoid mislabeling nokhwa as broken.
- TCC is near-unobservable from a harness (TCC.db denied, tccd logs redacted) — permission behavior reconstructed from getUserMedia latencies (2.6 s / >120 s pending / 33-153 ms warm).
- Privacy-safe verification on a shared desktop: two screenshots DISCARDED (region capture caught the user's video call; window capture caught a stale private camera frame) — settled on CGWindowID-scoped capture of the Gallery tab.

## egui — peek

```yaml
framework: egui
peek:
  build_ok: true            # only dep future-incompat notice (block v0.1.6 via nokhwa/objc)
  launch_ok: true           # plain 12 s + 4 instrumented runs; in-app wgpu screenshots of all 3 tabs
  loc_production: 878
  loc_verification: 299     # env-gated hooks + 60 kittest tests + probe.rs
  helper_crates: [nokhwa =0.10.11 (input-avfoundation), cpal =0.17.3, rodio =0.22.2, image =0.25.10, egui_kittest (dev), nokhwa-bindings-macos (dev, probe)]
  camera_fps_observed: "29-30 presented fps sustained 30 s+ (1280x720 YUYV @30 negotiated; 1 drop in ~850; counter gated on viewport visibility)"
  cpu_pct_at_preview: "13.4-19.0% (mean ≈15.8% of one core, whole process incl. mic); RSS ~280-300 MiB"
  permission_outcome: "no prompt fired; a grant persisted from a prior session and the fresh unbundled binary streamed instantly. Responsible-terminal attribution is inferred from the shared-host observations; denial path unexercised"
  ratings:
    camera_pipeline: {rating: assembled, evidence: observed, note: "nokhwa thread → sync_channel(2, latest-wins) → ColorImage → TextureHandle created once + tex.set per frame + cross-thread request_repaint; egui half ~30 LoC, nokhwa half cost the time"}
    camera_permission_behavior: {rating: assembled, evidence: observed, note: "nokhwa_check/initialize state machine with spinner + in-UI degrade; grant persistence observed across 6+ launches; prompt/denial unexercised"}
    mic_meter: {rating: assembled, evidence: observed, note: "cpal F32 → RMS AtomicU32 → painter VU bar (dBFS, peak-hold) at 20 Hz via request_repaint_after(50ms)"}
    audio_playback: {rating: assembled, evidence: self-test, note: "rodio 0.22 mixer + SineWave; audibility not humanly confirmed"}
    thumbnail_grid: {rating: hand-rolled, evidence: observed, note: "4 decode threads → load_texture (16/frame cap) → ScrollArea+horizontal_wrapped; 200/200 in 0.14 s; texture cache fully manual (egui_extras::install_image_loaders recorded as the assembled alternative, not used)"}
    texture_upload_cost: {rating: built-in, evidence: self-test, note: "TextureHandle::set = ImageDelta::full → full 3.5 MiB re-upload per 720p frame (~105 MiB/s @30fps); CPU-side YUYV→RGB conversions dominate, not GPU upload"}
```

SURPRISES:
- On this M4 MacBook and with nokhwa 0.10.11, the obvious requests tested,
  including its `None` default, failed. Two observed matching problems were:
  macOS bindings mapped Apple's NV12 fourccs (420v/420f) to
  `FrameFormat::YUYV`, and `set_all` matched only a range's maximum fps while
  `supported_formats` also listed minima. `Closest(YUYV 720p@30)` plus a
  fallback ladder worked. This is a host/version-specific characterization,
  not proof about every nokhwa camera path.
- TCC never prompted; authorization persisted across the tested binaries and
  `ps` showed the same terminal ancestry. That supports, but does not directly
  prove, responsible-terminal attribution and is not an independent host-level
  confirmation.
- macOS App Nap stalled request_repaint_after-only repaint loops for 45 s in an unfocused window (cross-thread request_repaint() always wakes); eframe skips painting AND ViewportCommand::Screenshot while occluded although App::ui keeps running — naive presented-FPS counters overreport until gated on viewport().visible().
- The entire hardware stack (nokhwa+cpal+rodio+image) adds only ~1.6 MiB to the binary; 720p@30 preview costs ~16% of one core.

TIME_SINK:
- nokhwa format negotiation: raw-format probe tool + source-diving nokhwa-core/bindings (~35% of total).
- Screenshot/verification vs occlusion + App Nap (two capture runs silently killed).
- Audio API churn means many older rodio/cpal examples need mechanical updates;
  the audit did not establish that all pre-2025 example code is unusable.
(Privacy: full camera screenshot kept OUT of the repo; committed header crop shows fps/format evidence only.)

## slint — peek

```yaml
framework: slint
peek:
  build_ok: true            # 59.5 s warm; no-op rebuild 0.46 s
  launch_ok: true           # 7 runs, all exit 0; gallery <1 s after launch
  loc_production: 689       # Rust 526 + .slint 163
  loc_verification: 109
  helper_crates: ["nokhwa =0.10.11 (input-avfoundation)", "cpal =0.17.3", "rodio =0.22.2", "image =0.25.10"]
  camera_fps_observed: 30.0  # presented≈delivered≈captured (no drops) at 1920x1080 YUYV@30
  cpu_pct_at_preview: 34     # mean of 35 s steady-state incl. mic; mic-only 15.5; idle 0.1-0.4; RSS 305 MiB preview
  permission_outcome: "no prompt; the unbundled binary was authorized on its first launch under the shared terminal grant. Responsible-host attribution is inferred; denial unexercised"
  ratings:
    camera_pipeline: {rating: assembled, evidence: observed, note: "nokhwa thread → decode to SharedPixelBuffer → invoke_from_event_loop → Image::from_rgba8_premultiplied — literally Slint's documented pattern, 30 fps on first successful open. All friction was nokhwa's format negotiation. Presented-fps via Window::set_rendering_notifier."}
    camera_permission_behavior: {rating: assembled, evidence: observed, note: "nokhwa_check/initialize + 35 s wait + in-UI degrade. Bonus observed: camera held by a concurrent agent → 'Lock Rejected' at open; app degrades in-UI, clean exit."}
    mic_meter: {rating: assembled, evidence: observed, note: "cpal on dedicated thread (!Send), RMS → AtomicU32 → 50 ms slint::Timer → dB-mapped property; VU bar ~25 lines .slint with animate width + peak-hold."}
    audio_playback: {rating: assembled, evidence: self-test, note: "rodio 0.22 fully renamed API; lazy sink in RefCell on UI thread; queued OK, not ear-verified."}
    thumbnail_grid: {rating: assembled, evidence: observed, note: "No async image loading/grid/recycling in Slint. 4 threads → thumbnail(208) → set_row_data into placeholder-prefilled VecModel; 200 JPEGs in 823 ms cold; UI never blocked; textures cached per-item after first upload."}
    texture_upload_cost: {rating: assembled, evidence: self-test, note: "Source-verified: buffer Images get ImageCacheKey::Invalid, so there is no global keyed TextureCache entry for reuse across changing camera images. Per-item graphics caches still exist, but this source change pays a full 7.91 MiB texture re-create/upload per frame (~237 MiB/s) + CPU convert + alloc. 34% of one core at 1080p30 (vs 15.5% mic-only)."}
```

SURPRISES:
- nokhwa needed 3 source-dives before first frame: =0.10.9 NO LONGER COMPILES (bindings 0.2.4 broke a channel type within a patch range); Closest matches only exact resolution+format pairs; macOS bindings advertise range-min frame rates that set_all can never set. Working recipe: AbsoluteHighestFrameRate.
- Window::take_snapshot() works on femtovg GL — free, TCC-less, window-scoped screenshots with zero desktop interaction.
- TCC never prompted in this fourth app-level observation: the unbundled binary
  inherited the existing shared-terminal grant on its first launch. Because
  all observations share one host/grant, this is repetition rather than an
  independent TCC condition.
- Presented≈captured with zero coalesced frames at 1080p30 — the full-upload path keeps up effortlessly; the 15.5% CPU for a 20 Hz VU bar (full-window GL redraws) was proportionally the bigger eyebrow-raiser.

TIME_SINK:
- ~50%: nokhwa format negotiation — two failed launch cycles, three distinct traps, diagnosed from crate sources.
- ~15%: presented-vs-captured fps design (rendering-notifier + counter split).
- ~15%: measurement runs (CPU sampling, TCC observation, camera-contention retries).

## dioxus — peek

```yaml
framework: dioxus
peek:
  build_ok: true          # 321.8 s clean under contention (noncanonical), leaf rebuild 1.8 s; 5.85 MiB stripped; 305 unique crates
  launch_ok: true         # window up ~1 s; scripted self-test drove all capabilities to DONE at 46.7 s
  loc_production: 868     # incl. ~150 lines embedded page JS — the camera pumps ARE the frame path
  loc_verification: 245
  helper_crates: [tokio(time), nokhwa=0.10.11, image=0.25(jpeg), cpal=0.17, rodio=0.22, objc2-foundation(NSProcessInfo AppNap guard)]
  camera_fps_observed: "JS getUserMedia: 30.0 fps presented (rvfc, 640x480@30; 16.5 fps in dark room — WebKit adaptive capture). Rust path (nokhwa→JPEG→asset-handler long-poll→<img>): 29.9 presented / 30.1 captured, 3.3 ms/frame Rust-side."
  cpu_pct_at_preview: "JS path 5.2-9.0% total tree @ ~337 MiB; Rust path 32.9% total @ ~354 MiB — Rust path ≈4-6× JS for identical pixels"
  permission_outcome: "No prompt; the grant pre-existed and persisted across tested runs/binaries/frameworks. An AVCaptureDevice probe and process ancestry support responsible-terminal attribution on this host but do not expose TCC's key directly; device-busy degrade exercised gracefully"
  ratings:
    camera_pipeline: {rating: assembled, evidence: observed, note: "JS path near built-in — dioxus:// is a secure context + wry auto-grants WKWebView media permission, camera pixels never touch Rust; Rust path assembled from nokhwa+image+use_asset_handler long-poll, both ~30 fps."}
    camera_permission_behavior: {rating: built-in, evidence: observed, note: "WebKit+TCC+wry handle the path. Every tested framework binary from this shell shared the existing grant; attribution to the responsible terminal is the leading host-specific interpretation."}
    mic_meter: {rating: assembled, evidence: observed, note: "cpal f32 → RMS/peak atomics → 20 Hz signal mirror → CSS VU bar; cpal Stream is !Send → parked thread."}
    audio_playback: {rating: assembled, evidence: self-test, note: "rodio 0.22 new API, Ok twice per run; API-contract verification."}
    thumbnail_grid: {rating: assembled, evidence: observed, note: "use_asset_handler + one IO thread + <img loading=lazy>; 200/200, UI responsive; WebKit does decode/downscale/texture-cache; +235 MiB tree RSS fully paged in."}
    texture_upload_cost: {rating: hand-rolled, evidence: observed, note: "No texture primitive in dioxus-desktop: per frame = YUYV→RGB + JPEG encode (3.3 ms) + protocol copy + WebContent decode + upload; 32.9% vs 5-9% CPU (Rust vs JS)."}
```

SURPRISES:
- dioxus:// is a secure context and wry hardcodes WKPermissionDecision::Grant — getUserMedia in RSX worked first try at 30 fps with ZERO Rust involvement and zero config.
- App Nap froze the ENTIRE Dioxus/tokio event loop mid-run when occluded (audio callbacks kept running); a media app needs a raw NSActivity assertion — dioxus/tao expose nothing.
- AVFoundation's config lock is machine-wide mutual exclusion (sibling agent's app blocked runs 2-3), yet WebKit's brokered getUserMedia captured concurrently the whole time.
- In the tested nokhwa-bindings-macos version, pixel formats were mis-mapped
  (420v/f→YUYV, NV12→10-bit), so the NV12 request did not match this host's
  FaceTime camera; occluded-window screenshots revealed stale window-server
  pixels (iteration-3's paint deferral made visible).

TIME_SINK:
- Camera-contention forensics across 3 failed runs (user's live call, own just-stopped WebKit session, sibling agent's app).
- Diagnosing the App Nap freeze from a 0.5%-CPU flatline in the sampler trace.
- Shared-desktop measurement hygiene: WebKit XPC helpers reparent to launchd (baseline-diff attribution); a region screenshot caught the user's private call — deleted immediately, switched to window-ID-scoped capture.

## xilem — peek

(FRICTION.md reconstructed by a finisher from surviving run artifacts after the
original agent's transcript was lost; landed in the repo by the orchestrator.)

```yaml
framework: xilem
peek:
  build_ok: true            # =0.4.0; no-op rebuild 0.26 s; 10.5 MiB stripped
  launch_ok: true           # fresh 12 s run: alive, gallery hook fired, clean exit (observed)
  loc_production: 1021      # main.rs 504 + camera_view.rs 161 + media.rs 356
  loc_verification: 50      # always-on inert hooks ([peek-probe] 1 Hz stdout, ps self-sampling)
  helper_crates: ["nokhwa =0.10.11", "cpal =0.17.3", "rodio =0.22.2", "image 0.25 (jpeg-only)"]
  camera_fps_observed: "29.5-30.6 presented (== captured) at 1080p30, steady incl. an accidental 7 h soak"
  cpu_pct_at_preview: "~60-75% of one core camera-only (77-86% with mic meter); ~0-2% idle; RSS 120→~230-320 MiB"
  permission_outcome: "No TCC prompt — unbundled binary inherited the terminal's persisted grant; busy-camera 'Lock Rejected' degraded to in-UI error + 10×2 s retries, no crash"
  ratings:
    camera_pipeline: {rating: assembled, evidence: observed, note: "nokhwa thread → fresh 7.91 MiB RGBA Blob per 1920×1080 frame → MessageProxy → custom masonry widget via Scene::draw_image; stock image() relayouts every frame, so a paint-only widget was hand-built."}
    camera_permission_behavior: {rating: assembled, evidence: observed, note: "nokhwa wraps AVFoundation auth; 35 s prompt-wait + degradation coded, never triggered (grant pre-existed)."}
    mic_meter: {rating: assembled, evidence: observed, note: "cpal → 20 Hz RMS/dBFS → built-in progress_bar; live in screenshot."}
    audio_playback: {rating: assembled, evidence: observed, note: "rodio sine on throwaway thread (sink not Send); 'played ×1' on screen."}
    thumbnail_grid: {rating: assembled, evidence: observed, note: "No built-in async loader/virtualization/texture-cache; task_raw + spawn_blocking(4) streams 200 thumbs into flex rows in a portal; thumbs capped 128 px since vello 0.6 atlas re-upload behavior unverified."}
    texture_upload_cost: {rating: hand-rolled, evidence: self-test, note: "Per frame: full YUYV→RGBA CPU convert + fresh 7.91 MiB Blob alloc (blob-id equality is the dirty-check) + full re-encode/upload, no partial path; ~60-75% of one core at 1080p30. A local seven-hour observation showed no measurable RSS trend, but its raw series was not retained and it does not prove the absence of every leak."}
```

SURPRISES:
- FRICTION.md was a draft stub — the audit was reconstructed from surviving logs/screenshots (now landed).
- With nokhwa 0.10.11 and this FaceTime camera, the tested requests did not
  negotiate 640×480/720p NV12 and the app used 1920×1080 YUYV; the RGBA
  texture payload was 7.91 MiB/frame, about 4× the intended payload. This is
  a host/version-specific result, not a universal camera rule.
- Concurrent agents' peek apps hold the AVFoundation configuration lock: Camera::new fails 'Lock Rejected' until retried — a whole launch session failed before the retry loop existed.
- The previous instance kept running after its session died. A local accidental
  seven-hour observation stayed at 29.5–30.6 fps with no measured RSS trend;
  because the raw series was not retained, it is evidence against a large leak
  in that run rather than proof that the path cannot leak.

TIME_SINK:
- Camera contention + format negotiation (multiple failed sessions before stable preview).
- Hand-rolling the CameraPreview masonry Widget + xilem View pair (paint-only updates, honest presented-FPS counting).
- Bridging non-Send device handles (nokhwa/cpal/rodio) onto dedicated threads wired into xilem's worker/MessageProxy model.

## xilem — grid + fetch

(Verified/documented by a finisher after the builder's transcript was lost;
FRICTION time-went sections flagged reconstructed-from-code.)

```yaml
framework: xilem
grid:
  build_ok: true          # no-op rebuild 0.35 s
  launch_ok: true
  loc_production: 1022    # main.rs 486 + widgets.rs 536
  loc_verification: 217   # external CGEvent swift+sh drivers (nothing in-binary)
  helper_crates: []       # xilem =0.4.0 only
  build_ms: 27.6          # 21.7 on final check
  filter_ms_1char: 1.5
  filter_ms_4char: 6.1
  rss_after_load_mib: 130.4  # 150.4→151.1 after ~130k px scripted scroll (plateaus)
  ratings:
    table_widget: {rating: hand-rolled, evidence: observed, note: "No table widget in xilem/masonry 0.4; flex rows + two custom masonry widgets (RowFrame, DragHandle) with full View plumbing — 536 LoC, half the app."}
    virtualization: {rating: built-in, evidence: observed, note: "STOCK virtual_scroll view; scripted ~130k px deep scroll smooth, RSS plateau. Coded around documented caveats (transient out-of-range ids, empty-range jank)."}
    sort: {rating: assembled, evidence: synthetic-input, note: "SORT_MS Value asc 18.6 / desc 1.6. ▲/▼ render as TOFU — ASCII ^/v used (the babel Han-fallback bug reaching into UI chrome)."}
    filter_latency: {rating: assembled, evidence: observed, note: "1-char 1.5 ms, 4-char 6.1 ms typed via CGEvent — under a frame at 100k."}
    column_resize: {rating: hand-rolled, evidence: synthetic-input, note: "Custom DragHandle widget (capture_pointer + window-coord dx); scripted 80 px drag verified. Predicted weakest cell — needed a full custom widget but works."}
    row_selection: {rating: hand-rolled, evidence: synthetic-input, note: "Stock views can't report modifier keys; RowFrame reads PointerState.modifiers. Click + shift-range + cmd-toggle verified."}
    cell_custom_render: {rating: assembled, evidence: observed, note: "Chips compose from stock sized_box(label).background_color().corner_radius(); interactive cells would need masonry widgets."}
fetch:
  build_ok: true
  launch_ok: true
  loc_production: 616     # single main.rs, ZERO custom widgets
  loc_verification: 166
  helper_crates: [reqwest (no-TLS +json), serde, "tokio (direct dep only for select!/pin! macros)"]
  ratings:
    async_integration: {rating: built-in, evidence: observed, note: "xilem creates its own tokio Runtime; stock worker_raw spawns long-lived tasks with UnboundedSender in, MessageProxy out. Three workers under one fork."}
    http_client_choice: {rating: assembled, evidence: observed, note: "reqwest riding xilem's bundled tokio. TRAP: xilem re-exports tokio without `macros`, so select! needs a direct tokio dep."}
    debounce_stale: {rating: assembled, evidence: observed, note: "Worker-side select! debounce (5 keystrokes → 1 SEARCH_ISSUE) + real in-flight cancellation. The aborted query left no post-delay SEARCH line; because the server logs only after sleeping, that proves it did not complete to the log point, not that no request arrived."}
    progress_streaming: {rating: assembled, evidence: observed, note: "chunk() loop → proxy per chunk → stock progress_bar; mid-run screenshot at 41%."}
    cancellation_real: {rating: assembled, evidence: observed, note: "Cancel breaks the select! arm, dropping the pinned Response future → TCP close; no AbortHandle needed. Server: 'ABORT /download after 33/64 chunks' past the noted offset."}
    error_retry_ux: {rating: assembled, evidence: observed, note: "FlakyState enum; manual retry; server attempt=30 (global counter, matches egui's finding)."}
```

SURPRISES:
- Grid's riskiest requirement (100k virtualization) was the one thing xilem gives away FREE (virtual_scroll is a stock view); everything table-shaped around it (headers, sort clicks, selection, resize) needed hand-rolled masonry widgets — the exact INVERSE of egui, where the table is free and nothing needed custom widgets.
- Fetch needed zero custom widgets — the only xilem app in the suite where the stock view set sufficed; the tokio-native model makes real abort the DEFAULT (drop the future) rather than an achievement.
- The aborted search left no post-delay `SEARCH` line. That corroborates
  cancellation before handler completion, but the server's log placement does
  not prove the request never arrived.
- The server's flaky counter is effectively shared (attempt=30, matches egui).

TIME_SINK:
- Hitting the in-flight search-abort window with synthetic keystrokes (<300 ms window; 262 ms spacing landed it).
- Full scripted UI verification of two GUI apps via CGEvent — most of the session; the apps themselves needed zero fixes.
- Reconstructing "where the time went" with the original transcript lost.

## Freya — Peek + Grid + Fetch (2026-08-03 re-implementation)

These rows describe the apps re-implemented from scratch on 2026-08-03. They
SUPERSEDE the 2026-08-02 Freya extension rows that previously sat here: those
crates were never retained, and LoC, helper crates and several ratings differ
materially (conflicts listed at the end of this section). The generated
`iter4-canonical-builds` table above still carries the PRE-re-implementation
freya-grid/freya-fetch/freya-peek build rows; it is machine-regenerated and
must not be hand-edited, so treat its Freya numbers as stale until the next
measurement pass.

```yaml
framework: freya
version: "=0.4.0"
cohort: 2026-08-03-expansion
canonical_measurement: pending_new_cohort
evidence_scope: "macOS 26.5.2 (M4 Pro, rustc 1.96.1) release builds, clean and reproducible under --locked; GRID_SELFTEST and FETCH_SELFTEST scripted passes with retained selftest-log.txt/selftest-err.txt; a PEEK_SELFTEST camera+mic run; synthetic CGEvent input and window-scoped screenshots"
peek:
  build_ok: true
  launch_ok: true          # PEEK_SELFTEST=1 run, camera + mic auto-started, one beep at t≈2 s, 1 Hz status lines; screenshots of all three tabs
  loc_total: 585
  loc_production: 510
  loc_verification: 75
  helper_crates: ["cpal =0.17.3", rodio 0.22.2, async-io 2.6.0]   # nokhwa owned by freya-camera; image not needed (ImageViewer decodes); no tokio
  camera_fps_observed: "presented 29–31, steady 30 (counted in a use_side_effect on camera.frame, so only frames that reached the reactive graph) at a negotiated 1920x1080 @ 30 fps, over a 17 s run"
  cpu_pct_at_preview: "27.3–31.4% of one core with camera + mic + 1 Hz logging, RSS 282 MiB (10 × ps at 1 Hz); camera stopped / gallery tab: 0.4–1.9%, RSS 208–224 MiB"
  permission_outcome: "freya::camera::init() logged camera-permission: granted=true; NO prompt fired (the grant already existed for the responsible process, as for the other ports on this host). Denial branch exists by construction (CameraViewer::error_renderer + camera.error) but is unexercised. The unbundled binary never crashed."
  gallery_completion: "200 JPEGs (3.2 MiB) render immediately and scroll cleanly to the last partial row"
  ratings:
    camera_pipeline: {rating: built-in, evidence: observed, note: "Freya's `camera` feature re-exports freya-camera, a real first-party integration: use_camera(CameraConfig::default) spawns the nokhwa capture thread, converts each frame to RGBA, builds a Skia ImageHandle and pushes it into a State<Option<ImageHandle>>; CameraViewer::new(camera) renders it with loading_placeholder and error_renderer hooks. The app writes NO capture thread, NO frame slot and NO texture upload — only the fps counter and the Start/Stop toggle. A GUI framework that ships a camera integration is unique in this study."}
    camera_permission_behavior: {rating: built-in, evidence: "observed (grant path); unexercised (denial)", note: "freya::camera::init() is a documented one-liner for main that blocks on the AVFoundation prompt and returns the answer."}
    mic_meter: {rating: assembled, evidence: observed, note: "Freya covers nothing here. cpal 0.17.3 input stream on its own std::thread (cpal::Stream is !Send, so it cannot live in component state; the thread parks and drops the stream when an AtomicBool flips). Callback stores buffer RMS in an AtomicU32; a 20 Hz async-io Timer loop on Freya's executor copies it into a signal and maps to −60..0 dBFS behind a stock ProgressBar. Real ambient audio: rms 0.00114–0.00387, 9,364 callbacks over ~100 s (~94/s = 512-sample buffers @ 48 kHz). ~60 LoC — the largest single block of the app and the only hardware plumbing Freya does not own."}
    audio_playback: {rating: assembled, evidence: self-test, note: "rodio 0.22.2 DeviceSinkBuilder::open_default_sink() → mixer().add(SineWave 880 Hz, 180 ms, 0.10) on a dedicated thread with a 280 ms keep-alive (dropping the MixerDeviceSink stops playback, which rodio even warns about on stderr). beeps=1, beep_err=\"\" from t=2 onward; audibility not independently confirmed."}
    thumbnail_grid: {rating: built-in, evidence: observed, note: "ImageViewer::new(ImageSource::Path(p)) does async load, decode TO THE LAYOUT SIZE (DecodeMode::FromLayout), caching and error/loading states, inside a VirtualScrollView of 6-wide rows so only visible rows mount. No spawn_blocking, no semaphore, no decode cache in app code — the iced port needed all three."}
    texture_upload_cost: {rating: "assembled by the framework (full re-upload per frame)", evidence: "self-test (CPU/RSS) + source-only (mechanics)", note: "ImageHandle::from_rgba wraps a raster SkImage; handles are immutable with no update-in-place or dirty-rect API, so a live preview costs per frame: YUV→RGBA convert + an 8.3 MiB Bytes allocation inside freya-camera + a new SkImage + a State write that dirties the tree + a FULL-TREE repaint (render_pipeline.rs still carries `// TODO: Use incremental rendering`). Switching tabs stops capture entirely, because use_camera is owned by the tab's component scope."}
grid:
  build_ok: true
  launch_ok: true          # GRID_SELFTEST=1 release run, exit 0, SELFTEST DONE pass=14 fail=0; plus an interactive run driven with synthetic CGEvent scroll
  loc_total: 724
  loc_production: 600
  loc_verification: 124
  helper_crates: [async-io 2.6.0]   # verification only (Freya's executor has no timer); no table/virtualization/PRNG crate — 10-line xorshift*
  build_ms: 13.01           # retained log; FRICTION quotes 12.8–13.0 across runs
  filter_ms_1char: 5.15
  filter_ms_4char: 7.59     # 2-char 7.82, 3-char 10.22, clearing back to 100k 0.52
  rss_after_load_mib: 106.1
  rss_after_long_scroll_mib: 107.2   # after 400 wheel ticks, landing on row ~10,400
  dataset_cross_check: "byte-identical to the iced and floem ports (same xorshift* seed and draw order): SORT name asc first_id=70664, name desc first_id=28613, id asc first_id=0"
  ratings:
    table_widget: {rating: assembled, evidence: "source-only + observed", note: "Freya ships a Table family (Table/TableHead/TableBody/TableRow/TableCell plus a TableArrow sort indicator and column_widths) but it is a LAYOUT HELPER: you hand it one element per cell, so at 100k×6 it would materialise 600k elements. Rejected after reading freya-components/src/table.rs. The grid is a header rect row plus a VirtualScrollView body with the same visual result. A name that will mislead someone into a 600k-element layout."}
    virtualization: {rating: built-in, evidence: "self-test + observed", note: "VirtualScrollView::new_controlled(builder, controller).length(n).item_size(26.) calls the builder ONLY for rows in the viewport — no spacer arithmetic, no scroll-offset bookkeeping (the iced port hand-rolled all of it). Self-test: programmatic scrolls to y=800/260000/0 produced WINDOW first=30/10000/0, where `first` is recorded INSIDE the item builder, so it is the row really built rather than a computed guess. It also takes a ScrollController, so a test can scroll it programmatically."}
    sort: {rating: "built-in (interaction) / hand-rolled (logic)", evidence: self-test, note: "Header cells are ordinary rects with .on_press; the comparator, asc/desc toggle and ▲/▼ indicator are ~40 LoC of app code. SORT name asc 23.12 ms, name desc 0.92 ms, id asc 2.54 ms at 100k. AccessibilityRole::ColumnHeader is a one-liner on the same element."}
    filter_latency: {rating: built-in, evidence: self-test, note: "Input writes a State<String>; a use_side_effect re-derives the visible set and prints FILTER_MS. Wiring through an effect means the initial run must be skipped explicitly or startup prints a spurious FILTER_MS 0."}
    column_resize: {rating: hand-rolled, evidence: "self-test + source", note: "A 7 px divider rect after each header takes on_pointer_down to arm; a root-level on_global_pointer_move streams cursor x while armed and on_global_pointer_press commits (~30 LoC). Freya's ResizableContainer splits a container into panels — it is not a column-width mechanism. Self-test drives the same functions: RESIZE col=id width=125. Also: rect has no cursor_icon property, so the col-resize cursor is set imperatively with Cursor::set from on_pointer_enter/leave."}
    row_selection: {rating: "assembled + workaround", evidence: self-test, note: ".on_press per row with plain / shift-range / cmd-toggle semantics. WORKAROUND: PressEventData carries NO modifier state (MouseEventData has global_location, element_location, button and nothing else), so a root-level on_global_key_down/up pair mirrors live Modifiers into a signal — the same shape iced needed. SELECT count=1 clicked_id=5, count=4 clicked_id=8 (shift range), count=1 clicked_id=2."}
    cell_custom_render: {rating: built-in, evidence: observed, note: "A cell is just an element: rect().background(..).rounded_full().child(label()) with per-status colours; green Ok / amber Warn / red Err pills verified in the screenshots."}
fetch:
  build_ok: true
  launch_ok: true          # FETCHER_PORT=7879 FETCH_SELFTEST=1 release run, exit 0, SELFTEST DONE pass=10 fail=0, against a purpose-started server whose log was captured
  loc_total: 654
  loc_production: 520
  loc_verification: 130
  helper_crates: ["reqwest =0.12.24 (json+stream, no default features)", "tokio 1 (rt-multi-thread, time)", "serde 1 (derive)", "futures-util 0.3"]
  live_server_probe: "full lifecycle on a private instance (port 7879): search + debounce/stale, streamed download, cancellation, and the /flaky retry cycle"
  cancellation_abort_line: "ts_ms=1785784540637 request_id=18 peer=127.0.0.1:52103 ABORT /download after 12/64 chunks; client DL_CANCELLED 1572864/8388608"
  flaky_reset_retry_trace: "FLAKY_ERR attempts=1, FLAKY_ERR attempts=2, FLAKY_OK attempts=3 server_attempt=3"
  ratings:
    async_integration: {rating: built-in, evidence: "self-test + observed", note: "THE BEST ASYNC ERGONOMICS OF THE COHORT. Freya has its own single-threaded executor (spawn(fut) -> TaskHandle, spawn_forever, use_future). Tokio interop is documented and is exactly Builder::new_multi_thread() + rt.enter() before launch, then keep using FREYA's spawn: futures are polled ON THE UI THREAD with the Tokio reactor available, so `self.results.set(results)` right after .await just works — no channel, no Send bound, no foreign-thread wakeup shim, no message enum. tokio::spawn is the thing you must not use (it needs Send), which the docs say plainly."}
    http_client_choice: {rating: assembled, evidence: source-only, note: "reqwest =0.12.24, default-features off + json + stream — no TLS for 127.0.0.1, and bytes_stream() is what makes progress plus real cancellation possible. Freya provides no HTTP client; it ships an optional `query` feature (freya-query, TanStack-Query-style cache) and a `remote-asset` feature for images, but neither is a general client."}
    debounce_stale: {rating: "hand-rolled (5 lines)", evidence: self-test, note: "No debounce helper (floem has one). spawn returns a TaskHandle, so it is: cancel the previous handle, spawn a task that sleeps 250 ms then requests. Because cancelling DROPS THE FUTURE at its await point, the pending request dies with it — stale protection IS real cancellation, not a sequence guard. SEARCH_QUEUED gen=2 q=\"co\" never reaches SEARCH_READY and the server records request_id=16 SEARCH_CANCEL, while gen=3 q=\"br\" completes; stale_seen stayed 0 because nothing survived to be discarded."}
    progress_streaming: {rating: assembled, evidence: "self-test + observed", note: "response.bytes_stream() + futures_util StreamExt::next(), writing Download::Running { received, total } straight into a signal per chunk; ProgressBar::new(percent) is stock. 90–96 monotonic DL_PROGRESS lines per run; interactively verified at 3.00 / 8.00 MiB — 37.5%."}
    cancellation_real: {rating: built-in, evidence: "self-test + server log", note: "TaskHandle::cancel() drops the task, which drops the Response mid-bytes_stream, which closes the TCP connection; the server proves it (ABORT after 12/64 chunks matched byte-exactly to the client's 1,572,864). The self-test also asserts no further progress arrives afterwards."}
    error_retry_ux: {rating: assembled, evidence: self-test, note: "/flaky maps onto a small Flaky enum; the button relabels itself 'Retry /flaky' while failed and the error text shows inline. Manual retry, no backoff."}
  sharp_edge: "`x.set(*x.peek() + 1)` PANICS — peek() returns a ReadRef which, as an argument temporary, is still alive while set() takes the write borrow, so the GenerationalBox refuses. Because Freya's release-mode panic hook shows a modal rfd dialog and calls exit(1) BEFORE chaining to the previous hook, this produced a hung window and an empty stderr; diagnosis needed a debug build plus an app-level set_hook. Two lines of code, ~20 minutes. The same shape sank the board app."
cohort_gaps:
  - "Canonical clean/incremental build seconds, unique crate counts and binary sizes for all three re-implemented apps (the generated table above predates them)."
  - "Peek denial/prompt paths for camera and microphone remain unexercised on this host (pre-existing responsible-process grant)."
  - "Audible confirmation of the rodio beep (self-test evidence only)."
```

SURPRISES:
- Freya is the only framework in the study that ships a CAMERA integration (use_camera + CameraViewer) and a stock VirtualScrollView, so two of the round's most expensive cells (camera pipeline, 100k-row virtualization) collapse to a component call each.
- Its async story is the inverse of every Elm-shaped framework here: a single-threaded executor on the UI thread means `await` then assign to a signal, and TaskHandle::cancel is simultaneously the debounce, the stale guard and protocol-level download cancellation.
- `Table` exists and is NOT a data grid — it is a layout helper that would materialise 600k elements at this scale. The virtualized body is a different component entirely.
- The framework has no timer primitive, no StreamExt, no debounce and no modifier state on press events; each gap is small, but every app in the cohort pays the async-io dependency for the timer.
- Release-mode panics become a modal "Fatal Error" dialog plus exit(1) with nothing on stderr, which converts trivial borrow mistakes into opaque hangs.

TIME_SINK:
- Reading freya-components' Table (to reject it) and VirtualScrollView's get_render_range, so the WINDOW assertion could be meaningful rather than a restatement of the test's own arithmetic.
- The peek()-temporary panic hidden behind the modal panic dialog, and designing the fetch self-test so the "stale" step really overlaps two requests (380 ms: past the 250 ms debounce, inside the server's 150–300 ms latency).
- The mic thread (!Send cpal::Stream) and rodio 0.22's renamed API surface — the only hardware plumbing Freya does not own.

CONFLICTS WITH THE 2026-08-02 FREYA ROWS (superseded):
- LoC: old peek 453+15, grid 338+31, fetch 426+115; today 510+75, 600+124, 520+130.
- Grid `column_resize` was recorded as **not-achievable** ("Table exposes no divider gesture; explicit minus/plus controls are the documented approximation"); today it is **hand-rolled** and works (7 px divider + global pointer move, RESIZE col=id width=125).
- Grid `table_widget` was **built-in** (Freya Table); today it is **assembled**, with Table explicitly rejected as non-virtualizing.
- Grid `row_selection` shift-range was "omitted"; today shift-range and cmd-toggle both work via a mirrored-modifiers workaround.
- Fetch `async_integration` was **assembled** with a manually owned Tokio runtime; today it is **built-in** (Freya's own executor with a Tokio reactor guard), and `cancellation_real` moves from assembled to built-in.
- Peek was almost entirely `source-only` with camera FPS/CPU/permission `pending`; today's peek has observed FPS, CPU/RSS samples and a granted-permission log line.

## Vizia — Peek + Grid + Fetch (2026-08-03 re-implementation)

These rows describe the apps re-implemented from scratch on 2026-08-03 and
SUPERSEDE the 2026-08-02 Vizia extension rows. The generated
`iter4-canonical-builds` table above still carries the pre-re-implementation
vizia-grid/vizia-fetch/vizia-peek build rows and is machine-regenerated; treat
its Vizia numbers as stale until the next measurement pass.

```yaml
framework: vizia
version: "=0.4.0"
cohort: 2026-08-03-expansion
canonical_measurement: pending_new_cohort
evidence_scope: "macOS 26.5.2 (M4 Pro, rustc 1.96.1) release builds, clean and reproducible under --locked; GRID_SELFTEST and FETCH_SELFTEST scripted in-app passes with retained selftest-log.txt/selftest-err.txt; a 19 s PEEK_SELFTEST run with 1 Hz status lines; CGEvent synthetic input scoped to this app's window with window-scoped screenshots"
peek:
  build_ok: true
  launch_ok: true          # camera + mic + gallery live, well past the 10 s bar, 19 one-second status lines retained in selftest.log
  loc_total: 808
  loc_production: 700
  loc_verification: 108
  helper_crates: ["nokhwa 0.10.11 (input-avfoundation)", "cpal =0.17.3", rodio 0.22.2, "image 0.25 (jpeg, synthetic-camera hook only)"]   # NO image/texture helper needed for the real path — vizia::vg exposes the renderer
  release_binary_mib: 22.3   # Skia statically linked
  camera_fps_observed: "30/30 captured/presented sustained at 1920x1080 @ 30 fps YUYV for the whole run (presented counted INSIDE draw(), so only frames actually blitted)"
  cpu_pct_at_preview: "real 1080p30 + mic: 29.0% of one core, RSS 278 MiB; synthetic 720p30 + mic (PEEK_FAKE_CAMERA): 15.6%, 119 MiB; gallery only: 2.6%, 124 MiB (10 × 1 s ps samples each)"
  permission_outcome: "No prompt in ANY run for camera or microphone — nokhwa_check() returned true at first launch and audio flowed within the first second (callbacks=74 at t=1). TCC grants for unbundled CLI binaries attach to the responsible process, so a grant made for one binary covers others launched the same way. Denial path renders an in-place status line by construction but never executed — unexercised."
  gallery_completion: "200 JPEGs (320x240, 3.2 MiB) registered in 3 ms across an 8-thread pool, streamed into the grid in batches of 25 with no UI stall"
  ratings:
    camera_pipeline: {rating: assembled, evidence: "observed + self-test", note: "nokhwa 0.10.11 on a dedicated std::thread blocking in Camera::frame(), YUYV→RGBA on that thread, swapped into a Mutex<Option<Frame>>. The UI side is where vizia is unusual: the renderer IS Skia and vizia::vg re-exports skia-safe, so the preview is a custom View whose draw() calls vg::images::raster_from_data(..) and canvas.draw_image_rect(..). There is NO framework image handle, no texture cache, no image feature flag and no encode step. Bridge ≈45 LoC."}
    camera_permission_behavior: {rating: assembled, evidence: "observed (grant); unexercised (denial)", note: "nokhwa_check()/nokhwa_initialize(cb) wrap AVAuthorizationStatus/requestAccess; the result lands in an AtomicI8 (-1/0/1) and is displayed and logged (perm=1)."}
    mic_meter: {rating: assembled, evidence: "observed + self-test", note: "cpal 0.17.3 input stream on its own thread (Stream is !Send and cannot live in vizia state; the thread owns it and parks on an mpsc until Stop). Callback stores buffer RMS in an AtomicU32; a 20 Hz cx.add_timer (50 ms, exactly SPEC-6's rate) maps it to −60..0 dBFS with fast-attack/slow-decay into two built-in ProgressBars. MacBook Pro Microphone (1 ch @ 48 kHz), RMS 0.0007–0.0124, ~93 callbacks/s, 1,759 callbacks over 19 s."}
    audio_playback: {rating: assembled, evidence: self-test, note: "rodio 0.22.2 DeviceSinkBuilder::open_default_sink() → mixer().add(SineWave 880 Hz, 180 ms, 0.10) on a background thread with a 280 ms keep-alive; beep_err=None in every self-test run. Audibility not independently verified. rodio 0.22 renamed the whole surface relative to the widely documented 0.17–0.19 examples."}
    thumbnail_grid: {rating: assembled, evidence: observed, note: "8-thread worker pool hands encoded JPEG bytes to ContextProxy::load_image(key, &bytes, Forever), which decodes through Skia and registers under a key; the grid renders Image::new(cx, key) in rows of 8 in a ScrollView. All 200 registered in 3 ms — honest only about REGISTRATION: skia_safe::Image::from_encoded is lazy, so pixel decode happens on first draw inside Skia. The caching story is entirely Skia's plus vizia's ResourceManager retention policy; there is no atlas/trim layer of vizia's own."}
    texture_upload_cost: {rating: "assembled (new raster image per frame)", evidence: "self-test + source-only", note: "Every presented frame allocates a fresh vg::Image over the RGBA buffer and issues one draw_image_rect; no update-in-place, no dirty-region API. Per frame: YUYV→RGBA CPU convert + 7.9 MiB Vec alloc + Data::new_bytes wrap + Skia's upload. The gap between the 29.0% real and 15.6% synthetic figures is the conversion plus the 2.25× larger buffer, not vizia. Side observation: when the Camera tab is not selected the view does not exist, so presented_fps drops to 0 while capture continues — Binding really does tear the view down."}
grid:
  build_ok: true
  launch_ok: true          # GRID_SELFTEST=1, SELFTEST DONE pass=14 fail=0; plus real CGEvent clicks/drags/scrolls verified from screenshots
  loc_total: 672
  loc_production: 500
  loc_verification: 172    # in-app cx.add_timer state machine, one step per 120 ms tick; no external driver script
  helper_crates: []        # vizia =0.4.0 default features only; 10-line xorshift* PRNG, packed-integer dates (no chrono)
  build_ms: 10.93          # retained log; FRICTION's headline quotes 10.67 from another run
  filter_ms_1char: 1.64
  filter_ms_4char: 7.70    # full range across runs 1.50–7.70; 2-char 3.25, 3-char 4.91, clear 2.70
  rss_after_load_mib: 139.6
  rss_after_long_scroll_mib: 290.0   # ~320 wheel events sweeping the whole 100k range — NOT flat
  ratings:
    table_widget: {rating: built-in, evidence: "observed + self-test", note: "vizia 0.4 ships VirtualTable — a REAL data grid, not a layout helper: a VirtualList body plus a header row of Resizable cells, with sort_state / sort_cycle / resizable_columns / selectable / selected_row_ids modifiers and on_sort / on_row_select callbacks. Columns are TableColumn::new(key, header_fn, cell_fn) with per-column width/min_width/sortable/resizable/hidden signals. The only framework in this cohort where SPEC-7's table is a view-constructor call."}
    virtualization: {rating: built-in, evidence: self-test, note: "VirtualList underneath: fixed item_height, a spacer VStack sized to num_items × item_height, and a recycled pool of ceil(viewport/item_height)+2 item views. Proven by instrumenting the CELL TEMPLATE itself — exactly 22 row indices at each of 12 scroll ratios from 8% to 100% (WINDOW ratio=1.00 first=99978 last=99999 cells_built=22 rows=100000). CAVEAT: RSS is NOT flat across a long scroll (140 → 290 MiB); row views are recycled but Skia's paragraph/glyph caches and vizia's per-entity style stores grow as new text is shaped — it plateaus rather than leaking linearly."}
    sort: {rating: built-in, evidence: "self-test + synthetic-input", note: "Header press → on_sort(cx, key, direction); the app owns the comparator, TableSortCycle::BiState gives asc↔desc and TableHeader renders the indicator. Retained log: SORT name asc first_id=40505 ms=18.62, name desc first_id=32705 ms=13.03, id asc first_id=0 ms=3.03 (FRICTION's prose quotes a faster 8.6/8.5/2.0 ms run; a real header click under synthetic input measured 11.3–12.1 ms including the widget round-trip)."}
    filter_latency: {rating: assembled, evidence: self-test, note: "Textbox::on_edit → recompute from scratch (substring filter, then re-apply the active sort), self-timed with Instant. An order of magnitude under a frame at 100k rows, so no debounce and no worker thread. Publishing is cheap: rows go to the table as Signal<Arc<[Row]>> (VirtualTable accepts any V: Deref<Target=[T]> + Clone), so handing over a new 100k-row view is a refcount bump."}
    column_resize: {rating: built-in, evidence: "synthetic-input + self-test", note: "Real divider drag, not an approximation: VirtualTable wraps every non-final header cell in vizia's Resizable view, which owns the handle and writes the column's width signal. Verified with a real 110 px CGEvent drag on the Name/Category divider (before/after screenshots) and asserted in the scripted run: RESIZE col=name width=220->310."}
    row_selection: {rating: "built-in + assembled", evidence: "synthetic-input + self-test", note: "Click selection is Selectable::Multi + selected_row_ids + on_row_select(cx, id); SHIFT-CLICK RANGE IS APP LOGIC — the callback reads cx.modifiers().shift() and the model expands from a stored anchor over the current view order. Real clicks: SELECT count=1 clicked_id=5 then shift-click → count=5 over [5,6,7,8,9]."}
    cell_custom_render: {rating: built-in, evidence: observed, note: "A cell is an arbitrary view tree built by a closure receiving Memo<Row>, so the status chip is a styled Label with toggle_class(\"ok\"/\"warn\"/\"err\"). Minor trap: the chip stretches to the column width unless given an explicit width (auto-width plus padding under-measures and clips the last glyph)."}
  the_one_real_trap: "VirtualTable's OWN row and cell wrappers eat the click that selects the row. It wraps each row in an HStack classed `table-row` and each cell in a VStack classed `table-cell`, both hoverable by default, so the hover target is a DESCENDANT of the ListItem carrying on_press → ListEvent::Select; vizia only fires an action when cx.current == meta.target, so row clicks silently did nothing. The fix is one CSS rule — `.table-row, .table-cell { pointer-events: none; }` — inside a BUILT-IN widget whose offending views are reachable only through undocumented CSS class names. Related: VirtualTable exposes no scroll position or on_scroll, so the virtualization evidence had to be gathered by instrumenting the cell template with atomics (VirtualList's content closure must be Copy and cannot capture an Rc)."
fetch:
  build_ok: true
  launch_ok: true          # driven with real typing/clicks against the shared fetcher-server on 7878; FETCH_SELFTEST run exits 0, SELFTEST DONE pass=10 fail=0. RSS at idle after a search + download: 107.6 MiB
  loc_total: 800
  loc_production: 620
  loc_verification: 180    # in-app 100 ms tick state machine with explicit wait(predicate, timeout) steps, so a hang reports SELFTEST TIMEOUT and can never be mistaken for a pass
  helper_crates: ["tokio 1 (rt-multi-thread, time, macros)", "reqwest =0.12.24 (json+stream, no default features)", "futures-util 0.3", "serde 1 (derive)"]
  live_server_probe: "full lifecycle on the shared server: debounce, stale-on-the-wire, streamed download, cancellation and a /flaky/reset retry cycle"
  cancellation_abort_line: "ts_ms=1785784304683 request_id=49 peer=127.0.0.1:52084 ABORT /download after 12/64 chunks; client DL_CANCELLED 1572864/8388608 (= exactly 12 × 128 KiB)"
  flaky_reset_retry_trace: "FLAKY_ERR attempts=1, FLAKY_ERR attempts=2, FLAKY_OK attempts=3 server_attempt=3, matched by the server's FLAKY attempt=1->500 / 2->500 / 3->200 after an explicit GET /flaky/reset"
  ratings:
    async_integration: {rating: hand-rolled, evidence: observed, note: "VIZIA HAS NO EXECUTOR AT ALL. Its entire async story is cx.spawn(|proxy| ..), which starts a raw std::thread and gives you a ContextProxy whose emit posts an event through winit's user-event proxy. No Task, no subscription, no runtime feature to pick — so the app builds its own tokio Runtime (2 workers, enable_all) and moves a ContextProxy clone into each task. That works because ContextProxy: Send; it is NOT Sync, so every task needs its own clone. Upside: there is no hidden executor mismatch — the iced trap where the default executor has no reactor and reqwest panics at runtime cannot happen here."}
    http_client_choice: {rating: assembled, evidence: observed, note: "reqwest 0.12.24, default-features off + json + stream (no TLS for localhost). Chosen because the app already owns a tokio runtime, bytes_stream() gives progress plus mid-body cancellation, and Client is Clone. ureq + threads would have needed a hand-rolled cancellation channel and would not have produced a real ABORT."}
    debounce_stale: {rating: assembled, evidence: "self-test + server log", note: "One mechanism does both: every keystroke abort()s the previous JoinHandle and spawns sleep(250 ms) → reqwest. Abort during the sleep is the debounce; abort after it drops the in-flight future, which is protocol-level cancellation. DEBOUNCE PROOF: five keystrokes a/am/amb/ambe/amber produced SEARCH_QUEUED gen=1..5 and exactly ONE server SEARCH_START q=\"amber\". STALE PROOF: typing `mossy` (297 ms server delay) then `prism` mid-flight produced the server pair SEARCH_START q=\"mossy\" … SEARCH_CANCEL — the older request killed ON THE WIRE — followed by SEARCH_START q=\"prism\" / SEARCH_DONE."}
    progress_streaming: {rating: assembled, evidence: "self-test + synthetic-input", note: "bytes_stream() + proxy.emit(DownloadProgress(received,total)) per chunk. Each emit wakes the vizia event loop through winit's user-event proxy, so there is NO POLLING — the one place vizia's thread+proxy model is strictly nicer than a channel drained by a timer. 73–77 DL_PROGRESS lines for the 8 MiB body. Structural note: the panel binds Memos of the state with a Binding only on the state KIND, so 77 progress events update two signals instead of rebuilding 77 view trees."}
    cancellation_real: {rating: assembled, evidence: "self-test + server log", note: "JoinHandle::abort() drops the in-flight reqwest::Response, closing the TCP connection; server ABORT line and client byte count are the same transfer."}
    error_retry_ux: {rating: assembled, evidence: self-test, note: "Four-state enum (Idle/Running/Failed/Succeeded), red error text, manual Retry per spec (no auto-backoff). The scripted run calls GET /flaky/reset first so the phase is deterministic on the shared server."}
cohort_gaps:
  - "Canonical clean/incremental build seconds, unique crate counts and binary sizes for all three re-implemented apps."
  - "Camera/microphone prompt and denial paths remain unexercised (pre-existing responsible-process grant)."
  - "Audible confirmation of the rodio beep (self-test evidence only)."
  - "The 140 → 290 MiB grid RSS growth is characterised as cache growth that plateaus; no long-soak series was retained to bound it."
```

SURPRISES:
- vizia is the only framework in the study with a genuine virtualized, sortable, resizable, selectable table widget in core — iced's `table` is O(rows × cols) widgets and egui/xilem/gpui/floem all hand-roll windowing.
- The renderer being Skia AND being EXPOSED (vizia::vg re-exports skia-safe) is why the camera app is short: raw RGBA goes straight into a raster image in a custom View::draw with no framework texture API in the way — and the two obvious entry points (Context::load_image, ContextProxy::load_image) both point away from the path that actually works for video.
- `Model` has no Send bound and models are visited before views on the same entity, which is what makes the tray/close-to-tray/`!Send` handles story clean.
- The framework is unusually silent about mistakes: press handlers on containers, event coordinates used as layout coordinates, and on_drop ordering all compile and do nothing visible. The pointer-events trap even lives inside a built-in widget.
- 100k-row synchronous filter+sort inside Model::event is a non-issue (≤8 ms), so no async, no incremental index, no debounce.

TIME_SINK:
- The pointer-events trap: row selection failing silently inside a built-in widget with no way to see why except reading VirtualTable's source.
- Designing falsifiable virtualization evidence ("it scrolls fast" is not evidence; "22 of 100,000 cells were materialised" is).
- Deciding how futures reach the UI thread (the Send-but-not-Sync shape of ContextProxy is the whole design constraint and is undocumented), and keeping 77 progress events from rebuilding the UI 77 times.
- Deciding the camera frame→screen path: ContextProxy::load_image is the obvious API and is wrong for video (it takes ENCODED bytes); the right answer is in no vizia example.

CONFLICTS WITH THE 2026-08-02 VIZIA ROWS (superseded):
- The old rows carried no LoC at all for grid/fetch/peek; today's are 672/800/808 total with explicit production/verification splits.
- Grid `sort` was **built-in** with a unit-tested comparator; today it is still built-in but backed by real header-click timings and full-vector monotonicity assertions.
- Grid `row_selection` was **built-in** with "Shift-range not exposed by the stable callback and omitted"; today shift-range works as app logic on top of the built-in callback (**built-in + assembled**).
- Fetch `async_integration` was **assembled** ("a dedicated two-worker Tokio runtime … because this Vizia path has no tagged future executor"); today it is rated **hand-rolled**, because vizia supplies nothing above a raw thread + proxy.
- Peek `camera_pipeline`/`texture_upload_cost` were **hand-rolled** with runtime cost pending; today they are **assembled** with 30/30 fps and 29.0% CPU / 278 MiB measured.
- Peek `thumbnail_grid` was a four-worker queue; today it is an eight-thread pool with a 3 ms registration figure and the Skia-lazy-decode caveat.
- All the old cancellation evidence (request_id=1, ABORT after 13/64 chunks, 1703936 bytes) belongs to the discarded implementation; today's retained trace is request_id=49, ABORT after 12/64 chunks, 1572864 bytes.

## Floem — Peek + Grid + Fetch (new framework, 2026-08-03)

```yaml
framework: floem
version: "git-778bb5f2"   # rev 778bb5f2aa08429e579ee2e6ac97e84fbf18b618 of lapce/floem main (2026-06-21), pinned identically in all 8 floem apps
version_deviation: "crates.io floem 0.2.0 (2024-11) is 20 months stale with a substantially different API, and `main` is UNPUBLISHABLE because it depends on the forked floem-winit and on understory_* crates via git. The maintainers direct users to main, so the git rev is the maintainer-recommended path; this deviation from the SPEC's =x.y.z rule is itself a research finding. Consequence absorbed by every floem app: the free-function view constructors used by ALL published documentation (v_stack, h_stack, button, label, text_input) are DEPRECATED at this rev in favour of struct constructors."
cohort: 2026-08-03-expansion
canonical_measurement: pending_new_cohort
evidence_scope: "macOS (M4 Pro, rustc 1.96.1) release builds, clean and reproducible under --locked; GRID_SELFTEST and FETCH_SELFTEST scripted passes with retained selftest-log.txt/selftest-err.txt; a PEEK_SELFTEST camera+mic run with 1 Hz status lines and window-scoped screenshots"
peek:
  build_ok: true
  launch_ok: true          # window-scoped screenshots show a LIVE camera frame, live VU bar and the full thumbnail grid
  loc_total: 947
  loc_production: 790
  loc_verification: 157
  helper_crates: ["nokhwa 0.10.11 (input-avfoundation)", "cpal =0.17.3", rodio 0.22.2, "image 0.25", "floem_renderer (same git rev — the Img struct for draw_img is NOT re-exported by floem, so raw-RGBA drawing needs a direct dep on floem's internal crate)", "raw-window-handle 0.6 + objc2 0.6 (verification-only screenshot hook)"]
  release_binary_mib: 19.9
  camera_fps_observed: "30 fps captured steady state; the paint counter reads ~48/s because floem repaints the canvas whenever the window repaints (20 Hz VU updates interleave with 30 Hz frames). Negotiated 1920x1080 @ 30 fps YUYV, but frames MUST be downscaled to ≤320x180 before display — see texture_upload_cost"
  cpu_pct_at_preview: "camera + mic + logging: 28.6% of one core, RSS 286 MiB (includes the 1080p YUYV→RGBA convert and the mandatory downscale); idle with gallery loaded: 0.0% CPU, 109 MiB"
  permission_outcome: "nokhwa_check() returned true at first launch — the TCC grant attached to the terminal host app by earlier runs persists across unbundled binaries (same finding as iced-peek). The prompt path (nokhwa_initialize callback → AtomicI8 → ExtSendTrigger → Perm signal) is implemented but never fired; denial branch unexercised. No crash anywhere; errors surface in-UI."
  gallery_completion: "200 JPEGs decoded and downscaled in 30–58 ms across runs on a hand-rolled 8-thread pool"
  ratings:
    camera_pipeline: {rating: "assembled (with a mandatory workaround)", evidence: observed, note: "Capture side is identical to iced-peek (nokhwa blocking in Camera::frame() on a std thread, YUYV→RGBA there). Presentation is floem-specific: the frame goes into a Mutex slot + ExtSendTrigger → an Effect bumps a frame_rev signal → a signal-tracked canvas paint closure calls Renderer::draw_img with raw RGBA. ~110 LoC of bridge code, plus the ≤320x180 downscale the atlas forces."}
    camera_permission_behavior: {rating: assembled, evidence: "observed (grant); unexercised (prompt/denial)", note: "nokhwa's AVFoundation check/request wired through ExtSendTrigger into a Perm signal."}
    mic_meter: {rating: assembled, evidence: observed, note: "cpal 0.17.3 input stream on its own thread (Stream is !Send), RMS into an AtomicU32; a 20 Hz exec_after chain maps to dBFS and drives a HAND-STYLED VU bar plus a slow-decay peak bar — floem has no progress-bar widget. Real ambient audio: rms 0.001–0.009, ~94 callbacks/s (512-sample buffers @ 48 kHz)."}
    audio_playback: {rating: assembled, evidence: self-test, note: "rodio 0.22.2 (DeviceSinkBuilder → mixer → 880 Hz SineWave, 180 ms, 0.10 amplitude, 280 ms keep-alive) on a plain std thread; the result crosses back via create_ext_action. `beep ok (×1)` in UI and log; audibility not independently verified."}
    thumbnail_grid: {rating: assembled, evidence: observed, note: "200 JPEGs decoded + downscaled (image 0.25 thumbnail(100,75)) on a hand-rolled 8-thread pool — floem has NO executor or blocking pool — funnelled through a queue + ExtSendTrigger, with one RwSignal<Option<Thumb>> per cell so each arriving thumb repaints only its own canvas. Grid is a flex-wrap dyn_stack in a scroll (min_height(0) load-bearing again). CACHING CAVEAT: each thumb is one content-hash entry in vger's colour atlas — cached across frames, BUT evicted wholesale whenever anything (e.g. the camera stream) forces an atlas clear, then silently re-uploaded."}
    texture_upload_cost: {rating: "not-achievable at full resolution", evidence: "observed + source-only", note: "HEADLINE. vger's image path is a single colour ATLAS keyed by CONTENT HASH (vger::render_image → GlyphCache::get_image_mask), so a video stream is a new hash and a new atlas region per frame. Two fatal interactions, read from floem-vger-rs glyphs.rs/atlas.rs: (1) the atlas only self-heals (full clear) when tracked usage crosses 70%, but (2) a region that fails to pack is dropped SILENTLY, without cleanup, and does not count toward usage. Net effect: any frame bigger than ~⅓ of the atlas dimension fragments the packer below the clear threshold and image drawing WEDGES PERMANENTLY — a 1080p (and even 640x360) preview goes black after ~3 frames while everything else keeps running. Shipped workaround: downscale every frame to ≤320x180 on the camera thread, which keeps a pack-fail-free cycle (≈13 packs → 70% → clear → repeat). Cost of that steady state: full re-upload every frame plus a whole-atlas clear every ~13 frames, which also evicts every gallery thumbnail and (on resize) glyphs. A second bug surfaced during bisection: cached rects are not invalidated on clear for reused hashes, so a constant-hash source draws a garbled tile."}
grid:
  build_ok: true
  launch_ok: true          # plain launch alive >8 s; GRID_SELFTEST=1 exits SELFTEST DONE pass=14 fail=0
  loc_total: 667
  loc_production: 547
  loc_verification: 120
  helper_crates: []        # 10-line xorshift* PRNG; sorting/filtering are std
  build_ms: 13.49
  filter_ms_1char: 4.60
  filter_ms_4char: 7.02    # 2-char 5.74, 3-char 9.00, clear 0.66
  rss_after_load_mib: 116.0
  rss_after_long_scroll_mib: 116.0   # unchanged ±1 MiB after a scripted 260,000 px jump-scroll
  dataset_cross_check: "byte-identical to the iced and freya ports: SORT name asc first_id=70664, name desc first_id=28613, id asc first_id=0"
  ratings:
    table_widget: {rating: assembled, evidence: observed, note: "floem has NO table widget. The grid is taffy flex rows plus a header row; column widths live in one RwSignal<[f64;6]> read by every cell's reactive style closure."}
    virtualization: {rating: built-in, evidence: "self-test + observed", note: "VirtualStack (understory_virtual_list) is real windowed virtualization — only ~40 row views exist, 100k rows sit at ~116 MiB, scrolling is instant. HEADLINE TRAP: without min_height(0) on the scroll's flex chain, taffy sizes the scroll to its 2.6-million-px min-content height, the clip never applies, the VirtualStack sees viewport == content and MATERIALIZES ALL 100k ROWS — 16 GiB RSS, 100% CPU shaping labels for minutes, NO WINDOW EVER APPEARS, and the event loop never goes idle so timers never fire. Nothing warns; the fix is one obscure style line. Diagnosis needed sample(1) stack dumps plus a view-creation counter (~1.5 h, the biggest line item in the app); the same pathology had already inflated floem-babel to 1.9 GiB."}
    sort: {rating: assembled, evidence: self-test, note: "Header Click listeners toggle asc/desc with a derived ▲/▼ label. Sorting 100k u32 indices: SORT name asc ms=12.22 first, then name desc 0.82 and id asc 2.04."}
    filter_latency: {rating: assembled, evidence: self-test, note: "Full recompute including re-sort. Filter-as-you-type is an Effect tracking the TextInput's buffer signal — no change-callback needed."}
    column_resize: {rating: hand-rolled, evidence: "self-test + observed", note: "7 px divider strips: PointerDown + cx.request_pointer_capture keeps PointerMove flowing outside the strip; width = start + Δx into the widths signal and every cell re-styles reactively. Pointer capture made this notably cleaner than iced's global-subscription dance (~30 LoC). Self-test drives the same math: RESIZE col=id width=125."}
    row_selection: {rating: assembled, evidence: self-test, note: "PointerDown carries modifier state (event.state.modifiers), so shift-range and cmd-toggle need NO global modifier tracking — iced and freya both needed a permanent modifier subscription. SELECT count=4 = a shift-range of 4."}
    cell_custom_render: {rating: assembled, evidence: observed, note: "The status chip is just a Label styled with background and rounded corners; any view can be a cell, no cell-renderer API needed."}
fetch:
  build_ok: true
  launch_ok: true          # plain launch alive >8 s; FETCH_SELFTEST=1 against the local axum server on 7878 exits SELFTEST DONE pass=10 fail=0
  loc_total: 711
  loc_production: 510
  loc_verification: 200
  helper_crates: ["reqwest =0.12.24 (json+stream, no default features — same pin as iced-fetch)", "tokio 1 (rt-multi-thread, time)", "serde 1 (derive)", "futures 0.3"]
  live_server_probe: "full lifecycle: debounce/stale with a server-side SEARCH_CANCEL, an 8 MiB streamed download, cancellation, and the /flaky retry cycle after reset"
  cancellation_abort_line: "ts_ms=1785782892763 request_id=20 peer=127.0.0.1:51969 ABORT /download after 12/64 chunks; client DL_CANCELLED 1572864/8388608, with zero further progress events in the following 700 ms"
  flaky_reset_retry_trace: "FLAKY_ERR attempts=1, FLAKY_ERR attempts=2, FLAKY_OK attempts=3 server_attempt=3"
  ratings:
    async_integration: {rating: assembled, evidence: self-test, note: "floem ships NO executor. The upstream tokio-timer example blesses the pattern used here: build a tokio multi-thread Runtime, then block_on(block_in_place(floem::launch(..))), after which tokio::spawn works from any UI closure. Futures never touch the UI thread; results come back via create_ext_action (one-shot) and update_signal_from_channel (streams), both built on floem's ExtSendTrigger foreign-thread wakeup. Clean, but 100% assembly-required and documented only by example."}
    http_client_choice: {rating: assembled, evidence: self-test, note: "reqwest =0.12.24 (same pin as iced-fetch), default-features off + json + stream. Chosen because the tokio runtime is already there."}
    debounce_stale: {rating: "built-in + assembled", evidence: self-test, note: "The 250 ms debounce is a floem BUILT-IN: debounce_action(query_signal, 250 ms, cb) — no timer bookkeeping at all, unique among the frameworks tested. Stale protection is real cancellation: each start_search aborts the previous tokio task via AbortHandle, dropping the in-flight reqwest future. Server proof of a mid-flight abort: request_id=17 … SEARCH_CANCEL. The generation counter never saw a stale response (stale=false on every SEARCH_READY; stale_seen == 0)."}
    progress_streaming: {rating: assembled, evidence: self-test, note: "bytes_stream() chunks → std mpsc → update_signal_from_channel → Download::Running signal → reactive progress bar. ~120 monotonic DL_PROGRESS lines over the 8 s stream. PAPERCUT: floem has NO progress-bar widget — the bar is two nested styled views with a reactive width_pct."}
    cancellation_real: {rating: assembled, evidence: "self-test + server log", note: "AbortHandle::abort() drops the reqwest Response mid-stream → TCP close, matched byte-exactly to the server's ABORT after 12/64 chunks."}
    error_retry_ux: {rating: assembled, evidence: self-test, note: "Non-2xx surfaced as an error state with a Retry button (manual retry per spec); deterministic cycle after /flaky/reset."}
cohort_gaps:
  - "Canonical clean/incremental build seconds, unique crate counts and binary sizes for all three floem apps (the generated table above has no floem rows at all)."
  - "Camera/microphone prompt and denial paths unexercised (pre-existing responsible-process grant)."
  - "Audible confirmation of the rodio beep (self-test evidence only)."
  - "The vger atlas wedge is characterised from source plus a synthetic-source bisection at 1080p/640x360/320x180; no upstream minimized reproduction has been filed."
```

SURPRISES:
- The taffy min-content trap is the most dangerous finding of the round: one missing `min_height(0)` silently converts floem's real virtualization into full materialization — 16 GiB RSS and NO WINDOW at 100k rows, 1.9 GiB at 11k lines — with no warning anywhere.
- vger's content-hash image atlas makes a video preview NOT-ACHIEVABLE above ~320x180: pack failures are dropped silently and don't count toward the 70% self-heal threshold, so the image path wedges permanently after ~3 frames. Everything else in the app keeps running, which is what makes it hard to attribute.
- `debounce_action` is a framework built-in — the only one in the study — and `ExtSendTrigger`/`create_ext_action`/`update_signal_from_channel` give floem the cleanest foreign-thread→UI bridge of any framework here, despite floem shipping no executor at all.
- PointerDown carrying modifier state removes the permanent modifier-tracking subscription that iced and freya both needed, and request_pointer_capture makes column-resize drags ~30 LoC.
- floem has no table widget, no progress-bar widget, no window-capture API and no AccessKit in the dependency tree at all.

TIME_SINK:
- The min-content/virtualization trap (~1.5 h in grid alone, after it had already inflated babel).
- The atlas-wedge diagnosis: synthetic-source bisection at three resolutions plus reading floem-vger-rs' packer, ~1.5 h for a 30-LoC workaround.
- Choosing the crossing-back mechanism per shape (one-shot completions → create_ext_action; streams → update_signal_from_channel, which even disposes itself when the sender drops on abort) — both undocumented outside source and examples.
- API archaeology against a moving `main`: the documented constructor surface is deprecated at this rev and in-source doc examples do not compile against it.
