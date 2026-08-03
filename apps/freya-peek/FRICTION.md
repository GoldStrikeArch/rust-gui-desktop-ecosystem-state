# FRICTION — Peek (freya =0.4.0), SPEC-6

Reference machine per spec. `cargo build --release` clean, `cargo build
--locked --release` reproduces. Verified with a `PEEK_SELFTEST=1` run (camera +
mic auto-started, one beep at t≈2 s, one status line per second) plus synthetic
CGEvent clicks/scrolls and window-scoped screenshots of all three tabs.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| camera_pipeline | **built-in** | **observed** | Freya's `camera` feature re-exports `freya-camera`, which is a real first-party camera integration: `use_camera(CameraConfig::default)` spawns a nokhwa capture thread, converts each frame to RGBA, builds a Skia `ImageHandle` and pushes it into a `State<Option<ImageHandle>>`; `CameraViewer::new(camera)` renders it, with `loading_placeholder` and `error_renderer` hooks. **The app writes no capture thread, no frame slot, no texture upload** — the only app code is the fps counter and the Start/Stop toggle. Negotiated **1920×1080 @ 30 fps**; measured *presented* fps (frames that reached the reactive graph, counted in a `use_side_effect` on `camera.frame`): **29–31, steady 30** over a 17 s self-test run (`t=2 … t=17`, `pres_fps=30` in `selftest.log`). |
| camera_permission_behavior | **built-in** | **observed** (grant path); **unexercised** (denial path) | `freya::camera::init()` is a documented one-liner for `main` that blocks on the AVFoundation prompt and returns the answer — the app logs `camera-permission: granted=true`. No prompt appeared on this machine (the grant already existed for the responsible process, as for the other ports in this cohort), so the denial path — which `CameraViewer::error_renderer` plus `camera.error` covers by construction — never fired. The unbundled binary did not crash at any point. |
| mic_meter | **assembled** | **observed** | Freya covers nothing here. `cpal 0.17.3` input stream on its own `std::thread` (a `cpal::Stream` is `!Send`, so it cannot live in component state; the thread parks and drops the stream when a shared `AtomicBool` flips). The callback stores buffer RMS in an `AtomicU32`; a 20 Hz `async-io` `Timer` loop on Freya's executor copies it into a signal and maps it to −60..0 dBFS behind a stock `ProgressBar`. Real ambient audio observed: `rms` 0.00114–0.00387 with **9,364 callbacks** over ~100 s (~94/s = 512-sample buffers @ 48 kHz). |
| audio_playback | **assembled** | **self-test** | `rodio 0.22.2`: `DeviceSinkBuilder::open_default_sink()` → `mixer().add(SineWave::new(880.0).take_duration(180 ms).amplify(0.10))` on a dedicated thread with a 280 ms keep-alive, because dropping the `MixerDeviceSink` stops playback (rodio even logs a warning about it, visible on stderr). `beeps=1 beep_err=""` from t=2 onward. Audible output was not independently confirmed by the harness, hence self-test rather than observed. |
| thumbnail_grid | **built-in** | **observed** | `ImageViewer::new(ImageSource::Path(p))` does async load, decode **to the layout size** (`DecodeMode::FromLayout`), caching and error/loading states — so the app's gallery is one component per thumbnail with a `loading_placeholder`, inside a `VirtualScrollView` of 6-wide rows so only the visible rows are mounted at all. 200 JPEGs (3.2 MiB) render immediately and scroll cleanly to the last partial row; CPU **0.4–1.9 %** while browsing the gallery, RSS **208–224 MiB**. No `spawn_blocking`, no semaphore, no decode cache in app code — the iced port needed all three. |
| texture_upload_cost | **assembled by the framework (full re-upload per frame)** | **self-test** (CPU/RSS), **source-only** (mechanics) | `ImageHandle::from_rgba(w, h, Bytes, AlphaType)` wraps a raster `SkImage`; handles are immutable and there is no update-in-place or dirty-rect API, so a live preview costs, per frame: YUV→RGBA conversion + an 8.3 MiB `Bytes` allocation (1920×1080×4) inside `freya-camera` + a new `SkImage` + a `State` write that dirties the tree + a **full-tree repaint** (`render_pipeline.rs` still carries `// TODO: Use incremental rendering`). Measured with camera + mic + 1 Hz logging running: **27.3–31.4 % of one core, RSS 282 MiB** (10 × `ps -o %cpu=,rss= -p <pid>` at 1 Hz). With the camera stopped (gallery tab): **0.4–1.9 %, 208–224 MiB**. Notably switching tabs *stops the capture*, because `use_camera` is owned by the tab's component scope. |

## Helper crates (and what Freya replaced)

- `cpal =0.17.3` — mic input. Pinned to the version `rodio 0.22.2` uses so the
  CoreAudio binding stack is deduped.
- `rodio 0.22.2` — the beep.
- `async-io 2.6.0` — the 20 Hz VU sampler, the 1 Hz fps window and the
  self-test's log ticker; Freya's executor has no timer.

**Not needed, unlike the other ports:** `nokhwa` (owned by `freya-camera`),
`image` (`ImageViewer` decodes), and `tokio` (`spawn_blocking` + `Semaphore`
were the iced port's way to keep 200 JPEG decodes off the UI thread; Freya's
`ImageViewer` handles that itself).

## LoC split

- 585 total in one `src/main.rs`
- ~75 verification hooks (`PEEK_SELFTEST` auto-start, timed beep, 1 Hz log line,
  fps accounting)
- ~510 production, of which the mic thread is ~60 — the largest single block,
  and the only piece of hardware plumbing Freya does not own.

## Sizes

- gallery assets: **3.2 MiB** (200 JPEGs)
- RSS: **208–224 MiB** browsing the gallery, **282 MiB** with a 1080p30
  preview live

## Where the time went

Very little, relative to the other ports: the camera section is ~30 lines
because `use_camera` + `CameraViewer` are first-party, and the gallery is ~35
because `ImageViewer` + `VirtualScrollView` are. The real work was the mic
thread (the usual `!Send` `cpal::Stream` dance) and rodio 0.22's renamed API
surface (`DeviceSinkBuilder`/`MixerDeviceSink`, not the `OutputStream`/`Sink`
of every tutorial).

## Surprises

- Good: **a GUI framework that ships a camera integration.** `use_camera`
  returns reactive `frame`/`info`/`error` signals and the capture's lifetime is
  the component scope, which makes Start/Stop literally "mount or don't mount".
- Good: `ImageViewer` decoding to the *layout* size means 128 px thumbnails are
  decoded at 128 px, not at source resolution, with no app-side downscaling.
- Bad: there is no way to update an image in place, and every frame dirties the
  whole tree (no incremental rendering yet), so a 1080p preview costs ~30 % of a
  core.
- Neutral: audio in/out remains entirely outside the framework, as everywhere
  else in this cohort.
