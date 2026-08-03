//! "Peek" — media & hardware test (SPEC-6), Freya 0.4.
//!
//! Architecture notes (research-relevant):
//! - Camera is **first-party**: the `camera` feature re-exports `freya-camera`,
//!   which runs nokhwa on its own thread, converts each frame to RGBA, builds a
//!   Skia `ImageHandle` and pushes it into a `State<Option<ImageHandle>>`.
//!   `CameraViewer::new(camera)` renders it. The app writes no capture thread,
//!   no frame slot and no texture upload — only the fps counter and Start/Stop
//!   (which is "mount / unmount the component that owns `use_camera`", because
//!   the capture is tied to the owning scope).
//! - Mic and audio-out are not covered by Freya: `cpal` input stream on its own
//!   thread (a `cpal::Stream` is `!Send`, so it cannot live in component state)
//!   writing RMS into an `AtomicU32`, sampled at 20 Hz; `rodio` for the beep.
//! - Gallery uses `ImageViewer` + `ImageSource::Path`, which does async load,
//!   decode-to-layout-size and caching, inside a `VirtualScrollView` so only
//!   the visible rows are mounted at all.
//!
//! Verification hooks (all gated behind env vars):
//!   PEEK_SELFTEST=1   auto-start camera + mic, one quiet beep at t≈2 s, and
//!                     one status line per second appended to $PEEK_LOG
//!                     (default `selftest.log`).
//!   PEEK_ASSETS=<dir> override the gallery directory.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        Mutex,
        atomic::{
            AtomicBool,
            AtomicU32,
            AtomicU64,
            Ordering,
        },
    },
    time::{
        Duration,
        Instant,
    },
};

use async_io::Timer;
use cpal::traits::{
    DeviceTrait,
    HostTrait,
    StreamTrait,
};
use freya::{
    camera::*,
    prelude::*,
};

const THUMBS_PER_ROW: usize = 6;
const THUMB: f32 = 128.0;
const ROW_H: f32 = THUMB + 10.0;

const BG: Color = Color::from_argb(255, 250, 250, 251);
const PANEL: Color = Color::WHITE;
const TEXT: Color = Color::from_argb(255, 26, 28, 33);
const MUTED: Color = Color::from_argb(255, 108, 115, 128);
const ACCENT: Color = Color::from_argb(255, 46, 112, 226);
const LINE: Color = Color::from_argb(255, 224, 227, 232);

fn assets_dir() -> PathBuf {
    std::env::var_os("PEEK_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../peek-assets"))
}

fn gallery_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(assets_dir())
        .map(|dir| {
            dir.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

// ---------------------------------------------------------------- mic

/// Shared state between the cpal callback thread and the UI.
struct MicShared {
    rms: AtomicU32,
    callbacks: AtomicU64,
    running: AtomicBool,
    error: Mutex<Option<String>>,
}

impl MicShared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            rms: AtomicU32::new(0),
            callbacks: AtomicU64::new(0),
            running: AtomicBool::new(false),
            error: Mutex::new(None),
        })
    }
}

/// `cpal::Stream` is `!Send`, so it cannot be stored in component state; it
/// lives on its own thread and is dropped when `running` goes false.
fn start_mic(shared: Arc<MicShared>) {
    shared.running.store(true, Ordering::Relaxed);
    std::thread::spawn(move || {
        let host = cpal::default_host();
        let Some(device) = host.default_input_device() else {
            *shared.error.lock().unwrap() = Some("no default input device".into());
            shared.running.store(false, Ordering::Relaxed);
            return;
        };
        let config = match device.default_input_config() {
            Ok(config) => config,
            Err(error) => {
                *shared.error.lock().unwrap() = Some(error.to_string());
                shared.running.store(false, Ordering::Relaxed);
                return;
            }
        };

        let cb_shared = shared.clone();
        let err_shared = shared.clone();
        let stream = device.build_input_stream(
            &config.config(),
            move |data: &[f32], _| {
                let sum: f32 = data.iter().map(|s| s * s).sum();
                let rms = (sum / data.len().max(1) as f32).sqrt();
                cb_shared.rms.store(rms.to_bits(), Ordering::Relaxed);
                cb_shared.callbacks.fetch_add(1, Ordering::Relaxed);
            },
            move |error| {
                *err_shared.error.lock().unwrap() = Some(error.to_string());
            },
            None,
        );

        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                *shared.error.lock().unwrap() = Some(error.to_string());
                shared.running.store(false, Ordering::Relaxed);
                return;
            }
        };
        if let Err(error) = stream.play() {
            *shared.error.lock().unwrap() = Some(error.to_string());
            shared.running.store(false, Ordering::Relaxed);
            return;
        }

        while shared.running.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
        drop(stream);
        shared.rms.store(0f32.to_bits(), Ordering::Relaxed);
    });
}

/// One short sine on the default output device. Dropping the sink stops
/// playback, so the thread has to outlive the tone.
fn beep(result: Arc<Mutex<Option<String>>>) {
    std::thread::spawn(move || {
        use rodio::{
            Source,
            source::SineWave,
        };
        match rodio::stream::DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => {
                sink.mixer().add(
                    SineWave::new(880.0)
                        .take_duration(Duration::from_millis(180))
                        .amplify(0.10),
                );
                std::thread::sleep(Duration::from_millis(280));
                *result.lock().unwrap() = None;
            }
            Err(error) => *result.lock().unwrap() = Some(error.to_string()),
        }
    });
}

// ---------------------------------------------------------------- main

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Camera,
    Audio,
    Gallery,
}

fn main() {
    // macOS gates camera access; `freya::camera::init()` blocks on the
    // AVFoundation authorization prompt and reports the answer.
    let granted = init();
    eprintln!("camera-permission: granted={granted}");

    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app)
                .with_title("Peek (freya)")
                .with_size(900.0, 600.0)
                .with_background(BG),
        ),
    )
}

fn app() -> impl IntoElement {
    let selftest = use_hook(|| std::env::var_os("PEEK_SELFTEST").is_some());
    let mut tab = use_state(|| if selftest { Tab::Camera } else { Tab::Camera });
    let mut camera_on = use_state(|| selftest);
    let mut mic_on = use_state(|| false);
    let mut level = use_state(|| 0.0f32);
    let mut beeps = use_state(|| 0u32);
    let mut presented_fps = use_state(|| 0u32);
    let frames = use_hook(|| Arc::new(AtomicU64::new(0)));
    let mic = use_hook(MicShared::new);
    let beep_error = use_hook(|| Arc::new(Mutex::new(None::<String>)));
    let files = use_hook(gallery_files);
    let started = use_hook(Instant::now);

    // 20 Hz VU sampler + 1 Hz fps counter, on Freya's own executor.
    use_hook({
        let mic = mic.clone();
        let frames = frames.clone();
        move || {
            spawn(async move {
                let mut last_frames = 0u64;
                let mut ticks = 0u32;
                loop {
                    Timer::after(Duration::from_millis(50)).await;
                    let rms = f32::from_bits(mic.rms.load(Ordering::Relaxed));
                    level.set(rms);
                    ticks += 1;
                    if ticks % 20 == 0 {
                        let now = frames.load(Ordering::Relaxed);
                        presented_fps.set((now - last_frames) as u32);
                        last_frames = now;
                    }
                }
            });
        }
    });

    // Verification hooks.
    if selftest {
        let mic_shared = mic.clone();
        let beep_result = beep_error.clone();
        let beep_for_log = beep_error.clone();
        let frames_for_log = frames.clone();
        let file_count = files.len();
        use_hook(move || {
            start_mic(mic_shared.clone());
            mic_on.set(true);
            spawn(async move {
                Timer::after(Duration::from_secs(2)).await;
                beep(beep_result.clone());
                beeps.set(1);
            });
            spawn(async move {
                let path = std::env::var_os("PEEK_LOG")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("selftest.log"));
                let mut last = 0u64;
                for t in 1.. {
                    Timer::after(Duration::from_secs(1)).await;
                    let now = frames_for_log.load(Ordering::Relaxed);
                    let line = format!(
                        "t={t} cam={} pres_fps={} frames={} mic_running={} mic_cbs={} \
                         rms={:.5} mic_err=\"{}\" beeps={} beep_err=\"{}\" thumbs={}\n",
                        if *camera_on.peek() { "running" } else { "stopped" },
                        now - last,
                        now,
                        mic_shared.running.load(Ordering::Relaxed),
                        mic_shared.callbacks.load(Ordering::Relaxed),
                        f32::from_bits(mic_shared.rms.load(Ordering::Relaxed)),
                        mic_shared.error.lock().unwrap().clone().unwrap_or_default(),
                        *beeps.peek(),
                        beep_for_log.lock().unwrap().clone().unwrap_or_default(),
                        file_count,
                    );
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                    {
                        use std::io::Write;
                        let _ = file.write_all(line.as_bytes());
                    }
                    last = now;
                }
            });
        });
    }

    let current = *tab.read();
    let cam_running = *camera_on.read();
    let mic_running = *mic_on.read();
    let rms = *level.read();
    let fps = *presented_fps.read();
    let mic_error = mic.error.lock().unwrap().clone();
    let mic_shared = mic.clone();
    let beep_result = beep_error.clone();
    let frames_for_pane = frames.clone();
    let uptime = started.elapsed().as_secs();

    rect()
        .expanded()
        .content(Content::flex())
        .background(BG)
        .color(TEXT)
        .padding(Gaps::new_all(10.))
        .spacing(8.)
        // --------------------------------------------------------- tabs
        .child(
            rect()
                .horizontal()
                .spacing(6.)
                .cross_align(Alignment::Center)
                .children([Tab::Camera, Tab::Audio, Tab::Gallery].map(|value| {
                    let label_text = match value {
                        Tab::Camera => "Camera",
                        Tab::Audio => "Audio",
                        Tab::Gallery => "Gallery",
                    };
                    Button::new()
                        .compact()
                        .maybe(current == value, |b| b.filled())
                        .on_press(move |_| tab.set(value))
                        .child(label_text)
                        .into()
                }))
                .child(
                    label()
                        .text(format!("uptime {uptime}s"))
                        .font_size(11.)
                        .color(MUTED),
                ),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.))
                .content(Content::flex())
                .background(PANEL)
                .rounded_md()
                .border(Border::new().fill(LINE).width(1.))
                .padding(Gaps::new_all(10.))
                .spacing(8.)
                .child(match current {
                    // ------------------------------------------- camera
                    Tab::Camera => rect()
                        .width(Size::fill())
                        .height(Size::fill())
                        .content(Content::flex())
                        .spacing(8.)
                        .child(
                            rect()
                                .horizontal()
                                .spacing(8.)
                                .cross_align(Alignment::Center)
                                .child(
                                    Button::new()
                                        .compact()
                                        .on_press(move |_| camera_on.toggle())
                                        .child(if cam_running { "Stop" } else { "Start" }),
                                )
                                .child(
                                    label()
                                        .text(format!("presented {fps} fps"))
                                        .font_size(12.)
                                        .color(if cam_running { ACCENT } else { MUTED }),
                                )
                                .child(
                                    label()
                                        .text(
                                            "frames arrive as Skia ImageHandles from freya-camera",
                                        )
                                        .font_size(11.)
                                        .color(MUTED),
                                ),
                        )
                        .maybe_child(cam_running.then(|| CameraPane {
                            frames: frames_for_pane,
                        }))
                        .maybe_child((!cam_running).then(|| {
                            rect()
                                .width(Size::fill())
                                .height(Size::flex(1.))
                                .center()
                                .child(label().text("camera stopped").color(MUTED))
                        }))
                        .into(),
                    // -------------------------------------------- audio
                    Tab::Audio => rect()
                        .width(Size::fill())
                        .height(Size::fill())
                        .spacing(10.)
                        .child(
                            rect()
                                .horizontal()
                                .spacing(8.)
                                .cross_align(Alignment::Center)
                                .child(
                                    Button::new()
                                        .compact()
                                        .on_press(move |_| {
                                            if *mic_on.peek() {
                                                mic_shared.running.store(false, Ordering::Relaxed);
                                                mic_on.set(false);
                                            } else {
                                                start_mic(mic_shared.clone());
                                                mic_on.set(true);
                                            }
                                        })
                                        .child(if mic_running { "Stop mic" } else { "Start mic" }),
                                )
                                .child(
                                    Button::new()
                                        .compact()
                                        .on_press(move |_| {
                                            beep(beep_result.clone());
                                            let count = *beeps.peek() + 1;
                                            beeps.set(count);
                                        })
                                        .child("Beep"),
                                )
                                .child(
                                    label()
                                        .text(format!("rms {rms:.5} · {:.0} dBFS", dbfs(rms)))
                                        .font_size(12.)
                                        .color(MUTED),
                                ),
                        )
                        .child(ProgressBar::new(vu_percent(rms)))
                        .maybe_child(mic_error.map(|error| {
                            label()
                                .text(format!("mic error: {error}"))
                                .font_size(12.)
                                .color(Color::from_argb(255, 176, 42, 42))
                        }))
                        .child(
                            label()
                                .text(format!(
                                    "callbacks {}",
                                    mic.callbacks.load(Ordering::Relaxed)
                                ))
                                .font_size(11.)
                                .color(MUTED),
                        )
                        .into(),
                    // ------------------------------------------ gallery
                    Tab::Gallery => gallery(files),
                }),
        )
}

fn dbfs(rms: f32) -> f32 {
    if rms <= 0.000_01 {
        -60.0
    } else {
        (20.0 * rms.log10()).clamp(-60.0, 0.0)
    }
}

fn vu_percent(rms: f32) -> f32 {
    ((dbfs(rms) + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0)
}

/// Owns the camera. Mounting starts the capture; unmounting (Stop) drops the
/// scope, which closes it — that is Freya's whole start/stop story.
#[derive(Clone)]
struct CameraPane {
    frames: Arc<AtomicU64>,
}

impl PartialEq for CameraPane {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.frames, &other.frames)
    }
}

impl Component for CameraPane {
    fn render(&self) -> impl IntoElement {
        let camera = use_camera(CameraConfig::default);
        let frames = self.frames.clone();

        // Count frames that actually reached the reactive graph (i.e. were
        // presented), not frames captured.
        use_side_effect(move || {
            if camera.frame.read().is_some() {
                frames.fetch_add(1, Ordering::Relaxed);
            }
        });

        let info = camera.info.read().clone();
        let error = camera.error.read().clone();

        rect()
            .width(Size::fill())
            .height(Size::flex(1.))
            .content(Content::flex())
            .spacing(4.)
            .child(
                label()
                    .text(match (&info, &error) {
                        (_, Some(error)) => format!("camera error: {error}"),
                        (Some(info), _) => format!("{}x{} @ {} fps", info.width, info.height, info.frame_rate),
                        _ => String::from("opening camera…"),
                    })
                    .font_size(11.)
                    .color(MUTED),
            )
            .child(
                CameraViewer::new(camera)
                    .width(Size::fill())
                    .height(Size::flex(1.))
                    .aspect_ratio(AspectRatio::Fit)
                    .loading_placeholder(
                        rect()
                            .expanded()
                            .center()
                            .child(label().text("waiting for the first frame…").color(MUTED)),
                    ),
            )
    }
}

/// 200 JPEG thumbnails. `ImageViewer` does the async load + decode + cache;
/// `VirtualScrollView` means only the visible rows are mounted at all.
fn gallery(files: Vec<PathBuf>) -> Element {
    let rows = files.len().div_ceil(THUMBS_PER_ROW);
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .content(Content::flex())
        .spacing(6.)
        .child(
            label()
                .text(format!(
                    "{} JPEGs from apps/peek-assets — decoded lazily by ImageViewer",
                    files.len()
                ))
                .font_size(11.)
                .color(MUTED),
        )
        .child(
            VirtualScrollView::new_with_data(files, move |row: usize, files: &Vec<PathBuf>| {
                let start = row * THUMBS_PER_ROW;
                let end = (start + THUMBS_PER_ROW).min(files.len());
                rect()
                    .key(row)
                    .horizontal()
                    .height(Size::px(ROW_H))
                    .spacing(6.)
                    .children(files[start..end].iter().enumerate().map(|(i, path)| {
                        ImageViewer::new(ImageSource::Path(path.clone()))
                            .key(start + i)
                            .width(Size::px(THUMB))
                            .height(Size::px(THUMB))
                            .aspect_ratio(AspectRatio::Fit)
                            .corner_radius(4.)
                            .loading_placeholder(
                                rect()
                                    .width(Size::px(THUMB))
                                    .height(Size::px(THUMB))
                                    .background(Color::from_argb(255, 238, 240, 243))
                                    .rounded_sm(),
                            )
                            .into()
                    }))
                    .into()
            })
            .length(rows)
            .item_size(ROW_H + 6.0)
            .width(Size::fill())
            .height(Size::flex(1.)),
        )
        .into()
}
