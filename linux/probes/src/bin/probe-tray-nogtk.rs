//! probe-tray-nogtk — claim under test (ecosystem map §1.6): on Linux,
//! tray-icon requires GTK (gtk::init + a GTK main loop on the same thread).
//!
//! This probe deliberately builds a TrayIcon WITHOUT calling gtk::init() and
//! records verbatim what tray-icon 0.24.1 does: Err? panic? GTK critical +
//! abort? The captured output/exit code is the evidence.

use tray_icon::{Icon, TrayIconBuilder};

fn main() {
    println!("probe-tray-nogtk: building TrayIcon WITHOUT gtk::init()");
    // 16x16 solid white RGBA icon, no image crate needed.
    let icon = Icon::from_rgba(vec![255u8; 16 * 16 * 4], 16, 16).expect("icon from rgba");

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("probe-tray-nogtk")
            .build()
    }));

    match outcome {
        Ok(Ok(_tray)) => println!("RESULT: TrayIcon::build returned Ok (claim NOT confirmed)"),
        Ok(Err(e)) => println!("RESULT: TrayIcon::build returned Err: {e}"),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".into());
            println!("RESULT: TrayIcon::build PANICKED: {msg}");
        }
    }
    // If the process dies before this line (GTK abort/segfault), the runner's
    // recorded exit code is the evidence instead.
    println!("probe-tray-nogtk: reached end of main");
}
