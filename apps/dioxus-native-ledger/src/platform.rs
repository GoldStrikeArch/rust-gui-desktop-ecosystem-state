//! SPEC-10 "Ledger" — the *Blitz-only* half.
//!
//! Same public surface as `apps/dioxus-ledger/src/platform.rs` so the shared
//! `app.rs` compiles unchanged: `selftest`, `place`, `launch`,
//! `clipboard_write`, `clipboard_read`.

use dioxus::prelude::Element;
use dioxus_native::winit::dpi::{LogicalPosition, LogicalSize};
use dioxus_native::winit::window::WindowLevel;
use dioxus_native::{Config, WindowAttributes};

pub fn selftest() -> bool {
    std::env::var_os("LEDGER_SELFTEST").is_some()
}

/// Verification-only: fixed geometry, so screenshots and synthetic input are
/// reproducible on this shared desktop.
pub fn place() -> bool {
    selftest() || on_top() || std::env::var_os("LEDGER_PLACE").is_some()
}

/// Verification-only: float above the *other* agents' windows on this shared
/// desktop. Kept as a separate knob from `place`: a raised window level puts
/// the CGWindow layer above 0, which hides it from `scripts/window-count.swift`
/// and from `tools/synth/synth bounds`.
fn on_top() -> bool {
    std::env::var_os("LEDGER_TOPMOST").is_some()
}

/// `main.rs` calls exactly this.
pub fn launch(root: fn() -> Element) {
    let mut attrs = WindowAttributes::default()
        .with_title("Ledger (dioxus-native)")
        .with_surface_size(LogicalSize::new(820.0, 560.0))
        .with_resizable(true);
    if place() {
        attrs = attrs.with_position(LogicalPosition::new(230.0, 300.0));
    }
    if on_top() {
        attrs = attrs.with_window_level(WindowLevel::AlwaysOnTop);
    }
    dioxus_native::launch_cfg(
        root,
        vec![],
        vec![Box::new(Config::new().with_window_attributes(attrs))],
    )
}

/// Put text on the system pasteboard (row copy as TSV).
///
/// blitz-shell's `BlitzShellProvider` already owns an `arboard` clipboard for
/// `<input>` copy/paste, but exposes it only through the private
/// `ShellProvider` context, so this is the same direct `arboard` call the
/// webview build makes.
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
