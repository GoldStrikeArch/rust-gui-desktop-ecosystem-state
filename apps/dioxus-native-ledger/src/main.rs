//! SPEC-10 "Ledger" — Dioxus Native (Blitz main), plain cargo.
//!
//! The UI/state half is SHARED, not copied: this is literally
//! `apps/dioxus-ledger/src/app.rs`, included with `#[path]`. Only
//! `platform.rs` differs from the webview build.
#[path = "../../dioxus-ledger/src/app.rs"]
mod app;
mod platform;

fn main() {
    platform::launch(app::App)
}
