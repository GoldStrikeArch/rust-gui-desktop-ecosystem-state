//! SPEC-10 "Ledger" — the *desktop-only* half.
//!
//! Only three things in this app are not portable Dioxus: creating the window,
//! and reading/writing the system pasteboard. `src/app.rs` never imports
//! `dioxus::desktop`; a Dioxus-Native (Blitz) port replaces this file only.

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

pub fn selftest() -> bool {
    std::env::var_os("LEDGER_SELFTEST").is_some()
}

/// Verification-only: fixed geometry + always-on-top. This desktop is shared
/// with six sibling agents whose windows keep covering this one, and an
/// occluded dioxus window parks its whole VirtualDom/tokio loop
/// (apps/dioxus-fetch/FRICTION.md), which would deadlock a scripted run.
pub fn place() -> bool {
    selftest() || std::env::var_os("LEDGER_PLACE").is_some()
}

/// `main.rs` calls exactly this.
pub fn launch(root: fn() -> Element) {
    let mut wb = WindowBuilder::new()
        .with_title("Ledger (dioxus)")
        .with_inner_size(LogicalSize::new(820.0, 560.0))
        .with_resizable(true);
    if place() {
        wb = wb
            .with_always_on_top(true)
            .with_position(dioxus::desktop::LogicalPosition::new(230.0, 300.0));
    }
    let mut cfg = Config::new().with_window(wb);
    if place() {
        // Verification-only hardening: sibling agents on this shared desktop send
        // ⌘W to whatever is frontmost, which killed two evidence runs. In this
        // mode a close hides the window instead of ending the process.
        cfg = cfg
            .with_close_behaviour(dioxus::desktop::WindowCloseBehaviour::WindowHides)
            .with_exits_when_last_window_closes(false);
    }
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(root)
}

/// Put text on the system pasteboard (row copy as TSV).
pub fn clipboard_write(text: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| e.to_string())
}

/// Read text from the system pasteboard (row paste from TSV).
pub fn clipboard_read() -> Result<String, String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .map_err(|e| e.to_string())
}
