//! "Peek" — media & hardware test (SPEC-6), floem git @ 778bb5f2.
//!
//! Architecture notes (research-relevant):
//! - Camera pipeline: nokhwa (AVFoundation) blocks in `Camera::frame()` on a
//!   dedicated std thread, decodes YUYV→RGBA there, swaps the frame into a
//!   `Mutex<Option<Frame>>` slot and pokes an `ExtSendTrigger`. On the UI
//!   thread an `Effect` bumps `frame_rev`; the preview is a `canvas` whose
//!   paint closure reads `frame_rev` (signal-tracked repaint) and calls
//!   `Renderer::draw_img` with the RAW RGBA pixels — floem's `img` view only
//!   accepts encoded bytes, so raw-buffer presentation requires the
//!   lower-level renderer API (+ a direct `floem_renderer` dependency for
//!   the un-re-exported `Img` type).
//! - Texture upload cost (vger renderer, HEADLINE): images live in a color
//!   ATLAS keyed by CONTENT HASH. A video stream is a new hash every frame,
//!   each packing a new atlas region; the atlas only recovers ("clear all")
//!   when usage crosses 70%, but a region that FAILS to pack is dropped
//!   silently and does NOT trigger cleanup — so any frame larger than
//!   ~1/3 of the atlas dimension wedges image drawing PERMANENTLY after a
//!   few frames (1080p and even 640x360 previews go black). The workaround
//!   is to downscale every frame to ≤320x180 on the camera thread so the
//!   70% clear fires before the packer fragments. Full math in FRICTION.md.
//! - Mic: cpal input stream on its own thread (`Stream` is `!Send`), RMS in
//!   an `AtomicU32`; the VU bar polls at 20 Hz via an `exec_after` chain.
//! - Gallery: 200 JPEGs decoded on a hand-rolled 8-thread pool (floem has no
//!   executor), results funneled through a queue + `ExtSendTrigger`; each
//!   thumbnail is a small canvas drawing its pre-built `ImageBrush`.
//! - `PEEK_SELFTEST=1`: auto-starts camera + mic, beeps at t≈2 s, appends a
//!   1 Hz status line to selftest.log (same field layout as iced-peek).
//!   `PEEK_FAKE_CAMERA=1`: synthetic 30 fps 1080p source (no TCC/nokhwa).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use floem::Application;
use floem::action::exec_after;
use floem::ext_event::{ExtSendTrigger, create_ext_action, create_trigger, register_ext_trigger};
use floem::kurbo::{Point, Rect, Size};
use floem::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};
use floem::prelude::*;
use floem::reactive::{Effect, Scope};
use floem::window::WindowConfig;
use floem_renderer::Img;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use nokhwa::Camera;
use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};

const THUMB_W: u32 = 100;
const THUMB_H: u32 = 75;

const PANEL_BG: Color = Color::from_rgb8(0xf4, 0xf4, 0xf6);
const BORDER: Color = Color::from_rgb8(0xc9, 0xc9, 0xd2);
const GREEN: Color = Color::from_rgb8(0x1d, 0x7a, 0x33);
const RED: Color = Color::from_rgb8(0xc2, 0x33, 0x2e);
const ACCENT: Color = Color::from_rgb8(0x3b, 0x6f, 0xe0);
const TEXT_DIM: Color = Color::from_rgb8(0x70, 0x70, 0x7a);

// ---------------------------------------------------------------------------
// Shared (cross-thread) state
// ---------------------------------------------------------------------------

struct Frame {
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
    seq: u64,
}

#[derive(Default)]
struct CamShared {
    frame: Mutex<Option<Frame>>,
    stop: AtomicBool,
    captured: AtomicU64,
    error: Mutex<Option<String>>,
    format: Mutex<Option<String>>,
}

#[derive(Default)]
struct MicShared {
    rms: AtomicU32,
    callbacks: AtomicU64,
    error: Mutex<Option<String>>,
    desc: Mutex<Option<String>>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Perm {
    Unknown,
    Prompting,
    Granted,
    Denied,
}

fn main() {
    Application::new()
        .window(
            |window_id| {
                // [verify] PEEK_SHOT=path: window-scoped screenshot after 5 s.
                if let Ok(path) = std::env::var("PEEK_SHOT") {
                    exec_after(Duration::from_secs(5), move |_| take_screenshot(window_id, &path));
                }
                app_view()
            },
            Some(
                WindowConfig::default()
                    .title("Peek (floem)")
                    .size(Size::new(900.0, 600.0)),
            ),
        )
        .run();
}

// ---------------------------------------------------------------------------
// App state (Copy signals) + view
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Peek {
    // camera
    cam_running: RwSignal<bool>,
    perm: RwSignal<Perm>,
    frame_rev: RwSignal<u64>,
    captured_fps: RwSignal<u64>,
    presented_fps: RwSignal<u64>,
    cam_status: RwSignal<String>,
    // audio
    mic_running: RwSignal<bool>,
    vu: RwSignal<f32>,   // 0..1 (dBFS mapped)
    peak: RwSignal<f32>, // slow-decay peak
    mic_status: RwSignal<String>,
    beeps: RwSignal<u32>,
    beep_status: RwSignal<String>,
    // gallery
    thumbs_loaded: RwSignal<usize>,
    thumbs_total: RwSignal<usize>,
    gallery_ms: RwSignal<Option<u64>>,
}

fn app_view() -> impl IntoView {
    let peek = Peek {
        cam_running: RwSignal::new(false),
        perm: RwSignal::new(if nokhwa::nokhwa_check() { Perm::Granted } else { Perm::Unknown }),
        frame_rev: RwSignal::new(0),
        captured_fps: RwSignal::new(0),
        presented_fps: RwSignal::new(0),
        cam_status: RwSignal::new(String::from("stopped")),
        mic_running: RwSignal::new(false),
        vu: RwSignal::new(0.0),
        peak: RwSignal::new(0.0),
        mic_status: RwSignal::new(String::from("stopped")),
        beeps: RwSignal::new(0),
        beep_status: RwSignal::new(String::new()),
        thumbs_loaded: RwSignal::new(0),
        thumbs_total: RwSignal::new(0),
        gallery_ms: RwSignal::new(None),
    };

    let cam = Arc::new(CamShared::default());
    let mic = Arc::new(MicShared::default());
    let mic_stop: Arc<Mutex<Option<std::sync::mpsc::Sender<()>>>> = Arc::new(Mutex::new(None));
    let presented = Arc::new(AtomicU64::new(0));

    // Camera frames -> UI: trigger pokes, effect bumps frame_rev.
    let frame_trigger = create_trigger();
    Effect::new(move |_| {
        frame_trigger.track();
        peek.frame_rev.update(|r| *r += 1);
    });

    // Camera permission callback flag (set from an arbitrary thread).
    let perm_flag = Arc::new(AtomicI8::new(0));
    let perm_trigger = create_trigger();
    {
        let perm_flag = perm_flag.clone();
        let cam = cam.clone();
        Effect::new(move |_| {
            perm_trigger.track();
            match perm_flag.load(Ordering::Relaxed) {
                1 => {
                    peek.perm.set(Perm::Granted);
                    if peek.cam_running.get_untracked() {
                        spawn_camera(cam.clone(), frame_trigger);
                    }
                }
                -1 => {
                    peek.perm.set(Perm::Denied);
                    peek.cam_running.set(false);
                    peek.cam_status.set(String::from("camera permission denied"));
                }
                _ => {}
            }
        });
    }

    let start_camera = {
        let cam = cam.clone();
        move || {
            if peek.cam_running.get_untracked() {
                return;
            }
            cam.stop.store(false, Ordering::Relaxed);
            *cam.error.lock().unwrap() = None;
            peek.cam_running.set(true);
            peek.cam_status.set(String::from("starting…"));

            if std::env::var_os("PEEK_FAKE_CAMERA").is_some() {
                spawn_fake_camera(cam.clone(), frame_trigger);
            } else if nokhwa::nokhwa_check() {
                peek.perm.set(Perm::Granted);
                spawn_camera(cam.clone(), frame_trigger);
            } else {
                // Triggers the TCC prompt; callback may fire on any thread.
                peek.perm.set(Perm::Prompting);
                let perm_flag = perm_flag.clone();
                nokhwa::nokhwa_initialize(move |granted| {
                    perm_flag.store(if granted { 1 } else { -1 }, Ordering::Relaxed);
                    register_ext_trigger(perm_trigger);
                });
            }
        }
    };

    let stop_camera = {
        let cam = cam.clone();
        move || {
            cam.stop.store(true, Ordering::Relaxed);
            peek.cam_running.set(false);
            peek.cam_status.set(String::from("stopped"));
        }
    };

    let start_mic = {
        let mic = mic.clone();
        let mic_stop = mic_stop.clone();
        move || {
            if peek.mic_running.get_untracked() {
                return;
            }
            *mic.error.lock().unwrap() = None;
            mic.callbacks.store(0, Ordering::Relaxed);
            let (tx, rx) = std::sync::mpsc::channel();
            *mic_stop.lock().unwrap() = Some(tx);
            let shared = mic.clone();
            std::thread::spawn(move || mic_thread(shared, rx));
            peek.mic_running.set(true);
            mic_poll(peek, mic.clone());
        }
    };

    let stop_mic = {
        let mic_stop = mic_stop.clone();
        move || {
            *mic_stop.lock().unwrap() = None; // sender drop stops the thread
            peek.mic_running.set(false);
            peek.vu.set(0.0);
        }
    };

    let beep = move || {
        let send = create_ext_action(Scope::new(), move |result: Result<(), String>| {
            match result {
                Ok(()) => {
                    peek.beeps.update(|b| *b += 1);
                    peek.beep_status.set(String::from("beep ok"));
                }
                Err(error) => peek.beep_status.set(format!("beep failed: {error}")),
            }
        });
        std::thread::spawn(move || send(play_beep()));
    };

    // Gallery decode kickoff (async: 8 worker threads + trigger funnel).
    let thumbs = start_gallery(peek);

    // 1 Hz status/fps tick (+ selftest log line when enabled).
    let selftest: Option<Arc<PathBuf>> = std::env::var_os("PEEK_SELFTEST").map(|_| {
        Arc::new(
            std::env::var_os("PEEK_SELFTEST_LOG")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("selftest.log")),
        )
    });
    {
        let cam = cam.clone();
        let mic = mic.clone();
        let presented = presented.clone();
        let ticks = RwSignal::new(0u64);
        let last_cap = RwSignal::new(0u64);
        let last_pres = RwSignal::new(0u64);
        fn tick_loop(
            peek: Peek,
            cam: Arc<CamShared>,
            mic: Arc<MicShared>,
            presented: Arc<AtomicU64>,
            ticks: RwSignal<u64>,
            last_cap: RwSignal<u64>,
            last_pres: RwSignal<u64>,
            selftest: Option<Arc<PathBuf>>,
        ) {
            exec_after(Duration::from_secs(1), move |_| {
                ticks.update(|t| *t += 1);
                let cap = cam.captured.load(Ordering::Relaxed);
                let pres = presented.load(Ordering::Relaxed);
                peek.captured_fps.set(cap - last_cap.get_untracked());
                peek.presented_fps.set(pres - last_pres.get_untracked());
                last_cap.set(cap);
                last_pres.set(pres);

                if peek.cam_running.get_untracked() {
                    let error = cam.error.lock().unwrap().clone();
                    let format = cam.format.lock().unwrap().clone();
                    peek.cam_status.set(match (error, format) {
                        (Some(e), _) => format!("error: {e}"),
                        (None, Some(f)) => f,
                        _ => String::from("starting…"),
                    });
                }

                if let Some(path) = &selftest {
                    append_selftest_line(
                        path, peek, &cam, &mic, ticks.get_untracked(),
                    );
                }
                tick_loop(peek, cam, mic, presented, ticks, last_cap, last_pres, selftest);
            });
        }
        tick_loop(peek, cam, mic, presented, ticks, last_cap, last_pres, selftest.clone());
    }

    // Self-test boot: auto-start camera + mic, one quiet beep at t≈2 s.
    if selftest.is_some() {
        let start_camera = start_camera.clone();
        let start_mic = start_mic.clone();
        exec_after(Duration::from_millis(400), move |_| {
            start_camera();
            start_mic();
        });
        exec_after(Duration::from_secs(2), move |_| beep());
    }

    Stack::vertical((
        camera_section(peek, cam.clone(), presented, start_camera, stop_camera),
        audio_section(peek, start_mic, stop_mic, beep),
        gallery_section(peek, thumbs),
    ))
    .style(|s| s.flex_col().gap(10.0).padding(12.0).size_full())
}

// ---------------------------------------------------------------------------
// Camera section
// ---------------------------------------------------------------------------

fn camera_section(
    peek: Peek,
    cam: Arc<CamShared>,
    presented: Arc<AtomicU64>,
    start_camera: impl Fn() + Clone + 'static,
    stop_camera: impl Fn() + Clone + 'static,
) -> impl IntoView {
    let header = Stack::horizontal((
        Label::new("Camera").style(|s| s.font_size(15.0).color(Color::from_rgb8(0x20,0x20,0x28))),
        Button::new(Label::derived(move || {
            if peek.cam_running.get() { "Stop" } else { "Start" }
        }))
        .action(move || {
            if peek.cam_running.get_untracked() {
                stop_camera();
            } else {
                start_camera();
            }
        }),
        Label::derived(move || {
            format!(
                "captured {} fps · presented {} fps",
                peek.captured_fps.get(),
                peek.presented_fps.get()
            )
        })
        .style(|s| s.font_size(12.0).color(TEXT_DIM)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Label::derived(move || format!("{} · perm {:?}", peek.cam_status.get(), peek.perm.get()))
            .style(|s| s.font_size(12.0).color(TEXT_DIM)),
    ))
    .style(|s| s.gap(10.0).items_center().width_full());

    // The preview: a signal-tracked canvas presenting the latest RGBA frame
    // through Renderer::draw_img (content-hash keyed → full re-upload per
    // frame in the vger atlas).
    let preview = canvas(move |cx, size| {
        peek.frame_rev.track();
        let slot = cam.frame.lock().unwrap();
        let Some(frame) = slot.as_ref() else {
            cx.fill(&Rect::ZERO.with_size(size).to_rounded_rect(6.0), Color::BLACK.with_alpha(0.85), 0.0);
            return;
        };

        let brush = ImageBrush::new(ImageData {
            data: Blob::new(frame.pixels.clone()),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: frame.width,
            height: frame.height,
        })
        .with_quality(ImageQuality::Low);

        // Aspect-fit destination rect.
        let hash_bytes: &[u8] = &frame.seq.to_le_bytes();
        let (fw, fh) = (frame.width as f64, frame.height as f64);
        let scale = (size.width / fw).min(size.height / fh);
        let (dw, dh) = (fw * scale, fh * scale);
        let origin = Point::new((size.width - dw) / 2.0, (size.height - dh) / 2.0);

        cx.fill(&Rect::ZERO.with_size(size).to_rounded_rect(6.0), Color::BLACK.with_alpha(0.85), 0.0);
        cx.draw_img(
            Img { img: brush, hash: hash_bytes },
            Rect::from_origin_size(origin, Size::new(dw, dh)),
        );
        presented.fetch_add(1, Ordering::Relaxed);
    })
    .style(|s| s.width_full().height(220.0));

    Stack::vertical((header, preview)).style(section_style)
}

fn spawn_camera(shared: Arc<CamShared>, trigger: ExtSendTrigger) {
    std::thread::spawn(move || camera_thread(shared, trigger));
}

fn camera_thread(shared: Arc<CamShared>, trigger: ExtSendTrigger) {
    let set_error = |msg: String| {
        *shared.error.lock().unwrap() = Some(msg);
    };

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

    let mut seq = 0u64;
    while !shared.stop.load(Ordering::Relaxed) {
        let buffer = match camera.frame() {
            Ok(b) => b,
            Err(e) => {
                set_error(format!("frame failed: {e}"));
                break;
            }
        };
        match buffer.decode_image::<RgbAFormat>() {
            Ok(img) => {
                let (w, h) = img.dimensions();
                // vger-atlas workaround: publish at most 320x180 per frame.
                let (width, height, pixels) = downscale_to_fit(&img.into_raw(), w, h, 320, 180);
                seq += 1;
                *shared.frame.lock().unwrap() = Some(Frame {
                    width,
                    height,
                    pixels: Arc::new(pixels),
                    seq,
                });
                shared.captured.fetch_add(1, Ordering::Relaxed);
                register_ext_trigger(trigger);
            }
            Err(e) => {
                set_error(format!("decode failed: {e}"));
                break;
            }
        }
    }
    let _ = camera.stop_stream();
}

/// Nearest-neighbor downscale to fit within `max_w`×`max_h` (camera-thread
/// side). Required workaround for the vger atlas wedge described in the
/// module docs — larger per-frame uploads permanently kill image drawing.
fn downscale_to_fit(
    pixels: &[u8],
    width: u32,
    height: u32,
    max_w: u32,
    max_h: u32,
) -> (u32, u32, Vec<u8>) {
    if width <= max_w && height <= max_h {
        return (width, height, pixels.to_vec());
    }
    let scale = (max_w as f64 / width as f64).min(max_h as f64 / height as f64);
    let (dw, dh) = (
        ((width as f64 * scale) as u32).max(1),
        ((height as f64 * scale) as u32).max(1),
    );
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        let sy = (y as u64 * height as u64 / dh as u64) as u32;
        for x in 0..dw {
            let sx = (x as u64 * width as u64 / dw as u64) as u32;
            let src = ((sy * width + sx) * 4) as usize;
            let dst = ((y * dw + x) * 4) as usize;
            out[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
        }
    }
    (dw, dh, out)
}

/// [verify] Synthetic 30 fps 1080p RGBA source (no TCC, no nokhwa): isolates
/// the frame→texture path; scrolling gradient so motion is visible.
fn spawn_fake_camera(shared: Arc<CamShared>, trigger: ExtSendTrigger) {
    std::thread::spawn(move || {
        let (width, height) = std::env::var("PEEK_FAKE_SIZE").ok().and_then(|v| { let mut it = v.split("x"); Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?)) }).unwrap_or((1920u32, 1080u32));
        *shared.format.lock().unwrap() = Some(format!("{width}x{height} @ 30 fps SYNTHETIC"));
        let mut t: u32 = 0;
        while !shared.stop.load(Ordering::Relaxed) {
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            for y in 0..height {
                let row = (y * width * 4) as usize;
                for x in 0..width {
                    let i = row + (x * 4) as usize;
                    pixels[i] = ((x + t * 4) % 256) as u8;
                    pixels[i + 1] = ((y + t * 2) % 256) as u8;
                    pixels[i + 2] = 128;
                    pixels[i + 3] = 255;
                }
            }
            t = t.wrapping_add(1);
            let (dw, dh, pixels) = downscale_to_fit(&pixels, width, height, 320, 180);
            *shared.frame.lock().unwrap() = Some(Frame {
                width: dw,
                height: dh,
                pixels: Arc::new(pixels),
                seq: u64::from(t),
            });
            shared.captured.fetch_add(1, Ordering::Relaxed);
            register_ext_trigger(trigger);
            std::thread::sleep(Duration::from_millis(33));
        }
    });
}

// ---------------------------------------------------------------------------
// Audio section
// ---------------------------------------------------------------------------

fn audio_section(
    peek: Peek,
    start_mic: impl Fn() + Clone + 'static,
    stop_mic: impl Fn() + Clone + 'static,
    beep: impl Fn() + Clone + 'static,
) -> impl IntoView {
    let vu_bar = Empty::new()
        .style(move |s| {
            s.height(14.0)
                .width_pct(f64::from(peek.vu.get()) * 100.0)
                .background(GREEN)
                .border_radius(3.0)
        })
        .container()
        .style(|s| {
            s.height(14.0)
                .flex_grow(1.0)
                .background(BORDER.with_alpha(0.5))
                .border_radius(3.0)
        });

    let peak_bar = Empty::new()
        .style(move |s| {
            s.height(4.0)
                .width_pct(f64::from(peek.peak.get()) * 100.0)
                .background(ACCENT)
                .border_radius(2.0)
        })
        .container()
        .style(|s| {
            s.height(4.0)
                .flex_grow(1.0)
                .background(BORDER.with_alpha(0.3))
                .border_radius(2.0)
        });

    Stack::vertical((
        Stack::horizontal((
            Label::new("Audio").style(|s| s.font_size(15.0).color(Color::from_rgb8(0x20,0x20,0x28))),
            Button::new(Label::derived(move || {
                if peek.mic_running.get() { "Stop mic" } else { "Start mic" }
            }))
            .action(move || {
                if peek.mic_running.get_untracked() {
                    stop_mic();
                } else {
                    start_mic();
                }
            }),
            Button::new("Beep").action(move || beep()),
            Label::derived(move || {
                let beeps = peek.beeps.get();
                let status = peek.beep_status.get();
                if status.is_empty() { String::new() } else { format!("{status} (×{beeps})") }
            })
            .style(move |s| {
                let failed = peek.beep_status.with(|b| b.contains("failed"));
                s.font_size(12.0).color(if failed { RED } else { GREEN })
            }),
            Empty::new().style(|s| s.flex_grow(1.0)),
            Label::derived(move || peek.mic_status.get())
                .style(|s| s.font_size(12.0).color(TEXT_DIM)),
        ))
        .style(|s| s.gap(10.0).items_center().width_full()),
        Stack::vertical((vu_bar, peak_bar)).style(|s| s.flex_col().gap(3.0).width_full()),
    ))
    .style(section_style)
}

/// 20 Hz VU poll while the mic runs: RMS → dBFS → 0..1 bar + decaying peak.
fn mic_poll(peek: Peek, mic: Arc<MicShared>) {
    exec_after(Duration::from_millis(50), move |_| {
        if !peek.mic_running.get_untracked() {
            return;
        }
        let rms = f32::from_bits(mic.rms.load(Ordering::Relaxed));
        let db = 20.0 * rms.max(1e-6).log10(); // dBFS
        let level = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        peek.vu.set(level);
        peek.peak.update(|p| *p = (*p * 0.97).max(level));

        let callbacks = mic.callbacks.load(Ordering::Relaxed);
        let error = mic.error.lock().unwrap().clone();
        let desc = mic.desc.lock().unwrap().clone();
        peek.mic_status.set(match (error, desc) {
            (Some(e), _) => format!("error: {e}"),
            (None, Some(d)) => format!("{d} · {callbacks} callbacks"),
            _ => String::from("starting…"),
        });

        mic_poll(peek, mic.clone());
    });
}

/// Owns the !Send cpal Stream for its whole lifetime.
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
        return set_error(format!("unsupported sample format {:?}", config.sample_format()));
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

    let _ = stop.recv(); // park until the UI drops the sender
    drop(stream);
}

fn play_beep() -> Result<(), String> {
    use rodio::Source;
    use rodio::source::SineWave;

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
// Gallery
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Thumb {
    brush: ImageBrush,
    hash: Arc<Vec<u8>>,
}

fn asset_dir() -> PathBuf {
    std::env::var_os("PEEK_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../peek-assets"))
}

fn list_jpegs(dir: &PathBuf) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg")))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

/// Kick off async decoding: 8 worker threads over a shared work queue,
/// results funneled to the UI thread via a queue + ExtSendTrigger.
fn start_gallery(peek: Peek) -> std::rc::Rc<Vec<RwSignal<Option<Thumb>>>> {
    let paths = list_jpegs(&asset_dir());
    let total = paths.len();
    peek.thumbs_total.set(total);

    let slots: std::rc::Rc<Vec<RwSignal<Option<Thumb>>>> =
        std::rc::Rc::new((0..total).map(|_| RwSignal::new(None)).collect());

    let started = Instant::now();
    let results: Arc<Mutex<Vec<(usize, u32, u32, Vec<u8>)>>> = Arc::new(Mutex::new(Vec::new()));
    let trigger = create_trigger();

    // Drain effect: applies decoded thumbnails to their slot signals.
    {
        let slots = slots.clone();
        let results = results.clone();
        Effect::new(move |_| {
            trigger.track();
            let mut batch = results.lock().unwrap();
            for (index, w, h, rgba) in batch.drain(..) {
                let brush = ImageBrush::new(ImageData {
                    data: Blob::new(Arc::new(rgba)),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width: w,
                    height: h,
                });
                let hash = Arc::new(format!("thumb-{index}").into_bytes());
                slots[index].set(Some(Thumb { brush, hash }));
                peek.thumbs_loaded.update(|n| *n += 1);
            }
            if peek.thumbs_loaded.get_untracked() == total && peek.gallery_ms.get_untracked().is_none()
            {
                peek.gallery_ms.set(Some(started.elapsed().as_millis() as u64));
            }
        });
    }

    // Hand-rolled decode pool (floem has no executor / blocking pool).
    let queue: Arc<Mutex<std::collections::VecDeque<(usize, PathBuf)>>> =
        Arc::new(Mutex::new(paths.into_iter().enumerate().collect()));
    for _ in 0..8 {
        let queue = queue.clone();
        let results = results.clone();
        std::thread::spawn(move || {
            loop {
                let job = queue.lock().unwrap().pop_front();
                let Some((index, path)) = job else { break };
                if let Ok(img) = image::open(&path) {
                    let thumb = img.thumbnail(THUMB_W, THUMB_H).to_rgba8();
                    let (w, h) = thumb.dimensions();
                    results.lock().unwrap().push((index, w, h, thumb.into_raw()));
                    register_ext_trigger(trigger);
                }
            }
        });
    }

    slots
}

fn gallery_section(peek: Peek, slots: std::rc::Rc<Vec<RwSignal<Option<Thumb>>>>) -> impl IntoView {
    let header = Stack::horizontal((
        Label::new("Gallery").style(|s| s.font_size(15.0).color(Color::from_rgb8(0x20,0x20,0x28))),
        Label::derived(move || {
            let loaded = peek.thumbs_loaded.get();
            let total = peek.thumbs_total.get();
            match peek.gallery_ms.get() {
                Some(ms) => format!("{loaded}/{total} thumbnails · decoded in {ms} ms"),
                None => format!("{loaded}/{total} thumbnails…"),
            }
        })
        .style(|s| s.font_size(12.0).color(TEXT_DIM)),
    ))
    .style(|s| s.gap(10.0).items_center().width_full());

    let count = slots.len();
    let grid = dyn_stack(
        move || 0..count,
        |index| *index,
        move |index| {
            let slot = slots[index];
            canvas(move |cx, size| {
                match slot.get() {
                    Some(thumb) => cx.draw_img(
                        Img { img: thumb.brush.clone(), hash: thumb.hash.as_slice() },
                        Rect::ZERO.with_size(size),
                    ),
                    None => cx.fill(
                        &Rect::ZERO.with_size(size).to_rounded_rect(4.0),
                        BORDER.with_alpha(0.4),
                        0.0,
                    ),
                }
            })
            .style(|s| s.width(THUMB_W as f64).height(THUMB_H as f64))
        },
    )
    .style(|s| {
        s.flex_row()
            .flex_wrap(floem::taffy::style::FlexWrap::Wrap)
            .gap(6.0)
            .width_full()
    });

    Stack::vertical((
        header,
        grid.scroll()
            .style(|s| s.width_full().flex_grow(1.0).min_height(0.0)),
    ))
    .style(|s| section_style(s).flex_grow(1.0).min_height(0.0))
}

fn section_style(s: floem::style::Style) -> floem::style::Style {
    s.flex_col()
        .gap(8.0)
        .padding(10.0)
        .width_full()
        .background(PANEL_BG)
        .border(1.0)
        .border_color(BORDER)
        .border_radius(8.0)
}

// ---------------------------------------------------------------------------
// [verify] selftest log (1 Hz, same field layout as iced-peek)
// ---------------------------------------------------------------------------

fn append_selftest_line(path: &PathBuf, peek: Peek, cam: &CamShared, mic: &MicShared, ticks: u64) {
    use std::io::Write;

    let line = format!(
        "t={} cam={} perm={:?} fmt=\"{}\" cap_fps={} pres_fps={} cam_err=\"{}\" \
         mic_running={} mic_cbs={} rms={:.5} mic_err=\"{}\" beeps={} beep_err=\"{}\" \
         thumbs={}/{} thumb_ms={:?}\n",
        ticks,
        if peek.cam_running.get_untracked() { "running" } else { "stopped" },
        peek.perm.get_untracked(),
        cam.format.lock().unwrap().clone().unwrap_or_default(),
        peek.captured_fps.get_untracked(),
        peek.presented_fps.get_untracked(),
        cam.error.lock().unwrap().clone().unwrap_or_default(),
        peek.mic_running.get_untracked(),
        mic.callbacks.load(Ordering::Relaxed),
        f32::from_bits(mic.rms.load(Ordering::Relaxed)),
        mic.error.lock().unwrap().clone().unwrap_or_default(),
        peek.beeps.get_untracked(),
        peek.beep_status.with_untracked(|b| {
            if b.contains("failed") { b.clone() } else { String::new() }
        }),
        peek.thumbs_loaded.get_untracked(),
        peek.thumbs_total.get_untracked(),
        peek.gallery_ms.get_untracked(),
    );
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}


/// [verify] Window-scoped screenshot via NSWindow windowNumber (CGWindowID),
/// immune to other windows on the shared desktop (same helper as floem-babel).
fn take_screenshot(window_id: floem::window::WindowId, path: &str) {
    use floem::WindowIdExt;
    let num = window_id
        .with_window_handle(|handle| match handle.as_raw() {
            raw_window_handle::RawWindowHandle::AppKit(h) => {
                let view = h.ns_view.as_ptr() as *mut objc2::runtime::AnyObject;
                unsafe {
                    let win: *mut objc2::runtime::AnyObject = objc2::msg_send![&*view, window];
                    if win.is_null() {
                        None
                    } else {
                        let n: isize = objc2::msg_send![&*win, windowNumber];
                        Some(n as i64)
                    }
                }
            }
            _ => None,
        })
        .flatten();
    let Some(num) = num else {
        eprintln!("screenshot: FAILED: no window number");
        return;
    };
    match std::process::Command::new("screencapture")
        .args(["-x", "-o", &format!("-l{num}"), path])
        .status()
    {
        Ok(s) if s.success() => eprintln!("screenshot: saved {path}"),
        other => eprintln!("screenshot: FAILED: {other:?}"),
    }
}
