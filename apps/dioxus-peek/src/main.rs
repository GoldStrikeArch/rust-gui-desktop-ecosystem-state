//! "Peek" — SPEC-6 media & hardware test on Dioxus 0.7.9 desktop (wry/tao,
//! WKWebView on macOS).
//!
//! Architecture notes (the finding, in code form):
//! * JS camera path (primary): the pixels never touch Rust. `getUserMedia`
//!   inside the webview feeds a `<video>` element; WebKit's GPU process does
//!   capture, color conversion and compositing. wry's WKUIDelegate auto-grants
//!   the WKWebView-level media permission (`WKPermissionDecision::Grant` in
//!   wry 0.53.5), so the only real gate is macOS TCC on the host process.
//! * Rust camera path (secondary): nokhwa (AVFoundation) captures on a Rust
//!   thread, each frame is JPEG-encoded and handed to page JS through a
//!   `use_asset_handler` long-poll (`fetch('/camframe/next/<seq>')` resolves
//!   when a newer frame exists). One custom-protocol round trip + one full
//!   JPEG decode per frame on the WebKit side — this is the texture-upload
//!   cost being measured.
//! * Mic: cpal input callback → RMS/peak atomics → 20 Hz signal poll → CSS bar.
//! * Beep: rodio sine through the default output sink, on its own thread.
//! * Gallery: 200 JPEGs served by an asset handler off one IO thread into an
//!   `<img loading="lazy">` grid; WebKit does decode/downscale/texture cache.
//!
//! Threads never touch Dioxus signals (UnsyncStorage): they write statics
//! (atomics + mutexed strings) and one 20 Hz `use_future` mirrors those into
//! signals. Verification hooks live in autotest.rs (PEEK_AUTOTEST=1).

mod autotest;

use dioxus::desktop::wry::http::{header, Response, StatusCode};
use dioxus::desktop::{
    use_asset_handler, wry, Config, LogicalPosition, LogicalSize, WindowBuilder,
};
use dioxus::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Shared state written by hardware threads, polled by the UI at 20 Hz.
// ---------------------------------------------------------------------------

struct CamShared {
    /// Latest encoded frame: (sequence number, JPEG bytes).
    latest: Mutex<Option<(u64, Arc<Vec<u8>>)>>,
    /// Long-poll responders parked until the next frame arrives.
    waiters: Mutex<Vec<wry::RequestAsyncResponder>>,
    run: AtomicBool,
    seq: AtomicU64,
    capture_fps_x10: AtomicU32,
    encode_ms_x10: AtomicU32,
    status: Mutex<String>,
}

static CAM: CamShared = CamShared {
    latest: Mutex::new(None),
    waiters: Mutex::new(Vec::new()),
    run: AtomicBool::new(false),
    seq: AtomicU64::new(0),
    capture_fps_x10: AtomicU32::new(0),
    encode_ms_x10: AtomicU32::new(0),
    status: Mutex::new(String::new()),
};

struct MicShared {
    rms_x1000: AtomicU32,
    peak_x1000: AtomicU32,
    run: AtomicBool,
    status: Mutex<String>,
}

static MIC: MicShared = MicShared {
    rms_x1000: AtomicU32::new(0),
    peak_x1000: AtomicU32::new(0),
    run: AtomicBool::new(false),
    status: Mutex::new(String::new()),
};

static MIC_STOP: Mutex<Option<mpsc::Sender<()>>> = Mutex::new(None);
static AUDIO_STATUS: Mutex<String> = Mutex::new(String::new());

/// Single IO thread for gallery file reads so the wry protocol callback
/// (which runs on the main thread) never blocks on disk.
static GALLERY_IO: OnceLock<mpsc::Sender<(PathBuf, wry::RequestAsyncResponder)>> = OnceLock::new();

fn set_cam_status(s: impl Into<String>) {
    *CAM.status.lock().unwrap() = s.into();
}

fn set_mic_status(s: impl Into<String>) {
    *MIC.status.lock().unwrap() = s.into();
}

fn set_audio_status(s: impl Into<String>) {
    *AUDIO_STATUS.lock().unwrap() = s.into();
}

// ---------------------------------------------------------------------------
// Rust camera path: nokhwa (AVFoundation) → JPEG → asset-handler long-poll.
// ---------------------------------------------------------------------------

fn start_rust_cam() {
    if CAM.run.swap(true, Ordering::SeqCst) {
        return; // already running
    }
    std::thread::spawn(cam_thread);
}

fn stop_rust_cam() {
    CAM.run.store(false, Ordering::SeqCst);
}

/// Flush parked long-poll responders (204) so the JS pump can exit cleanly.
fn finish_cam() {
    CAM.run.store(false, Ordering::SeqCst);
    CAM.capture_fps_x10.store(0, Ordering::Relaxed);
    for r in CAM.waiters.lock().unwrap().drain(..) {
        r.respond(
            Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body(Vec::new())
                .unwrap(),
        );
    }
}

fn respond_frame(r: wry::RequestAsyncResponder, seq: u64, jpeg: &Arc<Vec<u8>>) {
    r.respond(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::CACHE_CONTROL, "no-store")
            .header("x-seq", seq.to_string())
            .body(jpeg.as_ref().clone())
            .unwrap(),
    );
}

fn cam_thread() {
    use nokhwa::pixel_format::RgbFormat;
    use nokhwa::utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType};

    // TCC: nokhwa_check() reads AVCaptureDevice authorization; nokhwa_initialize()
    // calls requestAccess (fires the OS prompt on first use) and calls back.
    let pre_authorized = nokhwa::nokhwa_check();
    println!("peek: camera TCC pre-authorized: {pre_authorized}");
    if !pre_authorized {
        set_cam_status("requesting camera access (macOS TCC prompt may appear)…");
        let (tx, rx) = mpsc::channel();
        nokhwa::nokhwa_initialize(move |granted| {
            let _ = tx.send(granted);
        });
        match rx.recv_timeout(Duration::from_secs(45)) {
            Ok(true) => println!("peek: camera TCC granted"),
            Ok(false) => {
                println!("peek: camera TCC DENIED");
                set_cam_status("camera access denied (TCC) — Rust path unavailable. Grant in System Settings > Privacy & Security > Camera, then Start again.");
                finish_cam();
                return;
            }
            Err(_) => {
                println!("peek: camera TCC unanswered after 45 s");
                set_cam_status("no answer to the TCC prompt after 45 s — press Start to retry");
                finish_cam();
                return;
            }
        }
    }

    set_cam_status("opening camera (nokhwa/AVFoundation)…");
    // Format negotiation quirk (observed in run 1): nokhwa-bindings-macos
    // 0.2.4 maps AVFoundation's 420v/420f bi-planar YCbCr to
    // FrameFormat::YUYV and maps FrameFormat::NV12 to the *10-bit* biplanar
    // pixel format, so an NV12 request can never match a stock FaceTime
    // camera. Walk a fallback chain instead of hardcoding one format.
    // Second quirk (observed in run 2): AVCaptureDevice's configuration lock
    // is rejected while another capture session still holds the device —
    // WebKit's GPU process keeps the camera for a couple of seconds after
    // `track.stop()`. Retry a few rounds before giving up.
    let mut opened = None;
    'rounds: for round in 0..5 {
        if round > 0 {
            set_cam_status(format!(
                "camera busy (held by another capture session?) — retry {round}/4…"
            ));
            std::thread::sleep(Duration::from_secs(2));
            if !CAM.run.load(Ordering::SeqCst) {
                finish_cam();
                return;
            }
        }
        let attempts: [(&str, RequestedFormatType); 3] = [
            (
                "Closest 640x480@30 YUYV",
                RequestedFormatType::Closest(CameraFormat::new_from(640, 480, FrameFormat::YUYV, 30)),
            ),
            ("HighestFrameRate(30)", RequestedFormatType::HighestFrameRate(30)),
            ("None (first reported format)", RequestedFormatType::None),
        ];
        for (label, rt) in attempts {
            match nokhwa::Camera::new(CameraIndex::Index(0), RequestedFormat::new::<RgbFormat>(rt)) {
                Ok(c) => {
                    println!("peek: camera opened with request '{label}' (round {round})");
                    opened = Some(c);
                    break 'rounds;
                }
                Err(e) => println!("peek: camera request '{label}' failed (round {round}): {e}"),
            }
        }
    }
    let Some(mut camera) = opened else {
        println!("peek: no camera format request succeeded");
        set_cam_status("no camera format request succeeded (see log)");
        finish_cam();
        return;
    };
    if let Ok(formats) = camera.compatible_camera_formats() {
        let first: Vec<String> = formats.iter().take(6).map(|f| f.to_string()).collect();
        println!(
            "peek: camera reports {} formats; first: {}",
            formats.len(),
            first.join(" | ")
        );
    }
    if let Err(e) = camera.open_stream() {
        println!("peek: open_stream failed: {e}");
        set_cam_status(format!("open_stream failed: {e}"));
        finish_cam();
        return;
    }
    let fmt = camera.camera_format();
    println!("peek: camera open: {fmt}");
    set_cam_status(format!("capturing {fmt}"));

    let mut win_start = Instant::now();
    let mut win_frames = 0u32;
    while CAM.run.load(Ordering::SeqCst) {
        let buffer = match camera.frame() {
            Ok(b) => b,
            Err(e) => {
                println!("peek: frame() failed: {e}");
                set_cam_status(format!("frame() failed: {e}"));
                break;
            }
        };
        let t0 = Instant::now();
        let rgb = match buffer.decode_image::<RgbFormat>() {
            Ok(i) => i,
            Err(e) => {
                set_cam_status(format!("frame decode failed: {e}"));
                break;
            }
        };
        let mut jpeg = Vec::with_capacity(64 * 1024);
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 70);
        if let Err(e) = enc.encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        ) {
            set_cam_status(format!("jpeg encode failed: {e}"));
            break;
        }
        // EMA (~16 frames) of the per-frame decode+encode cost.
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let prev = CAM.encode_ms_x10.load(Ordering::Relaxed) as f64 / 10.0;
        let ema = if prev == 0.0 { ms } else { prev + (ms - prev) / 16.0 };
        CAM.encode_ms_x10.store((ema * 10.0) as u32, Ordering::Relaxed);

        let seq = CAM.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let bytes = Arc::new(jpeg);
        *CAM.latest.lock().unwrap() = Some((seq, bytes.clone()));
        for waiter in CAM.waiters.lock().unwrap().drain(..) {
            respond_frame(waiter, seq, &bytes);
        }

        win_frames += 1;
        let elapsed = win_start.elapsed();
        if elapsed >= Duration::from_secs(1) {
            let fps = win_frames as f64 / elapsed.as_secs_f64();
            CAM.capture_fps_x10.store((fps * 10.0) as u32, Ordering::Relaxed);
            win_frames = 0;
            win_start = Instant::now();
        }
    }
    let _ = camera.stop_stream();
    finish_cam();
    set_cam_status("stopped");
}

// ---------------------------------------------------------------------------
// Mic level meter: cpal input stream → RMS/peak atomics.
// ---------------------------------------------------------------------------

fn start_mic() {
    if MIC.run.swap(true, Ordering::SeqCst) {
        return;
    }
    let (tx, rx) = mpsc::channel::<()>();
    *MIC_STOP.lock().unwrap() = Some(tx);
    std::thread::spawn(move || mic_thread(rx));
}

fn stop_mic() {
    if let Some(tx) = MIC_STOP.lock().unwrap().take() {
        let _ = tx.send(());
    }
}

fn mic_thread(stop: mpsc::Receiver<()>) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let mic_done = |msg: String| {
        println!("peek: mic: {msg}");
        set_mic_status(msg);
        MIC.run.store(false, Ordering::SeqCst);
        MIC.rms_x1000.store(0, Ordering::Relaxed);
        MIC.peak_x1000.store(0, Ordering::Relaxed);
    };

    set_mic_status("opening default input (cpal) — mic TCC prompt may appear…");
    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        mic_done("no default input device".into());
        return;
    };
    let name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "?".into());
    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            mic_done(format!("default_input_config failed: {e}"));
            return;
        }
    };
    let sample_format = config.sample_format();
    if sample_format != cpal::SampleFormat::F32 {
        mic_done(format!("unsupported input format {sample_format:?} (f32-only demo)"));
        return;
    }
    let stream_config: cpal::StreamConfig = config.into();
    let stream = device.build_input_stream(
        &stream_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut sum = 0.0f64;
            let mut peak = 0.0f32;
            for &s in data {
                sum += (s as f64) * (s as f64);
                peak = peak.max(s.abs());
            }
            let rms = (sum / data.len().max(1) as f64).sqrt() as f32;
            MIC.rms_x1000.store((rms * 1000.0) as u32, Ordering::Relaxed);
            // Peak-hold with slow decay so the bar has a visible high-water mark.
            let prev = MIC.peak_x1000.load(Ordering::Relaxed);
            let now = ((peak * 1000.0) as u32).max(prev.saturating_sub(8));
            MIC.peak_x1000.store(now, Ordering::Relaxed);
        },
        |e| eprintln!("peek: cpal stream error: {e}"),
        None,
    );
    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            mic_done(format!("build_input_stream failed: {e}"));
            return;
        }
    };
    if let Err(e) = stream.play() {
        mic_done(format!("stream.play failed: {e}"));
        return;
    }
    println!("peek: mic capturing from \"{name}\" @ {} Hz", stream_config.sample_rate);
    set_mic_status(format!(
        "capturing \"{name}\" @ {} Hz, {} ch",
        stream_config.sample_rate, stream_config.channels
    ));
    let _ = stop.recv(); // parked until Stop (or app teardown drops the sender)
    drop(stream);
    mic_done("stopped".into());
}

// ---------------------------------------------------------------------------
// Audio out: rodio sine beep on its own thread (streams are !Send).
// ---------------------------------------------------------------------------

fn play_beep() {
    std::thread::spawn(|| {
        use rodio::source::SineWave;
        use rodio::Source;
        set_audio_status("opening default output (rodio)…");
        match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                sink.mixer().add(
                    SineWave::new(880.0)
                        .take_duration(Duration::from_millis(180))
                        .amplify(0.25),
                );
                set_audio_status("beep: 880 Hz / 180 ms playing…");
                std::thread::sleep(Duration::from_millis(450));
                set_audio_status("beep played (880 Hz, 180 ms, rodio default sink)");
                println!("peek: beep played via rodio");
            }
            Err(e) => {
                set_audio_status(format!("rodio open_default_sink failed: {e}"));
                println!("peek: rodio failed: {e}");
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Gallery IO thread.
// ---------------------------------------------------------------------------

fn assets_dir() -> PathBuf {
    std::env::var_os("PEEK_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../peek-assets"))
}

fn spawn_gallery_io() {
    let (tx, rx) = mpsc::channel::<(PathBuf, wry::RequestAsyncResponder)>();
    let _ = GALLERY_IO.set(tx);
    std::thread::spawn(move || {
        for (path, responder) in rx {
            match std::fs::read(&path) {
                Ok(bytes) => responder.respond(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "image/jpeg")
                        .header(header::CACHE_CONTROL, "max-age=3600")
                        .body(bytes)
                        .unwrap(),
                ),
                Err(e) => responder.respond(
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(format!("{e}").into_bytes())
                        .unwrap(),
                ),
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Page JS. The camera pumps are production code — they ARE the frame path.
// ---------------------------------------------------------------------------

/// Installed once at startup; returns an environment probe for the log.
const JS_HELPERS: &str = r#"
window.__setText = (id, t) => { const el = document.getElementById(id); if (el) el.textContent = t; };
window.__jsStats = { fps: 0, frames: 0, err: '' };
window.__rustStats = { fps: 0, frames: 0, err: '' };
return [location.origin, String(window.isSecureContext), typeof navigator.mediaDevices];
"#;

/// Primary camera path: getUserMedia → <video>; rvfc counts *presented* frames.
const JS_CAM_START: &str = r#"
(async () => {
  const v = document.getElementById('js_video');
  const s = window.__jsStats;
  if (window.__jsStream) return;
  s.err = '';
  window.__setText('js_status', 'requesting getUserMedia…');
  if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
    s.err = 'navigator.mediaDevices unavailable (origin ' + location.origin
          + ', secureContext=' + window.isSecureContext + ')';
    window.__setText('js_status', s.err);
    return;
  }
  let stream = null;
  for (let attempt = 0; attempt < 4 && !stream; attempt++) {
    try {
      stream = await navigator.mediaDevices.getUserMedia(
        { video: { width: 640, height: 480 }, audio: false });
    } catch (e) {
      s.err = e.name + ': ' + e.message;
      // The device can still be held by a just-stopped capture session
      // (ours or another process's) — retry those, fail everything else.
      if (e.name === 'NotReadableError' || e.name === 'AbortError') {
        window.__setText('js_status', 'camera busy (' + s.err + ') — retrying…');
        await new Promise(res => setTimeout(res, 2000));
      } else {
        window.__setText('js_status', 'getUserMedia failed — ' + s.err);
        return;
      }
    }
  }
  if (!stream) {
    window.__setText('js_status', 'getUserMedia failed after retries — ' + s.err);
    return;
  }
  s.err = '';
  window.__jsStream = stream;
  v.srcObject = stream;
  await v.play();
  const st = stream.getVideoTracks()[0].getSettings();
  window.__setText('js_status',
    'live ' + st.width + 'x' + st.height + '@' + (st.frameRate || '?') + ' (WebKit-internal capture)');
  let frames = 0, t0 = performance.now();
  const tick = () => {
    if (!window.__jsStream) return;
    frames++; s.frames++;
    const now = performance.now();
    if (now - t0 >= 1000) {
      s.fps = Math.round(frames * 10000 / (now - t0)) / 10;
      window.__setText('js_fps', s.fps.toFixed(1) + ' fps presented');
      frames = 0; t0 = now;
    }
    v.requestVideoFrameCallback(tick);
  };
  v.requestVideoFrameCallback(tick);
})();
"#;

const JS_CAM_STOP: &str = r#"
const v = document.getElementById('js_video');
if (window.__jsStream) { window.__jsStream.getTracks().forEach(t => t.stop()); window.__jsStream = null; }
if (v) v.srcObject = null;
window.__jsStats.fps = 0;
window.__setText('js_fps', '—');
window.__setText('js_status', 'stopped');
"#;

/// Secondary path pump: long-poll /camframe/next/<seq>, blob → <img>. Counts a
/// frame as presented when img.onload fires (decode complete, handed to the
/// compositor).
const JS_RUST_PUMP: &str = r#"
(async () => {
  if (window.__rustRun) return;
  window.__rustRun = true;
  const img = document.getElementById('rust_img');
  const s = window.__rustStats;
  s.err = ''; s.fps = 0;
  let seq = 0, frames = 0, t0 = performance.now();
  window.__setText('rust_status', 'long-polling /camframe…');
  while (window.__rustRun) {
    try {
      const r = await fetch('/camframe/next/' + seq, { cache: 'no-store' });
      if (!r.ok) {   // 204 = camera not running / flushed
        await new Promise(res => setTimeout(res, 250));
        continue;
      }
      seq = parseInt(r.headers.get('x-seq') || '0', 10);
      const blob = await r.blob();
      const url = URL.createObjectURL(blob);
      await new Promise((res, rej) => { img.onload = res; img.onerror = rej; img.src = url; });
      URL.revokeObjectURL(url);
      frames++; s.frames++;
      const now = performance.now();
      if (now - t0 >= 1000) {
        s.fps = Math.round(frames * 10000 / (now - t0)) / 10;
        window.__setText('rust_fps', s.fps.toFixed(1) + ' fps presented');
        frames = 0; t0 = now;
      }
    } catch (e) {
      s.err = String(e);
      window.__setText('rust_status', 'pump error: ' + s.err);
      await new Promise(res => setTimeout(res, 300));
    }
  }
  window.__setText('rust_fps', '—');
  window.__setText('rust_status', 'stopped');
})();
"#;

const JS_RUST_STOP: &str = "window.__rustRun = false;";

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Camera,
    Audio,
    Gallery,
}

/// App Nap guard. Observed in run 1: once the window was fully occluded the
/// OS App-Napped the (unbundled, non-frontmost) process — cpal's audio
/// callbacks kept running but tokio timers, and with them the entire Dioxus
/// update loop, froze mid-autotest. A live-media app must hold an NSActivity
/// assertion; `…AllowingIdleSystemSleep` avoids taking any system-wide sleep
/// assertion (scoped strictly to this process).
fn disable_app_nap() {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    let options = NSActivityOptions::UserInitiatedAllowingIdleSystemSleep
        | NSActivityOptions::LatencyCritical;
    let reason = NSString::from_str("Peek: live camera/mic preview must keep ticking");
    let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(options, &reason);
    std::mem::forget(token); // hold for process lifetime
}

fn main() {
    disable_app_nap();
    spawn_gallery_io();
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_window(
                WindowBuilder::new()
                    .with_title("Peek (dioxus)")
                    .with_inner_size(LogicalSize::new(900.0, 600.0))
                    .with_position(LogicalPosition::new(60.0, 60.0))
                    .with_resizable(true),
            ),
        )
        .launch(App);
}

/// Write-if-changed so the 20 Hz mirror loop doesn't re-render idle UI.
fn sync<T: PartialEq + 'static>(sig: &mut Signal<T>, v: T) {
    if *sig.peek() != v {
        sig.set(v);
    }
}

#[component]
fn App() -> Element {
    let mut tab = use_signal(|| Tab::Camera);
    let mut cam_status = use_signal(String::new);
    let mut cap_fps = use_signal(|| 0.0f64);
    let mut enc_ms = use_signal(|| 0.0f64);
    let mut mic_status = use_signal(String::new);
    let mut mic_rms = use_signal(|| 0.0f32);
    let mut mic_peak = use_signal(|| 0.0f32);
    let mut audio_status = use_signal(String::new);

    // Rust camera path: latest frame long-poll (see JS_RUST_PUMP).
    use_asset_handler("camframe", move |request, responder| {
        let after: u64 = request
            .uri()
            .path()
            .rsplit('/')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if !CAM.run.load(Ordering::SeqCst) {
            responder.respond(
                Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .body(Vec::new())
                    .unwrap(),
            );
            return;
        }
        let latest = CAM.latest.lock().unwrap().clone();
        match latest {
            Some((seq, bytes)) if seq > after => respond_frame(responder, seq, &bytes),
            _ => CAM.waiters.lock().unwrap().push(responder),
        }
    });

    // Gallery: /gallery/imgNNN.jpg → IO thread → JPEG response.
    let assets = assets_dir();
    use_asset_handler("gallery", move |request, responder| {
        let path = request.uri().path().to_string();
        let file = path.trim_start_matches("/gallery/");
        let safe = !file.is_empty()
            && file
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
        if !safe {
            responder.respond(
                Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Vec::new())
                    .unwrap(),
            );
            return;
        }
        if let Some(tx) = GALLERY_IO.get() {
            let _ = tx.send((assets.join(file), responder));
        }
    });

    // One-time JS helper install + environment probe (secure-context finding).
    use_future(move || async move {
        match document::eval(JS_HELPERS).join::<Vec<String>>().await {
            Ok(v) => println!("peek: webview env probe [origin, secureContext, mediaDevices]: {v:?}"),
            Err(e) => println!("peek: env probe eval failed: {e:?}"),
        }
    });

    // 20 Hz mirror: hardware-thread statics → signals.
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            sync(&mut cam_status, CAM.status.lock().unwrap().clone());
            sync(&mut cap_fps, CAM.capture_fps_x10.load(Ordering::Relaxed) as f64 / 10.0);
            sync(&mut enc_ms, CAM.encode_ms_x10.load(Ordering::Relaxed) as f64 / 10.0);
            sync(&mut mic_status, MIC.status.lock().unwrap().clone());
            sync(&mut mic_rms, MIC.rms_x1000.load(Ordering::Relaxed) as f32 / 1000.0);
            sync(&mut mic_peak, MIC.peak_x1000.load(Ordering::Relaxed) as f32 / 1000.0);
            sync(&mut audio_status, AUDIO_STATUS.lock().unwrap().clone());
        }
    });

    // Verification hooks (no-op unless PEEK_AUTOTEST=1).
    autotest::maybe_start(tab);

    // Log-scale VU: -60 dBFS..0 dBFS → 0..100 %.
    let to_pct = |v: f32| -> f64 {
        if v <= 0.0 {
            0.0
        } else {
            (((20.0 * (v as f64).log10()) + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0)
        }
    };
    let rms_pct = to_pct(*mic_rms.read());
    let peak_pct = to_pct(*mic_peak.read());
    let active = *tab.read();
    let tab_style = |me: Tab| {
        if active == me {
            "padding: 6px 16px; border: 1px solid #7aa2ff; background: #24304d; color: #dbe4ff; border-radius: 6px;"
        } else {
            "padding: 6px 16px; border: 1px solid #3a3d45; background: #1b1d23; color: #b8bac2; border-radius: 6px;"
        }
    };
    let show = |me: Tab| if active == me { "display: flex;" } else { "display: none;" };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; box-sizing: border-box; \
                    margin: 0; padding: 12px; gap: 10px; font-family: system-ui, sans-serif; \
                    background: #14161a; color: #e8e8ea; font-size: 14px;",

            div {
                style: "display: flex; gap: 8px; align-items: center;",
                span { style: "font-size: 17px; font-weight: 700; margin-right: 12px;", "Peek (dioxus)" }
                button { style: tab_style(Tab::Camera), onclick: move |_| tab.set(Tab::Camera), "Camera" }
                button { style: tab_style(Tab::Audio), onclick: move |_| tab.set(Tab::Audio), "Audio" }
                button { style: tab_style(Tab::Gallery), onclick: move |_| tab.set(Tab::Gallery), "Gallery" }
                span { style: "margin-left: auto; color: #7d808a; font-size: 12px;",
                       "SPEC-6 hardware round — wry/WKWebView" }
            }

            // ------------------------------------------------ Camera section
            div {
                style: "{show(Tab::Camera)} flex: 1; gap: 16px; min-height: 0;",

                div {
                    style: "flex: 1; display: flex; flex-direction: column; gap: 8px;",
                    span { style: "font-weight: 600;", "JS path — getUserMedia inside WKWebView (primary)" }
                    video {
                        id: "js_video",
                        autoplay: true,
                        muted: true,
                        playsinline: true,
                        style: "width: 100%; aspect-ratio: 4/3; background: #000; border-radius: 8px; object-fit: contain;",
                    }
                    div {
                        style: "display: flex; gap: 8px; align-items: center;",
                        button {
                            style: "padding: 5px 14px;",
                            onclick: move |_| { document::eval(JS_CAM_START); },
                            "Start"
                        }
                        button {
                            style: "padding: 5px 14px;",
                            onclick: move |_| { document::eval(JS_CAM_STOP); },
                            "Stop"
                        }
                        span { id: "js_fps", style: "font-variant-numeric: tabular-nums; color: #9ee493;", "—" }
                    }
                    span { id: "js_status", style: "color: #9a9daa; font-size: 12px; min-height: 16px;", "idle" }
                }

                div {
                    style: "flex: 1; display: flex; flex-direction: column; gap: 8px;",
                    span { style: "font-weight: 600;", "Rust path — nokhwa → JPEG → long-poll <img> (secondary)" }
                    img {
                        id: "rust_img",
                        style: "width: 100%; aspect-ratio: 4/3; background: #000; border-radius: 8px; object-fit: contain;",
                    }
                    div {
                        style: "display: flex; gap: 8px; align-items: center;",
                        button {
                            style: "padding: 5px 14px;",
                            onclick: move |_| {
                                start_rust_cam();
                                document::eval(JS_RUST_PUMP);
                            },
                            "Start"
                        }
                        button {
                            style: "padding: 5px 14px;",
                            onclick: move |_| {
                                document::eval(JS_RUST_STOP);
                                stop_rust_cam();
                            },
                            "Stop"
                        }
                        span { id: "rust_fps", style: "font-variant-numeric: tabular-nums; color: #9ee493;", "—" }
                    }
                    span {
                        style: "color: #9a9daa; font-size: 12px;",
                        "capture {cap_fps:.1} fps · decode+encode {enc_ms:.1} ms/frame (Rust side)"
                    }
                    span { id: "rust_status", style: "color: #9a9daa; font-size: 12px; min-height: 16px;", "idle" }
                    span { style: "color: #9a9daa; font-size: 12px;", "{cam_status}" }
                }
            }

            // ------------------------------------------------- Audio section
            div {
                style: "{show(Tab::Audio)} flex: 1; flex-direction: column; gap: 14px;",

                span { style: "font-weight: 600;", "Mic level — cpal input stream → RMS (20 Hz UI poll)" }
                div {
                    style: "display: flex; gap: 8px; align-items: center;",
                    button { style: "padding: 5px 14px;", onclick: move |_| start_mic(), "Start" }
                    button { style: "padding: 5px 14px;", onclick: move |_| stop_mic(), "Stop" }
                    span {
                        style: "font-variant-numeric: tabular-nums; color: #9ee493;",
                        "RMS {(*mic_rms.read() * 100.0):.1} %  ·  {rms_pct:.0} % of scale"
                    }
                }
                div {
                    style: "position: relative; width: 60%; max-width: 480px; height: 24px; \
                            background: #22242b; border: 1px solid #3a3d45; border-radius: 6px; overflow: hidden;",
                    div {
                        style: "position: absolute; inset: 0; width: {rms_pct}%; \
                                background: linear-gradient(90deg, #3fb950 0%, #d29922 70%, #f85149 100%); \
                                transition: width 40ms linear;",
                    }
                    div {
                        style: "position: absolute; top: 0; bottom: 0; left: {peak_pct}%; width: 2px; background: #e8e8ea;",
                    }
                }
                span { style: "color: #9a9daa; font-size: 12px; min-height: 16px;", "{mic_status}" }

                span { style: "font-weight: 600; margin-top: 10px;", "Audio out — rodio sine beep" }
                div {
                    style: "display: flex; gap: 8px; align-items: center;",
                    button { style: "padding: 5px 14px;", onclick: move |_| play_beep(), "Beep (880 Hz)" }
                    span { style: "color: #9a9daa; font-size: 12px;", "{audio_status}" }
                }
            }

            // ----------------------------------------------- Gallery section
            div {
                style: "{show(Tab::Gallery)} flex: 1; flex-direction: column; gap: 8px; min-height: 0;",
                span {
                    style: "color: #9a9daa; font-size: 12px;",
                    "200 JPEGs from apps/peek-assets via use_asset_handler — `loading=\"lazy\"`, \
                     decode/downscale/texture-cache all inside WebKit"
                }
                div {
                    id: "gallery",
                    style: "flex: 1; overflow-y: auto; display: grid; \
                            grid-template-columns: repeat(auto-fill, minmax(96px, 1fr)); gap: 6px; min-height: 0;",
                    for i in 0..200 {
                        img {
                            key: "{i}",
                            loading: "lazy",
                            src: "/gallery/img{i:03}.jpg",
                            style: "width: 100%; aspect-ratio: 4/3; object-fit: cover; border-radius: 4px; background: #22242b;",
                        }
                    }
                }
            }
        }
    }
}
