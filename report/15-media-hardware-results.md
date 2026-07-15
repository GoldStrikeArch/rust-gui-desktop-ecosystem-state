# Media & hardware: "Peek" in seven frameworks (macOS)

**Run dates:** 2026-07-09..10. Evidence labels per cell: observed /
self-test / synthetic-input / source-only / unexercised (per-app FRICTION.md
files carry the full notes; raw rows in
[data/iter4-rows.md](data/iter4-rows.md)).

Iteration 4, SPEC-6: camera preview (~30 fps, the per-frame texture-upload
benchmark), mic level meter (cpal), audio playback (rodio), and a
200-JPEG thumbnail gallery, per framework. All seven built, launched, and
showed a live camera preview at ~30 fps. Tauri is a recorded specification
deviation for audio: it used WebAudio/getUserMedia rather than the required
cpal mic and rodio-or-cpal playback path.

## The headline: observed texture-path CPU outcomes

Broadly the same task — put a camera frame on screen ~30 times a second — but
not a controlled cross-framework benchmark. Costs are self-observed `ps`
summaries at the listed resolutions; only Dioxus retained raw per-sample CPU
data, while the other rows retain per-app narrative summaries and supporting
artifacts of varying strength:

| Framework | Path | fps | CPU (one core) | Mechanism |
|---|---|---|---:|---|
| tauri | JS getUserMedia | 30 | ~4–12% (process family) | camera never touches Rust; WebKit brokered capture |
| dioxus | JS getUserMedia | 30 | 5.2–9.0% (family) | same; 16.5 fps in a dark room (WebKit adaptive capture) |
| gpui | **zero-copy surface** | 30–31 | **~8%** | `surface()`/CVMetalTextureCache — IOSurface *is* the Metal texture |
| egui | CPU upload (720p) | 29–30 | ~16% | TextureHandle.set = full re-upload; conversions dominate |
| gpui (alt) | CPU upload | 30 | ~22% | NV12→BGRA convert + atlas upload; must drop old frames or the atlas leaks |
| iced | CPU upload (1080p) | 30 | ~28% | new image Handle per frame (handles are immutable + identity-keyed) |
| slint | CPU upload (1080p) | 30 | ~34% | no global keyed cache for buffer Images (`ImageCacheKey::Invalid`); changing camera images re-creates/uploads textures |
| dioxus (alt) | Rust→JPEG→webview | 29.9 | ~33% (family) | 4–6× the JS path for identical pixels |
| xilem | CPU upload (1080p) | 29.5–30.6 | **~60–75%** | fresh 7.91 MiB Blob per frame (blob-id equality is the dirty-check) + custom paint-only widget |

Reading: every implementation reached about 30 fps on this M4 Pro. The observed
CPU spread is consistent with texture-path architecture mattering, but it is
not attributable purely to that variable because resolution, microphone state,
process scope, helper attribution, and machine contention differed. GPUI ships
the sample's only explicit native-framework API and Rust-visible
IOSurface→Metal-texture path (public but undocumented), and its
Metal renderer asserts specifically the `420f` full-range NV12 format; `420v`
and other formats panic. The webviews sidestep the problem entirely via
getUserMedia (zero config: wry hard-codes the WKWebView media-capture grant
and `tauri://`/`dioxus://` are secure contexts) — with the notable bonus that
WebKit's *brokered* capture kept streaming while other apps held the camera,
where AVFoundation's machine-wide configuration lock gave native apps
`Lock Rejected`.

## The permission story (TCC) — four observations on one host

On this machine, **unbundled cargo binaries** launched from the same terminal
inherited an existing camera/mic grant associated with the responsible-process
chain. Four agents observed authorization persisting across tested runs,
rebuilds, and frameworks, but they shared the same host, terminal, and prior
grant; this is repeated local observation rather than four independent TCC
conditions or a universal macOS rule. Denial paths were coded but not exercised
because the audit did not modify TCC. Tauri adds a
twist: `tauri-build` embeds `Info.plist` into dev binaries
(`__TEXT,__info_plist`), so usage-description keys work even unbundled.

## The nokhwa problem (upstream-actionable)

Every tested path that used `nokhwa` 0.10.11 with this host's FaceTime camera
hit the same wall before the first frame. Four native implementations used it;
GPUI deliberately used direct AVFoundation instead because nokhwa's CPU-copy
path could not feed GPUI's zero-copy surface API:
1. macOS bindings **mis-map Apple's NV12 fourccs (420v/420f) to
   `FrameFormat::YUYV`** (and NV12 to a 10-bit format) — with the tested
   nokhwa/bindings versions, an NV12 request did not match this host's FaceTime
   camera.
2. `set_all` matches only a range's **max** fps while `supported_formats`
   also advertises the mins — every "@15fps" entry it enumerates is
   unsettable; even `RequestedFormatType::None` can fail.
3. `=0.10.9` no longer compiles (nokhwa-bindings-macos 0.2.4 broke a channel
   type within the patch range).
Working recipe converged on by the agents: `AbsoluteHighestFrameRate` or
`Closest(YUYV ...)` + a fallback ladder. Also: `Camera::new` fails with
`Lock Rejected` (or hangs, n=1) while any other AVFoundation app holds the
camera. The tested apps needed bounded retry/degradation handling under that
contention; this does not establish that every device or deployment requires a
retry loop.

## Audio + gallery (the quieter cells)

- **Mic meter**: cpal worked in six implementations with the same broad shape:
  a dedicated thread because `Stream` is `!Send`, an RMS atomic, and ~20 Hz UI
  polling. One hazard: a
  *pending* TCC prompt is invisible at the API level — streams build and
  deliver zero callbacks.
- **Playback**: rodio 0.22 worked in the six implementations that used it;
  Tauri used a WebAudio oscillator. Rodio renamed core stream/sink types
  (for example `OutputStream`→`MixerDeviceSink`) while pinning cpal 0.17
  (cpal is at 0.18), so many older examples require mechanical updates; the
  upstream changelog describes the renamed functionality as equivalent.
  Audibility was API-verified, not ear-verified.
- **Gallery**: all implementations loaded 200 JPEGs; reported local times
  ranged from 0.14 s to 4 s, but a raw comparable timing trace was not retained
  for every app. The *caching story* differs: gpui caches decoded images app-wide by
  path (built-in); webviews delegate to WebKit; egui/iced/slint/xilem manage
  textures manually with framework-specific cache semantics.
- **App Nap** is a real hazard for media apps: it stalled egui's
  `request_repaint_after`-only loop 45 s and froze dioxus's *entire*
  event loop while occluded; no `NSActivity` assertion API was found in the
  audited framework surfaces.

## Caveats

macOS/M4 Pro only; one implementation per framework; CPU numbers are
self-observed `ps` summaries at stated resolutions, with raw per-sample data
retained only for Dioxus. The machine also ran sibling agents, and agents
re-measured when contention was visible. TCC
denial branches unexercised; audio audibility not human-verified; Tauri's
WebAudio path deviated from the cpal/rodio audio requirements. Camera
frames containing the user were kept out of the repo by agent policy.
