//! "Peek" — media & hardware test per apps/SPEC-6.md, built with xilem 0.4.0.
//!
//! Architecture: three long-lived `worker` views (camera, mic, beep) receive
//! commands over their channel and spawn dedicated OS threads that own the
//! non-`Send` device handles (nokhwa `Camera`, cpal/rodio streams). Threads
//! report back with `MessageProxy` messages; every camera frame arrives as a
//! fresh RGBA `ImageData` blob and is handed to a custom `CameraPreview`
//! widget that re-paints (paint-only, no relayout) per frame. A `task_raw`
//! decodes the 200 gallery JPEGs on `spawn_blocking` threads at startup, and
//! a 1 Hz probe task computes presented/captured FPS and self-observes CPU%.

mod camera_view;
mod media;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use xilem::core::one_of::OneOf2;
use xilem::core::{fork, NoElement, View};
use xilem::masonry::peniko::{Color, ImageData};
use xilem::masonry::properties::types::{AsUnit, Length};
use xilem::style::Style as _;
use xilem::tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use xilem::view::{
    flex_col, flex_row, image, label, portal, progress_bar, sized_box, task_raw, text_button,
    worker, FlexExt as _, FlexSpacer,
};
use xilem::winit::error::EventLoopError;
use xilem::{EventLoop, FontWeight, ViewCtx, WidgetView, WindowOptions, Xilem};

use camera_view::camera_preview;
use media::{BeepCmd, BeepMsg, CamCmd, CamMsg, GalleryMsg, MicCmd, MicMsg};

const GALLERY_COLS: usize = 8;
const PLACEHOLDER_BG: Color = Color::from_rgb8(0x20, 0x24, 0x2b);

// ===========================================================================
// App state
// ===========================================================================

struct Peek {
    // Camera
    cam_sender: Option<UnboundedSender<CamCmd>>,
    cam_running: bool,
    frame: Option<ImageData>,
    cam_format: String,
    cam_status: String,
    frames_received: u64,
    presented: Arc<AtomicU64>,
    fps_presented: f64,
    fps_captured: f64,
    cpu_pct: Option<f32>,
    rss_mib: Option<f64>,
    last_probe_at: Instant,
    last_presented: u64,
    last_received: u64,
    // Mic
    mic_sender: Option<UnboundedSender<MicCmd>>,
    mic_running: bool,
    mic_rms: f32,
    mic_peak: f32,
    mic_status: String,
    // Beep
    beep_sender: Option<UnboundedSender<BeepCmd>>,
    beep_count: u64,
    beep_status: String,
    // Gallery
    thumbs: Vec<Option<ImageData>>,
    thumbs_loaded: usize,
    gallery_status: String,
}

impl Peek {
    fn new() -> Self {
        Self {
            cam_sender: None,
            cam_running: false,
            frame: None,
            cam_format: String::new(),
            cam_status: "off".to_owned(),
            frames_received: 0,
            presented: Arc::new(AtomicU64::new(0)),
            fps_presented: 0.0,
            fps_captured: 0.0,
            cpu_pct: None,
            rss_mib: None,
            last_probe_at: Instant::now(),
            last_presented: 0,
            last_received: 0,
            mic_sender: None,
            mic_running: false,
            mic_rms: 0.0,
            mic_peak: 0.0,
            mic_status: "off".to_owned(),
            beep_sender: None,
            beep_count: 0,
            beep_status: "idle".to_owned(),
            thumbs: Vec::new(),
            thumbs_loaded: 0,
            gallery_status: "scanning…".to_owned(),
        }
    }

    // --- camera ---

    fn toggle_camera(&mut self) {
        if self.cam_running {
            if let Some(tx) = &self.cam_sender {
                let _ = tx.send(CamCmd::Stop);
            }
            self.cam_running = false;
            self.cam_status = "stopping…".to_owned();
        } else if let Some(tx) = &self.cam_sender {
            let _ = tx.send(CamCmd::Start);
            self.cam_running = true;
            self.cam_status = "starting…".to_owned();
        } else {
            self.cam_status = "worker not ready".to_owned();
        }
    }

    fn on_cam_msg(&mut self, msg: CamMsg) {
        match msg {
            CamMsg::Status(s) => self.cam_status = s,
            CamMsg::Format(f) => self.cam_format = f,
            CamMsg::Frame(data) => {
                self.frames_received += 1;
                self.frame = Some(data);
            }
            CamMsg::Error(e) => {
                eprintln!("[peek] camera error: {e}");
                self.cam_status = format!("error: {e}");
                self.cam_running = false;
            }
            CamMsg::Stopped => {
                if !self.cam_status.starts_with("error") {
                    self.cam_status = "stopped".to_owned();
                }
                self.cam_running = false;
                self.frame = None;
                self.fps_presented = 0.0;
                self.fps_captured = 0.0;
            }
        }
    }

    // --- mic ---

    fn toggle_mic(&mut self) {
        if self.mic_running {
            if let Some(tx) = &self.mic_sender {
                let _ = tx.send(MicCmd::Stop);
            }
            self.mic_running = false;
            self.mic_status = "stopping…".to_owned();
        } else if let Some(tx) = &self.mic_sender {
            let _ = tx.send(MicCmd::Start);
            self.mic_running = true;
            self.mic_peak = 0.0;
            self.mic_status = "starting…".to_owned();
        } else {
            self.mic_status = "worker not ready".to_owned();
        }
    }

    fn on_mic_msg(&mut self, msg: MicMsg) {
        match msg {
            MicMsg::Started(s) => self.mic_status = s,
            MicMsg::Level(rms) => {
                self.mic_rms = rms;
                self.mic_peak = self.mic_peak.max(rms);
            }
            MicMsg::Error(e) => {
                eprintln!("[peek] mic error: {e}");
                self.mic_status = format!("error: {e}");
                self.mic_running = false;
            }
            MicMsg::Stopped => {
                self.mic_running = false;
                self.mic_rms = 0.0;
                if !self.mic_status.starts_with("error") {
                    self.mic_status = "stopped".to_owned();
                }
            }
        }
    }

    // --- beep ---

    fn beep(&mut self) {
        if let Some(tx) = &self.beep_sender {
            let _ = tx.send(BeepCmd::Play);
            self.beep_status = "playing…".to_owned();
        } else {
            self.beep_status = "worker not ready".to_owned();
        }
    }

    fn on_beep_msg(&mut self, msg: BeepMsg) {
        match msg {
            BeepMsg::Done => {
                self.beep_count += 1;
                self.beep_status = format!("played ×{}", self.beep_count);
            }
            BeepMsg::Error(e) => {
                eprintln!("[peek] beep error: {e}");
                self.beep_status = format!("error: {e}");
            }
        }
    }

    // --- gallery ---

    fn on_gallery_msg(&mut self, msg: GalleryMsg) {
        match msg {
            GalleryMsg::Found(n) => {
                self.thumbs = vec![None; n];
                self.gallery_status = format!("0/{n}");
            }
            GalleryMsg::Thumb(i, data) => {
                if let Some(slot) = self.thumbs.get_mut(i) {
                    if slot.is_none() {
                        self.thumbs_loaded += 1;
                    }
                    *slot = Some(data);
                    self.gallery_status = format!("{}/{}", self.thumbs_loaded, self.thumbs.len());
                    if self.thumbs_loaded == self.thumbs.len() {
                        // Verification hook: proves async decode completed.
                        println!("[peek] gallery loaded: {} thumbnails", self.thumbs_loaded);
                    }
                }
            }
            GalleryMsg::Error(e) => {
                eprintln!("[peek] gallery error: {e}");
                self.gallery_status = format!("error: {e}");
            }
        }
    }

    // --- 1 Hz probe (FPS + self-observed CPU/RSS) ---

    fn on_probe(&mut self, sample: Option<(f32, f64)>) {
        let now = Instant::now();
        let dt = (now - self.last_probe_at).as_secs_f64().max(1e-3);
        let presented = self.presented.load(Ordering::Relaxed);
        if self.cam_running {
            self.fps_presented = (presented - self.last_presented) as f64 / dt;
            self.fps_captured = (self.frames_received - self.last_received) as f64 / dt;
        }
        self.last_probe_at = now;
        self.last_presented = presented;
        self.last_received = self.frames_received;
        if let Some((cpu, rss)) = sample {
            self.cpu_pct = Some(cpu);
            self.rss_mib = Some(rss);
        }
        // Verification hook: machine-readable once-per-second log line.
        if self.cam_running {
            println!(
                "[peek-probe] presented_fps={:.1} captured_fps={:.1} cpu_pct={} rss_mib={} mic={} thumbs={}",
                self.fps_presented,
                self.fps_captured,
                self.cpu_pct.map_or("?".to_owned(), |c| format!("{c:.1}")),
                self.rss_mib.map_or("?".to_owned(), |r| format!("{r:.0}")),
                self.mic_running,
                self.thumbs_loaded,
            );
        }
    }
}

// ===========================================================================
// UI sections
// ===========================================================================

fn heading(text: &'static str) -> impl WidgetView<Peek> + use<> {
    label(text).text_size(15.0).weight(FontWeight::BOLD)
}

fn camera_section(state: &Peek) -> impl WidgetView<Peek> + use<> {
    let preview = sized_box(camera_preview(state.frame.clone(), state.presented.clone()))
        .width(384.px())
        .height(288.px());

    let controls = flex_col((
        heading("Camera"),
        text_button(
            if state.cam_running { "Stop" } else { "Start" },
            |state: &mut Peek| state.toggle_camera(),
        ),
        label(format!("presented {:.1} fps", state.fps_presented))
            .text_size(22.0)
            .weight(FontWeight::BOLD),
        label(format!("captured {:.1} fps", state.fps_captured)).text_size(12.0),
        label(state.cam_format.clone()).text_size(11.0),
        label(format!("status: {}", state.cam_status)).text_size(11.0),
        label(format!(
            "self-observed: {} CPU, {} RSS",
            state
                .cpu_pct
                .map_or("–".to_owned(), |c| format!("{c:.1}%")),
            state
                .rss_mib
                .map_or("–".to_owned(), |r| format!("{r:.0} MiB")),
        ))
        .text_size(11.0),
    ))
    .gap(Length::px(6.0));

    flex_row((preview, controls)).gap(Length::px(14.0))
}

fn audio_section(state: &Peek) -> impl WidgetView<Peek> + use<> {
    let db = 20.0 * f32::max(state.mic_rms, 1e-5).log10();
    let level = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    flex_col((
        heading("Audio"),
        flex_row((
            text_button(
                if state.mic_running { "Stop mic" } else { "Start mic" },
                |state: &mut Peek| state.toggle_mic(),
            ),
            sized_box(progress_bar(Some(f64::from(level)))).width(280.px()),
            label(format!("{db:.1} dBFS (peak rms {:.4})", state.mic_peak)).text_size(11.0),
            FlexSpacer::Flex(1.0),
            text_button("Beep", |state: &mut Peek| state.beep()),
            label(state.beep_status.clone()).text_size(11.0),
        ))
        .gap(Length::px(10.0)),
        label(format!("mic: {}", state.mic_status)).text_size(11.0),
    ))
    .gap(Length::px(4.0))
}

fn gallery_section(state: &Peek) -> impl WidgetView<Peek> + use<> {
    let rows: Vec<_> = state
        .thumbs
        .chunks(GALLERY_COLS)
        .map(|chunk| {
            let cells: Vec<_> = chunk
                .iter()
                .map(|thumb| match thumb {
                    Some(data) => OneOf2::A(
                        sized_box(image(data.clone()))
                            .width(96.px())
                            .height(72.px()),
                    ),
                    None => OneOf2::B(
                        sized_box(label("…"))
                            .width(96.px())
                            .height(72.px())
                            .background_color(PLACEHOLDER_BG),
                    ),
                })
                .collect();
            flex_row(cells).gap(Length::px(4.0))
        })
        .collect();

    flex_col((
        flex_row((
            heading("Gallery"),
            label(state.gallery_status.clone()).text_size(11.0),
        ))
        .gap(Length::px(10.0)),
        portal(flex_col(rows).gap(Length::px(4.0))).flex(1.0),
    ))
    .gap(Length::px(4.0))
}

// ===========================================================================
// Background views (workers + tasks)
// ===========================================================================

/// Camera worker: owns the capture thread's stop-flag lifecycle.
fn camera_worker() -> impl View<Peek, (), ViewCtx, Element = NoElement> {
    worker(
        |proxy, mut rx: UnboundedReceiver<CamCmd>| async move {
            let mut flag: Option<Arc<AtomicBool>> = None;
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    CamCmd::Start => {
                        if flag.as_ref().is_some_and(|f| f.load(Ordering::Relaxed)) {
                            continue; // already running
                        }
                        let f = Arc::new(AtomicBool::new(true));
                        flag = Some(f.clone());
                        let proxy = proxy.clone();
                        std::thread::spawn(move || media::camera_thread(proxy, f));
                    }
                    CamCmd::Stop => {
                        if let Some(f) = &flag {
                            f.store(false, Ordering::Relaxed);
                        }
                    }
                }
            }
        },
        |state: &mut Peek, sender| state.cam_sender = Some(sender),
        |state: &mut Peek, msg| state.on_cam_msg(msg),
    )
}

fn mic_worker() -> impl View<Peek, (), ViewCtx, Element = NoElement> {
    worker(
        |proxy, mut rx: UnboundedReceiver<MicCmd>| async move {
            let mut flag: Option<Arc<AtomicBool>> = None;
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    MicCmd::Start => {
                        if flag.as_ref().is_some_and(|f| f.load(Ordering::Relaxed)) {
                            continue;
                        }
                        let f = Arc::new(AtomicBool::new(true));
                        flag = Some(f.clone());
                        let proxy = proxy.clone();
                        std::thread::spawn(move || media::mic_thread(proxy, f));
                    }
                    MicCmd::Stop => {
                        if let Some(f) = &flag {
                            f.store(false, Ordering::Relaxed);
                        }
                    }
                }
            }
        },
        |state: &mut Peek, sender| state.mic_sender = Some(sender),
        |state: &mut Peek, msg| state.on_mic_msg(msg),
    )
}

fn beep_worker() -> impl View<Peek, (), ViewCtx, Element = NoElement> {
    worker(
        |proxy, mut rx: UnboundedReceiver<BeepCmd>| async move {
            while let Some(BeepCmd::Play) = rx.recv().await {
                let proxy = proxy.clone();
                std::thread::spawn(move || media::beep_thread(proxy));
            }
        },
        |state: &mut Peek, sender| state.beep_sender = Some(sender),
        |state: &mut Peek, msg| state.on_beep_msg(msg),
    )
}

/// Startup gallery load: list the JPEGs, decode+downscale on up to 4 blocking
/// threads, stream results back one thumbnail at a time.
fn gallery_loader() -> impl View<Peek, (), ViewCtx, Element = NoElement> {
    task_raw(
        |proxy| async move {
            let dir = media::assets_dir();
            let mut files: Vec<_> = match std::fs::read_dir(&dir) {
                Ok(rd) => rd
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
                    })
                    .collect(),
                Err(e) => {
                    let _ = proxy.message(GalleryMsg::Error(format!(
                        "cannot read {}: {e}",
                        dir.display()
                    )));
                    return;
                }
            };
            files.sort();
            if proxy.message(GalleryMsg::Found(files.len())).is_err() {
                return;
            }
            let semaphore = Arc::new(xilem::tokio::sync::Semaphore::new(4));
            for (i, path) in files.into_iter().enumerate() {
                let Ok(permit) = semaphore.clone().acquire_owned().await else {
                    return;
                };
                let proxy = proxy.clone();
                xilem::tokio::task::spawn_blocking(move || {
                    let msg = match media::decode_thumb(&path) {
                        Ok(data) => GalleryMsg::Thumb(i, data),
                        Err(e) => GalleryMsg::Error(e),
                    };
                    let _ = proxy.message(msg);
                    drop(permit);
                });
            }
        },
        |state: &mut Peek, msg| state.on_gallery_msg(msg),
    )
}

/// 1 Hz probe: FPS bookkeeping + self-observation of CPU%/RSS via `ps`.
fn probe() -> impl View<Peek, (), ViewCtx, Element = NoElement> {
    task_raw(
        |proxy| async move {
            let pid = std::process::id().to_string();
            loop {
                xilem::tokio::time::sleep(Duration::from_secs(1)).await;
                let pid = pid.clone();
                let sample = xilem::tokio::task::spawn_blocking(move || {
                    let out = std::process::Command::new("ps")
                        .args(["-o", "%cpu=,rss=", "-p", &pid])
                        .output()
                        .ok()?;
                    let text = String::from_utf8(out.stdout).ok()?;
                    let mut fields = text.split_whitespace();
                    let cpu: f32 = fields.next()?.parse().ok()?;
                    let rss_kib: f64 = fields.next()?.parse().ok()?;
                    Some((cpu, rss_kib / 1024.0))
                })
                .await
                .ok()
                .flatten();
                if proxy.message(sample).is_err() {
                    break;
                }
            }
        },
        |state: &mut Peek, sample| state.on_probe(sample),
    )
}

// ===========================================================================
// App
// ===========================================================================

fn app_logic(state: &mut Peek) -> impl WidgetView<Peek> + use<> {
    let body = flex_col((
        camera_section(state),
        audio_section(state),
        gallery_section(state).flex(1.0),
    ))
    .gap(Length::px(10.0))
    .padding(10.0);

    fork(
        body,
        (
            camera_worker(),
            mic_worker(),
            beep_worker(),
            gallery_loader(),
            probe(),
        ),
    )
}

fn main() -> Result<(), EventLoopError> {
    let window = WindowOptions::new("Peek (xilem)")
        .with_initial_inner_size(xilem::dpi::LogicalSize::new(900.0, 600.0))
        .with_resizable(true);
    Xilem::new_simple(Peek::new(), app_logic, window).run_in(EventLoop::with_user_event())
}
