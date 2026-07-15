# FRICTION — tauri-peek (SPEC-6 "Peek", media & hardware)

Framework: **Tauri =2.11.5 + tauri-build =2.6.3** (wry 0.55.1 locked), manual
plain-cargo setup cribbed from `apps/tauri-app/` — no Node.js, no npm, no
tauri-cli, no external JS libraries. Reference machine: Apple M4 Pro, 24 GB,
macOS 26.5.2, rustc 1.96.1. All measurements from the unbundled debug binary
(`target/debug/tauri-peek`), dependencies at opt-level 2 (see below).

Evidence labels: observed / self-test / synthetic-input / source-only /
unexercised.

## The architectural headline

In Tauri the natural camera/mic/audio path is **entirely inside the
WKWebView**: JS `getUserMedia` + `<video>`, WebAudio `AnalyserNode`, WebAudio
oscillator. Camera and mic samples **never enter the Rust process** — capture
and compositing happen in WebKit's out-of-process helpers
(`com.apple.WebKit.GPU` / `.WebContent`, XPC services with ppid 1, attributed
to the app via "responsible pid"). Rust's total involvement in the primary
camera path is zero lines. The SPEC-6 Rust-side path (nokhwa → frame →
webview) was also built as a secondary comparison; it works, but slower and
far more fragile (details below).

Enablers for getUserMedia inside the Tauri webview (all observed):

- `tauri://localhost` **is a secure context** (`window.isSecureContext ===
  true`, `navigator.mediaDevices` defined). No config needed.
- wry 0.55.1 implements
  `webView:requestMediaCapturePermissionForOrigin:...` and **unconditionally
  answers `WKPermissionDecision::Grant`** (source-only:
  `wry-0.55.1/src/wkwebview/class/wry_web_view_ui_delegate.rs:126`). So there
  is no browser-style per-origin prompt — the only gate is macOS TCC itself.
  There is no Tauri/wry config surface to intercept or customize this.
- The unbundled cargo binary needs `NSCameraUsageDescription` /
  `NSMicrophoneUsageDescription`. tauri-codegen embeds `./Info.plist` (next
  to tauri.conf.json) into the binary's `__TEXT,__info_plist` section for
  dev-context builds (`dev = cfg!(not(feature = "custom-protocol"))`, i.e.
  always in this manual setup). Verified with `otool -s __TEXT __info_plist`
  (observed).

## Capability audit

### camera_pipeline — **built-in** (primary) / **hand-rolled** (secondary) — observed

Primary (JS getUserMedia → `<video srcObject>`): **30.0 fps median at
640x480@30** ("MacBook Pro Camera"), fps counted with
`requestVideoFrameCallback` (per *presented* video frame — WebKit supports
rVFC). Stream acquisition 103–153 ms warm across runs; 2601 ms on the very
first run (TCC resolution included). There is no frame→texture step to write
or measure in app code: WebKit GPU process captures, WebContent composites.
Notably the getUserMedia path kept delivering 30 fps **while another app held
the camera** (video call in progress) — WebKit's capture is brokered by the
system camera stack and shares the device.

Secondary (nokhwa 0.10.11 AVFoundation → YUYV→RGBA decode in a worker thread
→ latest-frame mailbox → webview polls a `#[tauri::command]` returning
`tauri::ipc::Response` raw bytes (16-byte header + 1.2 MiB RGBA payload) →
`putImageData` on canvas): **19.0 fps median at 640x480@30 YUYV** when the
camera was uncontended (observed, run 3). The cap is Rust-side capture+decode
(`capture_fps≈19.4` logged in the worker), not IPC: the webview drew
essentially every decoded frame. Raw-IPC poll round-trip ≈1 ms (observed:
~165 polls/s against a 5 ms idle sleep when no frames flowed). Two failure
modes observed on later runs when another process held the camera: (a)
`Camera::new: Could not set device property lockForConfiguration ... Lock
Rejected` — nokhwa needs exclusive configuration access that WebKit does not;
(b) once, `Camera::new` hung silently with no error (nondeterministic). Both
degrade gracefully in-UI (error string / 0.0 fps), never crash.

Debug-build trap (observed): with default dev profile (-O0 deps), nokhwa's
YUYV→RGBA conversion managed **2.6 fps at 1920x1080**. Fixed with
`[profile.dev.package."*"] opt-level = 2` in Cargo.toml plus requesting
`Closest(640x480@30 YUYV)` instead of `AbsoluteHighestFrameRate` (which had
picked 1080p).

### camera_permission_behavior — **built-in** (OS-level) — observed, with gaps

For the unbundled cargo binary with the embedded Info.plist:

- **First-ever run**: camera getUserMedia resolved *granted* after 2.6 s; mic
  getUserMedia then stayed pending **>120 s** (our timeout) without resolving
  — consistent with a TCC prompt sitting unanswered on the user's screen (the
  user was on a video call at the time). The app degraded gracefully: promise
  simply never resolved; UI kept running; no crash. I cannot see the prompt
  or its attribution text from this harness (prompts render on the user's
  session; `TCC.db` is unreadable — "authorization denied" — and `tccd`
  unified-log entries are fully redacted), so *which app name the prompt
  displays* for an unbundled binary is **unexercised** here. The camera/mic
  asymmetry on run 1 suggests camera was either quickly Allowed by the user
  or already authorized via the responsible-process chain, while the mic
  prompt went unnoticed.
- **Persistence**: by the next session (3 days later), both camera and mic
  resolved granted in 33–153 ms on every run — the grant **persists across
  runs, rebuilds of the same path, and reboots** (observed). Note the TCC
  identity of an unbundled binary is its path; rebuilding in place kept the
  grant.
- **Denial**: never denied by the user, so the denial path is **unexercised**
  (the code path shows `NotAllowedError` in-UI; getUserMedia rejection
  handling itself is self-tested via the >120 s pending case).
- Rust/nokhwa uses the same TCC service: `nokhwa_initialize`
  (requestAccessForMediaType) returned granted in ~250 ms every time after
  the first session (observed) — one TCC grant covers both paths since both
  are the same process.

### mic_meter — **built-in** — observed

JS-only: `getUserMedia({audio:true})` → `AnalyserNode` (fftSize 2048) →
RMS/dBFS → CSS-width VU bar at 20 Hz (50 ms interval). Rust bypassed
entirely; cpal comparison **unexercised** (not needed — would only re-test
TCC, already covered by nokhwa). Levels in a quiet room: RMS 0.0000–0.0005
(observed; nobody spoke during the runs, so the meter was verified live but
near the noise floor — the bar and dBFS readout updated at 20 Hz throughout).

### audio_playback — **built-in** — observed

WebAudio `OscillatorNode` beep (880 Hz, 150 ms envelope). Worked immediately
**without any user gesture** — `AudioContext.state === "running"` straight
from the verification harness (observed). wry/WKWebView does not enforce
Safari's gesture requirement here. rodio/cpal not needed: **unexercised**.

### thumbnail_grid — **assembled** — observed

200 JPEGs served over Tauri's `asset:` protocol into `<img loading="lazy"
decoding="async">` in a CSS grid. Three pieces of config were required (each
failure mode is silent-ish broken images otherwise):

1. Cargo feature `protocol-asset` on the tauri crate (the handler is
   compiled out without it);
2. `app.security.assetProtocol.enable: true` + scope — the static scope in
   tauri.conf.json can't express a repo-relative dir, but
   `app.asset_protocol_scope().allow_directory(dir, false)` in setup works
   (runtime scope extension, same mechanism the persisted-scope plugin uses);
3. CSP `img-src ... asset: http://asset.localhost` (macOS uses the
   `asset://localhost/<percent-encoded-abs-path>` form generated by JS
   `core.convertFileSrc`).

The ACL/capability system is NOT involved — `core:default` suffices;
`convertFileSrc` is a pure URL helper. Results: initial viewport **80/200
images loaded in ~31 ms**; scrolling loaded the rest lazily, **200/200 in
~3.9 s total, 0 errors**; decode/downscale/caching is entirely WebKit's
(async decode off the main thread; no jank observed; no app-side cache
code exists). The UI thread never blocked — Rust only returned the 200 path
strings (~1 ms `list_images` command).

### texture_upload_cost — **n/a for the primary path** — observed (partial)

There is no app-visible texture upload for getUserMedia; what replaces it is
**WebKit compositor cost across the process family**. Sampled with `ps`
(decaying %CPU) during 640x480@30 preview on the M4 Pro: app process
~1.4–1.8%, `com.apple.WebKit.GPU` ~0.7–2.4%, `com.apple.WebKit.WebContent`
~2–10% → **total ≈ 4–12% of one core** (observed; shared desktop, so treat as
a range, not a point). RSS of app+helpers ≈ 280–440 MiB depending on whether
the gallery had loaded. For the secondary path, one frame costs: RGBA decode
+ 1.2 MiB Vec copy + raw-IPC transfer + `putImageData`; the poll loop alone
(no frames flowing) cost app ~4% + WebContent ~5% (observed); CPU *with*
frames flowing was not sampled — the camera became contended before I could
repeat the measurement (**unexercised**; the 19 fps ceiling itself is
observed).

## Helper crates (and why)

- `nokhwa =0.10.11` (`input-avfoundation`) — SPEC-6's mandated Rust camera
  path. Pulls an old `objc`/`block` stack (cargo warns `block v0.1.6` will be
  rejected by a future rustc — future-incompat, worth knowing).
- `serde`/`serde_json` — required by `#[tauri::command]` IPC (same as base
  app). No other helpers; **no JS libraries** (vanilla DOM + `window.__TAURI__`).

## LoC split (production vs verification)

- Production: **~663** — Rust ~243 (src/main.rs 263 − ~20 lines of
  verification hook commands `verify_mode`/`log_stat`), frontend 420
  (index.html 72, main.js 296, styles.css 52). build.rs 10.
- Verification: **~205** — ui/verify.js 146 (phase auto-driver), 
  verify/cpu_sample.sh 39, the two Rust hook commands ~20.
- The in-UI live FPS counters are counted as production: SPEC-6 requires them.

## Sizes (MiB)

- Debug binary: **14.4 MiB** (deps at opt-level 2; was 24 MiB at -O0).
- `target/`: **2845 MiB** after full build; 451 packages in Cargo.lock.
- peek-assets: 3.2 MiB (200 JPEGs, served from disk, not embedded).
- App RSS at camera preview incl. WebKit helpers: ~280–440 MiB (observed).

## Build & launch verification — observed

- `cargo build` clean on first attempt, 1m25s cold (lockfile copied from
  tauri-app pins the identical tauri tree); 9m20s after adding the dep
  opt-level override (full dep rebuild).
- Multiple launches (~25–80 s each) driven by `PEEK_VERIFY=1|rust` phases;
  every capability exercised live; window-scoped screenshot
  (`screenshot.png`, Gallery tab) taken via CGWindowID so no other window
  content could leak in.
- Gotcha: frontend assets are embedded at proc-macro expansion
  (`generate_context!`), and editing `ui/*.js` does **not** dirty the crate —
  `cargo build` no-ops and you run stale UI. `touch src/main.rs` first.

## Where the time went

1. Camera contention forensics: distinguishing "nokhwa is broken" from "the
   user's video call holds the camera" (Lock Rejected vs one silent hang vs
   19 fps success) took several runs across sessions.
2. The debug-codegen decode trap (2.6 fps → profile override → 9m20s rebuild).
3. TCC observability: prompts, `TCC.db`, and `tccd` logs are all invisible
   from the harness; permission behavior had to be reconstructed from
   getUserMedia latencies across runs.
4. CPU attribution for XPC helpers (ppid 1): `launchctl procinfo` parsing was
   unreliable; ended up diffing WebKit pids before/after launch.
5. Privacy handling on a shared desktop: two screenshots had to be discarded
   (region capture caught an unrelated video-call window; a later
   window-scoped capture showed a stale private camera frame on the canvas);
   final screenshot policy = window-scoped + Gallery tab only.
