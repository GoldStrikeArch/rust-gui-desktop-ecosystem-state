// Peek (Tauri) — RCN GUI ecosystem research mini-app (SPEC-6: media & hardware).
//
// ARCHITECTURAL NOTE (the key finding of this iteration): in Tauri the
// idiomatic camera/mic/audio-out path lives entirely INSIDE the WKWebView —
// JS `getUserMedia` + `<video>`, WebAudio `AnalyserNode`, WebAudio oscillator.
// Camera/mic samples never touch this Rust process; WebKit's own GPU/media
// helper processes capture and composite the frames. Consequently there is no
// Rust-side "frame → texture upload" step at all: what replaces
// texture_upload_cost is WebKit compositor cost, measured externally as CPU%
// over the app process + its WebKit helper processes (see verify/cpu_sample.sh).
//
// The Rust-side path required by SPEC-6 (nokhwa → frame → webview) is also
// implemented as a SECONDARY, comparative path: a worker thread captures via
// nokhwa (AVFoundation), decodes to RGBA, and the webview polls frames over
// Tauri's raw-bytes IPC (`tauri::ipc::Response`) into a <canvas>. This
// deliberately measures what pushing pixels across the IPC bridge costs.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{
    CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution,
};
use nokhwa::Camera;
use tauri::{Manager, State};

// ---------------------------------------------------------------------------
// Gallery: list the 200 JPEGs so the frontend can convertFileSrc() them.
// ---------------------------------------------------------------------------

/// Absolute path of `apps/peek-assets/`. This is a research crate that is
/// always run from the repo, so CARGO_MANIFEST_DIR is the honest anchor.
fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("peek-assets")
}

/// Returns absolute paths of all *.jpg in apps/peek-assets, sorted.
/// The frontend turns each into an `asset://` URL via core.convertFileSrc.
#[tauri::command]
fn list_images() -> Result<Vec<String>, String> {
    let dir = assets_dir().canonicalize().map_err(|e| e.to_string())?;
    let mut paths: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| format!("read_dir {}: {}", dir.display(), e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|x| x.eq_ignore_ascii_case("jpg"))
                .unwrap_or(false)
        })
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    paths.sort();
    Ok(paths)
}

// ---------------------------------------------------------------------------
// Secondary camera path: nokhwa (AVFoundation) → RGBA → raw-bytes IPC poll.
// ---------------------------------------------------------------------------

struct CamFrame {
    seq: u32,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Default)]
struct RustCam {
    running: Arc<AtomicBool>,
    latest: Arc<Mutex<Option<CamFrame>>>,
    status: Arc<Mutex<String>>,
}

/// Starts the nokhwa capture worker (idempotent). The worker owns the Camera
/// (created inside the thread; nokhwa handles are not shared across threads),
/// decodes each frame to RGBA and publishes only the newest one.
#[tauri::command]
fn rust_cam_start(cam: State<'_, RustCam>) -> Result<String, String> {
    if cam.running.swap(true, Ordering::SeqCst) {
        return Ok("already running".into());
    }
    *cam.status.lock().unwrap() = "starting".into();

    // macOS: AVFoundation requires an explicit authorization request before a
    // capture session can start. nokhwa wraps requestAccessForMediaType; the
    // callback fires after TCC resolves (prompt click or instant if decided).
    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    nokhwa::nokhwa_initialize(move |granted| {
        let _ = tx.send(granted);
    });
    let granted = rx
        .recv_timeout(Duration::from_secs(90)) // generous: TCC prompt may be pending
        .map_err(|_| "timed out waiting for camera authorization".to_string())?;
    if !granted {
        cam.running.store(false, Ordering::SeqCst);
        *cam.status.lock().unwrap() = "camera authorization denied (TCC)".into();
        return Err("camera authorization denied (TCC)".into());
    }

    let running = cam.running.clone();
    let latest = cam.latest.clone();
    let status = cam.status.clone();
    std::thread::spawn(move || {
        // 640x480@30 to match the getUserMedia path (AbsoluteHighestFrameRate
        // picked 1920x1080 YUYV, an unfair decode workload for the comparison).
        let requested = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::Closest(
            CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 30),
        ));
        let fallback = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let mut camera = match Camera::new(CameraIndex::Index(0), requested)
            .or_else(|_| Camera::new(CameraIndex::Index(0), fallback))
        {
            Ok(c) => c,
            Err(e) => {
                *status.lock().unwrap() = format!("Camera::new failed: {e}");
                println!("[peek][rust-cam] Camera::new failed: {e}");
                running.store(false, Ordering::SeqCst);
                return;
            }
        };
        let fmt = camera.camera_format();
        if let Err(e) = camera.open_stream() {
            *status.lock().unwrap() = format!("open_stream failed: {e}");
            println!("[peek][rust-cam] open_stream failed: {e}");
            running.store(false, Ordering::SeqCst);
            return;
        }
        *status.lock().unwrap() = format!("capturing at {fmt}");
        println!("[peek][rust-cam] stream open: {fmt}");

        let mut seq: u32 = 0;
        let mut captured_in_window: u32 = 0;
        let mut window_start = Instant::now();
        while running.load(Ordering::SeqCst) {
            match camera.frame() {
                Ok(buf) => match buf.decode_image::<RgbAFormat>() {
                    Ok(img) => {
                        seq = seq.wrapping_add(1);
                        captured_in_window += 1;
                        let (w, h) = (img.width(), img.height());
                        *latest.lock().unwrap() = Some(CamFrame {
                            seq,
                            width: w,
                            height: h,
                            rgba: img.into_raw(),
                        });
                    }
                    Err(e) => {
                        println!("[peek][rust-cam] decode error: {e}");
                        std::thread::sleep(Duration::from_millis(50));
                    }
                },
                Err(e) => {
                    println!("[peek][rust-cam] frame error: {e}");
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            if window_start.elapsed() >= Duration::from_secs(2) {
                let fps = captured_in_window as f64 / window_start.elapsed().as_secs_f64();
                println!("[peek][rust-cam] capture_fps={fps:.1}");
                captured_in_window = 0;
                window_start = Instant::now();
            }
        }
        let _ = camera.stop_stream();
        println!("[peek][rust-cam] stopped");
        *status.lock().unwrap() = "stopped".into();
    });

    Ok("started".into())
}

#[tauri::command]
fn rust_cam_stop(cam: State<'_, RustCam>) {
    cam.running.store(false, Ordering::SeqCst);
}

#[tauri::command]
fn rust_cam_status(cam: State<'_, RustCam>) -> String {
    cam.status.lock().unwrap().clone()
}

/// Polls the newest captured frame over the raw-bytes IPC channel (no JSON,
/// no base64). Layout: 16-byte little-endian header [seq, width, height,
/// flags] + RGBA payload when flags bit0 = "new frame vs `last_seq`".
/// Returning only the header when nothing changed keeps the poll cheap.
#[tauri::command]
fn rust_cam_frame(last_seq: u32, cam: State<'_, RustCam>) -> tauri::ipc::Response {
    let guard = cam.latest.lock().unwrap();
    let mut out: Vec<u8>;
    match guard.as_ref() {
        Some(f) if f.seq != last_seq => {
            out = Vec::with_capacity(16 + f.rgba.len());
            out.extend_from_slice(&f.seq.to_le_bytes());
            out.extend_from_slice(&f.width.to_le_bytes());
            out.extend_from_slice(&f.height.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&f.rgba);
        }
        other => {
            let seq = other.map(|f| f.seq).unwrap_or(0);
            out = Vec::with_capacity(16);
            out.extend_from_slice(&seq.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }
    }
    tauri::ipc::Response::new(out)
}

// ---------------------------------------------------------------------------
// Verification hooks (instrumentation only — not part of the product).
// ---------------------------------------------------------------------------

/// Verification-harness mode: "" (off, human drives), "1" (all phases), or a
/// phase subset like "rust" — ui/verify.js interprets the value.
#[tauri::command]
fn verify_mode() -> String {
    std::env::var("PEEK_VERIFY").unwrap_or_default()
}

/// Lets the frontend write evidence lines to stdout so the external harness
/// can scrape fps / permission outcomes / gallery progress from one log.
#[tauri::command]
fn log_stat(line: String) {
    println!("[peek] {line}");
}

// ---------------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .manage(RustCam::default())
        .setup(|app| {
            // The asset:// protocol is scope-checked. The scope in
            // tauri.conf.json is static; for a repo-relative directory we
            // extend it at runtime instead of hardcoding a machine path.
            let dir = assets_dir().canonicalize()?;
            app.asset_protocol_scope().allow_directory(&dir, false)?;
            println!("[peek] asset scope: {}", dir.display());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_images,
            rust_cam_start,
            rust_cam_stop,
            rust_cam_status,
            rust_cam_frame,
            verify_mode,
            log_stat
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
