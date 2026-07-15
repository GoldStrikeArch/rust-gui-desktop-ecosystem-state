# FRICTION — Peek (iced =0.14.0)

Reference: SPEC-6.md. Built + verified on macOS (M4 Pro, rustc 1.96.1):
`cargo build --release` clean; binary launched with camera + mic + gallery
live and stayed alive 66 s before being killed by the harness (well past the
10 s bar) — **observed**. Screenshot of the running app with a live camera
frame: `screenshot.png` (dark-room frame: single ceiling light + operator
silhouette — the run happened at ~01:30 local, the picture is real, just
dark).

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| camera_pipeline | **assembled** | **observed** | `nokhwa 0.10.11` (`input-avfoundation`) on a dedicated `std::thread` blocking in `Camera::frame()` (the AVFoundation backend parks on a flume channel and drains it after each recv, so it is latest-wins on both sides), YUYV→RGBA decoded on that thread, swapped into a `Mutex<Option<Frame>>` slot; UI polls at 8 ms via `iced::time::every` and wraps pixels in a **new `image::Handle::from_rgba` per frame**. Negotiated 1920x1080 @ 30 fps YUYV (`AbsoluteHighestFrameRate`). Measured **presented** fps (new handles actually installed, each forcing a redraw — counted in `update`, shown live + logged): steady-state mode **30/30 captured/presented**; over a full 54 s shared-desktop run avg 27.3 captured / 25.7 presented with dips to ~15 while sibling agents compiled. iced has no video/texture-stream primitive; the bridge (thread, slot, poll, fps counters) is ~90 LoC of app code. |
| camera_permission_behavior | **assembled** | **observed** (grant path); **unexercised** (denial path) | See "TCC findings" below. `nokhwa` ships `nokhwa_check()` / `nokhwa_initialize(cb)` wrapping AVAuthorizationStatus / requestAccess; app polls the callback result via an `AtomicI8` + 200 ms subscription. No crash at any point; UI degrades to an in-place error/denied banner (that branch never fired — camera was already authorized). |
| mic_meter | **assembled** | **observed** | `cpal 0.17.3` input stream on its own thread (`cpal::Stream` is `!Send` — it cannot live in iced state; the thread parks on an mpsc and drops the stream on Stop). Callback stores buffer RMS in an `AtomicU32`; 20 Hz (`every(50 ms)`) subscription maps to −60..0 dBFS with fast-attack/slow-decay into two `progress_bar`s (VU + peak). Real ambient audio observed: rms fluctuated 0.0004–0.072 across runs, 1169+ callbacks/13 s (≈93/s = 512-sample buffers @ 48 kHz). |
| audio_playback | **assembled** | **self-test** | `rodio 0.22.2`: `DeviceSinkBuilder::open_default_sink()` → `mixer().add(SineWave::new(880.0).take_duration(180 ms).amplify(0.10))` on the tokio blocking pool, with a 280 ms keep-alive sleep because dropping `MixerDeviceSink` kills playback. `BeepDone(Ok)` observed in the selftest log with no error; audible output not independently verified by the harness (no one listened on a schedule), hence self-test. rodio 0.22 renamed the whole surface (`OutputStream` → `MixerDeviceSink`, `Sink` → `Player`) relative to widely-documented 0.17–0.19 examples. |
| thumbnail_grid | **assembled** | **observed** | 200 JPEGs (320x240, 3.2 MiB total) decoded via `image 0.25.10` inside `tokio::task::spawn_blocking`, throttled by `tokio::sync::Semaphore(8)`, one `Task::perform` per file so thumbnails stream into the grid without blocking the UI (placeholders render meanwhile). All 200 decoded in **256–624 ms** across runs (shared-desktop load dependent). Grid is manual `column`-of-`row` chunks (no grid widget in iced 0.14) inside `scrollable`. Caching story: the app caches decoded RGBA as `Handle`s in state; iced_wgpu keeps a CPU copy keyed by handle id and uploads to its 2048² texture atlas the first time an image is drawn (scrollable culls off-viewport widgets, so uploads are lazy); `trim()` evicts atlas entries not drawn since the previous frame, so scrolling far away and back re-uploads (CPU copy is kept, no re-decode). Observed: gallery screenshot with all visible thumbs rendered. |
| texture_upload_cost | **built-in (full re-upload per frame)** | **self-test** (CPU), **source-only** (mechanics) | iced image handles are immutable and identity-keyed; there is no update-in-place/dirty-region API, so a live preview costs: YUYV→RGBA CPU convert (nokhwa, camera thread) + 7.9 MiB `Vec` alloc per frame + atlas allocate + `write_texture` upload + previous entry trimmed one frame later (mechanics read from `iced_wgpu-0.14.0/src/image/{raster.rs,atlas.rs}`). Measured cost at 1920x1080 @ 30 fps (camera + mic + 1 Hz selftest log running): **avg 27.8% / peak 34.4% of one core, RSS max 229 MiB** over 30 samples; idle app (gallery loaded, no camera/mic): **~1% CPU, 97 MiB RSS**. Command: `ps -o %cpu=,rss= -p <PID>` at 1 Hz for 30 s after a 35 s warmup (harness script; raw samples in the run log). A synthetic 30 fps 1920x1080 generator (`PEEK_FAKE_CAMERA`, verification hook) renders through the identical path, isolating the pipeline from nokhwa/TCC. |

## TCC findings (unbundled `cargo` binary, macOS 26.5.2)

- **Camera: no prompt fired in any run.** `nokhwa_check()` returned `true` at
  first launch — camera access was already authorized for this context.
  Attribution: TCC grants for unbundled CLI binaries attach to the
  *responsible process* (the terminal/host app that spawned the shell), not
  the binary; a previous agent's camera app on this shared machine evidently
  triggered the prompt and the grant **persists across runs and across
  different unbundled binaries** launched from the same host app. The TCC
  database is not readable without Full Disk Access
  (`sqlite3 -readonly .../TCC.db` → "authorization denied", observed), so the
  exact client string could not be captured. The prompt-pending code path
  (Prompting state + 200 ms poll of the `nokhwa_initialize` callback) exists
  but never fired — **unexercised**.
- **Microphone: prompt inferred at first run.** The cpal stream built and
  `play()`ed immediately, but delivered **0 data callbacks for the first
  ~4–5 s**, then started flowing (44 callbacks at t=5, ~93/s thereafter) —
  consistent with the TCC mic prompt appearing and the user clicking Allow
  ≈5 s in. Notably the **rodio beep fired at t≈2 s but only completed at
  t=5 s** — CoreAudio output was apparently also held until the prompt
  resolved. Subsequent runs: callbacks flowed within the first second
  (grant persisted). No error, no crash: a pending (or denied) mic is
  indistinguishable from silence at the cpal API level; the app surfaces
  "stream up, but 0 callbacks" in-UI as the only hint.
- **Denial**: never denied (user allowed everything) — degradation branches
  (error banner, `Perm::Denied` message) are **unexercised**.
- No TCC state was modified.

## Helper crates (and why)

| Crate | Version | Why |
|---|---|---|
| nokhwa | 0.10.11 (`input-avfoundation`, default `decoding`) | Camera capture + YUYV→RGBA. Bindings crate: nokhwa-bindings-macos 0.2.4. |
| cpal | =0.17.3 | Mic input. Pinned one generation behind current (0.18.1) deliberately: rodio 0.22.2 still depends on cpal 0.17, and matching it avoids compiling two CoreAudio binding stacks into one binary. |
| rodio | 0.22.2 | Beep. Current API generation (DeviceSinkBuilder/Mixer/Player). |
| image | 0.25.10 | JPEG decode + `thumbnail()` downscale; same version iced's `image` feature already pulls in (single copy in the graph). |
| tokio | 1.x (rt, sync, time) | Direct dep purely for `spawn_blocking` + `Semaphore` inside iced Tasks; the runtime itself is the one iced's `tokio` executor feature drives. |

iced features: `image` (the image widget does not exist without it),
`tokio` (as with iced-dash, `iced::time::every` only exists under the
tokio/smol executor features).

## Totals / sizes

- **LoC: 854 total** (single `src/main.rs`, heavily commented) —
  **production ≈ 736, verification hooks ≈ 118** (selftest boot + 1 Hz log
  writer, `PEEK_TAB`, `PEEK_DUMP_FRAME`, `PEEK_FAKE_CAMERA` synthetic
  source). Hooks are all env-gated and marked `// [verify]`.
- Release binary: **14.9 MiB**; RSS idle **~97 MiB**, camera running peak
  **229 MiB**; assets 3.2 MiB; target/ after release build 897 MiB;
  526 locked packages.
- Build: full release build ~3.5 min cold on the shared machine; iced-peek
  crate alone rebuilds in ~5 s.

## Where the time went

1. **A ghost no-draw hunt.** One early window capture showed the whole
   preview area as exact theme background while the on-screen fps counters
   read 28/28 — which sent this investigation through iced_wgpu atlas
   sources, an alpha-channel theory, and a synthetic-source bisection…
   until dumping the exact RGBA bytes (`PEEK_DUMP_FRAME`) revealed the
   camera was working perfectly and *the room was nearly pitch black at
   00:14*. The no-draw capture itself remains unreproduced (n=1): 13/13
   later captures across two runs, including a 12-shot burst, all show the
   quad drawn. Recorded as an anomaly, not a bug I can pin on iced.
2. **API archaeology across the audio crates**: rodio 0.22 renamed
   everything; cpal 0.17 deprecated `Device::name()` mid-series and its
   `SampleRate` is a bare `u32` (docs/examples online are mostly for the
   older newtype); rodio pinning cpal 0.17 vs current 0.18 forced the
   version-dedup decision.
- Also noted: nokhwa's macOS callback tags every frame `FrameFormat::GRAY`
  and `frame()` silently re-stamps it with the negotiated format
  (source-only observation, nokhwa-bindings-macos 0.2.4) — harmless here,
  but a hint of the crate's maturity level; its `block 0.1.6` transitive dep
  trips cargo's future-incompatibility report.

## Fallbacks / gaps

- None functional: all five SPEC features work. Gaps: TCC denial branches
  unexercised (never denied); audible beep verified only at API level;
  presented-fps is counted at handle-install (each install triggers a
  redraw, but compositor-level presentation is not instrumented — iced has
  no frame-presented callback).
