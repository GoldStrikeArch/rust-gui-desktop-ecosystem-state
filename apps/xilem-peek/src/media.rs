//! Hardware plumbing for "Peek": camera capture thread (nokhwa/AVFoundation),
//! mic RMS thread (cpal), beep playback thread (rodio), and gallery thumbnail
//! decoding (image). Everything here runs off the UI thread and reports back
//! through xilem `MessageProxy`s.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{
    CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution,
};
use nokhwa::{nokhwa_check, nokhwa_initialize, Camera};
use rodio::source::SineWave;
use rodio::{DeviceSinkBuilder, Source};
use xilem::core::MessageProxy;
use xilem::masonry::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

/// Seconds to wait for the user to answer a TCC permission prompt.
const PERMISSION_WAIT_SECS: u64 = 35;

pub fn rgba_image_data(rgba: Vec<u8>, width: u32, height: u32) -> ImageData {
    ImageData {
        data: Blob::new(Arc::new(rgba)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
}

// ===========================================================================
// Camera
// ===========================================================================

#[derive(Debug)]
pub enum CamCmd {
    Start,
    Stop,
}

#[derive(Debug)]
pub enum CamMsg {
    /// Human-readable progress ("requesting permission…").
    Status(String),
    /// The negotiated capture format, reported once after open.
    Format(String),
    /// One decoded RGBA frame.
    Frame(ImageData),
    Error(String),
    Stopped,
}

/// Blocking camera loop; run on a dedicated `std::thread`.
///
/// Handles the macOS TCC dance: if authorization is not yet determined,
/// `nokhwa_initialize` fires the system prompt and we wait (up to
/// [`PERMISSION_WAIT_SECS`]) for the user's answer.
///
/// The `running` flag doubles as the "a capture thread is alive" latch that
/// the worker consults before spawning another thread, so it must be cleared
/// on *every* exit path (including errors), not just on requested stops.
pub fn camera_thread(proxy: MessageProxy<CamMsg>, running: Arc<AtomicBool>) {
    camera_thread_inner(&proxy, &running);
    running.store(false, Ordering::Relaxed);
}

fn camera_thread_inner(proxy: &MessageProxy<CamMsg>, running: &AtomicBool) {
    let send = |m: CamMsg| proxy.message(m).is_ok();

    if !nokhwa_check() {
        send(CamMsg::Status(
            "requesting camera permission (TCC)…".to_owned(),
        ));
        let (tx, rx) = std::sync::mpsc::channel();
        nokhwa_initialize(move |granted| {
            let _ = tx.send(granted);
        });
        match rx.recv_timeout(Duration::from_secs(PERMISSION_WAIT_SECS)) {
            Ok(true) => {}
            Ok(false) => {
                send(CamMsg::Error("camera permission denied by TCC".to_owned()));
                return;
            }
            Err(_) => {
                send(CamMsg::Error(format!(
                    "no TCC answer within {PERMISSION_WAIT_SECS} s (prompt still pending or suppressed)"
                )));
                return;
            }
        }
    }

    // Prefer a modest 640x480@30 NV12 stream (cheap NV12→RGBA convert, ~1.2 MiB
    // RGBA per frame); fall back to whatever the device offers.
    let requests = [
        RequestedFormatType::Closest(CameraFormat::new(
            Resolution::new(640, 480),
            FrameFormat::NV12,
            30,
        )),
        RequestedFormatType::Closest(CameraFormat::new(
            Resolution::new(1280, 720),
            FrameFormat::NV12,
            30,
        )),
        RequestedFormatType::AbsoluteHighestFrameRate,
    ];
    // Another app (e.g. a running video call) can hold the device's
    // configuration lock, making every open fail with "Lock Rejected".
    // Retry for a while — the lock is often held only transiently.
    const OPEN_ATTEMPTS: u32 = 10;
    let mut camera = None;
    let mut last_err = String::new();
    'open: for attempt in 1..=OPEN_ATTEMPTS {
        for req in requests {
            match Camera::new(
                CameraIndex::Index(0),
                RequestedFormat::new::<RgbAFormat>(req),
            ) {
                Ok(cam) => {
                    camera = Some(cam);
                    break 'open;
                }
                Err(e) => last_err = e.to_string(),
            }
        }
        if !running.load(Ordering::Relaxed) {
            send(CamMsg::Stopped);
            return;
        }
        send(CamMsg::Status(format!(
            "camera busy ({attempt}/{OPEN_ATTEMPTS}): {last_err}"
        )));
        std::thread::sleep(Duration::from_secs(2));
    }
    let Some(mut camera) = camera else {
        send(CamMsg::Error(format!("cannot open camera: {last_err}")));
        return;
    };

    if let Err(e) = camera.open_stream() {
        send(CamMsg::Error(format!("cannot open stream: {e}")));
        return;
    }
    let fmt = camera.camera_format();
    send(CamMsg::Format(format!(
        "{} {}x{}@{}fps → RGBA",
        fmt.format(),
        fmt.resolution().width(),
        fmt.resolution().height(),
        fmt.frame_rate()
    )));
    send(CamMsg::Status("live".to_owned()));

    while running.load(Ordering::Relaxed) {
        match camera.frame() {
            Ok(buffer) => match buffer.decode_image::<RgbAFormat>() {
                Ok(img) => {
                    let (w, h) = (img.width(), img.height());
                    let data = rgba_image_data(img.into_raw(), w, h);
                    if !send(CamMsg::Frame(data)) {
                        break; // event loop gone / view torn down
                    }
                }
                Err(e) => {
                    send(CamMsg::Error(format!("frame decode failed: {e}")));
                    break;
                }
            },
            Err(e) => {
                send(CamMsg::Error(format!("frame capture failed: {e}")));
                break;
            }
        }
    }

    let _ = camera.stop_stream();
    send(CamMsg::Stopped);
}

// ===========================================================================
// Microphone
// ===========================================================================

#[derive(Debug)]
pub enum MicCmd {
    Start,
    Stop,
}

#[derive(Debug)]
pub enum MicMsg {
    Started(String),
    /// RMS of all samples seen since the previous message (linear, 0..1).
    Level(f32),
    Error(String),
    Stopped,
}

/// Blocking mic loop; run on a dedicated `std::thread`. The cpal callback
/// accumulates sum-of-squares; this thread drains it at ~20 Hz into RMS
/// messages. Building the input stream is what fires the mic TCC prompt.
///
/// Like [`camera_thread`], clears `running` on every exit path.
pub fn mic_thread(proxy: MessageProxy<MicMsg>, running: Arc<AtomicBool>) {
    mic_thread_inner(&proxy, &running);
    running.store(false, Ordering::Relaxed);
}

fn mic_thread_inner(proxy: &MessageProxy<MicMsg>, running: &AtomicBool) {
    let send = |m: MicMsg| proxy.message(m).is_ok();

    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        send(MicMsg::Error("no default input device".to_owned()));
        return;
    };
    #[allow(deprecated)] // `description()` allocates a whole struct for a label
    let name = device.name().unwrap_or_else(|_| "unknown".to_owned());
    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            send(MicMsg::Error(format!("no input config: {e}")));
            return;
        }
    };
    if config.sample_format() != cpal::SampleFormat::F32 {
        send(MicMsg::Error(format!(
            "unsupported sample format {:?}",
            config.sample_format()
        )));
        return;
    }
    let rate = config.sample_rate();

    // (sum of squares, sample count) accumulated by the audio callback.
    let accum = Arc::new(Mutex::new((0.0_f64, 0_usize)));
    let cb_accum = accum.clone();
    let err_flag = Arc::new(Mutex::new(None::<String>));
    let cb_err = err_flag.clone();
    let stream = match device.build_input_stream(
        &config.into(),
        move |data: &[f32], _| {
            let mut acc = cb_accum.lock().unwrap();
            for &s in data {
                acc.0 += f64::from(s) * f64::from(s);
            }
            acc.1 += data.len();
        },
        move |e| {
            *cb_err.lock().unwrap() = Some(e.to_string());
        },
        None,
    ) {
        Ok(s) => s,
        Err(e) => {
            send(MicMsg::Error(format!("cannot build input stream: {e}")));
            return;
        }
    };
    if let Err(e) = stream.play() {
        send(MicMsg::Error(format!("cannot start input stream: {e}")));
        return;
    }
    send(MicMsg::Started(format!("{name} @ {rate} Hz")));

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(50));
        if let Some(e) = err_flag.lock().unwrap().take() {
            send(MicMsg::Error(format!("stream error: {e}")));
            break;
        }
        let (sq, n) = {
            let mut acc = accum.lock().unwrap();
            std::mem::replace(&mut *acc, (0.0, 0))
        };
        let rms = if n > 0 { (sq / n as f64).sqrt() } else { 0.0 };
        if !send(MicMsg::Level(rms as f32)) {
            break;
        }
    }

    drop(stream);
    send(MicMsg::Stopped);
}

// ===========================================================================
// Beep (audio out)
// ===========================================================================

#[derive(Debug)]
pub enum BeepCmd {
    Play,
}

#[derive(Debug)]
pub enum BeepMsg {
    Done,
    Error(String),
}

/// Open the default output, play a short 880 Hz sine, let it finish, drop.
/// Run on a throwaway `std::thread` (the rodio stream is not `Send`).
pub fn beep_thread(proxy: MessageProxy<BeepMsg>) {
    let result = (|| -> Result<(), String> {
        let sink = DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
        sink.mixer().add(
            SineWave::new(880.0)
                .take_duration(Duration::from_millis(180))
                .amplify(0.25),
        );
        // Keep the device open until the tone has surely drained.
        std::thread::sleep(Duration::from_millis(300));
        Ok(())
    })();
    let _ = proxy.message(match result {
        Ok(()) => BeepMsg::Done,
        Err(e) => BeepMsg::Error(e),
    });
}

// ===========================================================================
// Gallery thumbnails
// ===========================================================================

#[derive(Debug)]
pub enum GalleryMsg {
    /// Number of JPEGs found on disk (sent once, sizes the placeholder grid).
    Found(usize),
    /// One decoded + downscaled thumbnail.
    Thumb(usize, ImageData),
    Error(String),
}

/// Max thumbnail edge in *pixels*. 320x240 sources become 128x96 RGBA
/// (48 KiB each, ~9.4 MiB for all 200) — deliberately small: every image in
/// the scene is re-uploaded into vello's atlas on every rendered frame.
pub const THUMB_MAX_EDGE: u32 = 128;

/// Decode one JPEG and downscale it to a thumbnail. Blocking; run via
/// `spawn_blocking`.
pub fn decode_thumb(path: &Path) -> Result<ImageData, String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let thumb = img.thumbnail(THUMB_MAX_EDGE, THUMB_MAX_EDGE).to_rgba8();
    let (w, h) = (thumb.width(), thumb.height());
    Ok(rgba_image_data(thumb.into_raw(), w, h))
}

/// The `apps/peek-assets` directory, resolved relative to this crate.
pub fn assets_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../peek-assets")
}
