//! "Peek" — media & hardware test (SPEC-6), iced =0.14.0.
//!
//! Architecture notes (research-relevant):
//! - Camera: `nokhwa` (AVFoundation) runs on a *dedicated std::thread* that
//!   blocks in `Camera::frame()` (the backend parks on a flume channel fed by
//!   the AVCaptureSession callback). Each frame is decoded to RGBA on that
//!   thread and swapped into a latest-wins `Mutex<Option<Frame>>` slot. The
//!   UI polls the slot from an `iced::time::every(8 ms)` subscription and
//!   wraps fresh pixels in `image::Handle::from_rgba` — a **new handle every
//!   frame**, because iced handles are immutable and identity-keyed, so every
//!   camera frame is a full RGBA texture re-upload in iced_wgpu's image
//!   cache (old entries are trimmed once unused). "Presented FPS" counts
//!   handles actually swapped into the widget tree (each swap triggers a
//!   redraw), not frames captured; both counters are shown.
//! - Permission: `nokhwa::nokhwa_check()` reads AVAuthorizationStatus;
//!   `nokhwa_initialize(cb)` triggers the TCC prompt. The callback fires on
//!   an OS thread, so it writes an AtomicI8 that a 200 ms subscription polls
//!   while the prompt is pending.
//! - Mic: `cpal` input stream lives on its own thread too (cpal::Stream is
//!   !Send, so it cannot be stored in iced state and must be kept alive on
//!   the thread that built it). The data callback stores RMS into an
//!   AtomicU32 (f32 bits); a 50 ms (20 Hz) subscription reads it and drives
//!   a `progress_bar` VU meter with dB mapping + peak-hold decay.
//! - Beep: `rodio 0.22` (DeviceSinkBuilder → MixerDeviceSink → mixer().add),
//!   fire-and-forget via Task + spawn_blocking.
//! - Gallery: 200 JPEGs decoded via `image` inside
//!   `tokio::task::spawn_blocking`, throttled by a `Semaphore(8)`, one
//!   `Task::perform` per file so thumbnails stream in without blocking the
//!   UI. Handles are created once and cached in state; iced_wgpu keeps the
//!   decoded RGBA in an internal cache keyed by handle id and uploads a
//!   texture the first time each image is actually drawn (scrollable culls
//!   off-viewport images, so uploads happen lazily as you scroll).
//!
//! Verification hooks (not part of the production surface) are marked with
//! `// [verify]` and gated behind the PEEK_SELFTEST env var: they auto-start
//! camera/mic, fire one quiet beep, and append one status line per second to
//! a log file so an external harness can check behavior without OCR.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iced::time::{self, Instant};
use iced::widget::{
    button, column, container, image, progress_bar, row, scrollable, text,
};
use iced::{Center, Element, Fill, Subscription, Task};

use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const THUMB_W: u32 = 160;
const THUMB_H: u32 = 120;
const THUMBS_PER_ROW: usize = 5;
const DECODE_PARALLELISM: usize = 8;

pub fn main() -> iced::Result {
    iced::application(Peek::new, Peek::update, Peek::view)
        .title(|_: &Peek| String::from("Peek (iced)"))
        .window_size((900.0, 600.0))
        .subscription(Peek::subscription)
        .run()
}

// ---------------------------------------------------------------------------
// Shared state written by the hardware threads
// ---------------------------------------------------------------------------

/// One decoded RGBA camera frame.
struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Written by the camera thread, read by the UI poll subscription.
#[derive(Default)]
struct CamShared {
    /// Latest-wins frame slot. The camera thread overwrites; the UI takes.
    frame: Mutex<Option<Frame>>,
    /// Bumped once per frame put into the slot (drop detection + capture fps).
    captured: AtomicU64,
    /// Set to ask the camera thread to exit at the next frame boundary.
    stop: AtomicBool,
    /// Negotiated format, e.g. "1920x1080 @ 30fps NV12", set once.
    format: Mutex<Option<String>>,
    /// Fatal error from the camera thread (open/stream/decode).
    error: Mutex<Option<String>>,
}

/// Written by the cpal input callback, read at 20 Hz by the UI.
#[derive(Default)]
struct MicShared {
    /// RMS of the most recent input buffer, stored as f32 bits.
    rms: AtomicU32,
    /// Number of data callbacks so far (0 forever = silent denial symptom).
    callbacks: AtomicU64,
    /// Device description, set once the stream is up.
    desc: Mutex<Option<String>>,
    /// Error from stream construction or the error callback.
    error: Mutex<Option<String>>,
}

/// Camera TCC status as seen by this app: -1 pending/unknown, 0 denied, 1 ok.
type PermFlag = AtomicI8;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Camera,
    Audio,
    Gallery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Perm {
    Unknown,
    Prompting,
    Granted,
    Denied,
}

struct Peek {
    tab: Tab,

    // Camera
    cam: Arc<CamShared>,
    cam_running: bool,
    perm: Perm,
    perm_flag: Arc<PermFlag>,
    preview: Option<image::Handle>,
    preview_size: (u32, u32),
    presented_in_window: u32,
    presented_fps: u32,
    captured_at_tick: u64,
    captured_fps: u32,

    // Audio
    mic: Arc<MicShared>,
    mic_running: bool,
    mic_stop: Option<std::sync::mpsc::Sender<()>>,
    vu_level: f32, // 0..1 after dB mapping, with decay
    vu_peak: f32,
    beeps_done: u32,
    beep_error: Option<String>,

    // Gallery
    thumbs: Vec<Option<image::Handle>>,
    thumb_errors: usize,
    thumbs_loaded: usize,
    gallery_started: Instant,
    gallery_done_ms: Option<u64>,

    // [verify] selftest logging
    selftest: Option<PathBuf>,
    ticks: u64,
    frame_dumped: bool,
}

#[derive(Debug, Clone)]
enum Message {
    TabSelected(Tab),
    ToggleCamera,
    PermPoll,
    CameraPoll,
    SecondTick,
    ToggleMic,
    MicTick,
    Beep,
    BeepDone(Result<(), String>),
    ThumbLoaded(usize, Result<image::Handle, String>),
}

impl Peek {
    fn new() -> (Self, Task<Message>) {
        let assets = asset_dir();
        let paths = list_jpegs(&assets);
        let n = paths.len();

        // Stream thumbnail decodes in from the blocking pool, 8 at a time.
        let semaphore = Arc::new(tokio::sync::Semaphore::new(DECODE_PARALLELISM));
        let gallery = Task::batch(paths.into_iter().enumerate().map(|(i, path)| {
            let semaphore = Arc::clone(&semaphore);
            Task::perform(
                async move {
                    let _permit = semaphore.acquire_owned().await;
                    tokio::task::spawn_blocking(move || decode_thumb(&path))
                        .await
                        .unwrap_or_else(|e| Err(format!("decode task panicked: {e}")))
                },
                move |result| Message::ThumbLoaded(i, result),
            )
        }));

        // [verify] selftest: auto-start camera + mic, one quiet beep at t≈2 s.
        let selftest = std::env::var_os("PEEK_SELFTEST").map(|_| {
            std::env::var_os("PEEK_LOG")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("selftest.log"))
        });
        let boot = if selftest.is_some() {
            Task::batch([
                Task::done(Message::ToggleCamera),
                Task::done(Message::ToggleMic),
                Task::perform(tokio::time::sleep(Duration::from_secs(2)), |_| Message::Beep),
            ])
        } else {
            Task::none()
        };

        // [verify] initial tab override so the harness can screenshot any tab.
        let tab = match std::env::var("PEEK_TAB").as_deref() {
            Ok("audio") => Tab::Audio,
            Ok("gallery") => Tab::Gallery,
            _ => Tab::Camera,
        };

        (
            Self {
                tab,
                cam: Arc::new(CamShared::default()),
                cam_running: false,
                perm: if nokhwa::nokhwa_check() { Perm::Granted } else { Perm::Unknown },
                perm_flag: Arc::new(AtomicI8::new(-1)),
                preview: None,
                preview_size: (0, 0),
                presented_in_window: 0,
                presented_fps: 0,
                captured_at_tick: 0,
                captured_fps: 0,
                mic: Arc::new(MicShared::default()),
                mic_running: false,
                mic_stop: None,
                vu_level: 0.0,
                vu_peak: 0.0,
                beeps_done: 0,
                beep_error: None,
                thumbs: vec![None; n],
                thumb_errors: 0,
                thumbs_loaded: 0,
                gallery_started: Instant::now(),
                gallery_done_ms: None,
                selftest,
                ticks: 0,
                frame_dumped: false,
            },
            Task::batch([gallery, boot]),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TabSelected(tab) => self.tab = tab,

            Message::ToggleCamera => {
                if self.cam_running {
                    self.cam.stop.store(true, Ordering::Relaxed);
                    self.cam_running = false;
                    self.presented_fps = 0;
                    self.captured_fps = 0;
                } else if nokhwa::nokhwa_check() {
                    self.perm = Perm::Granted;
                    self.start_camera();
                } else {
                    // Ask TCC. The callback lands on an OS thread; the
                    // PermPoll subscription watches the flag while pending.
                    self.perm = Perm::Prompting;
                    let flag = Arc::clone(&self.perm_flag);
                    flag.store(-1, Ordering::Relaxed);
                    nokhwa::nokhwa_initialize(move |granted| {
                        flag.store(granted as i8, Ordering::Relaxed);
                    });
                }
            }

            Message::PermPoll => match self.perm_flag.load(Ordering::Relaxed) {
                1 => {
                    self.perm = Perm::Granted;
                    self.start_camera();
                }
                0 => self.perm = Perm::Denied,
                _ => {}
            },

            Message::CameraPoll => {
                // Take the latest frame (if any) and re-wrap it as a fresh
                // image handle: full per-frame texture re-upload by design.
                if let Some(frame) = self.cam.frame.lock().unwrap().take() {
                    // [verify] PEEK_DUMP_FRAME=path: write the exact RGBA
                    // bytes handed to Handle::from_rgba out as a PNG, once.
                    if let Some(path) = std::env::var_os("PEEK_DUMP_FRAME") {
                        if !self.frame_dumped {
                            self.frame_dumped = true;
                            let _ = ::image::save_buffer(
                                &path,
                                &frame.pixels,
                                frame.width,
                                frame.height,
                                ::image::ColorType::Rgba8,
                            );
                        }
                    }
                    self.preview_size = (frame.width, frame.height);
                    self.preview = Some(image::Handle::from_rgba(
                        frame.width,
                        frame.height,
                        frame.pixels,
                    ));
                    self.presented_in_window += 1;
                }
            }

            Message::SecondTick => {
                self.ticks += 1;
                if self.cam_running {
                    self.presented_fps = self.presented_in_window;
                    self.presented_in_window = 0;
                    let captured = self.cam.captured.load(Ordering::Relaxed);
                    self.captured_fps = (captured - self.captured_at_tick) as u32;
                    self.captured_at_tick = captured;
                }
                // [verify] one machine-readable status line per second.
                if let Some(path) = &self.selftest {
                    self.append_selftest_line(path.clone());
                }
            }

            Message::ToggleMic => {
                if self.mic_running {
                    if let Some(tx) = self.mic_stop.take() {
                        let _ = tx.send(()); // thread drops the stream + exits
                    }
                    self.mic_running = false;
                    self.vu_level = 0.0;
                    self.vu_peak = 0.0;
                } else {
                    self.mic = Arc::new(MicShared::default());
                    let (tx, rx) = std::sync::mpsc::channel();
                    let shared = Arc::clone(&self.mic);
                    std::thread::spawn(move || mic_thread(shared, rx));
                    self.mic_stop = Some(tx);
                    self.mic_running = true;
                }
            }

            Message::MicTick => {
                let rms = f32::from_bits(self.mic.rms.load(Ordering::Relaxed));
                // Map RMS to a -60..0 dBFS bar with fast attack, slow decay.
                let db = 20.0 * rms.max(1e-6).log10();
                let target = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
                self.vu_level = if target > self.vu_level {
                    target
                } else {
                    self.vu_level * 0.80
                };
                self.vu_peak = (self.vu_peak * 0.995).max(target);
            }

            Message::Beep => {
                return Task::perform(
                    async {
                        tokio::task::spawn_blocking(play_beep)
                            .await
                            .unwrap_or_else(|e| Err(format!("beep task panicked: {e}")))
                    },
                    Message::BeepDone,
                );
            }

            Message::BeepDone(result) => match result {
                Ok(()) => {
                    self.beeps_done += 1;
                    self.beep_error = None;
                }
                Err(e) => self.beep_error = Some(e),
            },

            Message::ThumbLoaded(i, result) => {
                match result {
                    Ok(handle) => self.thumbs[i] = Some(handle),
                    Err(_) => self.thumb_errors += 1,
                }
                self.thumbs_loaded += 1;
                if self.thumbs_loaded == self.thumbs.len() && self.gallery_done_ms.is_none() {
                    self.gallery_done_ms =
                        Some(self.gallery_started.elapsed().as_millis() as u64);
                }
            }
        }

        Task::none()
    }

    fn start_camera(&mut self) {
        self.cam = Arc::new(CamShared::default());
        self.captured_at_tick = 0;
        self.presented_in_window = 0;
        let shared = Arc::clone(&self.cam);
        // [verify] PEEK_FAKE_CAMERA=WxH swaps nokhwa for a synthetic 30 fps
        // animated-gradient source: isolates the frame→texture path from the
        // real capture stack (and from TCC) for pipeline verification.
        if let Some(size) = std::env::var("PEEK_FAKE_CAMERA")
            .ok()
            .and_then(|s| s.split_once('x').map(|(w, h)| (w.parse(), h.parse())))
        {
            if let (Ok(w), Ok(h)) = size {
                std::thread::spawn(move || fake_camera_thread(shared, w, h));
                self.cam_running = true;
                return;
            }
        }
        std::thread::spawn(move || camera_thread(shared));
        self.cam_running = true;
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![time::every(Duration::from_secs(1)).map(|_| Message::SecondTick)];

        if self.cam_running {
            // Poll well above 30 Hz so a fresh frame never waits long; each
            // poll that finds a frame installs a new handle (=> redraw).
            subs.push(time::every(Duration::from_millis(8)).map(|_| Message::CameraPoll));
        }
        if self.perm == Perm::Prompting {
            subs.push(time::every(Duration::from_millis(200)).map(|_| Message::PermPoll));
        }
        if self.mic_running {
            subs.push(time::every(Duration::from_millis(50)).map(|_| Message::MicTick));
        }

        Subscription::batch(subs)
    }

    // -----------------------------------------------------------------------
    // View
    // -----------------------------------------------------------------------

    fn view(&self) -> Element<'_, Message> {
        let tab_button = |label, tab| {
            let b = button(text(label).size(14)).on_press(Message::TabSelected(tab));
            if self.tab == tab {
                b.style(button::primary)
            } else {
                b.style(button::secondary)
            }
        };

        let tabs = row![
            tab_button("Camera", Tab::Camera),
            tab_button("Audio", Tab::Audio),
            tab_button("Gallery", Tab::Gallery),
        ]
        .spacing(8);

        let content: Element<'_, Message> = match self.tab {
            Tab::Camera => self.camera_view(),
            Tab::Audio => self.audio_view(),
            Tab::Gallery => self.gallery_view(),
        };

        column![tabs, content].spacing(12).padding(12).into()
    }

    fn camera_view(&self) -> Element<'_, Message> {
        let preview: Element<'_, Message> = match (&self.preview, self.cam_running) {
            (Some(handle), _) => image(handle.clone()).width(Fill).height(Fill).into(),
            (None, true) => center_label("Waiting for first frame…"),
            (None, false) => center_label("Camera stopped. Press Start."),
        };

        let status = match self.perm {
            Perm::Prompting => String::from(
                "Requesting camera permission (TCC prompt should be visible)…",
            ),
            Perm::Denied => String::from(
                "Camera permission DENIED by TCC — preview unavailable. \
                 Re-enable in System Settings > Privacy & Security > Camera.",
            ),
            _ => {
                if let Some(e) = self.cam.error.lock().unwrap().clone() {
                    format!("Camera error: {e}")
                } else if let Some(f) = self.cam.format.lock().unwrap().clone() {
                    format!("Negotiated: {f}")
                } else if self.cam_running {
                    String::from("Opening camera…")
                } else {
                    String::from("Idle.")
                }
            }
        };

        let fps = format!(
            "captured: {} fps   presented: {} fps   ({}x{})",
            self.captured_fps, self.presented_fps, self.preview_size.0, self.preview_size.1
        );

        column![
            row![
                button(text(if self.cam_running { "Stop" } else { "Start" }))
                    .on_press(Message::ToggleCamera),
                text(fps).size(14),
            ]
            .spacing(16)
            .align_y(Center),
            text(status).size(13),
            container(preview).width(Fill).height(Fill),
        ]
        .spacing(8)
        .into()
    }

    fn audio_view(&self) -> Element<'_, Message> {
        let mic_status = if let Some(e) = self.mic.error.lock().unwrap().clone() {
            format!("Mic error: {e}")
        } else if let Some(d) = self.mic.desc.lock().unwrap().clone() {
            let cbs = self.mic.callbacks.load(Ordering::Relaxed);
            if cbs == 0 && self.mic_running {
                format!("{d} — stream up, but 0 callbacks so far")
            } else {
                format!("{d} — {cbs} callbacks")
            }
        } else if self.mic_running {
            String::from("Opening input stream…")
        } else {
            String::from("Mic idle.")
        };

        let beep_status = match &self.beep_error {
            Some(e) => format!("Beep error: {e}"),
            None => format!("Beeps played: {}", self.beeps_done),
        };

        column![
            text("Microphone level (RMS, dBFS mapped -60..0)").size(14),
            row![
                button(text(if self.mic_running { "Stop mic" } else { "Start mic" }))
                    .on_press(Message::ToggleMic),
                column![
                    progress_bar(0.0..=1.0, self.vu_level).girth(24.0),
                    progress_bar(0.0..=1.0, self.vu_peak).girth(6.0),
                ]
                .spacing(4)
                .width(Fill),
            ]
            .spacing(16)
            .align_y(Center),
            text(mic_status).size(13),
            iced::widget::rule::horizontal(1),
            row![
                button(text("Beep (880 Hz, 180 ms)")).on_press(Message::Beep),
                text(beep_status).size(13),
            ]
            .spacing(16)
            .align_y(Center),
        ]
        .spacing(12)
        .into()
    }

    fn gallery_view(&self) -> Element<'_, Message> {
        let header = text(format!(
            "{}/{} thumbnails decoded{}{}",
            self.thumbs_loaded,
            self.thumbs.len(),
            match self.gallery_done_ms {
                Some(ms) => format!(" in {ms} ms"),
                None => String::new(),
            },
            if self.thumb_errors > 0 {
                format!(" ({} errors)", self.thumb_errors)
            } else {
                String::new()
            }
        ))
        .size(14);

        let mut grid = column![].spacing(6);
        for chunk in self.thumbs.chunks(THUMBS_PER_ROW) {
            let mut r = row![].spacing(6);
            for slot in chunk {
                let cell: Element<'_, Message> = match slot {
                    Some(handle) => image(handle.clone())
                        .width(THUMB_W as f32)
                        .height(THUMB_H as f32)
                        .into(),
                    None => container(text("…").size(12))
                        .width(THUMB_W as f32)
                        .height(THUMB_H as f32)
                        .center(Fill)
                        .style(container::bordered_box)
                        .into(),
                };
                r = r.push(cell);
            }
            grid = grid.push(r);
        }

        column![header, scrollable(grid).width(Fill).height(Fill)]
            .spacing(8)
            .into()
    }

    // -----------------------------------------------------------------------
    // [verify] selftest log — everything below in this impl is a hook
    // -----------------------------------------------------------------------

    fn append_selftest_line(&self, path: PathBuf) {
        use std::io::Write;

        let cam_state = if self.cam_running { "running" } else { "stopped" };
        let cam_err = self.cam.error.lock().unwrap().clone().unwrap_or_default();
        let format = self.cam.format.lock().unwrap().clone().unwrap_or_default();
        let mic_cbs = self.mic.callbacks.load(Ordering::Relaxed);
        let mic_err = self.mic.error.lock().unwrap().clone().unwrap_or_default();
        let line = format!(
            "t={} cam={} perm={:?} fmt=\"{}\" cap_fps={} pres_fps={} cam_err=\"{}\" \
             mic_running={} mic_cbs={} rms={:.5} mic_err=\"{}\" beeps={} beep_err=\"{}\" \
             thumbs={}/{} thumb_ms={:?}\n",
            self.ticks,
            cam_state,
            self.perm,
            format,
            self.captured_fps,
            self.presented_fps,
            cam_err,
            self.mic_running,
            mic_cbs,
            f32::from_bits(self.mic.rms.load(Ordering::Relaxed)),
            mic_err,
            self.beeps_done,
            self.beep_error.clone().unwrap_or_default(),
            self.thumbs_loaded,
            self.thumbs.len(),
            self.gallery_done_ms,
        );
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

fn center_label(label: &str) -> Element<'_, Message> {
    container(text(label).size(16)).center(Fill).into()
}

// ---------------------------------------------------------------------------
// Camera thread (nokhwa / AVFoundation)
// ---------------------------------------------------------------------------

fn camera_thread(shared: Arc<CamShared>) {
    let set_error = |msg: String| {
        *shared.error.lock().unwrap() = Some(msg);
    };

    // Highest frame rate wins ties toward the SPEC's ~30 fps target; the
    // actually negotiated format is reported in the UI.
    let requested =
        RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = match Camera::new(CameraIndex::Index(0), requested) {
        Ok(c) => c,
        Err(e) => return set_error(format!("open failed: {e}")),
    };

    let fmt = camera.camera_format();
    *shared.format.lock().unwrap() = Some(format!(
        "{}x{} @ {} fps {}",
        fmt.width(),
        fmt.height(),
        fmt.frame_rate(),
        fmt.format()
    ));

    if let Err(e) = camera.open_stream() {
        return set_error(format!("open_stream failed: {e}"));
    }

    while !shared.stop.load(Ordering::Relaxed) {
        // Blocks until AVFoundation delivers the next sample buffer.
        let buffer = match camera.frame() {
            Ok(b) => b,
            Err(e) => {
                set_error(format!("frame failed: {e}"));
                break;
            }
        };
        match buffer.decode_image::<RgbAFormat>() {
            Ok(img) => {
                let (width, height) = img.dimensions();
                *shared.frame.lock().unwrap() = Some(Frame {
                    width,
                    height,
                    pixels: img.into_raw(),
                });
                shared.captured.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                set_error(format!("decode failed: {e}"));
                break;
            }
        }
    }

    let _ = camera.stop_stream();
}

// [verify] Synthetic 30 fps RGBA source for pipeline isolation (no TCC, no
// nokhwa): scrolling two-tone gradient so motion is visible in screenshots.
fn fake_camera_thread(shared: Arc<CamShared>, width: u32, height: u32) {
    *shared.format.lock().unwrap() =
        Some(format!("{width}x{height} @ 30 fps SYNTHETIC"));
    let mut t: u32 = 0;
    while !shared.stop.load(Ordering::Relaxed) {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let i = ((y * width + x) * 4) as usize;
                pixels[i] = ((x + t * 4) % 256) as u8;
                pixels[i + 1] = ((y + t * 2) % 256) as u8;
                pixels[i + 2] = 128;
                pixels[i + 3] = 255;
            }
        }
        *shared.frame.lock().unwrap() = Some(Frame { width, height, pixels });
        shared.captured.fetch_add(1, Ordering::Relaxed);
        t = t.wrapping_add(1);
        std::thread::sleep(Duration::from_millis(33));
    }
}

// ---------------------------------------------------------------------------
// Mic thread (cpal) — owns the !Send Stream for its whole lifetime
// ---------------------------------------------------------------------------

fn mic_thread(shared: Arc<MicShared>, stop: std::sync::mpsc::Receiver<()>) {
    let set_error = |msg: String| {
        *shared.error.lock().unwrap() = Some(msg);
    };

    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        return set_error(String::from("no default input device"));
    };
    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => return set_error(format!("no input config: {e}")),
    };
    if config.sample_format() != cpal::SampleFormat::F32 {
        // CoreAudio is always f32; anything else is out of scope for Peek.
        return set_error(format!(
            "unsupported sample format {:?}",
            config.sample_format()
        ));
    }

    *shared.desc.lock().unwrap() = Some(format!(
        "{} ({} ch @ {} Hz)",
        device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| String::from("unknown input")),
        config.channels(),
        config.sample_rate()
    ));

    let data_shared = Arc::clone(&shared);
    let err_shared = Arc::clone(&shared);
    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let sum: f32 = data.iter().map(|s| s * s).sum();
            let rms = (sum / data.len().max(1) as f32).sqrt();
            data_shared.rms.store(rms.to_bits(), Ordering::Relaxed);
            data_shared.callbacks.fetch_add(1, Ordering::Relaxed);
        },
        move |e| {
            *err_shared.error.lock().unwrap() = Some(format!("stream error: {e}"));
        },
        None,
    );
    let stream = match stream {
        Ok(s) => s,
        Err(e) => return set_error(format!("build_input_stream failed: {e}")),
    };
    if let Err(e) = stream.play() {
        return set_error(format!("play failed: {e}"));
    }

    // Park until the UI asks us to stop (or it drops the sender on exit).
    let _ = stop.recv();
    drop(stream);
}

// ---------------------------------------------------------------------------
// Beep (rodio)
// ---------------------------------------------------------------------------

fn play_beep() -> Result<(), String> {
    use rodio::source::SineWave;
    use rodio::Source;

    let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|e| format!("open_default_sink failed: {e}"))?;
    sink.log_on_drop(false);
    sink.mixer().add(
        SineWave::new(880.0)
            .take_duration(Duration::from_millis(180))
            .amplify(0.10),
    );
    // Keep the OS sink alive until the tone has played out.
    std::thread::sleep(Duration::from_millis(280));
    Ok(())
}

// ---------------------------------------------------------------------------
// Gallery decode
// ---------------------------------------------------------------------------

fn asset_dir() -> PathBuf {
    // Allow the harness to point elsewhere; default to the repo layout.
    std::env::var_os("PEEK_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../peek-assets"))
}

fn list_jpegs(dir: &PathBuf) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
                })
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

/// Decode + downscale one JPEG into a ready-to-upload RGBA handle.
/// Runs on the tokio blocking pool; the handle itself is just Arc'd bytes.
fn decode_thumb(path: &PathBuf) -> Result<image::Handle, String> {
    let img = ::image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let thumb = img.thumbnail(THUMB_W, THUMB_H).to_rgba8();
    let (w, h) = thumb.dimensions();
    Ok(image::Handle::from_rgba(w, h, thumb.into_raw()))
}
