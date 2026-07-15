# FRICTION — slint-peek (Peek, Slint =1.17.1)

Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2. Default slint features
(winit backend, femtovg GL renderer). Camera: built-in "MacBook Pro Camera",
mic: "MacBook Pro Microphone".

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| camera_pipeline | **assembled** | **observed** | nokhwa capture thread → `Buffer::decode_image_to_buffer::<RgbAFormat>` into a fresh `SharedPixelBuffer<Rgba8Pixel>` (Send) → `slint::invoke_from_event_loop` → `Image::from_rgba8_premultiplied` set on an `in property <image>` bound to an `Image` element. This is exactly Slint's documented pattern (the `Image` rustdoc shows the thread→`SharedPixelBuffer` handoff verbatim), so the Slint half is near zero friction; all real friction was nokhwa-side (see Surprises). **Measured presented fps: 29.9–30.0** (Δpresented 1078 frames / 36 s in a 40 s run; 360/12 s in a second run), with presented ≈ delivered ≈ captured — the femtovg backend never fell behind 1080p30. Presented frames counted via `Window::set_rendering_notifier` (`AfterRendering` redraws where the delivered counter advanced); the notifier registered without error on the GL backend. |
| camera_permission_behavior | **assembled** | **observed** (grant path) / **unexercised** (denial) | nokhwa exposes `nokhwa_check()` (AVAuthorizationStatus) and `nokhwa_initialize(cb)` (`requestAccessForMediaType`); the wait-for-verdict (35 s channel timeout) and degrade-to-status-text logic are hand-rolled. Observed: `nokhwa_check()` returned *authorized on the very first launch of this binary* — no prompt ever fired in 4 runs. See TCC section. Denial/timeout paths are coded (in-UI error, no crash) but never triggered. |
| mic_meter | **assembled** | **observed** | cpal 0.17 default input stream (F32, 48 kHz) built and owned by a dedicated thread (`cpal::Stream` is `!Send`); data callback stores linear RMS into an `AtomicU32` (f32 bits); a 50 ms `slint::Timer` maps it to a 60 dB scale and writes two float properties; the VU bar is ~25 lines of .slint (filled Rectangle + peak-hold marker + `animate width`). Observed: status line shows the device, bar visible in snapshot, nonzero `mic_level` ticks (0.017/0.076) in a quiet room — one blip coincided with the auto-beep. |
| audio_playback | **assembled** | **self-test** | rodio 0.22 (`default-features=false, features=["playback"]` — the 0.22 API is fully renamed: `DeviceSinkBuilder::open_default_sink()` → `MixerDeviceSink`, `mixer().add(...)`; the classic `OutputStream::try_default`/`Sink` from 0.20 is gone). Sink opened lazily on first beep on the UI thread and kept alive in a `RefCell`; `SineWave::new(880).take_duration(250ms).amplify(0.2)` queued without error ("beep #1 queued", status updated). Audible output not verified by ear (agent-driven run), hence self-test. |
| thumbnail_grid | **assembled** | **observed** | No built-in async image loading, no grid-view widget, no recycling. Hand-rolled: 4 worker threads decode+downscale JPEGs (`image::open` + `thumbnail(208,208)`), each result crosses to the UI thread as a `SharedPixelBuffer` and lands via `VecModel::set_row_data` into a placeholder-prefilled model (stable order, progressive fill); the grid is a `for` repeater with index-math x/y inside a `ScrollView`. **200 JPEGs in 823 ms cold / 140–410 ms warm** (4 threads), UI never blocked. All 200 `Image` elements are live (no virtualization) — fine at this scale. |
| texture_upload_cost | n/a (measurement) | **self-test** (CPU) + **source-only** (cache story) | Buffer-backed images get `ImageCacheKey::Invalid` (i-slint-core `graphics/image.rs`), so they never enter the femtovg `TextureCache`; instead each `Image` *item* holds its texture in the per-item graphics cache (`ItemGraphicsCache`, femtovg `itemrenderer.rs::draw_image_impl`), invalidated when `source` changes. Net effect: **gallery thumbs upload once and are cached across frames; the camera frame is a full 1920×1080 RGBA (7.9 MiB) texture re-created + re-uploaded every frame** (~237 MiB/s at 30 fps), plus one CPU-side YUYV→RGBA convert and one 7.9 MiB allocation per frame on the capture thread. Measured process CPU (`ps`, 100% = 1 core): **~34% steady at 1080p30 preview + mic meter** (mean of 35 samples, range 32.5–36.4), ~15.5% with mic meter only (20 Hz full-window redraws + width animation), ~0.1–0.4% idle. RSS: ~305 MiB during preview vs ~128 MiB idle. |

## macOS TCC (permissions) — deliverable

- **Prompt attribution**: for this unbundled `cargo` binary, no camera or mic
  prompt ever appeared, and `AVCaptureDevice authorizationStatusForMediaType`
  reported *authorized* on the binary's first-ever launch. TCC therefore keyed
  the grant to the **responsible process (the terminal host that spawned the
  binary), not the binary itself** — the host already held camera+mic grants
  (earlier iterations/agents on this machine). Evidence label: **observed**
  (the inheritance); the attribution mechanism itself is inferred — direct
  confirmation via `TCC.db` was not possible (read-only sqlite open →
  "authorization denied", no Full Disk Access; left untouched per rules).
- **Persistence**: grants persisted across runs and across *different*
  binaries launched from the same host (slint-peek was brand new and started
  authorized) — **observed**.
- **Denial behavior**: **unexercised**. Code path: `nokhwa_initialize`
  callback false → in-UI "camera permission denied (TCC)" status, camera
  section degrades, app keeps running; cpal reports errors through its error
  callback. No crash path found in testing the happy side; denial never
  triggered because no prompt ever fired.
- TCC state was never modified.

## Helper crates (and why)

| Crate | Pin | Why |
|---|---|---|
| nokhwa (`input-avfoundation`) | =0.10.11 | Spec-mandated camera capture. **=0.10.9 does not compile**: it resolves nokhwa-bindings-macos 0.2.4, whose frame channel grew a timestamp field (semver break inside a patch range). 0.10.11 matches. |
| cpal | =0.17.3 | Spec-mandated mic input. 0.17 renamed `Device::name()` → `description().name()`; `SampleRate` is a plain `u32` alias now. |
| rodio | =0.22.2 | Beep. `default-features=false, features=["playback"]` drops all symphonia decoders. Entirely new API vs 0.20 (see table). |
| image | =0.25.10 | Gallery JPEG decode + `thumbnail()` downscale (Slint can decode via `Image::load_from_path`, but gives no control over downscale size or thread handoff). `png` feature only serves the PEEK_SNAPSHOT verification hook. Same major as nokhwa-core's own image dep — no duplicate majors. |

## Verification hooks (env-gated, in `src/verify.rs`)

`PEEK_AUTO` (auto-start camera+mic at 1.5 s, beep at 4 s), `PEEK_LOG` (1 Hz
counter ticks), `PEEK_SECS` (auto-quit + `PEEK_STATS` line), `PEEK_SNAPSHOT`
(window self-screenshot via `Window::take_snapshot()` — worked on the femtovg
GL backend, 1800×1200 PNG, see `peek-window.png`). No desktop interaction was
needed for any verification.

## LoC split (production vs verification)

- Production Rust: **526** (`src/main.rs` 523 + `build.rs` 3)
- Production Slint DSL: **163** (`ui/main.slint`; no verification code in DSL)
- Verification Rust: **109** (`src/verify.rs`)
- Total: 798

## Measurements

- Binary: 16,231,712 B raw (**15.5 MiB**), 14,207,264 B stripped (**13.5 MiB**).
- RSS: ~128 MiB idle (after gallery load), ~305 MiB during 1080p30 preview.
- Dependency graph: **349** unique crate names incl. the app (`cargo tree -e normal,build`).
- Canonical clean release build **59.5 s** (`cargo clean` first, warm crate
  cache); **no-op rebuild 0.46 s**; post-edit incremental 4–9 s. (The first
  ever build, including downloads, took ~2m40s.)
- Camera: 1920×1080 "YUYV" @ 30 fps (nokhwa maps AVFoundation 420v/420f *and*
  yuvs to `FrameFormat::YUYV`; the capture output converts to packed 4:2:2, so
  the YUYV decoder is correct — colors verified in snapshot).
- Launch check: window up and gallery fully decoded in <1 s; 10 s+ runs clean
  exit 0 (observed, 5 runs).

## Where the time went

1. ~50% nokhwa format negotiation: three separate traps (see Surprises) each
   diagnosed from crate source, two failed launch cycles before first frame.
2. ~15% presented-fps design: rendering-notifier + delivered/presented counter
   split (measuring *presented*, not captured, per spec).
3. ~15% verification hooks + measurement runs (CPU sampling, TCC observation).
4. ~10% gallery (mostly deciding placeholder-prefill vs push ordering).
5. ~10% UI layout/styling. The Slint side of the camera path itself was
   essentially free — the documented pattern worked first try.

## Surprises

- Bad (nokhwa, 3 traps): (1) `=0.10.9` no longer builds because
  nokhwa-bindings-macos 0.2.4 broke its internal channel type within a patch
  release — had to bump to 0.10.11. (2) `RequestedFormatType::Closest` only
  matches when the exact resolution+format pair exists (its fps pass filters
  on the *requested* resolution, not the closest one it just found) — "Cannot
  fulfill request" on a camera without 1280×720 NV12. (3) The macOS bindings
  advertise frame-rate-range *minimums* (e.g. 640×480@15) that `set_all()`
  can never set (it only matches a range's max), so `RequestedFormatType::
  None` (first advertised) fails to open. Working recipe:
  `AbsoluteHighestFrameRate` (always a range max) + optional re-negotiation.
  Bonus: `compatible_camera_formats()` returned an empty list pre-open, so
  manual re-negotiation had nothing to work with and the opener's 1080p30 won.
- Good: Slint's thread→`SharedPixelBuffer`→`invoke_from_event_loop`→
  `Image::from_rgba8_premultiplied` pattern hit 30 fps at 1080p on the first
  successful open, with zero dropped presents and ~34% of one core total.
- Good: `Window::take_snapshot()` works on the GL backend — free, scoped,
  TCC-less window screenshots for verification.
- Observed (shared machine): when another process held the camera,
  `Camera::new` failed with "lockForConfiguration … Lock Rejected"; the app
  degraded to an in-UI error and exited cleanly — device *contention*, unlike
  permission, is a per-open runtime failure you must handle.
- Neutral: buffer-backed images bypass Slint's keyed texture cache by design
  (`ImageCacheKey::Invalid`); per-item caching still saves the gallery, but a
  30 fps camera pays a full texture re-upload per frame — there is no dirty-
  rect or partial-update path for `Image` on the GL renderer.
