# FRICTION — Peek (gpui =0.2.2)

Reference: SPEC-6.md. Built + verified on macOS 26.x (M4 Pro, rustc 1.96.1).
`cargo build --release`: clean first try, **7 m 53 s** cold (fresh target dir,
all deps), binary **7.9 MiB** unstripped, **640 unique crate names** in
Cargo.lock (gpui-app iteration 1 was 391; the camera/audio stack adds ~250).
Only warnings: the known `objc`-macro cfg lint noise + the `block v0.1.6`
future-incompat note gpui itself carries. Same `runtime_shaders` pin/trap as
apps/gpui-app (see its GAPS.md).

## Headline finding

gpui has a **real zero-copy video path**: `gpui::surface(CVPixelBuffer)` →
`Window::paint_surface` → Metal renderer binds the buffer's two NV12 planes
straight off the IOSurface via `CVMetalTextureCache` (the plumbing Zed uses
for screen-share tiles). The catch: the renderer **`assert_eq!`s the pixel
format** to `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` ("420f") — feed
it anything else and the app panics in paint. AVFoundation happily delivers
exactly that format, so camera→screen costs no per-frame CPU conversion or
texture upload at all. gpui ships **no camera API**, so the capture side is
hand-written AVFoundation/objc glue — but using only crates gpui already
depends on (`objc`, `media`/gpui_media, `core-video`), cribbed almost
line-for-line from gpui's own `platform/mac/screen_capture.rs`.

**Why not nokhwa (spec suggestion):** nokhwa's AVFoundation backend copies
every frame out of the CVPixelBuffer into a `Vec<u8>` and CPU-converts to
RGB; gpui's surface element wants the IOSurface-backed buffer itself. Using
nokhwa would have forced the CPU-upload path below as the *only* path (and
added a second AVFoundation binding stack). Decision: direct AVFoundation
(~370 LoC incl. the NV12→BGRA comparison converter). [source-only for
nokhwa's internals — read `nokhwa-bindings-macos` 0.2.4, did not build it]

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| camera_pipeline | **assembled** | **observed** | AVCaptureSession + AVCaptureVideoDataOutput (videoSettings pinned to 420f) → delegate on a serial dispatch queue → `Mutex<Option<CVPixelBuffer>>` + capacity-2 wake channel → entity task sets the frame + `cx.notify()` → `surface(frame.clone())` in render. **30–31 fps presented** sustained (measured as renders that painted a not-yet-presented frame, 1 s trailing window), at 640×480 and at 1920×1080 depending on which default camera macOS picked (see surprises). Screenshot-verified live preview, correct colors, `ObjectFit::Contain`. When the window is fully occluded gpui stops presenting (fps→0) while capture keeps running at ~30/s — presented ≠ captured is directly visible in the logs. |
| camera_permission_behavior | **hand-rolled** | **observed** (granted path) / **unexercised** (denial path) | gpui has zero TCC surface; ~30 LoC of `AVCaptureDevice authorizationStatusForMediaType:` / `requestAccessForMediaType:` (objc + block). Observed: status was **already `authorized` at first-ever launch** of this binary — the TCC grant belongs to the *responsible process*, the terminal ancestor (parent chain: gpui-peek ← zsh ← claude ← zsh ← login ← **Ghostty.app**), not to the unbundled binary; no prompt fired in any run, and the grant persisted across four separate launches (and trivially across rebuilds, since it predates the binary). The request/denied/restricted branches (in-UI error, no crash) are written but could not be exercised without modifying TCC state (not done); TCC.db is unreadable without Full Disk Access ("authorization denied" — left untouched) and TCC unified-log entries are redacted. No Info.plist/usage-description embedding was needed — expected only if the binary were its own responsible process. |
| mic_meter | **assembled** | **observed** | cpal 0.17 default input stream (f32/i16/u16 handled) → RMS per callback into an `AtomicU32` → 20 Hz gpui timer task smooths (fast attack/slow decay) + `cx.notify()` → fixed-width bar div. Live values observed: RMS 0.0002–0.11 tracking real ambient sound across runs; ~94 callbacks/s. Note: on macOS a mic-TCC *denial* would not error — CoreAudio just delivers silence — so the meter shows 0; with access granted we saw nonzero signal, distinguishing the two. |
| audio_playback | **assembled** | **self-test** | rodio 0.22 (`default-features = false, features=["playback"]`): `DeviceSinkBuilder::open_default_sink()` kept for app lifetime, `SineWave 880 Hz → take_duration(180 ms) → amplify(0.25)` into `sink.mixer().add(...)`. "BEEP ok" with no error on every trigger; nobody was in the room to confirm audibility, hence self-test not observed. rodio 0.22 renamed the 0.20-era API (OutputStream → MixerDeviceSink et al.). |
| thumbnail_grid | **built-in** | **observed** | `img(Arc<Path>)` + `uniform_list`. gpui's asset system does the whole async story for you: `fs::read` + decode run on the background executor, the element re-renders when ready, and the decoded BGRA `RenderImage` is cached **app-wide keyed by path** (GPU sprite-atlas upload on first paint, reused after). `uniform_list` virtualizes rows (7 tiles/row × 29 rows), so only visible rows' images are requested at all. No built-in downscale: the cache holds full-size decoded images — fine for these 200 small JPEGs (3.2 MiB total on disk), a real memory concern for camera-roll-sized sources. Scrolling not scripted (no synthetic input in this env); grid render + async population screenshot-verified. |
| texture_upload_cost | **built-in** (zero-copy path exists) | **self-test** (ps sampling + in-app counters) | Same machine, same 1920×1080@30 camera: **zero-copy surface path ≈ 5–10 % CPU** (typ. ~8 %, RSS ~226 MiB) — per frame it's a CFRetain + scene node; the Metal texture is the IOSurface, no copy, no upload. **CPU path (what img()-only frameworks must do): ≈ 16–26 % CPU** (typ. ~22 %), of which NV12→BGRA in scalar Rust is 2.9–5.0 ms/frame, plus a full 1920×1080×4 ≈ 7.9 MiB atlas upload per frame (new `Arc<RenderImage>` each frame; old ones must be freed by hand with `window.drop_image` or the atlas grows without bound — with it, RSS was stable at ~265 MiB). At 640×480 the zero-copy run measured 4.5–6.5 %. Idle app: 0.3 %. |

## Helper crates (and why)

| Crate | Why |
|---|---|
| `objc` 0.2, `block` 0.1 | Delegate class (`ClassDecl` + `msg_send!`) and the requestAccess completion block — same stack gpui's mac backend uses. |
| `media` (= `gpui_media` 0.2.2) | `CMSampleBuffer::image_buffer()` — Zed's own CoreMedia binding, already in gpui's tree. |
| `core-video` =0.4.3, `core-foundation` =0.10.0 | The exact `CVPixelBuffer` type `gpui::surface()` accepts (pins match gpui's so the types unify), + TCFType wrap/retain. |
| `dispatch` 0.2 | `dispatch_queue_create` for the sample-buffer delegate queue. |
| `image` 0.25, `smallvec` 1 | Construct `RenderImage::new(SmallVec<[image::Frame;1]>)` for the CPU comparison path (versions unify with gpui's own). |
| `futures` 0.3 | Camera-thread → UI wake channel + permission oneshot (already in gpui's tree). |
| `cpal` 0.17 | Mic input (spec-mandated). rodio 0.22 shares the same cpal 0.17. |
| `rodio` 0.22 (playback only) | Beep. Decoder features off. |

None of these added a new native-binding ecosystem beyond what gpui already
links; the only genuinely new subsystems are cpal/rodio (audio).

## LoC split (cloc-style raw `wc -l`, incl. comments)

- **Production: ~1,274** — main.rs 768 (UI/entity/pump minus hooks below),
  camera.rs 379, audio.rs 131, build.rs 9, minus the 13 log-hook lines.
- **Verification hooks: ~95** — verify.rs 82 (`PEEK_AUTOSTART` /
  `PEEK_STATS` / `PEEK_MODE` env hooks + STATS printer, needed because
  synthetic clicks/keystrokes are blocked in this environment) + 13
  `println!` transition lines (CAMERA_AUTH/MIC/BEEP/STATS evidence) inside
  production files.

## Sizes (MiB)

- Binary: **7.9 MiB** (release, unstripped; gpui-app iteration 1 was 5.0).
- Target dir after one release build: **1.3 GiB** (~1,331 MiB).
- Assets: 200 JPEGs, 3.2 MiB; RSS at full tilt (camera 1080p30 + gallery):
  **~220–265 MiB**.

## Launch verification

- Run 1 (`PEEK_AUTOSTART=camera,mic,beep PEEK_STATS=1`): alive 418 s, 30.0
  fps presented from t=4 s, mic RMS live, beep OK, gallery 200; exited 0 when
  its window was closed (quit-on-last-window-close wired as in gpui-app).
  [observed]
- Idle run: window screenshot saved to `launch-idle.png` (camera off, mic
  off, gallery populated); 0.3 % CPU, alive >10 s, killed cleanly.
  [observed — screenshot in repo]
- Live-preview screenshots (camera frames visible, both modes, correct
  colors, fps counter on screen) were taken and reviewed but **not
  committed** — they show the user via their camera. [observed]
- Window interaction was scoped to this app's own window: window IDs were
  resolved per-PID (scripts/window-count.swift) and `screencapture -l<id>`
  captures only that window; no clicks/keys were injected, no system state
  touched.

## Where the time went

1. **Reading gpui + renderer source before writing any code** — finding
   `surface()`/`paint_surface`, discovering the NV12 `assert_eq!` (which
   dictated the whole capture configuration), confirming `img()`'s async
   asset path and `window.drop_image`, and pinning the four crates whose
   *types* must unify with gpui's (core-video/core-foundation/image/
   smallvec). This is what made the build compile and run correctly on the
   first attempt.
2. **AVFoundation objc glue** — delegate class, videoSettings dict,
   permission block plumbing; made tractable by cribbing gpui's own
   screen-capture delegate wholesale.
3. **Re-running the benchmark for a fair comparison** — the default camera
   changed between runs (640×480 vs 1920×1080 device), so zero-copy vs CPU
   had to be re-measured at matched resolution; plus a session interruption
   mid-experiment.

## Surprises

- The zero-copy surface path not only exists but is *public* API in 0.2.2 —
  and undocumented outside Zed's own usage; the NV12-only assert is a
  landmine (a format guard in app code is mandatory or paint panics).
- TCC never prompted: for a terminal-launched unbundled binary the grant
  rides on the terminal app (Ghostty), so camera+mic were authorized before
  this binary ever ran.
- gpui stops presenting occluded windows: presented-fps drops to 0 while
  capture continues — the "frames presented, not captured" spec distinction
  is directly observable.
- `RenderImage`-per-frame leaks GPU atlas memory unless you call
  `window.drop_image` yourself; nothing warns you.
