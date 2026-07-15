// Peek (Tauri) frontend — PRODUCTION code.
// Architectural note: camera preview, mic meter and the beep run entirely in
// the WKWebView (getUserMedia / WebAudio). Rust is only involved in:
//   - list_images (gallery paths, served back via the asset:// protocol)
//   - the SECONDARY comparative camera path (nokhwa frames polled over IPC)
//   - verification logging (see verify.js).
const { invoke, convertFileSrc } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);

// ---------------------------------------------------------------- tabs ----
function showTab(name) {
  document.querySelectorAll(".tab").forEach((b) =>
    b.classList.toggle("active", b.dataset.tab === name));
  document.querySelectorAll(".panel").forEach((p) =>
    p.classList.toggle("active", p.id === `panel-${name}`));
  if (name === "gallery") loadGallery(); // lazy: first activation only
}
document.querySelectorAll(".tab").forEach((b) =>
  b.addEventListener("click", () => showTab(b.dataset.tab)));

// Shared stats surface (the live FPS counters are a spec requirement; the
// stats object is also read by verify.js).
const stats = {
  gumFps: 0, gumFrames: 0, gumState: "idle",
  rustFps: 0, rustDrawn: 0, rustPolls: 0, rustState: "idle",
  micRms: -1, micState: "idle",
  galleryTotal: 0, galleryLoaded: 0, galleryErrors: 0, galleryMs: 0,
};

// ------------------------------------------------- camera: getUserMedia ----
let gumStream = null;
let gumUsingRvfc = false;

async function startGum() {
  const video = $("gum-video");
  const status = $("gum-status");
  if (gumStream) return;
  stats.gumState = "requesting";
  status.textContent =
    `requesting… (secureContext=${window.isSecureContext}, mediaDevices=${typeof navigator.mediaDevices})`;
  if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
    stats.gumState = "unavailable";
    status.textContent = `getUserMedia unavailable (secureContext=${window.isSecureContext})`;
    return;
  }
  const t0 = performance.now();
  try {
    gumStream = await navigator.mediaDevices.getUserMedia({
      video: { width: 640, height: 480, frameRate: 30 },
    });
  } catch (e) {
    stats.gumState = `error:${e.name}`;
    status.textContent = `getUserMedia failed after ${(performance.now() - t0) | 0} ms: ${e.name} — ${e.message}`;
    return;
  }
  video.srcObject = gumStream;
  const track = gumStream.getVideoTracks()[0];
  const s = track.getSettings();
  stats.gumState = "running";
  status.textContent =
    `granted in ${(performance.now() - t0) | 0} ms — ${s.width}x${s.height}@${s.frameRate} (${track.label})`;
  $("gum-start").disabled = true;
  $("gum-stop").disabled = false;

  // FPS = frames actually presented by the compositor. WebKit supports
  // requestVideoFrameCallback (per presented video frame); rAF fallback.
  gumUsingRvfc = typeof video.requestVideoFrameCallback === "function";
  let frames = 0;
  let winStart = performance.now();
  const tick = () => {
    if (!gumStream) return;
    frames++; stats.gumFrames++;
    const now = performance.now();
    if (now - winStart >= 1000) {
      stats.gumFps = (frames * 1000) / (now - winStart);
      $("gum-fps").textContent = `${stats.gumFps.toFixed(1)} fps (${gumUsingRvfc ? "rVFC" : "rAF"})`;
      frames = 0; winStart = now;
    }
    if (gumUsingRvfc) video.requestVideoFrameCallback(tick);
    else requestAnimationFrame(tick);
  };
  if (gumUsingRvfc) video.requestVideoFrameCallback(tick);
  else requestAnimationFrame(tick);
}

function stopGum() {
  if (!gumStream) return;
  gumStream.getTracks().forEach((t) => t.stop());
  gumStream = null;
  $("gum-video").srcObject = null;
  stats.gumState = "stopped"; stats.gumFps = 0;
  $("gum-status").textContent = "stopped";
  $("gum-fps").textContent = "— fps";
  $("gum-start").disabled = false;
  $("gum-stop").disabled = true;
}
$("gum-start").addEventListener("click", startGum);
$("gum-stop").addEventListener("click", stopGum);

// --------------------------------------- camera: Rust nokhwa over IPC ----
let rustRunning = false;

async function startRust() {
  if (rustRunning) return;
  const status = $("rust-status");
  status.textContent = "starting nokhwa (may wait on TCC)…";
  stats.rustState = "requesting";
  try {
    await invoke("rust_cam_start");
  } catch (e) {
    stats.rustState = "error";
    status.textContent = `rust_cam_start failed: ${e}`;
    return;
  }
  rustRunning = true;
  stats.rustState = "running";
  $("rust-start").disabled = true;
  $("rust-stop").disabled = false;

  const canvas = $("rust-canvas");
  const ctx = canvas.getContext("2d");
  let lastSeq = 0;
  let drawn = 0;
  let winStart = performance.now();
  (async function loop() {
    while (rustRunning) {
      let buf;
      try {
        buf = await invoke("rust_cam_frame", { lastSeq });
      } catch (e) {
        status.textContent = `rust_cam_frame failed: ${e}`;
        break;
      }
      stats.rustPolls++;
      const dv = new DataView(buf);
      const seq = dv.getUint32(0, true);
      const w = dv.getUint32(4, true);
      const h = dv.getUint32(8, true);
      const isNew = dv.getUint32(12, true) & 1;
      if (isNew && w > 0) {
        lastSeq = seq;
        if (canvas.width !== w || canvas.height !== h) {
          canvas.width = w; canvas.height = h;
        }
        ctx.putImageData(new ImageData(new Uint8ClampedArray(buf, 16), w, h), 0, 0);
        drawn++; stats.rustDrawn++;
      } else {
        await new Promise((r) => setTimeout(r, 5)); // no new frame; don't spin
      }
      const now = performance.now();
      if (now - winStart >= 1000) {
        stats.rustFps = (drawn * 1000) / (now - winStart);
        $("rust-fps").textContent = `${stats.rustFps.toFixed(1)} fps`;
        invoke("rust_cam_status").then((s) => {
          if (rustRunning) status.textContent = `${s} — ${w}x${h} frames over raw IPC`;
        });
        drawn = 0; winStart = now;
      }
    }
  })();
}

async function stopRust() {
  if (!rustRunning) return;
  rustRunning = false;
  await invoke("rust_cam_stop");
  stats.rustState = "stopped"; stats.rustFps = 0;
  $("rust-status").textContent = "stopped";
  $("rust-fps").textContent = "— fps";
  $("rust-start").disabled = false;
  $("rust-stop").disabled = true;
}
$("rust-start").addEventListener("click", startRust);
$("rust-stop").addEventListener("click", stopRust);

// -------------------------------------------------------- mic VU meter ----
let micStream = null;
let micCtx = null;
let micTimer = null;

async function startMic() {
  if (micStream) return;
  const status = $("mic-status");
  stats.micState = "requesting";
  status.textContent = "requesting…";
  const t0 = performance.now();
  try {
    micStream = await navigator.mediaDevices.getUserMedia({ audio: true });
  } catch (e) {
    stats.micState = `error:${e.name}`;
    status.textContent = `getUserMedia(audio) failed after ${(performance.now() - t0) | 0} ms: ${e.name} — ${e.message}`;
    return;
  }
  micCtx = new AudioContext();
  const analyser = micCtx.createAnalyser();
  analyser.fftSize = 2048;
  micCtx.createMediaStreamSource(micStream).connect(analyser);
  const buf = new Float32Array(analyser.fftSize);
  stats.micState = "running";
  status.textContent = `granted in ${(performance.now() - t0) | 0} ms — ${micStream.getAudioTracks()[0].label}`;
  $("mic-start").disabled = true;
  $("mic-stop").disabled = false;
  micTimer = setInterval(() => { // ~20 Hz VU updates
    analyser.getFloatTimeDomainData(buf);
    let sum = 0;
    for (let i = 0; i < buf.length; i++) sum += buf[i] * buf[i];
    const rms = Math.sqrt(sum / buf.length);
    stats.micRms = rms;
    const db = 20 * Math.log10(rms || 1e-7);
    const pct = Math.min(100, Math.max(0, (db + 60) / 60 * 100)); // -60 dB floor
    $("vu-bar").style.width = `${pct}%`;
    $("mic-level").textContent = `RMS: ${rms.toFixed(4)} (${db.toFixed(1)} dBFS)`;
  }, 50);
}

function stopMic() {
  if (!micStream) return;
  clearInterval(micTimer);
  micStream.getTracks().forEach((t) => t.stop());
  micStream = null;
  if (micCtx) { micCtx.close(); micCtx = null; }
  stats.micState = "stopped"; stats.micRms = -1;
  $("vu-bar").style.width = "0%";
  $("mic-level").textContent = "RMS: —";
  $("mic-status").textContent = "stopped";
  $("mic-start").disabled = false;
  $("mic-stop").disabled = true;
}
$("mic-start").addEventListener("click", startMic);
$("mic-stop").addEventListener("click", stopMic);

// ------------------------------------------------------------- playback ----
let beepCtx = null;
async function beep() {
  const status = $("beep-status");
  try {
    if (!beepCtx) beepCtx = new AudioContext();
    if (beepCtx.state === "suspended") await beepCtx.resume();
    const osc = beepCtx.createOscillator();
    const gain = beepCtx.createGain();
    osc.frequency.value = 880;
    const t = beepCtx.currentTime;
    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.exponentialRampToValueAtTime(0.25, t + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.15);
    osc.connect(gain).connect(beepCtx.destination);
    osc.start(t);
    osc.stop(t + 0.16);
    status.textContent = `beeped (AudioContext state=${beepCtx.state})`;
  } catch (e) {
    status.textContent = `beep failed: ${e}`;
  }
}
$("beep-btn").addEventListener("click", beep);

// -------------------------------------------------------------- gallery ----
let galleryStarted = false;
async function loadGallery() {
  if (galleryStarted) return;
  galleryStarted = true;
  const status = $("gallery-status");
  const grid = $("gallery-grid");
  const t0 = performance.now();
  let paths;
  try {
    paths = await invoke("list_images");
  } catch (e) {
    status.textContent = `list_images failed: ${e}`;
    return;
  }
  stats.galleryTotal = paths.length;
  // The webview decodes/caches/downscales; we only hand it asset:// URLs.
  // loading=lazy defers offscreen fetches; decoding=async keeps decode off
  // the main thread so scrolling doesn't jank.
  for (const p of paths) {
    const img = document.createElement("img");
    img.loading = "lazy";
    img.decoding = "async";
    img.onload = () => {
      stats.galleryLoaded++;
      stats.galleryMs = performance.now() - t0;
      status.textContent = `${stats.galleryLoaded}/${stats.galleryTotal} loaded in ${stats.galleryMs | 0} ms (lazy: offscreen rows load on scroll)`;
    };
    img.onerror = () => {
      stats.galleryErrors++;
      status.textContent = `ERROR after ${stats.galleryLoaded} ok / ${stats.galleryErrors} failed — check assetProtocol scope + CSP`;
    };
    img.src = convertFileSrc(p);
    grid.appendChild(img);
  }
  status.textContent = `listed ${paths.length} images in ${(performance.now() - t0) | 0} ms…`;
}

// Controller surface for verify.js (and manual console poking).
window.Peek = { showTab, startGum, stopGum, startRust, stopRust, startMic, stopMic, beep, loadGallery, stats };
