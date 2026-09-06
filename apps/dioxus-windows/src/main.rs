//! SPEC-9 "Windows" — Dioxus 0.7.9 desktop (wry/tao), plain cargo.
//! All UI/state lives in `app.rs` (portable); every OS call in `platform.rs`.
mod app;
mod platform;

fn main() {
    platform::launch(app::App)
}
