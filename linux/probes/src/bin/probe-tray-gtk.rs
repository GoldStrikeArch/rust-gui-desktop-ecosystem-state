//! probe-tray-gtk — the "correct" Linux tray recipe per tray-icon docs:
//! gtk::init() first, TrayIcon created on the GTK thread, GTK loop pumped.
//!
//! In this bare container there is NO StatusNotifier host (the runner also
//! queries DBus for org.kde.StatusNotifierWatcher to document that). What this
//! probe can prove: whether construction/registration succeeds or errors
//! without a host. What it CANNOT prove: that an icon would actually be
//! visible on a real desktop — absence of a tray host is an environment
//! limitation, not a tray-icon failure.

use std::time::{Duration, Instant};
use tray_icon::{
    menu::{Menu, MenuItem},
    Icon, TrayIconBuilder,
};

fn main() {
    match gtk::init() {
        Ok(()) => println!("gtk::init OK"),
        Err(e) => {
            println!("RESULT: gtk::init FAILED: {e}");
            return;
        }
    }

    let icon = Icon::from_rgba(vec![255u8; 16 * 16 * 4], 16, 16).expect("icon from rgba");
    let menu = Menu::new();
    menu.append(&MenuItem::new("Quit", true, None))
        .expect("append menu item");

    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("probe-tray-gtk")
        .with_menu(Box::new(menu))
        .build();

    match &tray {
        Ok(_) => println!("RESULT: TrayIcon::build returned Ok (construction succeeds even with no SNI host on the bus)"),
        Err(e) => println!("RESULT: TrayIcon::build returned Err: {e}"),
    }

    // Pump the GTK loop ~3s so any asynchronous appindicator DBus registration
    // (and its failure path) gets a chance to run and print.
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("probe-tray-gtk: pumped GTK loop for 3s, exiting cleanly");
}
