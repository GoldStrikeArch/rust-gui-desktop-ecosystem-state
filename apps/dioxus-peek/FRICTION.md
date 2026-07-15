# FRICTION — dioxus-peek ("Peek", Dioxus 0.7.9 desktop/webview, SPEC-6)

Run dates: 2026-07-10 (four instrumented runs, 00:21–01:48 CEST). Platform:
Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1, wry 0.53.5 / tao
0.34.8 / WKWebView. All empirical claims are macOS-only. Evidence labels:
observed / self-test / synthetic-input / source-only / unexercised.

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| camera_pipeline | **assembled** | **observed** | Two paths, both live. **Primary — JS `getUserMedia`** (camera pixels never touch Rust): `dioxus://index.html` is a *secure context* in WKWebView (probe: `isSecureContext=true`, `mediaDevices=object`), and wry's WKUIDelegate auto-grants the WKWebView-layer media permission (source: wry 0.53.5 `wry_web_view_ui_delegate.rs` always answers `WKPermissionDecision::Grant`), so a `<video autoplay>` + ~40 lines of injected JS is the whole path. Measured with `requestVideoFrameCallback` (frames *presented*): **30.0 fps** sustained over 12 s in three runs (640×480@30). In a dark room the same path presented **16.5 fps** — WebKit's capture adapts frame rate to low light, while the fixed-format nokhwa session below held 30.0 under identical conditions. **Secondary — Rust**: nokhwa 0.10.11 (AVFoundation) capture thread → YUYV→RGB decode + JPEG-q70 encode (`image`) → `use_asset_handler("camframe")` long-poll → page JS `fetch` → blob → `<img>`. Measured **29.9 fps presented / 30.1 fps captured** over 14 s, **≈3.3 ms/frame** decode+encode on the Rust side. There is no framework texture primitive in dioxus-desktop; the webview boundary forces a full encode→custom-protocol→decode round trip per frame. |
| camera_permission_behavior | **built-in*** | **observed** (grant), **unexercised** (denial) | *Built-in in the sense that nothing had to be written: WebKit + TCC handle it, wry auto-grants the WKWebView layer. See "TCC" below for the deliverable details. The in-app denial paths exist (getUserMedia error → in-UI message; `nokhwa_check`/`nokhwa_initialize` with 45 s prompt wait → in-UI message) but a real denial was never provoked — TCC state was never modified per spec. The *device-busy* degrade path WAS exercised repeatedly (see surprises): errors surfaced in-UI, app never crashed. |
| mic_meter | **assembled** | **observed** (self-test) | cpal 0.17 default input (`MacBook Pro Microphone` @ 48 kHz, f32 mono) → RMS+decaying-peak in atomics from the audio callback → 20 Hz `use_future` mirror into signals → CSS-width VU bar (log scale, −60..0 dBFS). Ambient-room readings rms 0.001–0.002 with transient peaks to 0.112 confirm real signal (not TCC-silenced zeros). ~70 LoC. cpal churn: `Stream` is `!Send` (parked thread + channel to stop), `device.name()` deprecated mid-0.17 in favor of `description()`, `SampleRate` newtype became plain `u32`. |
| audio_playback | **assembled** | **self-test** | rodio 0.22 (`default-features=false, features=["playback"]`): `DeviceSinkBuilder::open_default_sink()` + `mixer().add(SineWave 880 Hz × 180 ms × 0.25)`, held 450 ms on its own thread. Returned Ok twice per run, no stderr. Label is self-test because the agent cannot hear the speaker; correctness by API contract. rodio 0.22 renamed the classic `OutputStream`/`Sink` API to `DeviceSinkBuilder`/`MixerDeviceSink`/`Player` and logs to stderr on drop unless `log_on_drop(false)`. |
| thumbnail_grid | **assembled** | **observed** | 200 JPEGs served through `use_asset_handler("gallery")`; handler runs on the main thread so it only parses + enqueues to one IO thread (`mpsc`), which reads the file and responds via the `Send` `RequestAsyncResponder`. UI is a plain CSS grid of `<img loading="lazy">` — lazy-load, async decode, downscale, and texture caching are all WebKit's. Autotest scrolled to the bottom and counted **200/200 `complete && naturalWidth>0`**; grid visually verified (screenshot.png). Cost: process-tree RSS grew ~310→545 MiB while the whole grid was paged in (WebKit decode caches; uncontaminated run-3 sample). UI stayed responsive (tab switches and counters kept updating during load). |
| texture_upload_cost | **hand-rolled** | **observed** | What a fresh camera frame costs, 640×480@30, total process tree incl. WebKit XPC helpers (they reparent to launchd — attributed by baseline-snapshot diff; per-process rows retained in cpu-samples-run*.csv): **JS path 5.2–9.0 %CPU** (breakdown: WebKit.GPU 2.5–6.4 %, WebContent 1.0–1.6 %, app ~1 %) at ~337 MiB tree RSS. **Rust path 32.9 %CPU** (app 18.2 % = capture+YUYV→RGB+JPEG-encode; WebContent 8.2 % = fetch/blob/JPEG-decode/paint; Networking 3.8 %; GPU 2.7 %) at ~354 MiB — ≈4–6× the JS path for the same pixels. Per frame the Rust path is: one YUYV→RGB conversion + JPEG encode (~3.3 ms), one Vec copy into the protocol response, one full JPEG decode in WebContent, one texture upload — no shared-memory or zero-copy channel exists in dioxus-desktop/wry. |

## TCC (deliverable)

Unbundled `cargo` binary, launched from a shell under **Ghostty**:

- **Attribution.** TCC keys camera/mic grants to the *responsible process* —
  the terminal app hosting the shell tree (verified chain: dioxus-peek → zsh →
  claude → zsh → login → Ghostty.app). A standalone Swift probe
  (`verify/tccstatus.swift`) run from the same shell reports
  `camera=authorized mic=authorized`, i.e. the grant belongs to Ghostty, not
  to the binary. Observed.
- **Prompts.** No prompt fired in any of the four runs: both services were
  already authorized for Ghostty before this experiment (granted at some
  earlier point). Consequence, observed across this round's sibling agents:
  *every* unbundled binary run from the same terminal shares one grant,
  framework-independent — camera worked identically for the egui/iced/xilem
  peek apps concurrently. `nokhwa_check()` (AVCaptureDevice authorization)
  returned `true` pre-open; getUserMedia resolved without UI.
- **Persistence.** The grant persisted across runs, rebuilds, binary edits
  and different binaries — it is per-responsible-app, not per-executable.
  Observed (runs 1–4 over 90 minutes plus sibling agents' apps).
- **Denial behavior.** Unexercised (spec forbids modifying TCC state; no
  prompt appeared to deny). Code paths for denial degrade in-UI (status
  strings), verified only for the adjacent "device busy" failure mode, which
  the app survives gracefully (observed, runs 2–3).
- **No Info.plist needed.** The unbundled binary has no usage-description
  strings; nothing crashed — the usage-string abort applies to bundled apps.
  WKWebView's own permission layer is auto-granted by wry (source-only).

## Helper crates (and why)

- `tokio` (features=["time"]) — dioxus-desktop runs on tokio but re-exports
  no timer; every poll/measurement loop needs it directly (same finding as
  iteration 2).
- `nokhwa =0.10.11` (input-avfoundation) — the Rust camera path; dioxus has
  no camera/capture anything.
- `image 0.25` (jpeg only) — JPEG-encode nokhwa frames; version matched to
  nokhwa's own `image` dependency (also already in-tree via dioxus-desktop).
- `cpal 0.17` — mic input (spec-mandated path); same 0.17 line rodio uses,
  so one cpal in the tree.
- `rodio 0.22` (playback only) — beep; decoders disabled.
- `objc2-foundation` (NSProcessInfo, NSString) — **App Nap guard** (see
  surprises). Already in-tree via wry; only enables two binding features.

## LoC split (production vs verification)

- Production: `src/main.rs` **868 lines** (722 non-blank/non-comment),
  including ~150 lines of embedded page JS — the two camera pumps are
  production code (they *are* the frame path), as is the `<video>`/VU/grid UI.
- Verification: `src/autotest.rs` **115 lines** (88 code) — env-gated
  (`PEEK_AUTOTEST=1`) scripted self-test that drives all capabilities and
  prints evidence lines; plus `verify/` **130 lines** of external tooling
  (orchestrate.py process-tree CPU/RSS sampler + window-scoped screenshots,
  findwin.swift, lockprobe.swift, tccstatus.swift). Ratio ≈ 3.5:1.

## Measurements

- Clean release build **321.8 s wall** — *noncanonical*: several sibling
  agents were compiling concurrently on the shared machine (user 319.5 s).
  No-op rebuild 10.0 s (also under contention; later leaf-only rebuilds took
  1.8 s). First `cargo check` after writing the code: 2 errors (cpal 0.17
  `SampleRate` newtype → plain `u32`), then clean.
- Binary **7,187,616 B raw (6.86 MiB) / 6,130,448 B stripped (5.85 MiB)**.
  Dependency graph **305 unique crate names** (incl. the app; +26 over
  dioxus-dash's 279, from nokhwa/cpal/rodio/image-jpeg).
- Launch verification (observed): `launch.log` (= run 4) — window up ~1 s,
  full autotest to `DONE` at 46.7 s, clean SIGTERM exit; env probe on line 1.
  Raw per-second CPU/RSS samples retained in `cpu-samples-run{1..4}.csv`
  (per-process rows; foreign-helper contamination auditable — run 2's tail
  and run 1 are contaminated, runs 3–4 are clean).
- fps numbers: see capability table; source lines in `launch-run*.log`.

## Where the time went

- ~40 % camera-device contention forensics across runs 1–3: run 1's format
  failure (user's live Zoom call held the camera; nokhwa NV12 request can
  never match anyway), run 2's `lockForConfiguration: Lock Rejected` (our own
  just-stopped WebKit capture still held the device), run 3's persistent
  rejection (a sibling agent's xilem-peek held the lock machine-wide for its
  whole lifetime — confirmed with a standalone probe, and confirmed released
  the moment that process exited). Fixes: fallback format chain, retry
  rounds, Rust-path-first ordering, getUserMedia retry on NotReadableError.
- ~25 % the App Nap freeze: mid-run-1 the whole Dioxus/tokio update loop
  stopped while cpal callbacks kept running — diagnosed from the CPU trace
  (0.5 % flatline), fixed with an NSActivity assertion.
- ~20 % measurement hygiene on a shared desktop: WebKit XPC helpers reparent
  to launchd (per-process attribution by baseline diff), window-scoped
  screenshots (`screencapture -l` + CGWindowList) after a region capture
  caught a private window overlapping ours (deleted immediately).
- ~15 % writing the actual app — the Dioxus part was the easy part.

## Surprises

- (+) `dioxus://` is a secure context in WKWebView and wry auto-grants the
  webview media permission: `getUserMedia` in an RSX `<video>` worked first
  try at 30 fps, zero Rust involvement, zero configuration.
- (−) App Nap suspends an occluded unbundled dioxus app *mid-frame-loop* —
  timers/event loop freeze while audio callbacks keep running. A media app
  must hold `NSProcessInfo beginActivity` (not exposed by dioxus/tao; needed
  raw objc2-foundation).
- (−) AVFoundation's configuration lock is machine-wide mutual exclusion and
  nokhwa cannot open a camera without it, while WebKit captures happily
  without exclusive access — so any other nokhwa-style app on the box blocks
  the Rust path but not the JS path.
- (−) nokhwa-bindings-macos 0.2.4 mis-maps pixel formats: 420v/420f report
  as `YUYV`, `NV12` maps to the 10-bit biplanar format — an NV12 request can
  never match a stock FaceTime camera, and `compatible_camera_formats()`
  returns an empty list post-open. Also its `block 0.1.6` dep is
  future-incompat-flagged.
- (−) Occluded windows keep *stale pixels* in the window server: a
  screenshot taken 4 s after switching tabs still showed the previous tab
  (retained: `shot-run3-stale-paint-audio-tab.png`) — iteration-3's "deferred
  paints" lesson made visible.
