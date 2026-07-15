# SPEC-6: "Peek" — media & hardware test

Iteration 4. Tests the initiative's "native hardware integration" hard part:
camera, microphone, audio out, and bulk image handling. The camera preview
doubles as a **per-frame texture-upload benchmark** — how does this framework
get a fresh RGBA buffer on screen 30 times a second?

## Functional requirements

1. **Window** titled `Peek (<framework>)`, ~900×600, three sections (tabs or
   stacked): Camera, Audio, Gallery.
2. **Camera preview**: live preview from the default camera at ~30 fps via
   the `nokhwa` crate (AVFoundation backend on macOS), rendered through the
   framework's image/texture mechanism. Show a live FPS counter (measured
   frames actually presented, not captured). Start/Stop button.
   - Webview frameworks: implement the Rust-side path (nokhwa → frame →
     webview surface) OR document precisely why JS `getUserMedia` is the only
     viable path and use it — the architectural difference (camera data
     bypassing Rust entirely) is a key finding either way. If both are quick,
     compare.
3. **Mic level meter**: `cpal` input stream → RMS level → live VU bar
   updating ~20 Hz. Start/Stop.
4. **Audio playback**: `rodio` (or cpal output) beep/click on a button.
5. **Gallery**: scrollable thumbnail grid of the 200 JPEGs in
   `apps/peek-assets/` (load asynchronously — the UI must not block while
   decoding; document the decode/downscale/cache story and whether the
   framework caches textures for you).

## Permissions (a deliverable, not an obstacle)

macOS TCC will gate camera/mic. **Document exactly what happens** for an
unbundled cargo binary: which process the prompt attributes to, whether a
denial crashes or degrades the app, whether approval persists across runs.
The user has agreed to click Allow when prompts appear — if a prompt is
pending, wait briefly; if access is denied or no prompt fires, degrade
gracefully (show the error in-UI) and record the behavior with an evidence
label. Do NOT modify TCC databases or fight the system.

## FRICTION.md (required — audit conventions)

Per capability: rating (built-in / assembled / hand-rolled / not-achievable)
+ **evidence label** (observed / self-test / synthetic-input / source-only /
unexercised) + 1–3 sentence note:
camera_pipeline (the frame→texture path + measured preview fps),
camera_permission_behavior, mic_meter, audio_playback, thumbnail_grid,
texture_upload_cost (what one frame costs: copy? full-texture re-upload?
measured CPU% at 30 fps if you can self-observe).
Also: helper crates + why; **LoC split: production vs verification hooks**;
sizes in **MiB**; where the time went.

## Implementation rules

Independent crate `apps/<framework>-peek/` (package `<framework>-peek`), same
pinned framework version as `apps/<framework>-app/`, fallback rule with
documented gaps, build + ~10 s launch verification (evidence-labeled). Shared
desktop: other agents run concurrently — scope interactions to your own
window (AX/pixel-verify before clicking), never toggle system-wide state.

## Reference machine

Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
