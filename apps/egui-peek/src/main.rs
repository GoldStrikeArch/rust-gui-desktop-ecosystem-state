//! "Peek" — SPEC-6 media & hardware test, egui 0.35 + eframe.
//!
//! - Camera: nokhwa (AVFoundation) on a capture thread → bounded channel of
//!   `egui::ColorImage` → one `TextureHandle` created on first frame, then
//!   `tex.set(..)` per frame (full-texture re-upload; egui's intended dynamic
//!   texture path). The capture thread calls `ctx.request_repaint()` per frame.
//!   FPS counter counts frames actually *presented* (texture updated inside a
//!   painted egui frame), not frames captured.
//! - Mic: cpal input stream → RMS into an AtomicU32 → VU bar repainted at
//!   ~20 Hz via `request_repaint_after`.
//! - Audio out: rodio `DeviceSinkBuilder` mixer + `SineWave` beep.
//! - Gallery: 200 JPEGs decoded+downscaled on 4 worker threads (image crate),
//!   uploaded with `ctx.load_texture` per thumbnail (manual texture cache:
//!   handles are ref-counted, freed when dropped; egui does NOT cache for us).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Peek (egui)", // window title
        options,
        Box::new(|_cc| Ok(Box::new(PeekApp::new()))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Camera,
    Audio,
    Gallery,
}

struct PeekApp {
    tab: Tab,
    cam: CamPanel,
    mic: MicPanel,
    beep: BeepPanel,
    gallery: Gallery,
    hooks: verif::Hooks,
}

impl PeekApp {
    fn new() -> Self {
        Self {
            tab: Tab::Camera,
            cam: CamPanel::new(),
            mic: MicPanel::new(),
            beep: BeepPanel::new(),
            gallery: Gallery::new(default_assets_dir()),
            hooks: verif::Hooks::from_env(),
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Peek");
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Camera, "Camera");
                ui.selectable_value(&mut self.tab, Tab::Audio, "Audio");
                ui.selectable_value(&mut self.tab, Tab::Gallery, "Gallery");
            });
        });

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Camera => self.cam.show(ui),
            Tab::Audio => {
                self.mic.show(ui);
                ui.separator();
                self.beep.show(ui);
            }
            Tab::Gallery => self.gallery.show(ui),
        });
    }
}

fn default_assets_dir() -> PathBuf {
    std::env::var_os("PEEK_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../peek-assets")))
}

impl eframe::App for PeekApp {
    // egui 0.35: `App::ui` (replacing `App::update` since 0.34) hands us the
    // root `Ui`; panels are laid out inside it.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        verif::tick(self, &ui.ctx().clone());
        self.show(ui);
    }
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

enum CamPermission {
    /// Not asked this run; actual TCC state unknown until checked.
    Unknown,
    /// `nokhwa_initialize` fired; waiting for the (async) user answer.
    Pending {
        since: Instant,
        result: Arc<Mutex<Option<bool>>>,
    },
    Granted,
    Denied,
}

enum CamMsg {
    Format(String),
    Frame(egui::ColorImage),
    Error(String),
}

struct CamSession {
    stop: Arc<AtomicBool>,
    rx: mpsc::Receiver<CamMsg>,
    captured: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl Drop for CamSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.join.take() {
            let _ = handle.join(); // returns after ≤ ~1 frame interval
        }
    }
}

struct CamPanel {
    permission: CamPermission,
    session: Option<CamSession>,
    tex: Option<egui::TextureHandle>,
    format_desc: Option<String>,
    error: Option<String>,
    /// Timestamps of presented frames (texture updates), for the FPS counter.
    presented: std::collections::VecDeque<Instant>,
    presented_total: u64,
}

impl CamPanel {
    fn new() -> Self {
        Self {
            permission: CamPermission::Unknown,
            session: None,
            tex: None,
            format_desc: None,
            error: None,
            presented: std::collections::VecDeque::new(),
            presented_total: 0,
        }
    }

    fn running(&self) -> bool {
        self.session.is_some()
    }

    /// Start clicked (or autostarted): resolve TCC permission, then spawn.
    fn start(&mut self, ctx: &egui::Context) {
        self.error = None;
        if nokhwa::nokhwa_check() {
            self.permission = CamPermission::Granted;
            self.spawn(ctx);
        } else {
            // Triggers the TCC camera prompt (AVCaptureDevice requestAccess);
            // the callback fires on a system thread once the user answers.
            let result = Arc::new(Mutex::new(None));
            let cb_result = Arc::clone(&result);
            let cb_ctx = ctx.clone();
            nokhwa::nokhwa_initialize(move |granted| {
                *cb_result.lock().unwrap() = Some(granted);
                cb_ctx.request_repaint();
            });
            self.permission = CamPermission::Pending {
                since: Instant::now(),
                result,
            };
        }
    }

    fn stop(&mut self) {
        self.session = None; // Drop impl signals the thread and joins it
    }

    fn spawn(&mut self, ctx: &egui::Context) {
        let stop = Arc::new(AtomicBool::new(false));
        let captured = Arc::new(AtomicU64::new(0));
        let dropped = Arc::new(AtomicU64::new(0));
        // Bounded: capture thread try_sends and drops frames if the UI lags,
        // so the queue can never grow and preview latency stays ≤ 2 frames.
        let (tx, rx) = mpsc::sync_channel::<CamMsg>(2);
        let join = std::thread::spawn({
            let stop = Arc::clone(&stop);
            let captured = Arc::clone(&captured);
            let dropped = Arc::clone(&dropped);
            let ctx = ctx.clone();
            move || capture_loop(&ctx, &stop, &captured, &dropped, &tx)
        });
        self.session = Some(CamSession {
            stop,
            rx,
            captured,
            dropped,
            join: Some(join),
        });
    }

    /// Drain the frame channel; upload at most the newest frame this paint.
    fn pump(&mut self, ctx: &egui::Context) {
        let mut latest = None;
        let mut ended_err = None;
        if let Some(session) = &self.session {
            loop {
                match session.rx.try_recv() {
                    Ok(CamMsg::Frame(img)) => latest = Some(img),
                    Ok(CamMsg::Format(desc)) => self.format_desc = Some(desc),
                    Ok(CamMsg::Error(err)) => ended_err = Some(err),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            }
        }
        if let Some(err) = ended_err {
            self.error = Some(err);
            self.stop();
        }
        if let Some(img) = latest {
            match &mut self.tex {
                // The dynamic-texture path: allocate once...
                None => {
                    self.tex = Some(ctx.load_texture("camera", img, egui::TextureOptions::LINEAR));
                }
                // ...then full-image re-upload into the same texture id.
                Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
            }
            // Only count "presented" when eframe will actually paint: if the
            // window is minimized or fully occluded, App::ui still runs but
            // rendering is skipped, so counting here would overreport.
            let visible = ctx.input(|i| i.viewport().visible().unwrap_or(true));
            if !visible {
                return;
            }
            let now = Instant::now();
            self.presented.push_back(now);
            self.presented_total += 1;
            while let Some(&front) = self.presented.front() {
                if now - front > Duration::from_secs(2) {
                    self.presented.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    /// Presented-frames-per-second over the trailing 1 s window.
    fn fps(&self) -> f32 {
        let now = Instant::now();
        let window = Duration::from_secs(1);
        let count = self
            .presented
            .iter()
            .filter(|t| now.duration_since(**t) <= window)
            .count();
        count as f32 / window.as_secs_f32()
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();

        // Permission state machine (prompt answer arrives async).
        if let CamPermission::Pending { since, result } = &self.permission {
            let outcome = *result.lock().unwrap();
            let waited = since.elapsed();
            match outcome {
                Some(true) => {
                    self.permission = CamPermission::Granted;
                    self.spawn(&ctx);
                }
                Some(false) => {
                    self.permission = CamPermission::Denied;
                    self.error = Some(
                        "Camera access denied by macOS (TCC). \
                         Grant it in System Settings → Privacy & Security → Camera."
                            .to_owned(),
                    );
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!(
                            "Waiting for the macOS camera permission prompt… ({:.0}s)",
                            waited.as_secs_f32()
                        ));
                    });
                    // Poll while the dialog is up (callback also repaints us).
                    ctx.request_repaint_after(Duration::from_millis(250));
                }
            }
        }

        self.pump(&ctx);

        ui.horizontal(|ui| {
            let pending = matches!(self.permission, CamPermission::Pending { .. });
            if self.running() {
                if ui.button("Stop camera").clicked() {
                    self.stop();
                }
            } else if !pending && ui.button("Start camera").clicked() {
                self.start(&ctx);
            }
            if self.running() {
                ui.label(format!("presented: {:.1} fps", self.fps()));
                if let Some(session) = &self.session {
                    let captured = session.captured.load(Ordering::Relaxed);
                    let dropped = session.dropped.load(Ordering::Relaxed);
                    ui.label(format!("captured: {captured}  dropped: {dropped}"));
                }
                // Keep the fps label honest even if capture stalls.
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        });
        if let Some(desc) = &self.format_desc {
            ui.label(desc.as_str());
        }
        if let Some(err) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, err);
        }

        // Aspect-fit preview.
        if let Some(tex) = &self.tex {
            let avail = ui.available_size();
            let size = tex.size_vec2();
            let scale = (avail.x / size.x).min(avail.y / size.y).min(1.0);
            ui.image((tex.id(), size * scale));
        } else if !self.running() {
            ui.weak("Camera is off.");
        }
    }
}

/// Runs on the capture thread; owns the (non-Send) AVFoundation camera.
fn capture_loop(
    ctx: &egui::Context,
    stop: &AtomicBool,
    captured: &AtomicU64,
    dropped: &AtomicU64,
    tx: &mpsc::SyncSender<CamMsg>,
) {
    use nokhwa::pixel_format::RgbFormat;
    use nokhwa::utils::{
        CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
    };

    let fail = |msg: String| {
        let _ = tx.try_send(CamMsg::Error(msg));
        ctx.request_repaint();
    };

    // Fallback ladder. Two nokhwa 0.10.11 macOS traps drive this:
    // (a) the bindings map Apple's NV12-family fourccs (420v/420f) to
    //     FrameFormat::YUYV, so NV12 requests never resolve on the built-in
    //     camera — YUYV is what its formats enumerate as;
    // (b) `set_all` only matches an AVFrameRateRange by its *max* frame rate
    //     (±1), while `supported_formats` also lists the range minimums, so
    //     e.g. the enumerated "…@15FPS" entries are not actually settable
    //     (this is why RequestedFormatType::None fails outright here).
    let attempts = [
        RequestedFormatType::Closest(CameraFormat::new_from(1280, 720, FrameFormat::YUYV, 30)),
        RequestedFormatType::AbsoluteHighestFrameRate,
        RequestedFormatType::None,
    ];
    let mut camera = None;
    let mut errors = Vec::new();
    for attempt in attempts {
        match nokhwa::Camera::new(CameraIndex::Index(0), RequestedFormat::new::<RgbFormat>(attempt))
        {
            Ok(cam) => {
                camera = Some(cam);
                break;
            }
            Err(err) => errors.push(format!("{attempt}: {err}")),
        }
    }
    let Some(mut camera) = camera else {
        return fail(format!("Failed to open camera: {}", errors.join(" | ")));
    };
    if let Err(err) = camera.open_stream() {
        return fail(format!("Failed to start camera stream: {err}"));
    }
    let format = camera.camera_format();
    let _ = tx.try_send(CamMsg::Format(format!(
        "{} — {}x{} {} @ {} fps (negotiated)",
        camera.info().human_name(),
        format.resolution().width(),
        format.resolution().height(),
        format.format(),
        format.frame_rate(),
    )));

    let mut rgb = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let buffer = match camera.frame() {
            Ok(buffer) => buffer,
            Err(err) => return fail(format!("Camera frame error: {err}")),
        };
        let (w, h) = (
            buffer.resolution().width() as usize,
            buffer.resolution().height() as usize,
        );
        rgb.resize(w * h * 3, 0);
        if let Err(err) = buffer.decode_image_to_buffer::<RgbFormat>(&mut rgb) {
            return fail(format!("Frame decode error: {err}"));
        }
        captured.fetch_add(1, Ordering::Relaxed);
        // CPU-side RGB→RGBA copy; the GPU upload happens on the UI thread.
        let image = egui::ColorImage::from_rgb([w, h], &rgb);
        match tx.try_send(CamMsg::Frame(image)) {
            Ok(()) => ctx.request_repaint(),
            Err(mpsc::TrySendError::Full(_)) => {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(mpsc::TrySendError::Disconnected(_)) => break,
        }
    }
    let _ = camera.stop_stream();
}

// ---------------------------------------------------------------------------
// Mic level meter
// ---------------------------------------------------------------------------

struct MicPanel {
    stream: Option<cpal::Stream>, // dropping it stops capture
    rms: Arc<AtomicU32>,          // f32 bits, written by the audio callback
    cb_error: Arc<Mutex<Option<String>>>,
    desc: Option<String>,
    error: Option<String>,
    peak_hold: f32,
    peak_at: Instant,
}

impl MicPanel {
    fn new() -> Self {
        Self {
            stream: None,
            rms: Arc::new(AtomicU32::new(0)),
            cb_error: Arc::new(Mutex::new(None)),
            desc: None,
            error: None,
            peak_hold: 0.0,
            peak_at: Instant::now(),
        }
    }

    fn running(&self) -> bool {
        self.stream.is_some()
    }

    fn start(&mut self) {
        self.error = None;
        if let Err(err) = self.try_start() {
            self.error = Some(err);
        }
    }

    fn try_start(&mut self) -> Result<(), String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No default input device")?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("No default input config: {e}"))?;
        let name = device
            .description()
            .map(|d| d.name().to_owned())
            .unwrap_or_else(|_| "<unknown>".into());
        self.desc = Some(format!(
            "{name} — {} ch @ {} Hz, {:?}",
            config.channels(),
            config.sample_rate(),
            config.sample_format(),
        ));
        if config.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!(
                "Unhandled input sample format {:?} (expected F32 on macOS)",
                config.sample_format()
            ));
        }
        let rms = Arc::clone(&self.rms);
        let cb_error = Arc::clone(&self.cb_error);
        // Building/playing an input stream is what trips the mic TCC gate.
        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let sum: f32 = data.iter().map(|s| s * s).sum();
                    let value = (sum / data.len().max(1) as f32).sqrt();
                    rms.store(value.to_bits(), Ordering::Relaxed);
                },
                move |err| {
                    *cb_error.lock().unwrap() = Some(format!("Mic stream error: {err}"));
                },
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {e}"))?;
        stream
            .play()
            .map_err(|e| format!("Failed to start input stream: {e}"))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) {
        self.stream = None;
        self.rms.store(0f32.to_bits(), Ordering::Relaxed);
        self.peak_hold = 0.0;
    }

    fn level(&self) -> f32 {
        f32::from_bits(self.rms.load(Ordering::Relaxed))
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("Microphone");
        let cb_error = self.cb_error.lock().unwrap().take();
        if let Some(err) = cb_error {
            self.error = Some(err);
            self.stop();
        }
        ui.horizontal(|ui| {
            if self.running() {
                if ui.button("Stop mic").clicked() {
                    self.stop();
                }
            } else if ui.button("Start mic").clicked() {
                self.start();
            }
            if let Some(desc) = &self.desc {
                ui.weak(desc.as_str());
            }
        });
        if let Some(err) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, err);
        }

        // VU bar: RMS → dBFS mapped from −60 dB..0 dB.
        let rms = self.level();
        let db = if rms > 0.0 { 20.0 * rms.log10() } else { -120.0 };
        let fraction = ((db + 60.0) / 60.0).clamp(0.0, 1.0) as f32;
        if fraction >= self.peak_hold || self.peak_at.elapsed() > Duration::from_secs(1) {
            self.peak_hold = fraction;
            self.peak_at = Instant::now();
        }

        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().min(420.0), 22.0),
            egui::Sense::hover(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        if self.running() {
            let fill = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width() * fraction, rect.height()),
            );
            let color = if fraction > 0.85 {
                egui::Color32::from_rgb(0xd9, 0x53, 0x4f)
            } else if fraction > 0.65 {
                egui::Color32::from_rgb(0xe8, 0xa8, 0x3c)
            } else {
                egui::Color32::from_rgb(0x4c, 0xaf, 0x50)
            };
            painter.rect_filled(fill, 4.0, color);
            let peak_x = rect.min.x + rect.width() * self.peak_hold;
            painter.vline(
                peak_x,
                rect.y_range(),
                egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
            );
        }
        ui.label(if self.running() {
            format!("{db:.1} dBFS (RMS)")
        } else {
            "Mic is off.".to_owned()
        });

        if self.running() {
            // ~20 Hz meter refresh; no repaints at all when the mic is off.
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }
}

// ---------------------------------------------------------------------------
// Audio playback (beep)
// ---------------------------------------------------------------------------

struct BeepPanel {
    sink: Option<rodio::MixerDeviceSink>, // opened lazily on first beep
    desc: Option<String>,
    error: Option<String>,
    beeps: u32,
}

impl BeepPanel {
    fn new() -> Self {
        Self {
            sink: None,
            desc: None,
            error: None,
            beeps: 0,
        }
    }

    fn beep(&mut self) {
        use rodio::Source as _;
        self.error = None;
        if self.sink.is_none() {
            match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(sink) => {
                    self.desc = Some(format!("Output: {:?}", sink.config()));
                    self.sink = Some(sink);
                }
                Err(err) => {
                    self.error = Some(format!("Failed to open audio output: {err}"));
                    return;
                }
            }
        }
        if let Some(sink) = &self.sink {
            let tone = rodio::source::SineWave::new(880.0)
                .take_duration(Duration::from_millis(150))
                .amplify(0.20);
            sink.mixer().add(tone);
            self.beeps += 1;
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("Playback");
        ui.horizontal(|ui| {
            if ui.button("Beep").clicked() {
                self.beep();
            }
            ui.label(format!("{} beep(s) played", self.beeps));
        });
        if let Some(desc) = &self.desc {
            ui.weak(desc.as_str());
        }
        if let Some(err) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, err);
        }
    }
}

// ---------------------------------------------------------------------------
// Gallery
// ---------------------------------------------------------------------------

const THUMB_MAX: u32 = 192; // decode target (2× the ~96 px display cell)
const GALLERY_WORKERS: usize = 4;
const UPLOADS_PER_FRAME: usize = 16; // texture uploads per painted frame

struct Thumb {
    name: String,
    tex: egui::TextureHandle,
}

enum ThumbMsg {
    Ok(usize, String, egui::ColorImage),
    Failed(String),
}

struct Gallery {
    dir: PathBuf,
    started: bool,
    rx: Option<mpsc::Receiver<ThumbMsg>>,
    slots: Vec<Option<Thumb>>,
    total: usize,
    loaded: usize,
    failed: usize,
    started_at: Option<Instant>,
    finished_in: Option<Duration>,
    error: Option<String>,
}

impl Gallery {
    fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            started: false,
            rx: None,
            slots: Vec::new(),
            total: 0,
            loaded: 0,
            failed: 0,
            started_at: None,
            finished_in: None,
            error: None,
        }
    }

    /// Kick off async decode on first show; the UI thread never decodes.
    fn start(&mut self, ctx: &egui::Context) {
        self.started = true;
        self.started_at = Some(Instant::now());
        let mut files: Vec<PathBuf> = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("jpg" | "jpeg" | "JPG" | "JPEG")
                    )
                })
                .collect(),
            Err(err) => {
                self.error = Some(format!("Cannot read {}: {err}", self.dir.display()));
                return;
            }
        };
        files.sort();
        self.total = files.len();
        self.slots = (0..self.total).map(|_| None).collect();

        let files = Arc::new(files);
        let next = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel::<ThumbMsg>();
        self.rx = Some(rx);
        for _ in 0..GALLERY_WORKERS {
            let files = Arc::clone(&files);
            let next = Arc::clone(&next);
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = files.get(i) else { break };
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let msg = match image::open(path) {
                        Ok(img) => {
                            // Triangle-filter downscale on the worker thread.
                            let thumb = img.thumbnail(THUMB_MAX, THUMB_MAX).to_rgb8();
                            let size = [thumb.width() as usize, thumb.height() as usize];
                            ThumbMsg::Ok(i, name, egui::ColorImage::from_rgb(size, &thumb))
                        }
                        Err(err) => ThumbMsg::Failed(format!("{name}: {err}")),
                    };
                    if tx.send(msg).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
        }
    }

    /// Receive decoded thumbnails; bound uploads per frame to keep paint smooth.
    fn pump(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else { return };
        let mut uploads = 0;
        while uploads < UPLOADS_PER_FRAME {
            match rx.try_recv() {
                Ok(ThumbMsg::Ok(i, name, img)) => {
                    let tex = ctx.load_texture(
                        format!("thumb:{name}"),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.slots[i] = Some(Thumb { name, tex });
                    self.loaded += 1;
                    uploads += 1;
                }
                Ok(ThumbMsg::Failed(err)) => {
                    self.failed += 1;
                    self.error = Some(err);
                }
                Err(mpsc::TryRecvError::Empty) | Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        if uploads > 0 {
            ctx.request_repaint(); // there may be more queued than the cap
        }
        if self.total > 0 && self.loaded + self.failed == self.total && self.finished_in.is_none()
        {
            self.finished_in = self.started_at.map(|t| t.elapsed());
            self.rx = None;
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.started {
            self.start(&ctx);
        }
        self.pump(&ctx);

        ui.horizontal(|ui| {
            ui.label(format!("{}/{} thumbnails", self.loaded, self.total));
            if let Some(elapsed) = self.finished_in {
                ui.weak(format!("loaded in {:.2}s", elapsed.as_secs_f32()));
            } else if self.started_at.is_some() && self.total > 0 {
                ui.spinner();
            }
            if self.failed > 0 {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("{} failed", self.failed),
                );
            }
        });
        if let Some(err) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, err);
        }
        ui.separator();

        let cell = egui::vec2(96.0, 72.0);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for slot in &self.slots {
                        match slot {
                            Some(thumb) => {
                                let size = thumb.tex.size_vec2();
                                let scale = (cell.x / size.x).min(cell.y / size.y);
                                let (rect, response) =
                                    ui.allocate_exact_size(cell, egui::Sense::hover());
                                let draw =
                                    egui::Rect::from_center_size(rect.center(), size * scale);
                                egui::Image::new((thumb.tex.id(), draw.size())).paint_at(ui, draw);
                                response.on_hover_text(&thumb.name);
                            }
                            None => {
                                let (rect, _) =
                                    ui.allocate_exact_size(cell, egui::Sense::hover());
                                ui.painter().rect_filled(
                                    rect.shrink(2.0),
                                    3.0,
                                    ui.visuals().faint_bg_color,
                                );
                            }
                        }
                    }
                });
            });
    }
}

// ---------------------------------------------------------------------------
// Verification hooks (NOT production behavior; all gated behind env vars,
// inert when none are set). Counted separately in the LoC split.
// ---------------------------------------------------------------------------

mod verif {
    use super::*;

    pub struct Hooks {
        autostart_camera: bool,
        autostart_mic: bool,
        autostart_beep: bool,
        tab: Option<Tab>,
        log: bool,
        shot_path: Option<PathBuf>,
        shot_after: Duration,
        exit_after: Option<Duration>,
        started: Instant,
        autostart_done: bool,
        shot_focused: bool,
        shot_requested: bool,
        shot_written: bool,
        last_log: Option<Instant>,
    }

    impl Hooks {
        pub fn from_env() -> Self {
            let autostart = std::env::var("PEEK_AUTOSTART").unwrap_or_default();
            Self {
                autostart_camera: autostart.contains("camera"),
                autostart_mic: autostart.contains("mic"),
                autostart_beep: autostart.contains("beep"),
                tab: match std::env::var("PEEK_TAB").as_deref() {
                    Ok("camera") => Some(Tab::Camera),
                    Ok("audio") => Some(Tab::Audio),
                    Ok("gallery") => Some(Tab::Gallery),
                    _ => None,
                },
                log: std::env::var_os("PEEK_LOG").is_some(),
                shot_path: std::env::var_os("PEEK_SHOT").map(PathBuf::from),
                shot_after: std::env::var("PEEK_SHOT_AFTER")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .map(Duration::from_secs_f64)
                    .unwrap_or(Duration::from_secs(6)),
                exit_after: std::env::var("PEEK_EXIT_AFTER")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .map(Duration::from_secs_f64),
                started: Instant::now(),
                autostart_done: false,
                shot_focused: false,
                shot_requested: false,
                shot_written: false,
                last_log: None,
            }
        }

        fn active(&self) -> bool {
            self.autostart_camera
                || self.autostart_mic
                || self.tab.is_some()
                || self.log
                || self.shot_path.is_some()
                || self.exit_after.is_some()
        }
    }

    pub fn tick(app: &mut PeekApp, ctx: &egui::Context) {
        if !app.hooks.active() {
            return;
        }
        let elapsed = app.hooks.started.elapsed();

        if !app.hooks.autostart_done {
            app.hooks.autostart_done = true;
            eprintln!(
                "[peek] t=0 nokhwa_check(camera_authorized)={}",
                nokhwa::nokhwa_check()
            );
            if let Some(tab) = app.hooks.tab {
                app.tab = tab;
            }
            if app.hooks.autostart_camera {
                app.cam.start(ctx);
            }
            if app.hooks.autostart_mic {
                app.mic.start();
            }
            if app.hooks.autostart_beep {
                app.beep.beep();
            }
        }

        let due = app
            .hooks
            .last_log
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
        if app.hooks.log && due {
            app.hooks.last_log = Some(Instant::now());
            let (captured, dropped) = app
                .cam
                .session
                .as_ref()
                .map(|s| {
                    (
                        s.captured.load(Ordering::Relaxed),
                        s.dropped.load(Ordering::Relaxed),
                    )
                })
                .unwrap_or((0, 0));
            eprintln!(
                "[peek] t={:.0}s visible={:?} cam_running={} presented_fps={:.1} \
                 presented_total={} captured={} dropped={} perm={} mic_running={} \
                 mic_rms={:.4} gallery={}/{} beeps={}",
                elapsed.as_secs_f32(),
                ctx.input(|i| i.viewport().visible()),
                app.cam.running(),
                app.cam.fps(),
                app.cam.presented_total,
                captured,
                dropped,
                match &app.cam.permission {
                    CamPermission::Unknown => "unknown",
                    CamPermission::Pending { .. } => "pending",
                    CamPermission::Granted => "granted",
                    CamPermission::Denied => "denied",
                },
                app.mic.running(),
                app.mic.level(),
                app.gallery.loaded,
                app.gallery.total,
                app.beep.beeps,
            );
            if let Some(err) = &app.cam.error {
                eprintln!("[peek] cam_error: {err}");
            }
            if let Some(err) = &app.mic.error {
                eprintln!("[peek] mic_error: {err}");
            }
        }
        if app.hooks.log {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        if let Some(path) = app.hooks.shot_path.clone() {
            // Raise our own window 1 s before the shot: eframe skips painting
            // (and thus screenshot capture) while fully occluded, which is the
            // norm on this shared desktop. Scoped to our own viewport.
            if !app.hooks.shot_focused
                && elapsed + Duration::from_secs(1) >= app.hooks.shot_after
            {
                app.hooks.shot_focused = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            if !app.hooks.shot_requested && elapsed >= app.hooks.shot_after {
                app.hooks.shot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            if !app.hooks.shot_written {
                let image = ctx.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(image) = image {
                    app.hooks.shot_written = true;
                    let [w, h] = image.size;
                    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                    match image::save_buffer(
                        &path,
                        &bytes,
                        w as u32,
                        h as u32,
                        image::ColorType::Rgba8,
                    ) {
                        Ok(()) => eprintln!("[peek] screenshot written to {}", path.display()),
                        Err(err) => eprintln!("[peek] screenshot write failed: {err}"),
                    }
                }
            }
            if !app.hooks.shot_written {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }

        if let Some(after) = app.hooks.exit_after {
            if elapsed >= after {
                eprintln!(
                    "[peek] exiting after {:.0}s (PEEK_EXIT_AFTER)",
                    after.as_secs_f32()
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (verification; drive the UI headlessly through AccessKit)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// Tiny synthetic JPEGs → exercises the full async gallery pipeline
    /// (decode threads → channel → load_texture) without camera/mic hardware.
    fn synthetic_assets(n: u32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("egui-peek-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..n {
            let img = image::RgbImage::from_fn(32, 24, |x, y| {
                image::Rgb([(x * 8) as u8, (y * 10) as u8, (i * 90) as u8])
            });
            img.save(dir.join(format!("t{i}.jpg"))).unwrap();
        }
        dir
    }

    fn new_harness(assets: PathBuf) -> Harness<'static, PeekApp> {
        let mut app = PeekApp::new();
        app.gallery = Gallery::new(assets);
        Harness::new_ui_state(|ui, app: &mut PeekApp| app.show(ui), app)
    }

    #[test]
    fn tabs_switch_and_camera_is_idle_by_default() {
        let mut harness = new_harness(synthetic_assets(0));
        harness.get_by_label("Start camera"); // camera tab is default, idle
        assert!(!harness.state().cam.running());

        harness.get_by_label("Audio").click();
        harness.step(); // step(), not run(): repaint-after counts as "animating"
        harness.get_by_label("Start mic");
        harness.get_by_label("Beep");
        assert!(!harness.state().mic.running());
    }

    #[test]
    fn gallery_decodes_async_and_uploads_textures() {
        let dir = synthetic_assets(3);
        let mut harness = new_harness(dir.clone());
        harness.get_by_label("Gallery").click();
        harness.step();
        assert_eq!(harness.state().gallery.total, 3);

        // Poll: worker threads decode in the background, UI uploads on pump.
        let deadline = Instant::now() + Duration::from_secs(10);
        while harness.state().gallery.loaded < 3 {
            assert!(Instant::now() < deadline, "gallery never finished loading");
            std::thread::sleep(Duration::from_millis(20));
            harness.step();
        }
        assert_eq!(harness.state().gallery.failed, 0);
        assert!(harness.state().gallery.finished_in.is_some());
        assert!(harness.state().gallery.slots.iter().all(|s| s.is_some()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
