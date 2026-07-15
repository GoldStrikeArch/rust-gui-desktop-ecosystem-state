# FRICTION — xilem-peek ("Peek", xilem 0.4.0 from crates.io)

NOTE: reconstructed 2026-07-10 from run artifacts after the original session
transcript (and its FRICTION writeup) was lost; the in-repo FRICTION.md is
still the 3-line DRAFT stub. Intended to replace it verbatim.

Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
Stack under xilem 0.4.0: masonry 0.4 / vello 0.6.0 / wgpu 26.0.1.

Measurements below come from the app's own 1 Hz probe (`[peek-probe]` stdout
lines: presented/captured fps + `ps`-based CPU%/RSS), on-screen counters
captured in `screenshot.png`, and `sample(1)` profiles. Camera negotiated
**YUYV 1920x1080@30fps → RGBA** (the requested 640x480/720p NV12 modes were
refused by the MacBook Pro camera), so every frame is an 8.3 MiB RGBA buffer.

## Capabilities

### camera_pipeline — **assembled** (evidence: observed)
Nothing camera-shaped is built in. Assembly: nokhwa capture loop on a
dedicated `std::thread` (the `Camera` handle is not `Send`) → per frame one
YUYV→RGBA convert into a fresh `Vec` → wrapped in a peniko `Blob`/`ImageData`
→ sent through a xilem `worker` `MessageProxy` → custom masonry widget
(`camera_view.rs`) draws it with `Scene::draw_image`. The stock `image()`
view calls `request_layout()` on every `set_image_data`, so a custom widget
with `request_paint_only()` was required for a sane 30 fps path. Measured:
**29.5–30.6 fps presented** (== captured) at 1080p30, steady across a 7-hour
run; FPS counter counts actual widget paints (a shared atomic bumped in
`paint()`), not frames received.

### camera_permission_behavior — **assembled** (evidence: observed for the
granted + busy-device paths; denial path unexercised)
nokhwa exposes the AVFoundation authorization API (`nokhwa_check` /
`nokhwa_initialize`); the app waits up to 35 s for the TCC answer and
degrades to an in-UI error string on denial/timeout. In practice **no TCC
prompt ever fired in a captured run** — the unbundled cargo binary inherits
the launching terminal context's existing grant, and approval persisted
across all runs. What *did* happen (observed, repeatedly): with concurrent
peek apps holding the camera, `Camera::new` fails with AVFoundation's
"lockForConfiguration … Lock Rejected"; the app shows the error in-UI and
retries (10 × 2 s) without crashing. Denial-crash behavior: unexercised.

### mic_meter — **assembled** (evidence: observed)
cpal input stream on its own thread; the audio callback accumulates
sum-of-squares, a 20 Hz drain converts to RMS → dBFS and feeds xilem's
built-in `progress_bar`. Observed live: "MacBook Pro Microphone @ 48000 Hz",
meter moving with room noise (screenshot: -56.2 dBFS, peak rms 0.0047).
No mic TCC prompt fired either (same inherited grant).

### audio_playback — **assembled** (evidence: observed)
rodio 0.22 `DeviceSinkBuilder::open_default_sink()` + 180 ms 880 Hz
`SineWave` on a throwaway thread (rodio's sink is not `Send`). Observed:
"played ×1" counter on screen; rodio's `Dropping DeviceSink` notice in the
run log confirms the sink actually opened and drained.

### thumbnail_grid — **assembled** (evidence: observed)
No built-in async image loading, no virtualized grid, no user-facing texture
cache story. Assembly: `task_raw` lists the 200 JPEGs, decodes + downscales
to ≤128 px on `spawn_blocking` threads (4-permit semaphore), streams thumbs
back one message at a time into placeholder slots; grid is plain
`flex_row`s inside a `portal` (scrollable, not virtualized — all 200
`image()` views live in the scene). UI never blocked; "200/200" reached
within the first probe second on every run. Thumbs are kept small
(128x96 RGBA ≈ 48 KiB, ~9.4 MiB total) because scene images are re-encoded
into the vello scene each frame; whether vello 0.6 re-uploads unchanged
atlas entries to the GPU per frame was not verified (source-only,
uncertain) — the conservative assumption drove the sizing.

### texture_upload_cost — **hand-rolled** measurement path (evidence: self-test)
Per frame: one full CPU pixel-format convert (YUYV→RGBA, nokhwa), one
8.3 MiB heap alloc + copy into a fresh `Blob` (no buffer reuse — `ImageData`
equality is blob-id based, so a fresh blob per frame is also what makes the
view's dirty-check work), one full-image scene encode + GPU upload (no
partial/dirty-rect update path exists). Self-observed cost at 1080p30:
**~60–75% of one core** camera-only (61% median in one 80 s run, 72–74% in
the 7 h run), **77–86%** with the mic meter also on, **~0–2%** idle. RSS:
~120 MiB idle → ~230–320 MiB live (blob churn; flat over 7 h, no leak).
`sample(1)` top-of-stack is dominated by waits — the cost is spread across
the capture/convert thread, allocator, and render, not one hot loop.

## Helper crates (and why)

- `nokhwa =0.10.11` (`input-avfoundation`) — camera capture; xilem/masonry
  have zero camera support.
- `cpal =0.17.3` — mic input stream; version matched to what rodio 0.22
  uses so only one cpal is in the tree.
- `rodio =0.22.2` (`playback` only, decoders off) — output beep.
- `image 0.25` (`jpeg` only) — gallery JPEG decode + `thumbnail()` downscale.

## LoC split

1071 total: **1021 production / 50 verification hooks**.
- `src/main.rs` 554 (504 production + 50 verification)
- `src/camera_view.rs` 161 (all production — the presented-frame atomic is
  the SPEC-required FPS counter, not a hook)
- `src/media.rs` 356 (all production)

No env-gated blocks; the hooks are always-on but inert: the once-per-second
`[peek-probe] …` stdout line (only while the camera runs), the
`[peek] gallery loaded: N thumbnails` line, and the `ps`-based CPU/RSS
self-sampling + its "self-observed: …" UI label.

## Sizes (MiB)

- Release binary: **13.0 MiB** unstripped, **10.5 MiB** stripped.
- `target/` after release build: **1243 MiB**.
- RSS: ~120 MiB idle (incl. 200 thumbs), ~230–320 MiB with 1080p30 preview.
- Incremental no-op release build: **0.26 s**. (Cold-build time was lost
  with the original session transcript and was not re-measured.)

## Where the time went

(Reconstructed from run artifacts; original transcript lost.)
1. **Camera device contention + format negotiation** — concurrent agents'
   peek apps hold the AVFoundation configuration lock, so `Camera::new`
   fails with "Lock Rejected"; several launch sessions (00:20–01:38) failed
   before the 10×2 s retry loop and the 3-tier format-request fallback
   (640x480 NV12 → 720p NV12 → highest-fps) made opens reliable.
2. **Custom preview widget** — working out that stock `image()` relayouts on
   every frame and hand-rolling a masonry `Widget` + xilem `View` pair with
   paint-only updates and honest presented-frame counting.
3. **Threading architecture** — none of the device handles (nokhwa `Camera`,
   cpal stream, rodio sink) are `Send`; each lives on a dedicated OS thread
   commanded via xilem `worker` channels and reporting via `MessageProxy`,
   with stop-flags that must clear on every exit path.
