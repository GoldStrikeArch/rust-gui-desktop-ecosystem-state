// Fetcher (Tauri) — SPEC-8, RCN GUI ecosystem research.
//
// Architecture (see FRICTION.md): ALL async/network logic lives in the
// webview's JS — debounce, stale protection, progress streaming, abort and
// retry are browser-idiom code against a fetch-compatible API. The Rust side
// is only: (1) tauri_plugin_http registration (reqwest behind IPC, needed
// because the tauri://localhost origin makes WKWebView CORS-block native
// fetch() to the CORS-header-less local server), (2) a config command
// exposing FETCHER_PORT, (3) a report command piping JS evidence to stdout.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    port: u16,
    selftest: bool,
}

/// SPEC-8 §5: the port comes from the FETCHER_PORT env var (default 7878).
/// The webview cannot read the environment, so Rust hands it over.
#[tauri::command]
fn get_config() -> Config {
    Config {
        port: std::env::var("FETCHER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(7878),
        selftest: std::env::var("FETCH_SELFTEST").is_ok(),
    }
}

/// Webview console → stdout pipe (self-test evidence + window.onerror).
#[tauri::command]
fn report(line: String) {
    println!("{line}");
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .invoke_handler(tauri::generate_handler![get_config, report])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
