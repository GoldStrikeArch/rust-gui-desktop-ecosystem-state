// VERIFICATION HOOKS — not production code. Env-gated automation used by the
// research harness to exercise the app without desktop interaction:
//
//   PEEK_AUTO=1          auto-invoke camera + mic Start ~1.5 s after launch
//   PEEK_SECS=<n>        quit after n seconds, printing a PEEK_STATS line
//   PEEK_LOG=1           print a PEEK_TICK counters line once per second
//   PEEK_SNAPSHOT=<png>  save a window snapshot 2 s before quitting
//                        (requires PEEK_SECS)

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

use crate::{CamStats, MainWindow};

pub fn install(ui: &MainWindow, cam: Arc<CamStats>) -> Vec<Timer> {
    let mut timers = Vec::new();

    if std::env::var("PEEK_AUTO").is_ok() {
        let ui_weak = ui.as_weak();
        Timer::single_shot(Duration::from_millis(1500), move || {
            if let Some(ui) = ui_weak.upgrade() {
                eprintln!("[peek][verify] auto-starting camera + mic");
                ui.invoke_toggle_camera();
                ui.invoke_toggle_mic();
            }
        });
        let ui_weak = ui.as_weak();
        Timer::single_shot(Duration::from_millis(4000), move || {
            if let Some(ui) = ui_weak.upgrade() {
                eprintln!("[peek][verify] auto-invoking beep");
                ui.invoke_beep();
            }
        });
    }

    if std::env::var("PEEK_LOG").is_ok() {
        let ui_weak = ui.as_weak();
        let cam = cam.clone();
        let t = Timer::default();
        let mut tick = 0u32;
        t.start(TimerMode::Repeated, Duration::from_secs(1), move || {
            tick += 1;
            let (thumbs, mic_level) = ui_weak
                .upgrade()
                .map(|ui| (ui.get_thumbs_loaded(), ui.get_mic_level()))
                .unwrap_or((-1, -1.0));
            println!(
                "PEEK_TICK t={} captured={} delivered={} presented={} thumbs={} mic_level={:.3}",
                tick,
                cam.captured.load(Ordering::Relaxed),
                cam.delivered.load(Ordering::Relaxed),
                cam.presented.load(Ordering::Relaxed),
                thumbs,
                mic_level,
            );
        });
        timers.push(t);
    }

    if let Ok(secs) = std::env::var("PEEK_SECS") {
        let secs: u64 = secs.parse().unwrap_or(12);

        if let Ok(path) = std::env::var("PEEK_SNAPSHOT") {
            let ui_weak = ui.as_weak();
            Timer::single_shot(Duration::from_secs(secs.saturating_sub(2)), move || {
                let Some(ui) = ui_weak.upgrade() else { return };
                match ui.window().take_snapshot() {
                    Ok(pb) => {
                        let (w, h) = (pb.width(), pb.height());
                        match image::save_buffer(
                            &path,
                            pb.as_bytes(),
                            w,
                            h,
                            image::ColorType::Rgba8,
                        ) {
                            Ok(()) => eprintln!("[peek][verify] snapshot saved to {path} ({w}x{h})"),
                            Err(e) => eprintln!("[peek][verify] snapshot save failed: {e}"),
                        }
                    }
                    Err(e) => eprintln!("[peek][verify] take_snapshot failed: {e}"),
                }
            });
        }

        let ui_weak = ui.as_weak();
        Timer::single_shot(Duration::from_secs(secs), move || {
            let (fps_p, fps_c, thumbs) = ui_weak
                .upgrade()
                .map(|ui| (ui.get_presented_fps(), ui.get_capture_fps(), ui.get_thumbs_loaded()))
                .unwrap_or((-1.0, -1.0, -1));
            println!(
                "PEEK_STATS captured={} delivered={} presented={} presented_fps={:.1} capture_fps={:.1} thumbs={}",
                cam.captured.load(Ordering::Relaxed),
                cam.delivered.load(Ordering::Relaxed),
                cam.presented.load(Ordering::Relaxed),
                fps_p,
                fps_c,
                thumbs,
            );
            let _ = slint::quit_event_loop();
        });
    }

    timers
}
