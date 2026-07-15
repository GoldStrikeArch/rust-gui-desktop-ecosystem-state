//! probe-hotkey-x11 — claim under test (ecosystem map §1.6): global-hotkey is
//! X11-only on Linux, but on X11 it works. Under Xvfb this tests the X11 half:
//! does XGrabKey-based registration succeed headlessly?
//!
//! If the runner finds xdotool it also synthesizes Ctrl+Shift+K one second in;
//! this binary polls the GlobalHotKeyEvent receiver for 5s either way (the
//! X11 backend runs its own listener thread, no winit loop needed).
//! Registration success/failure alone is the claim test; event delivery is a
//! bonus.

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use std::time::{Duration, Instant};

fn main() {
    println!("probe-hotkey-x11: creating GlobalHotKeyManager (DISPLAY={})",
        std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into()));

    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => {
            println!("RESULT: GlobalHotKeyManager::new OK");
            m
        }
        Err(e) => {
            println!("RESULT: GlobalHotKeyManager::new Err: {e}");
            return;
        }
    };

    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyK);
    match manager.register(hotkey) {
        Ok(()) => println!("RESULT: register(Ctrl+Shift+K) OK, id={}", hotkey.id()),
        Err(e) => {
            println!("RESULT: register(Ctrl+Shift+K) Err: {e}");
            return;
        }
    }

    let rx = GlobalHotKeyEvent::receiver();
    let start = Instant::now();
    let mut fired = false;
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(ev) = rx.try_recv() {
            println!("EVENT: GlobalHotKeyEvent id={} state={:?}", ev.id, ev.state);
            fired = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if !fired {
        println!("EVENT: none within 5s (no synthetic keypress available, or delivery failed)");
    }

    match manager.unregister(hotkey) {
        Ok(()) => println!("RESULT: unregister OK"),
        Err(e) => println!("RESULT: unregister Err: {e}"),
    }
}
