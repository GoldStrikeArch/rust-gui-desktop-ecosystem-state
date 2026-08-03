# FRICTION — Peek (floem git @ 778bb5f2), SPEC-6

Built + verified on macOS (M4 Pro, rustc 1.96.1): release build clean
(locked too). `PEEK_SELFTEST=1` runs auto-started camera + mic + beep and
logged 1 Hz status lines (selftest.log, same field layout as iced-peek);
window-scoped screenshots show a LIVE camera frame, live VU bar and the full
thumbnail grid — **observed**.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| camera_pipeline | **assembled (with a mandatory workaround)** | **observed** | Same capture side as iced-peek: `nokhwa 0.10.11` (AVFoundation) blocking in `Camera::frame()` on a std thread, YUYV→RGBA there; negotiated 1920x1080 @ 30 fps YUYV. Presentation side is floem-specific: the frame goes into a `Mutex` slot + `ExtSendTrigger` → an `Effect` bumps a `frame_rev` signal → a `canvas` paint closure (signal-tracked) calls `Renderer::draw_img` with raw RGBA. Steady state: **30 fps captured**; the paint counter reads ~48/s because floem repaints the canvas whenever the window repaints (20 Hz VU updates interleave with 30 Hz frames). MANDATORY workaround: frames must be downscaled to ≤320x180 before display — see texture_upload_cost. ~110 LoC of bridge code. |
| camera_permission_behavior | **assembled** | observed (grant path); unexercised (prompt/denial) | `nokhwa_check()` returned `true` at first launch — the TCC grant attached to the terminal host app by earlier agents' runs persists across unbundled binaries (same finding as iced-peek). The prompt path (`nokhwa_initialize` callback → AtomicI8 + ExtSendTrigger → Perm signal) is implemented but never fired; denial branch unexercised. No crash anywhere; errors surface in-UI. |
| mic_meter | **assembled** | **observed** | `cpal =0.17.3` input stream on its own thread (`Stream` is `!Send`), RMS into an `AtomicU32`; a 20 Hz `exec_after` chain maps to dBFS and drives a hand-styled VU bar + slow-decay peak bar (floem has no progress-bar widget). Real ambient audio observed: rms 0.001–0.009 fluctuating, ~94 callbacks/s (512-sample buffers @ 48 kHz), `MacBook Pro Microphone (1 ch @ 48000 Hz)`. |
| audio_playback | **assembled** | **self-test** | `rodio 0.22.2` (`DeviceSinkBuilder::open_default_sink()` → mixer → 880 Hz SineWave 180 ms @ 0.10 amplitude, 280 ms keep-alive sleep) on a plain std thread; result crosses back via `create_ext_action`. `beep ok (×1)` in UI + log; audible output not independently verified. |
| thumbnail_grid | **assembled** | **observed** | 200 JPEGs decoded + downscaled (`image 0.25`, `thumbnail(100,75)`) on a hand-rolled 8-thread pool (floem has NO executor/blocking pool), results funneled through a queue + `ExtSendTrigger`, one `RwSignal<Option<Thumb>>` per cell so each arriving thumb repaints only its own canvas. All 200 in **30–58 ms** across runs. Grid = flex-wrap `dyn_stack` in a scroll (`min_height(0)` again load-bearing). Caching: each thumb is one content-hash entry in vger's color atlas — cached across frames, BUT evicted wholesale whenever anything (e.g. the camera stream) forces an atlas clear, then silently re-uploaded. |
| texture_upload_cost | **NOT-ACHIEVABLE at full resolution** (headline) | observed + source-only | vger's image path is a single color ATLAS keyed by CONTENT HASH (`vger::render_image` → `GlyphCache::get_image_mask`). A video stream = a new hash per frame = a new atlas region per frame. Two fatal interactions, from source (floem-vger-rs `glyphs.rs`/`atlas.rs`): (1) the atlas only self-heals (full clear) when tracked usage crosses **70%**, but (2) a region that fails to pack is dropped **silently, without cleanup, and does not count toward usage**. Net effect: any frame bigger than ~⅓ of the atlas dimension fragments the packer below the clear threshold and image drawing wedges PERMANENTLY — a 1080p (and even 640x360) preview goes black after ~3 frames while everything else keeps running. Workaround shipped: downscale every frame to ≤320x180 on the camera thread (nearest-neighbor), which keeps the pack-fail-free cycle (≈13 packs → 70% → clear → repeat). Cost of that steady state: full re-upload every frame + a whole-atlas clear every ~13 frames, which also **evicts every gallery thumbnail and (on resize events) glyphs**. Measured: camera+mic+log running **28.6% CPU / 286 MiB RSS** (includes 1080p YUYV→RGBA convert + downscale); idle with gallery loaded **0.0% CPU / 109 MiB**. Isolation hook: `PEEK_FAKE_CAMERA=1` (+`PEEK_FAKE_SIZE=WxH`) renders a synthetic stream through the identical path. |

## Helper crates (mirroring iced-peek's pins)

nokhwa 0.10.11 (input-avfoundation) · cpal =0.17.3 (rodio still pins 0.17)
· rodio 0.22.2 · image 0.25 · plus `floem_renderer` (same git rev — the
`Img` struct for `draw_img` is NOT re-exported by floem, so raw-RGBA drawing
needs a direct dependency on floem's own internal crate; papercut) and
`raw-window-handle` + `objc2` (verification-only, window-scoped screenshot).

## The atlas wedge diagnosis (where the time went)

1. Preview black at 1080p with capture running at 30 fps → synthetic-source
   bisection (`PEEK_FAKE_CAMERA`): 1080p black, 640x360 black, constant-hash
   variant drew a GARBLED tile (stale AtlasInfo across clears — a second
   bug: cached rects are not invalidated on clear for reused hashes),
   320x180 streams correctly. Root cause then confirmed by reading
   floem-vger-rs: pack-failure paths return `None` rects and never clear.
   ~1.5 h; the workaround is 30 LoC.
2. Everything else was assembly work that went smoothly; the thread→signal
   bridges (`ExtSendTrigger`) are by now a well-worn pattern in this cohort.

## Totals / sizes

- LoC **947** (single main.rs): production ≈ 790, verification hooks ≈ 157
  (selftest log, fake camera, PEEK_SHOT window capture).
- Release binary **19.9 MiB**; RSS idle **109 MiB**, camera running
  **286 MiB**; assets 3.2 MiB.
- No TCC state modified; camera/mic prompts never fired (pre-existing grant
  on the shared host app).
