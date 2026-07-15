//! "Peek" — media & hardware test in gpui 0.2.2 (SPEC-6).
//!
//! Camera: AVFoundation (direct, see camera.rs) delivers IOSurface-backed NV12
//! CVPixelBuffers; the preview paints them with gpui's `surface()` element,
//! which binds the buffer's two planes as Metal textures via
//! CVMetalTextureCache — zero per-frame CPU copies ("Zero-copy" mode). A
//! "CPU upload" mode converts each frame NV12→BGRA in Rust and rebuilds an
//! `Arc<RenderImage>` for `img()` every frame — the path a framework without
//! a surface element would be stuck with — so the two costs can be compared.
//!
//! Mic: cpal input stream → RMS (audio thread) → 20 Hz UI task → VU bar.
//! Beep: rodio sine into the default output sink.
//! Gallery: 200 JPEGs via `img(path)` in a `uniform_list` — gpui decodes
//! asynchronously on the background executor and caches the decoded
//! `RenderImage` app-wide (keyed by path), uploading to the sprite atlas on
//! first paint.

mod audio;
mod camera;
mod verify;

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use core_video::pixel_buffer::{
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, CVPixelBuffer,
};
use futures::StreamExt;
use gpui::{
    div, img, prelude::*, px, rgb, size, surface, uniform_list, App, Application, Bounds, Context,
    ElementId, ObjectFit, RenderImage, SharedString, Stateful, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
};

const GALLERY_COLS: usize = 7;
const GALLERY_TILE_W: f32 = 116.0;
const GALLERY_TILE_H: f32 = 88.0;
const GALLERY_ROW_H: f32 = GALLERY_TILE_H + 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum UploadMode {
    /// CVPixelBuffer → gpui::surface() → CVMetalTextureCache (no CPU copy).
    ZeroCopy,
    /// NV12→BGRA in Rust + new Arc<RenderImage> per frame → img().
    Cpu,
}

impl UploadMode {
    fn label(&self) -> &'static str {
        match self {
            UploadMode::ZeroCopy => "zero-copy",
            UploadMode::Cpu => "cpu",
        }
    }
}

enum CamState {
    Off,
    Requesting { since: Instant },
    Running,
    Denied(String),
    Error(String),
}

struct PeekApp {
    // --- camera ---
    camera: Option<camera::Camera>,
    cam_state: CamState,
    frame: Option<CVPixelBuffer>,
    frame_wh: (usize, usize),
    frame_seq: u64,
    last_seq_rendered: u64,
    /// Timestamps of renders that presented a NEW camera frame (spec: measure
    /// frames actually presented, not captured).
    presented: VecDeque<Instant>,
    mode: UploadMode,
    cpu_image: Option<Arc<RenderImage>>,
    /// Replaced per-frame RenderImages waiting for window.drop_image() so the
    /// sprite atlas doesn't grow unboundedly in CPU mode.
    retired: Vec<Arc<RenderImage>>,
    cpu_convert_ms: f32,
    // --- mic ---
    mic_stream: Option<cpal::Stream>,
    mic_shared: Arc<audio::MicShared>,
    mic_level: f32,
    mic_err: Option<String>,
    // --- beep ---
    beeper: audio::Beeper,
    beep_err: Option<String>,
    // --- gallery ---
    gallery: Vec<Arc<Path>>,
    gallery_dir: String,
    started_at: Instant,
}

impl PeekApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let (gallery, gallery_dir) = load_gallery_paths();
        let mode = if std::env::var("PEEK_MODE").as_deref() == Ok("cpu") {
            UploadMode::Cpu
        } else {
            UploadMode::ZeroCopy
        };
        verify::install(cx);
        Self {
            camera: None,
            cam_state: CamState::Off,
            frame: None,
            frame_wh: (0, 0),
            frame_seq: 0,
            last_seq_rendered: 0,
            presented: VecDeque::new(),
            mode,
            cpu_image: None,
            retired: Vec::new(),
            cpu_convert_ms: 0.0,
            mic_stream: None,
            mic_shared: Arc::new(audio::MicShared::new()),
            mic_level: 0.0,
            mic_err: None,
            beeper: audio::Beeper::new(),
            beep_err: None,
            gallery,
            gallery_dir,
            started_at: Instant::now(),
        }
    }

    // ------------------------------------------------------------------
    // Camera control
    // ------------------------------------------------------------------

    fn camera_running(&self) -> bool {
        matches!(self.cam_state, CamState::Running)
    }

    fn toggle_camera(&mut self, cx: &mut Context<Self>) {
        if self.camera_running() {
            self.stop_camera(cx);
        } else {
            self.start_camera(cx);
        }
    }

    fn start_camera(&mut self, cx: &mut Context<Self>) {
        use camera::{AuthStatus, MediaKind};
        let status = camera::auth_status(MediaKind::Video);
        println!("CAMERA_AUTH initial={}", status.label());
        match status {
            AuthStatus::Authorized => self.begin_capture(cx),
            AuthStatus::NotDetermined => {
                // Fires the TCC prompt. The completion handler runs on an
                // arbitrary thread; route the verdict back through a channel.
                self.cam_state = CamState::Requesting {
                    since: Instant::now(),
                };
                let (tx, mut rx) = futures::channel::mpsc::unbounded::<bool>();
                camera::request_video_access(move |granted| {
                    let _ = tx.unbounded_send(granted);
                });
                cx.spawn(async move |this, cx| {
                    let granted = rx.next().await.unwrap_or(false);
                    this.update(cx, |app, cx| {
                        let waited = match &app.cam_state {
                            CamState::Requesting { since } => since.elapsed().as_millis(),
                            _ => 0,
                        };
                        println!(
                            "CAMERA_AUTH result={} waited_ms={}",
                            if granted { "granted" } else { "denied" },
                            waited
                        );
                        if granted {
                            app.begin_capture(cx);
                        } else {
                            app.cam_state = CamState::Denied(
                                "Camera access denied — grant it in System Settings → \
                                 Privacy & Security → Camera"
                                    .into(),
                            );
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
                cx.notify();
            }
            AuthStatus::Denied | AuthStatus::Restricted => {
                self.cam_state = CamState::Denied(format!(
                    "Camera access {} (TCC) — grant it in System Settings → \
                     Privacy & Security → Camera",
                    status.label()
                ));
                cx.notify();
            }
        }
    }

    fn begin_capture(&mut self, cx: &mut Context<Self>) {
        if self.camera.is_none() {
            match camera::Camera::new() {
                Ok((cam, wake_rx)) => {
                    let shared = cam.shared.clone();
                    self.camera = Some(cam);
                    // Frame pump: delegate queue wakes us; take the newest
                    // frame and hand it to the entity on the main thread.
                    cx.spawn(async move |this, cx| {
                        let mut wake_rx = wake_rx;
                        while wake_rx.next().await.is_some() {
                            if let Some(pb) = shared.take_latest() {
                                if this
                                    .update(cx, |app, cx| app.on_new_frame(pb, cx))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    })
                    .detach();
                }
                Err(e) => {
                    println!("CAMERA error: {e}");
                    self.cam_state = CamState::Error(e);
                    cx.notify();
                    return;
                }
            }
        }
        if let Some(cam) = &mut self.camera {
            cam.start();
            self.cam_state = CamState::Running;
            self.presented.clear();
            println!("CAMERA started");
            cx.notify();
        }
    }

    fn stop_camera(&mut self, cx: &mut Context<Self>) {
        if let Some(cam) = &mut self.camera {
            cam.stop();
        }
        self.cam_state = CamState::Off;
        self.frame = None;
        if let Some(old) = self.cpu_image.take() {
            self.retired.push(old);
        }
        self.presented.clear();
        println!("CAMERA stopped");
        cx.notify();
    }

    fn on_new_frame(&mut self, pb: CVPixelBuffer, cx: &mut Context<Self>) {
        if !self.camera_running() {
            return; // late frame after stop
        }
        // The metal renderer *asserts* NV12; guard so a format surprise
        // degrades instead of panicking the app.
        if pb.get_pixel_format() != kCVPixelFormatType_420YpCbCr8BiPlanarFullRange {
            let msg = format!(
                "unexpected pixel format 0x{:08x} (wanted 420f/NV12)",
                pb.get_pixel_format()
            );
            println!("CAMERA error: {msg}");
            self.cam_state = CamState::Error(msg);
            if let Some(cam) = &mut self.camera {
                cam.stop();
            }
            cx.notify();
            return;
        }
        self.frame_wh = (pb.get_width(), pb.get_height());
        if self.mode == UploadMode::Cpu {
            let t0 = Instant::now();
            if let Some((w, h, bgra)) = camera::nv12_to_bgra(&pb) {
                if let Some(buffer) = image::RgbaImage::from_raw(w, h, bgra) {
                    if let Some(old) = self.cpu_image.take() {
                        self.retired.push(old);
                    }
                    self.cpu_image = Some(Arc::new(RenderImage::new(smallvec::smallvec![
                        image::Frame::new(buffer)
                    ])));
                }
            }
            self.cpu_convert_ms = t0.elapsed().as_secs_f32() * 1000.0;
        }
        self.frame = Some(pb);
        self.frame_seq += 1;
        cx.notify();
    }

    fn set_mode(&mut self, mode: UploadMode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            if mode == UploadMode::ZeroCopy {
                if let Some(old) = self.cpu_image.take() {
                    self.retired.push(old);
                }
            }
            cx.notify();
        }
    }

    /// Presented-FPS over the trailing second (counted when a render sees a
    /// frame it hasn't presented before).
    fn presented_fps(&self) -> f32 {
        self.presented
            .iter()
            .filter(|t| t.elapsed() < Duration::from_secs(1))
            .count() as f32
    }

    // ------------------------------------------------------------------
    // Mic control
    // ------------------------------------------------------------------

    fn toggle_mic(&mut self, cx: &mut Context<Self>) {
        if self.mic_stream.is_some() {
            self.mic_stream = None; // dropping the cpal stream stops it
            self.mic_level = 0.0;
            println!("MIC stopped");
            cx.notify();
            return;
        }
        self.mic_err = None;
        println!(
            "MIC_AUTH initial={} (query only; cpal itself triggers the prompt)",
            camera::auth_status(camera::MediaKind::Audio).label()
        );
        match audio::start_mic(self.mic_shared.clone()) {
            Ok(stream) => {
                self.mic_stream = Some(stream);
                println!("MIC started");
                // 20 Hz meter task; ends when the mic is stopped.
                cx.spawn(async move |this, cx| loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(50))
                        .await;
                    let cont = this.update(cx, |app, cx| {
                        if app.mic_stream.is_none() {
                            return false;
                        }
                        let rms = app.mic_shared.rms();
                        // Fast attack, slow decay, like a hardware VU.
                        app.mic_level = if rms > app.mic_level {
                            rms
                        } else {
                            app.mic_level * 0.85 + rms * 0.15
                        };
                        cx.notify();
                        true
                    });
                    if !matches!(cont, Ok(true)) {
                        break;
                    }
                })
                .detach();
            }
            Err(e) => {
                println!("MIC error: {e}");
                self.mic_err = Some(e);
            }
        }
        cx.notify();
    }

    fn beep(&mut self, cx: &mut Context<Self>) {
        match self.beeper.beep() {
            Ok(()) => {
                self.beep_err = None;
                println!("BEEP ok count={}", self.beeper.beeps);
            }
            Err(e) => {
                println!("BEEP error: {e}");
                self.beep_err = Some(e);
            }
        }
        cx.notify();
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .h(px(26.))
        .px_3()
        .bg(rgb(0x3b82f6))
        .hover(|s| s.bg(rgb(0x2563eb)))
        .active(|s| s.bg(rgb(0x1d4ed8)))
        .rounded_md()
        .cursor_pointer()
        .text_color(gpui::white())
        .child(label.into())
}

fn mode_chip(
    id: &'static str,
    label: &'static str,
    selected: bool,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .px_2()
        .py_0p5()
        .rounded_md()
        .cursor_pointer()
        .border_1()
        .when(selected, |s| {
            s.bg(rgb(0xdbeafe)).border_color(rgb(0x3b82f6)).text_color(rgb(0x1d4ed8))
        })
        .when(!selected, |s| {
            s.border_color(rgb(0xd1d5db)).text_color(rgb(0x6b7280))
        })
        .child(label)
}

impl Render for PeekApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Free atlas textures of replaced per-frame RenderImages (CPU mode).
        for old in self.retired.drain(..) {
            let _ = window.drop_image(old);
        }

        // Presented-frame bookkeeping.
        if self.frame_seq != self.last_seq_rendered {
            self.last_seq_rendered = self.frame_seq;
            self.presented.push_back(Instant::now());
        }
        while let Some(front) = self.presented.front() {
            if front.elapsed() > Duration::from_millis(1500) {
                self.presented.pop_front();
            } else {
                break;
            }
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .bg(rgb(0xf3f4f6))
            .text_sm()
            .text_color(rgb(0x111827))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap_2()
                    .h(px(300.))
                    .child(self.render_camera_panel(cx))
                    .child(self.render_audio_panel(cx)),
            )
            .child(self.render_gallery(cx))
    }
}

impl PeekApp {
    fn render_camera_panel(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let running = self.camera_running();
        let fps = self.presented_fps();

        let preview: gpui::AnyElement = match (&self.cam_state, self.mode) {
            (CamState::Running, UploadMode::ZeroCopy) if self.frame.is_some() => {
                surface(self.frame.clone().unwrap())
                    .object_fit(ObjectFit::Contain)
                    .size_full()
                    .into_any_element()
            }
            (CamState::Running, UploadMode::Cpu) if self.cpu_image.is_some() => {
                img(self.cpu_image.clone().unwrap())
                    .object_fit(ObjectFit::Contain)
                    .size_full()
                    .into_any_element()
            }
            (CamState::Running, _) => centered_note("Waiting for first frame…"),
            (CamState::Off, _) => centered_note("Camera off"),
            (CamState::Requesting { .. }, _) => {
                centered_note("Waiting for camera permission (TCC prompt)…")
            }
            (CamState::Denied(msg), _) | (CamState::Error(msg), _) => {
                centered_note(msg.clone())
            }
        };

        let status = if running {
            format!(
                "{}×{} · {:.1} fps presented{}",
                self.frame_wh.0,
                self.frame_wh.1,
                fps,
                if self.mode == UploadMode::Cpu {
                    format!(" · NV12→BGRA {:.1} ms/frame", self.cpu_convert_ms)
                } else {
                    String::new()
                }
            )
        } else {
            "—".to_string()
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .bg(gpui::white())
            .rounded_md()
            .border_1()
            .border_color(rgb(0xe5e7eb))
            .child(div().flex_none().font_weight(gpui::FontWeight::BOLD).child("Camera"))
            .child(
                div()
                    .flex_1()
                    .rounded_md()
                    .bg(rgb(0x111827))
                    .overflow_hidden()
                    .child(preview),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        button("cam-toggle", if running { "Stop" } else { "Start" })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_camera(cx))),
                    )
                    .child(
                        mode_chip("mode-zero", "Zero-copy (surface)", self.mode == UploadMode::ZeroCopy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_mode(UploadMode::ZeroCopy, cx)
                            })),
                    )
                    .child(
                        mode_chip("mode-cpu", "CPU upload (img)", self.mode == UploadMode::Cpu)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_mode(UploadMode::Cpu, cx)
                            })),
                    )
                    .child(div().text_color(rgb(0x6b7280)).child(status)),
            )
    }

    fn render_audio_panel(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let mic_on = self.mic_stream.is_some();
        // RMS → dBFS → 0..1 bar over a -60 dB range.
        let db = 20.0 * self.mic_level.max(1e-6).log10();
        let norm = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        const BAR_W: f32 = 240.0;

        div()
            .flex_none()
            .w(px(290.))
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .bg(gpui::white())
            .rounded_md()
            .border_1()
            .border_color(rgb(0xe5e7eb))
            .child(div().flex_none().font_weight(gpui::FontWeight::BOLD).child("Audio"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        button("mic-toggle", if mic_on { "Stop mic" } else { "Start mic" })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_mic(cx))),
                    )
                    .child(div().text_color(rgb(0x6b7280)).child(if mic_on {
                        format!("{db:.0} dBFS")
                    } else {
                        "mic off".into()
                    })),
            )
            .child(
                // VU bar (20 Hz updates while the mic runs).
                div()
                    .flex_none()
                    .w(px(BAR_W))
                    .h(px(14.))
                    .rounded_md()
                    .bg(rgb(0xe5e7eb))
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(px(BAR_W * norm))
                            .bg(if norm > 0.85 {
                                rgb(0xdc2626)
                            } else if norm > 0.6 {
                                rgb(0xf59e0b)
                            } else {
                                rgb(0x22c55e)
                            }),
                    ),
            )
            .when_some(self.mic_err.clone(), |el, err| {
                el.child(div().text_color(rgb(0xdc2626)).child(err))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        button("beep", "Beep")
                            .on_click(cx.listener(|this, _, _, cx| this.beep(cx))),
                    )
                    .child(
                        div()
                            .text_color(rgb(0x6b7280))
                            .child(format!("{} played", self.beeper.beeps)),
                    ),
            )
            .when_some(self.beep_err.clone(), |el, err| {
                el.child(div().text_color(rgb(0xdc2626)).child(err))
            })
    }

    fn render_gallery(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let count = self.gallery.len();
        let rows = count.div_ceil(GALLERY_COLS);

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .bg(gpui::white())
            .rounded_md()
            .border_1()
            .border_color(rgb(0xe5e7eb))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap_2()
                    .child(div().font_weight(gpui::FontWeight::BOLD).child("Gallery"))
                    .child(
                        div()
                            .text_color(rgb(0x6b7280))
                            .child(format!("{count} JPEGs from {}", self.gallery_dir)),
                    ),
            )
            .child(
                div().flex_1().child(
                    uniform_list(
                        "gallery",
                        rows,
                        cx.processor(|this, range: std::ops::Range<usize>, _window, _cx| {
                            range
                                .map(|row| {
                                    let start = row * GALLERY_COLS;
                                    let end = (start + GALLERY_COLS).min(this.gallery.len());
                                    div()
                                        .h(px(GALLERY_ROW_H))
                                        .flex()
                                        .gap_1()
                                        .children(this.gallery[start..end].iter().map(|path| {
                                            div()
                                                .w(px(GALLERY_TILE_W))
                                                .h(px(GALLERY_TILE_H))
                                                .rounded_md()
                                                .bg(rgb(0xe5e7eb))
                                                .overflow_hidden()
                                                .child(
                                                    img(path.clone())
                                                        .size_full()
                                                        .object_fit(ObjectFit::Cover),
                                                )
                                        }))
                                })
                                .collect()
                        }),
                    )
                    .h_full(),
                ),
            )
    }
}

fn centered_note(text: impl Into<SharedString>) -> gpui::AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_2()
        .text_color(rgb(0x9ca3af))
        .child(text.into())
        .into_any_element()
}

fn load_gallery_paths() -> (Vec<Arc<Path>>, String) {
    // The assets live next to this crate in the research repo.
    let candidates = [
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../peek-assets"),
        PathBuf::from("apps/peek-assets"),
    ];
    for dir in candidates {
        if dir.is_dir() {
            let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|p| {
                            p.extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            paths.sort();
            let label = dir.to_string_lossy().into_owned();
            return (paths.into_iter().map(Arc::from).collect(), label);
        }
    }
    (Vec::new(), "apps/peek-assets (not found)".into())
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(900.), px(600.)), cx);

        // gpui apps do not quit when the last window closes unless told to.
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Peek (gpui)")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(PeekApp::new),
        )
        .unwrap();
        cx.activate(true);
    });
}
