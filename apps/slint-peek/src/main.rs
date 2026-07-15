// slint-peek — SPEC-6 "Peek": camera preview, mic VU meter, audio beep,
// async thumbnail gallery. Production code lives here; env-gated
// verification hooks (auto-start / stats logging / auto-quit / snapshot)
// live in verify.rs.

mod verify;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::{
    ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode,
    VecModel, Weak,
};

slint::include_modules!();

/// Counters shared between the capture thread, the rendering notifier and
/// the FPS timer. "presented" counts window redraws that displayed a camera
/// frame not shown before — the spec's "frames actually presented".
#[derive(Default)]
pub struct CamStats {
    pub run: AtomicBool,
    pub captured: AtomicU32,
    pub delivered: AtomicU32,
    pub presented: AtomicU32,
}

#[derive(Default)]
pub struct MicStats {
    pub run: AtomicBool,
    /// f32 bit pattern of the latest linear RMS from the cpal callback.
    pub level_bits: AtomicU32,
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    let cam = Arc::new(CamStats::default());
    let mic = Arc::new(MicStats::default());

    // ---- presented-frame counting -------------------------------------
    // The rendering notifier fires per redraw; a redraw whose delivered
    // counter advanced since the previous one showed a fresh camera frame.
    let notifier_ok = {
        let cam = cam.clone();
        let mut last_delivered = 0u32;
        ui.window()
            .set_rendering_notifier(move |state, _| {
                if matches!(state, slint::RenderingState::AfterRendering) {
                    let d = cam.delivered.load(Ordering::Relaxed);
                    if d != last_delivered {
                        last_delivered = d;
                        cam.presented.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .map_err(|e| eprintln!("[peek] rendering notifier unavailable ({e:?}); falling back to delivered count"))
            .is_ok()
    };

    // ---- FPS readout (2 Hz) --------------------------------------------
    let fps_timer = Timer::default();
    {
        let ui_weak = ui.as_weak();
        let cam = cam.clone();
        let mut last = (Instant::now(), 0u32, 0u32); // (t, presented, captured)
        fps_timer.start(TimerMode::Repeated, Duration::from_millis(500), move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            if !ui.get_camera_running() {
                return;
            }
            let now = Instant::now();
            let p = if notifier_ok {
                cam.presented.load(Ordering::Relaxed)
            } else {
                cam.delivered.load(Ordering::Relaxed)
            };
            let c = cam.captured.load(Ordering::Relaxed);
            let dt = now.duration_since(last.0).as_secs_f32();
            if dt > 0.0 {
                ui.set_presented_fps((p.wrapping_sub(last.1)) as f32 / dt);
                ui.set_capture_fps((c.wrapping_sub(last.2)) as f32 / dt);
            }
            last = (now, p, c);
        });
    }

    // ---- camera start/stop ----------------------------------------------
    ui.on_toggle_camera({
        let ui_weak = ui.as_weak();
        let cam = cam.clone();
        move || {
            let ui = ui_weak.unwrap();
            if ui.get_camera_running() {
                cam.run.store(false, Ordering::Relaxed);
                ui.set_camera_running(false);
                ui.set_camera_status("stopped — press Start".into());
                ui.set_presented_fps(0.0);
                ui.set_capture_fps(0.0);
            } else {
                ui.set_camera_running(true);
                ui.set_camera_status("starting camera…".into());
                cam.run.store(true, Ordering::Relaxed);
                spawn_camera_thread(ui_weak.clone(), cam.clone());
            }
        }
    });

    // ---- mic meter (20 Hz UI update) -------------------------------------
    let mic_timer = Timer::default();
    {
        let ui_weak = ui.as_weak();
        let mic = mic.clone();
        let mut peak = 0.0f32;
        mic_timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            if !ui.get_mic_running() {
                return;
            }
            let rms = f32::from_bits(mic.level_bits.load(Ordering::Relaxed));
            // Map linear RMS onto 0..1 across a 60 dB range.
            let db = 20.0 * rms.max(1e-6).log10();
            let level = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
            peak = level.max(peak - 0.008); // slow peak-hold decay
            ui.set_mic_level(level);
            ui.set_mic_peak(peak);
        });
    }

    ui.on_toggle_mic({
        let ui_weak = ui.as_weak();
        let mic = mic.clone();
        move || {
            let ui = ui_weak.unwrap();
            if ui.get_mic_running() {
                mic.run.store(false, Ordering::Relaxed);
                ui.set_mic_running(false);
                ui.set_mic_level(0.0);
                ui.set_mic_peak(0.0);
            } else {
                ui.set_mic_running(true);
                ui.set_mic_status("starting mic…".into());
                mic.run.store(true, Ordering::Relaxed);
                spawn_mic_thread(ui_weak.clone(), mic.clone());
            }
        }
    });

    // ---- audio out (rodio) ------------------------------------------------
    // The default sink is opened lazily on the first beep and kept alive for
    // the session (audio *output* has no TCC gate on macOS).
    ui.on_beep({
        let ui_weak = ui.as_weak();
        let sink: RefCell<Option<rodio::MixerDeviceSink>> = RefCell::new(None);
        let count = Cell::new(0u32);
        move || {
            let ui = ui_weak.unwrap();
            let mut guard = sink.borrow_mut();
            if guard.is_none() {
                match rodio::DeviceSinkBuilder::open_default_sink() {
                    Ok(s) => *guard = Some(s),
                    Err(e) => {
                        eprintln!("[peek] audio: failed to open output: {e}");
                        ui.set_audio_status(format!("audio out failed: {e}").into());
                        return;
                    }
                }
            }
            use rodio::Source;
            let src = rodio::source::SineWave::new(880.0)
                .take_duration(Duration::from_millis(250))
                .amplify(0.20);
            guard.as_ref().unwrap().mixer().add(src);
            count.set(count.get() + 1);
            eprintln!("[peek] audio: beep #{} queued", count.get());
            ui.set_audio_status(format!("beep #{} (880 Hz · 250 ms)", count.get()).into());
        }
    });

    // ---- gallery ------------------------------------------------------------
    start_gallery(&ui);

    // ---- verification hooks (env-gated; see verify.rs) ----------------------
    let _verify_timers = verify::install(&ui, cam.clone());

    ui.run()
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

fn set_cam_status(ui: &Weak<MainWindow>, msg: &str) {
    let ui = ui.clone();
    let msg = msg.to_string();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui.upgrade() {
            ui.set_camera_status(msg.into());
        }
    });
}

fn cam_fail(ui: &Weak<MainWindow>, msg: String) {
    eprintln!("[peek] camera: {msg}");
    let ui = ui.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui.upgrade() {
            ui.set_camera_running(false);
            ui.set_camera_status(msg.into());
        }
    });
}

fn spawn_camera_thread(ui_weak: Weak<MainWindow>, cam: Arc<CamStats>) {
    std::thread::spawn(move || {
        use nokhwa::pixel_format::RgbAFormat;
        use nokhwa::utils::{CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType};

        // macOS TCC: if not yet authorized, trigger the system prompt via
        // AVCaptureDevice requestAccessForMediaType and wait for the verdict
        // (the runner clicks Allow; budget ~35 s). Denial degrades to an
        // in-UI error, never a crash.
        if !nokhwa::nokhwa_check() {
            eprintln!("[peek] camera: not authorized yet; requesting access (TCC prompt expected)");
            set_cam_status(&ui_weak, "waiting for camera permission…");
            let (tx, rx) = std::sync::mpsc::channel();
            nokhwa::nokhwa_initialize(move |granted| {
                let _ = tx.send(granted);
            });
            match rx.recv_timeout(Duration::from_secs(35)) {
                Ok(true) => eprintln!("[peek] camera: permission granted"),
                Ok(false) => {
                    return cam_fail(
                        &ui_weak,
                        "camera permission denied (TCC) — preview unavailable".into(),
                    )
                }
                Err(_) => {
                    return cam_fail(
                        &ui_weak,
                        "camera permission prompt unanswered after 35 s".into(),
                    )
                }
            }
            if !cam.run.load(Ordering::Relaxed) {
                return; // stopped while waiting
            }
        }

        // Format negotiation is hand-rolled: RequestedFormatType::Closest only
        // matches when the exact requested resolution+format pair exists, and
        // None picks the first advertised entry — but nokhwa's macOS bindings
        // advertise frame-rate-range *minimums* (e.g. 15 fps) that set_all()
        // rejects (it only matches a range's max rate). AbsoluteHighestFrameRate
        // always lands on a range max, so it opens reliably; the preferred
        // format is set afterwards from the real list.
        let requested =
            RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let mut camera = match nokhwa::Camera::new(CameraIndex::Index(0), requested) {
            Ok(c) => c,
            Err(e) => return cam_fail(&ui_weak, format!("failed to open camera: {e}")),
        };
        match camera.compatible_camera_formats() {
            Ok(formats) => {
                eprintln!("[peek] camera: {} advertised formats", formats.len());
                for f in &formats {
                    eprintln!(
                        "[peek]   {}x{} {:?} @ {}",
                        f.resolution().width(),
                        f.resolution().height(),
                        f.format(),
                        f.frame_rate()
                    );
                }
                // Rank: decodable format, resolution closest to 1280x720,
                // frame rate closest to 30; NV12 > YUYV > MJPEG on ties.
                // Some advertised entries are unsettable (range-min fps), so
                // walk the ranking until one sticks.
                let decodable = [FrameFormat::NV12, FrameFormat::YUYV, FrameFormat::MJPEG];
                let mut ranked: Vec<_> = formats
                    .iter()
                    .filter(|f| decodable.contains(&f.format()))
                    .copied()
                    .collect();
                ranked.sort_by_key(|f| {
                    let dx = f.resolution().width() as i64 - 1280;
                    let dy = f.resolution().height() as i64 - 720;
                    let dist = dx * dx + dy * dy;
                    let fps_penalty = (f.frame_rate() as i64 - 30).abs() * 10_000;
                    let fmt_penalty =
                        decodable.iter().position(|d| *d == f.format()).unwrap() as i64;
                    dist + fps_penalty + fmt_penalty
                });
                for fmt in ranked.into_iter().take(8) {
                    match camera.set_camera_format(fmt) {
                        Ok(()) => break,
                        Err(e) => eprintln!(
                            "[peek] camera: set_camera_format({fmt}) rejected ({e}); trying next"
                        ),
                    }
                }
            }
            Err(e) => eprintln!("[peek] camera: could not list formats ({e}); keeping default"),
        }
        if let Err(e) = camera.open_stream() {
            return cam_fail(&ui_weak, format!("failed to open stream: {e}"));
        }
        let cf = camera.camera_format();
        let name = camera.info().human_name();
        let desc = format!(
            "{} · {}x{} {:?} @ {} fps",
            name,
            cf.resolution().width(),
            cf.resolution().height(),
            cf.format(),
            cf.frame_rate()
        );
        eprintln!("[peek] camera: {desc}");
        set_cam_status(&ui_weak, &desc);

        while cam.run.load(Ordering::Relaxed) {
            // frame() blocks until AVFoundation delivers the next sample.
            let frame = match camera.frame() {
                Ok(f) => f,
                Err(e) => {
                    cam_fail(&ui_weak, format!("frame error: {e}"));
                    break;
                }
            };
            cam.captured.fetch_add(1, Ordering::Relaxed);
            let res = frame.resolution();
            // Fresh SharedPixelBuffer per frame: NV12 -> RGBA decoded straight
            // into it on this thread; the buffer (Send) crosses to the UI
            // thread, where the (non-Send) slint::Image is constructed.
            let mut pb = SharedPixelBuffer::<Rgba8Pixel>::new(res.width(), res.height());
            if let Err(e) = frame.decode_image_to_buffer::<RgbAFormat>(pb.make_mut_bytes()) {
                eprintln!("[peek] camera: decode error: {e}");
                continue;
            }
            let ui2 = ui_weak.clone();
            let cam2 = cam.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui2.upgrade() {
                    // Alpha is opaque, so premultiplied == straight; the
                    // premultiplied constructor skips a conversion pass.
                    ui.set_camera_frame(Image::from_rgba8_premultiplied(pb));
                    cam2.delivered.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        let _ = camera.stop_stream();
        eprintln!("[peek] camera: capture thread exited");
    });
}

// ---------------------------------------------------------------------------
// Mic
// ---------------------------------------------------------------------------

fn set_mic_status(ui: &Weak<MainWindow>, msg: &str) {
    let ui = ui.clone();
    let msg = msg.to_string();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui.upgrade() {
            ui.set_mic_status(msg.into());
        }
    });
}

fn mic_fail(ui: &Weak<MainWindow>, msg: String) {
    eprintln!("[peek] mic: {msg}");
    let ui = ui.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui.upgrade() {
            ui.set_mic_running(false);
            ui.set_mic_status(msg.into());
        }
    });
}

/// The cpal Stream is !Send, so a dedicated thread builds it, owns it while
/// the run flag holds, and drops it on the same thread.
fn spawn_mic_thread(ui_weak: Weak<MainWindow>, mic: Arc<MicStats>) {
    std::thread::spawn(move || {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let Some(device) = host.default_input_device() else {
            return mic_fail(&ui_weak, "no default input device".into());
        };
        let name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "unknown mic".into());
        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => return mic_fail(&ui_weak, format!("no input config: {e}")),
        };
        let sample_format = config.sample_format();
        let rate = config.sample_rate();
        let stream_config: cpal::StreamConfig = config.into();
        let mic2 = mic.clone();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let sum: f32 = data.iter().map(|s| s * s).sum();
                    let rms = (sum / data.len().max(1) as f32).sqrt();
                    mic2.level_bits.store(rms.to_bits(), Ordering::Relaxed);
                },
                move |e| eprintln!("[peek] mic: stream error: {e}"),
                None,
            ),
            other => {
                return mic_fail(&ui_weak, format!("unsupported mic sample format {other:?}"))
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => return mic_fail(&ui_weak, format!("failed to build input stream: {e}")),
        };
        if let Err(e) = stream.play() {
            return mic_fail(&ui_weak, format!("failed to start input stream: {e}"));
        }
        eprintln!("[peek] mic: running · {name} @ {rate} Hz");
        set_mic_status(&ui_weak, &format!("mic running · {name} @ {rate} Hz"));
        while mic.run.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
        drop(stream);
        mic.level_bits.store(0, Ordering::Relaxed);
        set_mic_status(&ui_weak, "mic stopped");
        eprintln!("[peek] mic: stopped");
    });
}

// ---------------------------------------------------------------------------
// Gallery
// ---------------------------------------------------------------------------

const GALLERY_WORKERS: usize = 4;
const THUMB_MAX_PX: u32 = 208; // 104px cell at 2x scale factor

fn start_gallery(ui: &MainWindow) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../peek-assets");
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension()
                    .map(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            ui.set_gallery_status(format!("cannot read {}: {e}", dir.display()).into());
            return;
        }
    };
    paths.sort();
    let total = paths.len();

    // Placeholder-filled model: workers fill rows in-place so the grid order
    // stays stable regardless of decode completion order.
    let model = Rc::new(VecModel::from(vec![Image::default(); total]));
    ui.set_thumbs(ModelRc::from(model));
    ui.set_thumbs_total(total as i32);
    ui.set_gallery_status(format!("decoding {total} JPEGs on {GALLERY_WORKERS} threads…").into());

    let started = Instant::now();
    let loaded = Arc::new(AtomicU32::new(0));
    let ui_weak = ui.as_weak();
    let mut chunks: Vec<Vec<(usize, PathBuf)>> =
        (0..GALLERY_WORKERS).map(|_| Vec::new()).collect();
    for (i, p) in paths.into_iter().enumerate() {
        chunks[i % GALLERY_WORKERS].push((i, p));
    }
    for chunk in chunks {
        let ui_weak = ui_weak.clone();
        let loaded = loaded.clone();
        std::thread::spawn(move || {
            for (idx, path) in chunk {
                // Decode + downscale off the UI thread; only the small RGBA
                // buffer crosses over.
                let pb = decode_thumb(&path);
                let ui2 = ui_weak.clone();
                let loaded2 = loaded.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = ui2.upgrade() else { return };
                    if let Some(pb) = pb {
                        ui.get_thumbs()
                            .set_row_data(idx, Image::from_rgba8_premultiplied(pb));
                    }
                    let n = loaded2.fetch_add(1, Ordering::Relaxed) + 1;
                    ui.set_thumbs_loaded(n as i32);
                    if n as usize == total {
                        let msg = format!(
                            "{} thumbs decoded + downscaled in {} ms ({} threads)",
                            total,
                            started.elapsed().as_millis(),
                            GALLERY_WORKERS
                        );
                        eprintln!("[peek] gallery: {msg}");
                        ui.set_gallery_status(msg.into());
                    }
                });
            }
        });
    }
}

fn decode_thumb(path: &Path) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let img = image::open(path).ok()?;
    let thumb = img.thumbnail(THUMB_MAX_PX, THUMB_MAX_PX).into_rgba8();
    Some(SharedPixelBuffer::clone_from_slice(
        thumb.as_raw(),
        thumb.width(),
        thumb.height(),
    ))
}
