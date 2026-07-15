// Board (Tauri) — RCN GUI ecosystem research, iteration 2 (SPEC-3).
//
// Design choice (see FRICTION.md): all board state lives in the webview (JS).
// Drag-and-drop is inherently frontend-local and latency-sensitive — a Rust
// round-trip per dragover would add IPC hops for zero benefit, and iteration 1
// (tauri-app) already exercised the Rust-owned-state-over-IPC architecture.
// Consequently the entire Rust side of this app is the window shell below.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
