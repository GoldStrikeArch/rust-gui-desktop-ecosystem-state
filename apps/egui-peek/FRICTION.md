# FRICTION.md — Peek (egui 0.35 + eframe)

App: `apps/egui-peek/` · package `egui-peek` · `cargo run --release`
Window: "Peek (egui)", 900×600, tabs Camera / Audio / Gallery.

Verified on macOS 26.5 (M4 Pro): release build clean (no warnings), 2
egui_kittest tests pass, four live launches (30–45 s each, exit 0), in-app
wgpu screenshots of all three tabs (`screenshot-camera-header.png` — header
strip only, the full camera frame contains the user —,
`screenshot-audio.png`, `screenshot-gallery.png`).

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| camera_pipeline | **assembled** | **observed** | nokhwa `Camera` on a dedicated thread → `mpsc::sync_channel(2)` of `egui::ColorImage` (try_send, latest-wins, drops counted) → UI thread creates one `TextureHandle` (`ctx.load_texture`) then `tex.set(..)` per frame; capture thread calls `ctx.request_repaint()` per frame. **Measured live: 29–30 fps presented at 1280×720 for 30 s+, 1 frame dropped in ~850** (FPS counts texture updates in painted frames only — see occlusion note below). Getting the camera to *open* burned the time; the egui half is ~30 LoC and just works. |
| camera_permission_behavior | **assembled** | **observed** (grant path) / **unexercised** (denial) | `nokhwa_check()` + `nokhwa_initialize(cb)` wired to a Pending/Granted/Denied state machine with spinner + in-UI error. Observed: TCC grant attributed to the terminal app persisted from an earlier session — check returned `true` at t=0, camera+mic streamed immediately, **no prompt ever fired** across 6+ launches of a freshly built binary. Denial degrades to an in-UI error without crashing (code path never triggered). Full TCC writeup below. |
| mic_meter | **assembled** | **observed** | cpal `build_input_stream` (F32) → RMS per callback into an `AtomicU32` → painter-drawn VU bar (dBFS mapping −60..0, color bands, 1 s peak-hold) refreshed via `request_repaint_after(50 ms)` (~20 Hz, zero repaints when off). Observed live: "MacBook Pro Microphone — 1 ch @ 48000 Hz, F32", ambient RMS 0.01–0.07 while music/typing nearby, −63.9 dBFS in a quiet room (screenshot). |
| audio_playback | **assembled** | **self-test** | rodio 0.22 (API reworked vs 0.20!): `DeviceSinkBuilder::open_default_sink()` kept in app state, `sink.mixer().add(SineWave::new(880.0).take_duration(150ms).amplify(0.2))`. Triggered programmatically (verif hook): sink opened (`DeviceSinkConfig { 2 ch, 48000 Hz, F32 }`), mixer accepted the source, no error, UI shows "1 beep(s) played". Audible output not humanly confirmed — hence self-test, not observed. No TCC gate for output. |
| thumbnail_grid | **hand-rolled** | **observed** (+ synthetic-input in tests) | egui has no async image loading in core; 4 worker threads pull from an atomic index over the sorted file list, `image::open` → `thumbnail(192)` → `ColorImage`, channel to UI, `ctx.load_texture` per thumb capped at 16 uploads/frame; `ScrollArea` + `horizontal_wrapped` grid with placeholder cells. **Observed: 200/200 JPEGs decoded + uploaded in 0.14 s** (they're small synthetic JPEGs, 3.2 MiB total). kittest test drives the same pipeline with generated JPEGs (synthetic-input). Texture caching is entirely manual — handles are ref-counted, texture freed when the last handle drops; egui never caches for you. `egui_extras::install_image_loaders` was the sanctioned assembled alternative (URI-keyed cache, but decodes full-size on the UI thread pool and gives no downscale control) — recorded, deliberately not used. |
| texture_upload_cost | **built-in** (mechanism) | **self-test** | `TextureHandle::set` issues `ImageDelta::full` — a **full-texture re-upload every frame** (partial updates exist via `set_partial`, unused here). Cost chain per 1280×720 frame: YUYV→RGB decode (nokhwa, capture thread) → RGB→`Color32` copy (~3.5 MiB, capture thread) → egui tex upload (~3.5 MiB/frame ≈ 105 MiB/s at 30 fps) → paint. **Self-observed via `ps`: 13.4–19.0 %CPU (mean ≈ 15.8 % of one M4 Pro core) for the whole app** during 30 fps preview incl. mic stream; RSS ~280–300 MiB (wgpu). The GPU upload is not the bottleneck; the two CPU-side pixel conversions are. |

## TCC (macOS permissions) — deliverable

- **Attribution.** The unbundled cargo binary has no bundle ID; TCC walks up
  to the *responsible process*. Process chain observed at build time:
  `egui-peek ← zsh ← claude ← zsh ← login ← Ghostty.app`. Camera/mic access
  is therefore attributed to **Ghostty** (the terminal emulator), not to the
  binary — any prompt would name Ghostty, and grants land on Ghostty's TCC
  record.
- **Persistence.** Observed: `nokhwa_check()` returned `true` at t=0 on the
  very first launch of this freshly compiled binary, and the cpal mic stream
  delivered live samples instantly. The grant (made for Ghostty in some prior
  session) **persists across runs and across different unbundled binaries**
  run from the same terminal app. Consequence for this experiment: **no
  prompt ever appeared**, so prompt UX could not be re-observed
  (label: unexercised); the 30 s wait rule was never triggered.
- **Denial behavior.** Not exercisable without revoking TCC state (forbidden
  by the rules). Code path (source-only): `nokhwa_initialize(false)` →
  `Denied` state → red in-UI error naming System Settings; cpal errors →
  in-UI error. No unwraps on those paths; the app keeps running (camera tab
  usable, other tabs unaffected).
- **Quirk (observed).** `AVCaptureDevice` enumeration/open did not itself
  prompt — with authorization already granted the camera opens even without
  calling `nokhwa_initialize` first.

## Helper crates & why

- `nokhwa = "=0.10.11"` (`input-avfoundation`) — the spec-mandated camera
  crate; the feature also gates `nokhwa_initialize`/`nokhwa_check` (TCC).
- `cpal = "=0.17.3"` — mic input; pinned to the same 0.17 line rodio uses so
  only one cpal builds. 0.17 renamed `Device::name()` → `description()` and
  `sample_rate()` now returns plain `u32` — pre-0.16 snippets don't compile.
- `rodio = "=0.22.2"` (`default-features = false, features = ["playback"]`) —
  audio out without the symphonia decoder stack. The 0.21+ API is a full
  rework: `OutputStream::try_default()`/`Sink` are gone; it's
  `DeviceSinkBuilder`/`MixerDeviceSink`/`Player` now.
- `image = "=0.25.10"` (`jpeg`, `png` only) — gallery decode on worker
  threads + PNG encode for the screenshot hook.
- dev-only: `egui_kittest = "=0.35.0"` (AccessKit-driven UI tests),
  `nokhwa-bindings-macos = "=0.2.4"` (used by `examples/probe.rs` to dump the
  raw AVFoundation format list; already in-tree as nokhwa's own dep).
- Not used: `egui_extras` image loaders (see thumbnail_grid note).

## The nokhwa format-negotiation trap (cost ~1/3 of the time)

Out of the box, **every obvious request fails on the M4 MacBook Pro camera**,
including nokhwa's own default:

- `Closest(NV12 1280×720@30)` → "Cannot fulfill request": the bindings map
  Apple's NV12-family fourccs (`420v`/`420f`, incl. 10-bit) to
  `FrameFormat::YUYV`, so **NV12 never appears** in the enumerated formats
  and `Closest` hard-requires the FrameFormat to exist.
- `RequestedFormatType::None` → picks 640×480@**15** YUYV from the enumerated
  list, then `set_all` errors "Not Found/Rejected/Unsupported":
  `supported_formats()` explodes each `AVFrameRateRange` into min *and* max
  fps, but `set_all` only matches a range by its **max** fps (±1). Every
  "@15 fps" entry the crate itself enumerates is unsettable (ranges here are
  15–30).
- What works: `Closest(YUYV 1280×720@30)` (probe output: 640×480, 1280×720,
  1760×1328, 1328×1760, 1552×1552, 1920×1080, 1080×1920 — all "YUYV", all
  fps_list [15, 30]). Production code uses a 3-step fallback ladder
  (YUYV@30 → AbsoluteHighestFrameRate → None) with the errors surfaced in-UI.
  Delivered buffers are real interleaved yuvs (the bindings force the output
  pixel format), so nokhwa's YUYV→RGB decode yields correct colors —
  verified visually in the screenshot.

## Repaint / scheduling notes

- Camera: event-driven — capture thread `ctx.request_repaint()` per frame;
  frame rate is capture-driven, not display-driven. Mic: polled at 20 Hz via
  `request_repaint_after`. Idle app: zero repaints.
- **App Nap (observed):** with only `request_repaint_after` timers pending
  (no cross-thread `request_repaint()`), the unfocused, unbundled app got
  napped by macOS — a 1 Hz log loop stalled for **45 s** straight. Cross-
  thread `request_repaint()` (camera/gallery threads) reliably wakes the
  winit loop; timer-only UIs in background windows cannot trust
  `request_repaint_after` deadlines on macOS.
- eframe skips painting while the window is minimized/fully occluded
  (`ViewportInfo::visible()`); `App::ui` still runs. The FPS counter gates on
  `i.viewport().visible()` so it reports frames actually presented — on this
  shared desktop the window spent whole runs occluded and the naive count
  would have lied. This also silently disables `ViewportCommand::Screenshot`
  (capture happens in the paint path); the screenshot hook sends
  `ViewportCommand::Focus` (own window only) 1 s before capturing.

## LoC split & sizes

- `src/main.rs`: 1132 lines total = **878 production** + **194 verification
  hooks** (`mod verif`: env-gated autostart/log/screenshot/exit, inert
  without env vars) + **60 tests**. Plus `examples/probe.rs` 45 (verification
  tool). Verification total: 299.
- Binary: **13.6 MiB** release (iteration-1 egui-app: 12.0 MiB — the whole
  nokhwa+cpal+rodio+image stack adds only ~1.6 MiB). `target/release`: ~804 MiB. Assets: 3.2 MiB (200
  JPEGs). RSS at preview: ~280–300 MiB. Thumbnail VRAM ≈ 21 MiB
  (200 × ~192×144 RGBA); camera texture 3.5 MiB.
- Deps: 5 direct, 478 locked packages. `block v0.1.6` (via nokhwa's objc
  stack) emits a future-incompat warning on rustc 1.96.

## Where the time went

~35 % nokhwa format negotiation (probe tool + reading nokhwa/bindings
source); ~20 % capture/channel/texture pipeline + honest-FPS design; ~15 %
verification hooks and screenshot plumbing (incl. discovering the occlusion
and App Nap behaviors); ~15 % TCC/launch runs and CPU measurement; ~10 %
version-checking the audio crates' reworked APIs; ~5 % docs.
