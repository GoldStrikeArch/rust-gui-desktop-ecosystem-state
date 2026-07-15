//! Verification hooks — NOT production UI. With PEEK_AUTOTEST=1 the app runs a
//! scripted self-test that exercises every SPEC-6 capability and prints
//! evidence lines to stdout (captured in launch.log). Kept in its own file so
//! production vs verification LoC can be counted separately.

use dioxus::prelude::*;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::{Tab, CAM, MIC};

async fn sleep_ms(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

/// `return [__jsStats.fps, __jsStats.frames]`-style samplers.
async fn sample2(js: &str) -> (f64, f64) {
    match document::eval(js).join::<Vec<f64>>().await {
        Ok(v) if v.len() == 2 => (v[0], v[1]),
        _ => (-1.0, -1.0),
    }
}

pub fn maybe_start(mut tab: Signal<Tab>) {
    use_future(move || async move {
        if std::env::var_os("PEEK_AUTOTEST").is_none() {
            return;
        }
        let t0 = Instant::now();
        let log = move |m: String| println!("peek-autotest[{:5.1}s] {}", t0.elapsed().as_secs_f64(), m);

        sleep_ms(2500).await;
        // Iteration-3 lesson: idle+unfocused wry windows defer paints. Keep our
        // own window focused for the whole measurement (self-scoped action).
        dioxus::desktop::window().window.set_focus();
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        log(format!("BEGIN (window focused) wallclock={epoch:.1}"));

        // Phase 1 — Rust nokhwa → JPEG → long-poll path, on a fresh device
        // (run 2 showed AVCaptureDevice's config lock stays with WebKit's GPU
        // process for a while after track.stop(), so Rust goes first).
        log("phase rust_camera start".into());
        crate::start_rust_cam();
        document::eval(crate::JS_RUST_PUMP);
        for i in 0..14 {
            sleep_ms(1000).await;
            let (fps, frames) = sample2("return [window.__rustStats.fps, window.__rustStats.frames];").await;
            let cap = CAM.capture_fps_x10.load(Ordering::Relaxed) as f64 / 10.0;
            let enc = CAM.encode_ms_x10.load(Ordering::Relaxed) as f64 / 10.0;
            let status = CAM.status.lock().unwrap().clone();
            log(format!(
                "rust_cam t={i:2} presented_fps={fps:.1} frames_total={frames:.0} capture_fps={cap:.1} encode_ms={enc:.1} status='{status}'"
            ));
        }
        document::eval(crate::JS_RUST_STOP);
        crate::stop_rust_cam();
        sleep_ms(2000).await;

        // Phase 2 — JS getUserMedia path, 12 s of presented-fps samples.
        log("phase js_camera start".into());
        document::eval(crate::JS_CAM_START);
        for i in 0..12 {
            sleep_ms(1000).await;
            let (fps, frames) = sample2("return [window.__jsStats.fps, window.__jsStats.frames];").await;
            let err = document::eval("return window.__jsStats.err;")
                .join::<String>()
                .await
                .unwrap_or_default();
            log(format!("js_cam t={i:2} presented_fps={fps:.1} frames_total={frames:.0} err='{err}'"));
        }
        document::eval(crate::JS_CAM_STOP);
        sleep_ms(700).await;

        // Phase 3 — mic meter, 6 s of RMS samples.
        tab.set(Tab::Audio);
        log("phase mic start".into());
        crate::start_mic();
        for i in 0..6 {
            sleep_ms(1000).await;
            let rms = MIC.rms_x1000.load(Ordering::Relaxed) as f64 / 1000.0;
            let peak = MIC.peak_x1000.load(Ordering::Relaxed) as f64 / 1000.0;
            let status = MIC.status.lock().unwrap().clone();
            log(format!("mic t={i} rms={rms:.4} peak={peak:.4} status='{status}'"));
        }
        crate::stop_mic();
        sleep_ms(500).await;

        // Phase 4 — beep twice.
        log("phase beep start".into());
        crate::play_beep();
        sleep_ms(900).await;
        log(format!("beep status='{}'", crate::AUDIO_STATUS.lock().unwrap().clone()));
        crate::play_beep();
        sleep_ms(900).await;

        // Phase 5 — gallery: show the tab, let lazy-loading run, count loads.
        tab.set(Tab::Gallery);
        log("phase gallery start".into());
        sleep_ms(3000).await;
        // Scroll to the bottom to force every lazy image to load.
        document::eval("const g = document.getElementById('gallery'); g.scrollTop = g.scrollHeight;");
        sleep_ms(4000).await;
        let (total, loaded) = sample2(
            "const im = [...document.querySelectorAll('#gallery img')]; \
             return [im.length, im.filter(i => i.complete && i.naturalWidth > 0).length];",
        )
        .await;
        log(format!("gallery imgs_total={total:.0} imgs_loaded={loaded:.0}"));

        log("DONE".into());
    });
}
