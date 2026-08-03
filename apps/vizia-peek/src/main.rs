//! "Peek" — camera / mic / audio / bulk images (SPEC-6), vizia 0.4.
//!
//! Architecture notes (research-relevant):
//! - **Frame → screen**: vizia renders with Skia and re-exports it as
//!   `vizia::vg`, so the camera preview is a custom `View` whose `draw()`
//!   wraps the latest RGBA buffer in a `vg::Image` (`images::raster_from_data`)
//!   and `draw_image_rect`s it. There is no framework texture handle, no
//!   image feature flag, and no encode step — the pixels go from the capture
//!   thread into a mutex slot and from there into Skia once per presented
//!   frame.
//! - **Threads**: `cpal::Stream` and `nokhwa::Camera` are `!Send`/awkward, so
//!   each owns a dedicated `std::thread` and publishes into `Arc<…>` shared
//!   state. vizia's `cx.add_timer` polls that state (8 ms for the camera,
//!   50 ms for the VU meter), which is the same shape the tray app uses for
//!   its OS channels.
//! - **Gallery**: JPEG bytes are read on a small worker pool and handed to
//!   `ContextProxy::load_image`, which decodes through Skia and registers the
//!   image under a key; the grid then renders `Image::new(cx, key)`.
//!
//! Verification hooks (research only, opt-in):
//!   PEEK_SELFTEST=1        auto-start camera + mic, one quiet beep at t≈2 s,
//!                          and one machine-readable status line per second
//!   PEEK_LOG=<path>        where those lines go (default ./selftest.log)
//!   PEEK_TAB=camera|audio|gallery   initial tab, so a harness can shoot any one
//!   PEEK_FAKE_CAMERA=1     synthetic 30 fps RGBA source (isolates the
//!                          frame→Skia path from nokhwa/TCC)
//!   PEEK_ASSETS=<dir>      override the JPEG directory

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::{Camera, nokhwa_check, nokhwa_initialize};
use vizia::prelude::*;
use vizia::vg;

const COLUMNS: usize = 8;
const GALLERY_WORKERS: usize = 8;

// Presented-frame counters live in statics because the custom view's draw()
// takes &self and the timer callback needs to read them.
static PRESENTED: AtomicU64 = AtomicU64::new(0);

fn main() -> Result<(), ApplicationError> {
    let selftest = std::env::var_os("PEEK_SELFTEST").is_some();
    let log_path = std::env::var_os("PEEK_LOG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("selftest.log"));

    Application::new(move |cx| {
        cx.add_stylesheet(STYLE).expect("failed to add stylesheet");

        let tab = Signal::new(match std::env::var("PEEK_TAB").as_deref() {
            Ok("audio") => 1usize,
            Ok("gallery") => 2,
            _ => 0,
        });

        let camera = Arc::new(CamShared::default());
        let mic = Arc::new(MicShared::default());

        let camera_running = Signal::new(false);
        let camera_status = Signal::new(String::from("stopped"));
        let fps_text = Signal::new(String::from("0 fps"));
        let frame_seq = Signal::new(0u64);

        let mic_running = Signal::new(false);
        let mic_status = Signal::new(String::from("stopped"));
        let vu = Signal::new(0.0f32);
        let vu_peak = Signal::new(0.0f32);

        let gallery = Signal::new(Vec::<String>::new());
        let gallery_status = Signal::new(String::from("loading…"));

        // 8 ms: pick up the newest camera frame (~120 Hz poll for a 30 fps
        // source, so presentation is never the bottleneck).
        let frame_timer = cx.add_timer(Duration::from_millis(8), None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(PeekEvent::FrameTick);
            }
        });
        // 50 ms: VU meter at 20 Hz, per SPEC-6.
        let meter_timer = cx.add_timer(Duration::from_millis(50), None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(PeekEvent::MeterTick);
            }
        });
        // 1 s: fps rollup + the self-test status line.
        let second_timer = cx.add_timer(Duration::from_secs(1), None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(PeekEvent::SecondTick);
            }
        });

        Peek {
            camera: Arc::clone(&camera),
            mic: Arc::clone(&mic),
            tab,
            camera_running,
            camera_status,
            fps_text,
            frame_seq,
            mic_running,
            mic_status,
            vu,
            vu_peak,
            gallery,
            gallery_status,
            mic_stop: None,
            last_presented: 0,
            last_captured: 0,
            beep_error: None,
            selftest,
            log_path: log_path.clone(),
            ticks: 0,
        }
        .build(cx);

        cx.start_timer(frame_timer);
        cx.start_timer(meter_timer);
        cx.start_timer(second_timer);

        // Gallery load starts immediately and off the UI thread.
        cx.emit(PeekEvent::LoadGallery);

        if selftest {
            cx.emit(PeekEvent::ToggleCamera);
            cx.emit(PeekEvent::ToggleMic);
            let mut proxy = cx.get_proxy();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let _ = proxy.emit(PeekEvent::Beep);
            });
        }

        VStack::new(cx, move |cx| {
            // ---------------- tabs ----------------
            HStack::new(cx, move |cx| {
                for (index, name) in ["Camera", "Audio", "Gallery"].iter().enumerate() {
                    Button::new(cx, |cx| Label::new(cx, *name))
                        .variant(ButtonVariant::Text)
                        .class("tab")
                        .toggle_class("active", tab.map(move |t| *t == index))
                        .on_press(move |cx| cx.emit(PeekEvent::SetTab(index)));
                }
            })
            .class("tabs");

            let camera = Arc::clone(&camera);
            Binding::new(cx, tab, move |cx| match tab.get() {
                0 => {
                    let camera = Arc::clone(&camera);
                    VStack::new(cx, move |cx| {
                        HStack::new(cx, move |cx| {
                            Button::new(cx, |cx| {
                                Label::new(
                                    cx,
                                    camera_running.map(|running| {
                                        if *running { "Stop" } else { "Start" }.to_string()
                                    }),
                                )
                            })
                            .variant(ButtonVariant::Primary)
                            .on_press(|cx| cx.emit(PeekEvent::ToggleCamera));
                            Label::new(cx, fps_text).class("fps");
                            Label::new(cx, camera_status).class("dim").width(Stretch(1.0));
                        })
                        .class("row");

                        CameraView::new(cx, camera.clone(), frame_seq).class("preview");
                    })
                    .class("pane");
                }
                1 => {
                    VStack::new(cx, move |cx| {
                        HStack::new(cx, move |cx| {
                            Button::new(cx, |cx| {
                                Label::new(
                                    cx,
                                    mic_running.map(|running| {
                                        if *running { "Stop" } else { "Start" }.to_string()
                                    }),
                                )
                            })
                            .variant(ButtonVariant::Primary)
                            .on_press(|cx| cx.emit(PeekEvent::ToggleMic));
                            Button::new(cx, |cx| Label::new(cx, "Beep"))
                                .variant(ButtonVariant::Outline)
                                .on_press(|cx| cx.emit(PeekEvent::Beep));
                            Label::new(cx, mic_status).class("dim").width(Stretch(1.0));
                        })
                        .class("row");

                        Label::new(cx, "input level (RMS, -60..0 dBFS)").class("dim");
                        ProgressBar::horizontal(cx, vu).class("vu").width(Stretch(1.0));
                        Label::new(cx, "peak hold").class("dim");
                        ProgressBar::horizontal(cx, vu_peak).class("vu peak").width(Stretch(1.0));
                    })
                    .class("pane");
                }
                _ => {
                    VStack::new(cx, move |cx| {
                        Label::new(cx, gallery_status).class("dim");
                        ScrollView::new(cx, move |cx| {
                            VStack::new(cx, move |cx| {
                                Binding::new(cx, gallery, move |cx| {
                                    let keys = gallery.get();
                                    for chunk in keys.chunks(COLUMNS) {
                                        HStack::new(cx, |cx| {
                                            for key in chunk {
                                                Image::new(cx, key.clone())
                                                    .class("thumb")
                                                    .hoverable(false);
                                            }
                                        })
                                        .class("thumb-row");
                                    }
                                });
                            })
                            .class("thumb-grid");
                        })
                        .class("gallery");
                    })
                    .class("pane");
                }
            });
        })
        .class("app");
    })
    .title("Peek (vizia)")
    .inner_size((900, 600))
    .run()
}

// ---------------------------------------------------------------------------
// Camera preview view — raw RGBA straight into Skia
// ---------------------------------------------------------------------------

struct CameraView {
    shared: Arc<CamShared>,
}

impl CameraView {
    fn new(cx: &mut Context, shared: Arc<CamShared>, seq: Signal<u64>) -> Handle<'_, Self> {
        Self { shared }
            .build(cx, |_| {})
            // Redraw exactly when a new frame has been published.
            .bind(seq, |mut handle| handle.needs_redraw())
    }
}

impl View for CameraView {
    fn element(&self) -> Option<&'static str> {
        Some("camera-view")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let guard = self.shared.frame.lock().unwrap();
        let Some(frame) = guard.as_ref() else {
            return;
        };

        // The whole frame→screen path: wrap the RGBA bytes in a Skia raster
        // image and blit it, letterboxed into the view's bounds.
        let info = vg::ImageInfo::new(
            (frame.width as i32, frame.height as i32),
            vg::ColorType::RGBA8888,
            vg::AlphaType::Unpremul,
            None,
        );
        let data = unsafe { vg::Data::new_bytes(&frame.pixels) };
        let Some(image) =
            vg::images::raster_from_data(&info, data, frame.width as usize * 4)
        else {
            return;
        };

        let scale =
            (bounds.w / frame.width as f32).min(bounds.h / frame.height as f32);
        let (width, height) = (frame.width as f32 * scale, frame.height as f32 * scale);
        let dest = vg::Rect::from_xywh(
            bounds.x + (bounds.w - width) / 2.0,
            bounds.y + (bounds.h - height) / 2.0,
            width,
            height,
        );

        let mut paint = vg::Paint::default();
        paint.set_anti_alias(true);
        canvas.draw_image_rect(
            &image,
            None,
            dest,
            &paint,
        );

        PRESENTED.fetch_add(1, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

struct CamShared {
    frame: Mutex<Option<Frame>>,
    format: Mutex<Option<String>>,
    error: Mutex<Option<String>>,
    captured: AtomicU64,
    stop: AtomicBool,
    /// -1 unknown, 0 denied, 1 granted (from nokhwa's AVAuthorizationStatus).
    permission: AtomicI8,
}

impl Default for CamShared {
    fn default() -> Self {
        Self {
            frame: Mutex::new(None),
            format: Mutex::new(None),
            error: Mutex::new(None),
            captured: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            permission: AtomicI8::new(-1),
        }
    }
}

#[derive(Default)]
struct MicShared {
    rms: AtomicU32,
    callbacks: AtomicU64,
    desc: Mutex<Option<String>>,
    error: Mutex<Option<String>>,
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct Peek {
    camera: Arc<CamShared>,
    mic: Arc<MicShared>,
    tab: Signal<usize>,
    camera_running: Signal<bool>,
    camera_status: Signal<String>,
    fps_text: Signal<String>,
    frame_seq: Signal<u64>,
    mic_running: Signal<bool>,
    mic_status: Signal<String>,
    vu: Signal<f32>,
    vu_peak: Signal<f32>,
    gallery: Signal<Vec<String>>,
    gallery_status: Signal<String>,
    mic_stop: Option<std::sync::mpsc::Sender<()>>,
    last_presented: u64,
    last_captured: u64,
    beep_error: Option<String>,
    selftest: bool,
    log_path: PathBuf,
    ticks: u64,
}

enum PeekEvent {
    SetTab(usize),
    ToggleCamera,
    ToggleMic,
    Beep,
    BeepDone(Result<(), String>),
    FrameTick,
    MeterTick,
    SecondTick,
    LoadGallery,
    GalleryBatch { done: usize, total: usize, ms: f64 },
}

impl Model for Peek {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.take(|peek_event, _| match peek_event {
            PeekEvent::SetTab(index) => self.tab.set(index),

            PeekEvent::ToggleCamera => {
                if self.camera_running.get() {
                    self.camera.stop.store(true, Ordering::Relaxed);
                    self.camera_running.set(false);
                    self.camera_status.set(String::from("stopped"));
                } else {
                    self.camera.stop.store(false, Ordering::Relaxed);
                    *self.camera.error.lock().unwrap() = None;
                    self.camera_running.set(true);
                    self.camera_status.set(String::from("starting…"));

                    let shared = Arc::clone(&self.camera);
                    if std::env::var_os("PEEK_FAKE_CAMERA").is_some() {
                        std::thread::spawn(move || fake_camera_thread(shared, 1280, 720));
                    } else {
                        // TCC: ask AVFoundation up front so the prompt (if
                        // any) appears before the capture thread blocks.
                        let permission = Arc::clone(&self.camera);
                        nokhwa_initialize(move |granted| {
                            permission.permission.store(granted as i8, Ordering::Relaxed);
                        });
                        self.camera
                            .permission
                            .store(if nokhwa_check() { 1 } else { -1 }, Ordering::Relaxed);
                        std::thread::spawn(move || camera_thread(shared));
                    }
                }
            }

            PeekEvent::ToggleMic => {
                if self.mic_running.get() {
                    if let Some(tx) = self.mic_stop.take() {
                        let _ = tx.send(());
                    }
                    self.mic_running.set(false);
                    self.mic_status.set(String::from("stopped"));
                    self.vu.set(0.0);
                    self.vu_peak.set(0.0);
                } else {
                    self.mic = Arc::new(MicShared::default());
                    let (tx, rx) = std::sync::mpsc::channel();
                    let shared = Arc::clone(&self.mic);
                    std::thread::spawn(move || mic_thread(shared, rx));
                    self.mic_stop = Some(tx);
                    self.mic_running.set(true);
                    self.mic_status.set(String::from("starting…"));
                }
            }

            PeekEvent::Beep => {
                let mut proxy = cx.get_proxy();
                std::thread::spawn(move || {
                    let outcome = play_beep();
                    let _ = proxy.emit(PeekEvent::BeepDone(outcome));
                });
            }

            PeekEvent::BeepDone(outcome) => {
                self.beep_error = outcome.err();
                if let Some(error) = &self.beep_error {
                    self.mic_status.set(format!("beep failed: {error}"));
                }
            }

            PeekEvent::FrameTick => {
                // Publishing a new sequence number is what triggers the
                // custom view's redraw; the pixels never pass through an
                // event.
                let captured = self.camera.captured.load(Ordering::Relaxed);
                if captured != self.last_captured {
                    self.last_captured = captured;
                    self.frame_seq.set(captured);
                }
            }

            PeekEvent::MeterTick => {
                if self.mic_running.get() {
                    let rms = f32::from_bits(self.mic.rms.load(Ordering::Relaxed));
                    let db = 20.0 * rms.max(1e-6).log10();
                    let level = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
                    // Fast attack, slow decay (classic VU behaviour).
                    let current = self.vu.get();
                    self.vu.set(if level > current {
                        level
                    } else {
                        current + (level - current) * 0.25
                    });
                    let peak = self.vu_peak.get();
                    self.vu_peak.set(if level > peak { level } else { peak * 0.985 });
                }
            }

            PeekEvent::SecondTick => {
                self.ticks += 1;

                let presented = PRESENTED.load(Ordering::Relaxed);
                let captured = self.camera.captured.load(Ordering::Relaxed);
                let presented_fps = presented - self.last_presented;
                self.last_presented = presented;
                if self.camera_running.get() {
                    self.fps_text.set(format!("{presented_fps} fps presented"));
                    let format = self.camera.format.lock().unwrap().clone();
                    let error = self.camera.error.lock().unwrap().clone();
                    self.camera_status.set(match (error, format) {
                        (Some(error), _) => format!("camera error: {error}"),
                        (None, Some(format)) => format!(
                            "{format} · permission={} · {captured} frames captured",
                            match self.camera.permission.load(Ordering::Relaxed) {
                                1 => "granted",
                                0 => "denied",
                                _ => "unknown",
                            }
                        ),
                        (None, None) => String::from("starting…"),
                    });
                }

                if self.mic_running.get() {
                    let error = self.mic.error.lock().unwrap().clone();
                    let desc = self.mic.desc.lock().unwrap().clone();
                    let callbacks = self.mic.callbacks.load(Ordering::Relaxed);
                    self.mic_status.set(match (error, desc) {
                        (Some(error), _) => format!("mic error: {error}"),
                        (None, Some(desc)) => format!("{desc} · {callbacks} callbacks"),
                        (None, None) => String::from("starting…"),
                    });
                }

                if self.selftest {
                    let line = format!(
                        "t={} camera={} presented_fps={} captured={} perm={} mic={} rms={:.5} callbacks={} beep_err={:?} gallery={} status={:?}",
                        self.ticks,
                        self.camera_running.get(),
                        presented_fps,
                        captured,
                        self.camera.permission.load(Ordering::Relaxed),
                        self.mic_running.get(),
                        f32::from_bits(self.mic.rms.load(Ordering::Relaxed)),
                        self.mic.callbacks.load(Ordering::Relaxed),
                        self.beep_error,
                        self.gallery.get().len(),
                        self.gallery_status.get(),
                    );
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&self.log_path)
                    {
                        let _ = writeln!(file, "{line}");
                    }
                }
            }

            PeekEvent::LoadGallery => {
                let dir = asset_dir();
                let paths = list_jpegs(&dir);
                let total = paths.len();
                self.gallery_status.set(format!("0 / {total} decoded"));

                let started = std::time::Instant::now();
                let queue = Arc::new(Mutex::new(paths.into_iter().enumerate().collect::<
                    Vec<(usize, PathBuf)>,
                >()));
                let done = Arc::new(AtomicU64::new(0));

                for _ in 0..GALLERY_WORKERS {
                    let mut proxy = cx.get_proxy();
                    let queue = Arc::clone(&queue);
                    let done = Arc::clone(&done);
                    std::thread::spawn(move || {
                        loop {
                            let Some((index, path)) = queue.lock().unwrap().pop() else {
                                break;
                            };
                            let Ok(bytes) = std::fs::read(&path) else { continue };
                            let key = format!("thumb-{index:03}");
                            // Skia decodes; `load_image` queues the decoded
                            // image onto the UI thread's resource manager.
                            let _ = proxy.load_image(
                                key.clone(),
                                &bytes,
                                ImageRetentionPolicy::Forever,
                            );
                            let count = done.fetch_add(1, Ordering::Relaxed) + 1;
                            // Publish in batches so the grid streams in
                            // without rebuilding once per image.
                            if count % 25 == 0 || count as usize == total {
                                let _ = proxy.emit(PeekEvent::GalleryBatch {
                                    done: count as usize,
                                    total,
                                    ms: started.elapsed().as_secs_f64() * 1000.0,
                                });
                            }
                        }
                    });
                }
            }

            PeekEvent::GalleryBatch { done, total, ms } => {
                // Keys are deterministic, so the grid can be rebuilt from the
                // count alone.
                self.gallery
                    .set((0..done).map(|index| format!("thumb-{index:03}")).collect());
                self.gallery_status.set(if done == total {
                    format!("{total} JPEGs decoded in {ms:.0} ms ({GALLERY_WORKERS} workers)")
                } else {
                    format!("{done} / {total} decoded")
                });
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Capture threads
// ---------------------------------------------------------------------------

fn camera_thread(shared: Arc<CamShared>) {
    let set_error = |message: String| {
        *shared.error.lock().unwrap() = Some(message);
    };

    let requested =
        RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = match Camera::new(CameraIndex::Index(0), requested) {
        Ok(camera) => camera,
        Err(error) => return set_error(format!("open failed: {error}")),
    };

    let format = camera.camera_format();
    *shared.format.lock().unwrap() = Some(format!(
        "{}x{} @ {} fps {}",
        format.width(),
        format.height(),
        format.frame_rate(),
        format.format()
    ));

    if let Err(error) = camera.open_stream() {
        return set_error(format!("open_stream failed: {error}"));
    }

    while !shared.stop.load(Ordering::Relaxed) {
        let buffer = match camera.frame() {
            Ok(buffer) => buffer,
            Err(error) => {
                set_error(format!("frame failed: {error}"));
                break;
            }
        };
        match buffer.decode_image::<RgbAFormat>() {
            Ok(decoded) => {
                let (width, height) = decoded.dimensions();
                *shared.frame.lock().unwrap() =
                    Some(Frame { width, height, pixels: decoded.into_raw() });
                shared.captured.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                set_error(format!("decode failed: {error}"));
                break;
            }
        }
    }

    let _ = camera.stop_stream();
}

/// Verification hook: synthetic 30 fps RGBA source, so the frame→Skia path can
/// be measured without nokhwa or a TCC prompt in the way.
fn fake_camera_thread(shared: Arc<CamShared>, width: u32, height: u32) {
    *shared.format.lock().unwrap() = Some(format!("{width}x{height} @ 30 fps SYNTHETIC"));
    let mut phase = 0u32;
    while !shared.stop.load(Ordering::Relaxed) {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let index = ((y * width + x) * 4) as usize;
                pixels[index] = ((x + phase) % 256) as u8;
                pixels[index + 1] = ((y + phase) % 256) as u8;
                pixels[index + 2] = 128;
                pixels[index + 3] = 255;
            }
        }
        *shared.frame.lock().unwrap() = Some(Frame { width, height, pixels });
        shared.captured.fetch_add(1, Ordering::Relaxed);
        phase = phase.wrapping_add(4);
        std::thread::sleep(std::time::Duration::from_millis(33));
    }
}

fn mic_thread(shared: Arc<MicShared>, stop: std::sync::mpsc::Receiver<()>) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let set_error = |message: String| {
        *shared.error.lock().unwrap() = Some(message);
    };

    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        return set_error(String::from("no default input device"));
    };
    let config = match device.default_input_config() {
        Ok(config) => config,
        Err(error) => return set_error(format!("no input config: {error}")),
    };
    if config.sample_format() != cpal::SampleFormat::F32 {
        return set_error(format!("unsupported sample format {:?}", config.sample_format()));
    }

    *shared.desc.lock().unwrap() = Some(format!(
        "{} ({} ch @ {} Hz)",
        device
            .description()
            .map(|description| description.name().to_string())
            .unwrap_or_else(|_| String::from("unknown input")),
        config.channels(),
        config.sample_rate()
    ));

    let data_shared = Arc::clone(&shared);
    let error_shared = Arc::clone(&shared);
    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let sum: f32 = data.iter().map(|sample| sample * sample).sum();
            let rms = (sum / data.len().max(1) as f32).sqrt();
            data_shared.rms.store(rms.to_bits(), Ordering::Relaxed);
            data_shared.callbacks.fetch_add(1, Ordering::Relaxed);
        },
        move |error| {
            *error_shared.error.lock().unwrap() = Some(format!("stream error: {error}"));
        },
        None,
    );
    let stream = match stream {
        Ok(stream) => stream,
        Err(error) => return set_error(format!("build_input_stream failed: {error}")),
    };
    if let Err(error) = stream.play() {
        return set_error(format!("play failed: {error}"));
    }

    // `cpal::Stream` is !Send, so it lives and dies on this thread.
    let _ = stop.recv();
    drop(stream);
}

fn play_beep() -> Result<(), String> {
    use rodio::Source;
    use rodio::source::SineWave;

    let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|error| format!("open_default_sink failed: {error}"))?;
    sink.log_on_drop(false);
    sink.mixer().add(
        SineWave::new(880.0)
            .take_duration(std::time::Duration::from_millis(180))
            .amplify(0.10),
    );
    // Keep the OS sink alive until the tone has played out.
    std::thread::sleep(std::time::Duration::from_millis(280));
    Ok(())
}

// ---------------------------------------------------------------------------
// Gallery assets
// ---------------------------------------------------------------------------

fn asset_dir() -> PathBuf {
    std::env::var_os("PEEK_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../peek-assets"))
}

fn list_jpegs(dir: &PathBuf) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| {
                    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("jpg"))
                })
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.app { width: 1s; height: 1s; padding: 10px; vertical-gap: 8px; }
.tabs { height: auto; horizontal-gap: 6px; }
.tab { height: 28px; font-size: 13px; }
.tab.active { background-color: #4f9df7; color: #ffffff; }
.pane { width: 1s; height: 1s; vertical-gap: 8px; }
.row { height: auto; horizontal-gap: 10px; alignment: center; }
.dim { height: auto; font-size: 12px; color: #8a8a8a; }
.fps { height: auto; font-size: 13px; }

.preview {
    width: 1s;
    height: 1s;
    background-color: #000000;
    corner-radius: 6px;
}

.vu { height: 18px; }
.vu.peak { height: 8px; }

.gallery { width: 1s; height: 1s; }
.thumb-grid { width: auto; height: auto; vertical-gap: 4px; padding: 4px; }
.thumb-row { width: auto; height: auto; horizontal-gap: 4px; }
.thumb { width: 96px; height: 72px; corner-radius: 3px; background-size: contain; }
"#;
