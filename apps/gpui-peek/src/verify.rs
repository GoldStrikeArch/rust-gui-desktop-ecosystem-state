//! Verification hooks — NOT production UI. Env-gated so an agent can exercise
//! camera/mic/beep and read measurements from stdout without scripted UI
//! clicks (macOS blocks synthetic keystrokes/clicks in this environment).
//!
//!   PEEK_AUTOSTART=camera,mic,beep  start things shortly after launch
//!   PEEK_STATS=1                    print a STATS line every 2 s
//!   PEEK_MODE=cpu                   start in CPU-upload mode (read in main.rs)

use std::time::Duration;

use gpui::Context;

use crate::{CamState, PeekApp};

pub fn install(cx: &mut Context<PeekApp>) {
    let auto = std::env::var("PEEK_AUTOSTART").unwrap_or_default();
    let wants = |what: &str| auto.split(',').any(|s| s.trim() == what);

    if wants("camera") {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1000)).await;
            this.update(cx, |app, cx| app.start_camera(cx)).ok();
        })
        .detach();
    }
    if wants("mic") {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(2500)).await;
            this.update(cx, |app, cx| app.toggle_mic(cx)).ok();
        })
        .detach();
    }
    if wants("beep") {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(5000)).await;
            this.update(cx, |app, cx| app.beep(cx)).ok();
        })
        .detach();
    }

    if std::env::var("PEEK_STATS").is_ok() {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            if this.update(cx, |app, _| print_stats(app)).is_err() {
                break;
            }
        })
        .detach();
    }
}

fn print_stats(app: &PeekApp) {
    let cam = match &app.cam_state {
        CamState::Off => "off",
        CamState::Requesting { .. } => "requesting",
        CamState::Running => "running",
        CamState::Denied(_) => "denied",
        CamState::Error(_) => "error",
    };
    let captured = app
        .camera
        .as_ref()
        .map(|c| c.shared.captured.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0);
    println!(
        "STATS t={:.0}s cam={} mode={} fps_presented={:.1} captured={} convert_ms={:.1} \
         mic={} mic_rms={:.4} mic_callbacks={} beeps={} gallery={}",
        app.started_at.elapsed().as_secs_f32(),
        cam,
        app.mode.label(),
        app.presented_fps(),
        captured,
        app.cpu_convert_ms,
        if app.mic_stream.is_some() { "on" } else { "off" },
        app.mic_shared.rms(),
        app.mic_shared
            .callbacks
            .load(std::sync::atomic::Ordering::Relaxed),
        app.beeper.beeps,
        app.gallery.len(),
    );
}
