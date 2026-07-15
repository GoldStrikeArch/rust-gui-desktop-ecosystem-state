// Babel (Tauri) — SPEC-5 text & i18n stress test.
//
// Rust is deliberately thin: it embeds the shared corpus and provides a
// stdout reporting channel so headless runs can verify what the webview
// actually did. ALL text work — BiDi, shaping, font fallback, emoji,
// editing — is WKWebView's.
//
// BABEL_SELFTEST=1 triggers an in-webview self-test ~3.5 s after launch
// (big-doc load + scroll timing, grapheme deletion probe) and turns on a
// caret reporter so scripted arrow-key presses can be observed on stdout.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

/// The shared corpus, embedded at compile time — not retyped.
const CORPUS: &str = include_str!("../../babel-assets/corpus.txt");

#[tauri::command]
fn get_corpus() -> &'static str {
    CORPUS
}

#[tauri::command]
fn get_flags() -> serde_json::Value {
    serde_json::json!({ "selftest": std::env::var("BABEL_SELFTEST").is_ok() })
}

/// Frontend → stdout, so headless runs can verify in-webview behavior.
#[tauri::command]
fn report(msg: String) {
    println!("[babel] {msg}");
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_corpus, get_flags, report])
        .setup(|app| {
            // Screenshot helper: SPEC-5 allows resizing so all 11 corpus lines
            // are visible in the rendering pane at once.
            if std::env::var("BABEL_TALL").is_ok() {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_size(tauri::LogicalSize::new(900.0, 780.0));
                }
            }
            if std::env::var("BABEL_SELFTEST").is_ok() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(3500));
                    if let Some(win) = handle.get_webview_window("main") {
                        let _ = win.eval("window.__BABEL_SELFTEST__ && window.__BABEL_SELFTEST__()");
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
