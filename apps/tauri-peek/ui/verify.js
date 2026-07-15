// Peek (Tauri) — VERIFICATION instrumentation only (not product code).
// Always logs an environment probe; when the app is launched with
// PEEK_VERIFY=1 it auto-drives every capability in sequential phases and
// streams evidence lines to stdout via the log_stat command, so an external
// harness can scrape fps / permission latency / gallery progress and knows
// when to sample CPU (phase=..._begin markers).
(async function () {
  const { invoke } = window.__TAURI__.core;
  const log = (line) => invoke("log_stat", { line }).catch(() => {});
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const P = window.Peek;

  log(`boot secureContext=${window.isSecureContext} mediaDevices=${typeof navigator.mediaDevices} ` +
      `rVFC=${typeof HTMLVideoElement.prototype.requestVideoFrameCallback} href=${location.href}`);
  log(`ua=${navigator.userAgent}`);

  const mode = await invoke("verify_mode");
  if (!mode) return;
  log(`verify_mode=${mode} — auto-driving phases`);
  const rustOnly = mode === "rust";

  // Wait until pred() is true or timeout; polls 250 ms.
  async function waitFor(pred, timeoutMs) {
    const t0 = performance.now();
    while (performance.now() - t0 < timeoutMs) {
      if (pred()) return true;
      await sleep(250);
    }
    return false;
  }

  // -------- phase 1: getUserMedia camera (TCC prompt may block here) ------
  if (rustOnly) {
    log("phase=rust_request_begin");
    const t = performance.now();
    P.startRust();
    await waitFor(() => ["running", "error"].includes(P.stats.rustState), 120000);
    log(`phase=rust_request_end state=${P.stats.rustState} waited_ms=${(performance.now() - t) | 0} ` +
        `detail="${document.getElementById("rust-status").textContent}"`);
    if (P.stats.rustState === "running") {
      await sleep(2000);
      log("phase=rust_preview_begin (sample CPU now)");
      const samples = [];
      for (let i = 0; i < 8; i++) {
        await sleep(2000);
        samples.push(P.stats.rustFps);
        log(`rust_fps=${P.stats.rustFps.toFixed(1)} polls_total=${P.stats.rustPolls} drawn_total=${P.stats.rustDrawn}`);
      }
      samples.sort((a, b) => a - b);
      log(`phase=rust_preview_end median_fps=${samples[Math.floor(samples.length / 2)].toFixed(1)}`);
      await P.stopRust();
    }
    // End on the gallery tab: the deliverable screenshot must not contain
    // camera frames (privacy on a shared desktop) — JPEG assets only.
    P.showTab("gallery");
    await waitFor(() => P.stats.galleryLoaded >= 40, 15000);
    await sleep(1000);
    log(`gallery loaded=${P.stats.galleryLoaded}/${P.stats.galleryTotal} errors=${P.stats.galleryErrors}`);
    log("verify_done READY_FOR_SCREENSHOT");
    return;
  }

  log("phase=gum_request_begin (a TCC camera prompt may appear now)");
  const tCam = performance.now();
  P.startGum();
  await waitFor(() => P.stats.gumState !== "requesting" && P.stats.gumState !== "idle", 120000);
  log(`phase=gum_request_end state=${P.stats.gumState} waited_ms=${(performance.now() - tCam) | 0} ` +
      `detail="${document.getElementById("gum-status").textContent}"`);

  if (P.stats.gumState === "running") {
    await sleep(2000); // let fps settle
    log("phase=gum_preview_begin (sample CPU now)");
    const samples = [];
    for (let i = 0; i < 8; i++) {
      await sleep(2000);
      samples.push(P.stats.gumFps);
      log(`gum_fps=${P.stats.gumFps.toFixed(1)}`);
    }
    samples.sort((a, b) => a - b);
    log(`phase=gum_preview_end median_fps=${samples[Math.floor(samples.length / 2)].toFixed(1)}`);
  }

  // -------- phase 2: mic meter (second TCC prompt possible) ---------------
  P.showTab("audio");
  log("phase=mic_request_begin (a TCC microphone prompt may appear now)");
  const tMic = performance.now();
  P.startMic();
  await waitFor(() => P.stats.micState !== "requesting" && P.stats.micState !== "idle", 120000);
  log(`phase=mic_request_end state=${P.stats.micState} waited_ms=${(performance.now() - tMic) | 0} ` +
      `detail="${document.getElementById("mic-status").textContent}"`);
  if (P.stats.micState === "running") {
    let min = 1, max = 0;
    for (let i = 0; i < 12; i++) {
      await sleep(500);
      if (P.stats.micRms >= 0) { min = Math.min(min, P.stats.micRms); max = Math.max(max, P.stats.micRms); }
    }
    log(`mic_rms min=${min.toFixed(5)} max=${max.toFixed(5)} (updates at 20 Hz)`);
  }

  // -------- phase 3: beep (no user gesture in verify mode — record it) ----
  await P.beep();
  log(`beep_status="${document.getElementById("beep-status").textContent}"`);

  // -------- phase 4: gallery + forced scroll to exercise lazy loading -----
  log("phase=gallery_begin");
  P.showTab("gallery");
  await sleep(1500);
  log(`gallery_initial loaded=${P.stats.galleryLoaded}/${P.stats.galleryTotal} errors=${P.stats.galleryErrors} ms=${P.stats.galleryMs | 0}`);
  const panel = document.getElementById("panel-gallery");
  for (let i = 0; i < 10 && P.stats.galleryLoaded < P.stats.galleryTotal; i++) {
    panel.scrollTop += 400; // scroll to trigger lazy rows
    await sleep(600);
  }
  panel.scrollTop = panel.scrollHeight;
  await sleep(2000);
  log(`phase=gallery_end loaded=${P.stats.galleryLoaded}/${P.stats.galleryTotal} errors=${P.stats.galleryErrors} ms=${P.stats.galleryMs | 0}`);

  // -------- phase 5: Rust nokhwa path (GUM stopped for a clean measure) ---
  P.showTab("camera");
  P.stopGum();
  await sleep(1000);
  log("phase=rust_request_begin");
  const tRust = performance.now();
  P.startRust();
  await waitFor(() => ["running", "error"].includes(P.stats.rustState), 120000);
  log(`phase=rust_request_end state=${P.stats.rustState} waited_ms=${(performance.now() - tRust) | 0} ` +
      `detail="${document.getElementById("rust-status").textContent}"`);
  if (P.stats.rustState === "running") {
    await sleep(2000);
    log("phase=rust_preview_begin (sample CPU now)");
    const samples = [];
    for (let i = 0; i < 7; i++) {
      await sleep(2000);
      samples.push(P.stats.rustFps);
      log(`rust_fps=${P.stats.rustFps.toFixed(1)} polls_total=${P.stats.rustPolls} drawn_total=${P.stats.rustDrawn}`);
    }
    samples.sort((a, b) => a - b);
    log(`phase=rust_preview_end median_fps=${samples[Math.floor(samples.length / 2)].toFixed(1)}`);
    await P.stopRust();
  }

  // -------- done: restart the webview preview so a screenshot shows life --
  P.startGum();
  await sleep(1500);
  log("verify_done READY_FOR_SCREENSHOT");
})();
