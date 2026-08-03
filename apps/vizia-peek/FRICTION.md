# FRICTION — Peek (vizia =0.4.0), SPEC-6

Reference: SPEC-6.md. Built + verified on macOS 26.5.2 (M4 Pro, rustc
1.96.1): `cargo build --release` and `cargo build --locked --release` clean,
binary launched with camera + mic + gallery live and ran well past the 10 s
bar (19 one-second status lines retained in `selftest.log`) before being
killed — **observed**. No fallback was needed; every SPEC-6 capability is
real.

Evidence labels: **observed**, **self-test** (`PEEK_SELFTEST=1`, one
machine-readable status line per second in `selftest.log`),
**synthetic-input** (the `PEEK_FAKE_CAMERA` RGBA generator),
**source-only**, **unexercised**.

Self-test hooks (all opt-in, all off by default):

```
PEEK_SELFTEST=1     auto-start camera + mic, one quiet beep at t≈2 s,
                    one status line per second
PEEK_LOG=<path>     where those lines go (default ./selftest.log)
PEEK_TAB=camera|audio|gallery   initial tab, so a harness can shoot any one
PEEK_FAKE_CAMERA=1  synthetic 30 fps RGBA source (isolates frame→Skia)
PEEK_ASSETS=<dir>   override the JPEG directory
```

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| camera_pipeline | **assembled** | observed + self-test | `nokhwa 0.10.11` (`input-avfoundation`) on a dedicated `std::thread` blocking in `Camera::frame()`, YUYV→RGBA decoded on that thread, swapped into a `Mutex<Option<Frame>>`. The UI side is where vizia is unusual: because the renderer **is** Skia and `vizia::vg` re-exports `skia-safe`, the preview is a custom `View` whose `draw()` calls `vg::images::raster_from_data(&info, Data::new_bytes(&pixels), row_bytes)` and `canvas.draw_image_rect(..)`. **There is no framework image handle, no texture cache, no image feature flag and no encode step** — the bytes go from the capture thread straight into a Skia raster image once per presented frame. Negotiated **1920×1080 @ 30 fps YUYV**. Measured *presented* fps (counted inside `draw()`, so only frames actually blitted): **30/30 captured/presented sustained** for the whole run. The frame→screen bridge is ~45 LoC. |
| camera_permission_behavior | **assembled** | observed (grant path); **unexercised** (denial path) | `nokhwa_check()` / `nokhwa_initialize(cb)` wrap AVAuthorizationStatus / requestAccess; the result lands in an `AtomicI8` (-1 unknown / 0 denied / 1 granted) and is displayed in the status line and logged (`perm=1`). **No prompt fired in any run** — access was already authorized for the responsible process (the terminal that spawned the unbundled binary), so the grant persists across runs and across different unbundled binaries launched from the same host app. Nothing crashed at any point; the denied branch renders an in-place status line instead, but that branch never executed, so it is labelled unexercised. |
| mic_meter | **assembled** | observed + self-test | `cpal 0.17.3` input stream on its own thread — `cpal::Stream` is `!Send` and cannot live in vizia state, so the thread owns it and parks on an mpsc until Stop. The callback stores buffer RMS in an `AtomicU32`; a **20 Hz `cx.add_timer`** (50 ms, exactly SPEC-6's rate) maps it to −60..0 dBFS with fast-attack/slow-decay into two built-in `ProgressBar`s (level + peak hold). Real ambient audio observed: `MacBook Pro Microphone (1 ch @ 48000 Hz)`, RMS fluctuating 0.0007–0.0124 across the run, **~93 callbacks/s** (≈512-sample buffers at 48 kHz), 1,759 callbacks over 19 s. |
| audio_playback | **assembled** | self-test | `rodio 0.22.2`: `DeviceSinkBuilder::open_default_sink()` → `mixer().add(SineWave::new(880.0).take_duration(180 ms).amplify(0.10))` on a plain background thread, with a 280 ms keep-alive sleep because dropping the sink kills playback. Fired at t≈2 s in every self-test run with `beep_err=None`. Audible output was not independently verified by the harness, hence self-test rather than observed. Note for the cohort: rodio 0.22 renamed the whole surface relative to the widely-documented 0.17–0.19 examples. |
| thumbnail_grid | **assembled** | observed | 200 JPEGs (320×240, 3.2 MiB total) read by an **8-thread worker pool** (`std::thread` + a shared work queue), each handing its bytes to `ContextProxy::load_image(key, &bytes, Forever)`, which decodes through Skia and registers the image under a key; the grid then renders `Image::new(cx, key)` in rows of 8 inside a `ScrollView`. The UI never blocks: the grid is republished in batches of 25 as keys land, so thumbnails stream in. All 200 registered in **3 ms** — but that number is honest only about *registration*: `skia_safe::Image::from_encoded` is lazy, so the pixel decode happens on first draw inside Skia, which is also why the caching story is entirely Skia's (vizia's `ResourceManager` keeps the `Image` alive per retention policy and evicts unobserved ones; there is no atlas/trim layer of vizia's own). Gallery-only idle cost: **2.6 % CPU, 124 MiB RSS**. |
| texture_upload_cost | **assembled (new raster image per frame)** | self-test + source-only | Every presented frame allocates a fresh `vg::Image` over the RGBA buffer and issues one `draw_image_rect`; there is no update-in-place path and no dirty-region API, so the per-frame cost is: YUYV→RGBA CPU convert (nokhwa, capture thread) + 7.9 MiB `Vec` alloc + `Data::new_bytes` wrap + Skia's upload of the raster image. Measured over 10 × 1 s samples with `ps -o %cpu=,rss= -p <pid>`:<br>• real camera **1920×1080 @ 30 fps + mic**: **29.0 % of one core, 278 MiB RSS**<br>• synthetic **1280×720 @ 30 fps + mic** (`PEEK_FAKE_CAMERA`, isolates the frame→Skia path from nokhwa/TCC): **15.6 % CPU, 119 MiB RSS**<br>• gallery only, no camera/mic: **2.6 % CPU, 124 MiB RSS**<br>The gap between the two camera figures is the YUYV→RGBA conversion plus the 2.25× larger buffer, not vizia. A useful side observation: when the Camera tab is not selected the view does not exist, so `presented_fps` drops to **0** while capture continues — vizia's `Binding` really does tear the view down. |

## TCC findings (unbundled `cargo` binary, macOS 26.5.2)

- **Camera: no prompt in any run.** `nokhwa_check()` returned `true` at first
  launch. TCC grants for unbundled CLI binaries attach to the *responsible
  process* (the terminal that spawned the shell), not the binary, so a grant
  made for one unbundled binary covers others launched the same way. The
  prompt-pending path exists and is polled, but never fired — unexercised.
- **Microphone: no user-visible prompt either**, and unlike the iced cohort's
  run there was no dead window: `callbacks=74` was already logged at t=1,
  i.e. audio flowed within the first second. Same responsible-process
  explanation.
- **Denial does not crash.** Not exercised for real; by construction a failed
  `Camera::new`/`build_input_stream` writes into the shared `error` slot and
  the status line shows it in place, with the rest of the app unaffected.

## Helper crates

- `nokhwa 0.10.11` (`input-avfoundation`) — camera capture; vizia has nothing.
- `cpal =0.17.3` — mic input. Pinned to match rodio 0.22.2's cpal so only one
  CoreAudio binding stack is compiled.
- `rodio 0.22.2` — audio out (the beep).
- `image 0.25` (`default-features = false`, `jpeg`) — only for the synthetic
  camera hook; the gallery hands encoded JPEG bytes straight to Skia.

Notably **no image/texture helper was needed at all**, which is unusual for
this cohort: `vizia::vg` already exposes the renderer.

## LoC split

- Production: **~700** (`src/main.rs` 808 minus ~108 lines of verification
  hooks: the `PEEK_SELFTEST` status-line writer, the `PEEK_FAKE_CAMERA`
  generator, and the `PEEK_TAB`/`PEEK_LOG`/`PEEK_ASSETS` overrides).
- Verification: **~108**, all in-app. Retained evidence: `selftest.log`.

## Sizes (MiB)

- Release binary: **22.3 MiB** (Skia statically linked).
- Gallery assets: **3.2 MiB** (200 JPEGs).
- RSS: 124 MiB gallery-only · 119 MiB synthetic camera + mic · 278 MiB real
  1920×1080 camera + mic.

## Where the time went

1. Deciding the frame→screen path. `ContextProxy::load_image` is the obvious
   API and is **wrong** for video: it takes *encoded* bytes and would mean
   PNG/JPEG-encoding every frame. The right answer — a custom `View` that
   builds a `vg::Image` from raw RGBA — is not in any vizia example.
2. Ownership plumbing: `Arc<CamShared>` has to be cloned once for the model,
   once for the `Binding` closure, and again inside the tab arm, because a
   `Binding`'s closure is `Fn` (it may re-run) while the `VStack` builder it
   contains is `move`.
3. Camera, mic, beep and gallery themselves were fast — the same crates as
   the rest of the cohort, driven by ordinary timers.

## Surprises

- Good: sustained **30/30 captured/presented at 1920×1080** with no
  framework texture API in the way. The renderer being Skia, and being
  *exposed*, is the single biggest reason this app is short.
- Good: 200 JPEGs registered in 3 ms across 8 threads with zero UI stall,
  because Skia decodes lazily at draw time.
- Bad: 29 % of a core for a 1080p30 preview. There is no way to hand Skia a
  reusable/mutable texture, so each frame is a fresh raster image.
- Bad: `Context::load_image` is unreachable from `EventContext`, and
  `ContextProxy::load_image` only accepts *encoded* bytes — the two obvious
  entry points both point away from the path that actually works for video.
