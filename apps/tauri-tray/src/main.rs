// Tray Notes (Tauri) — SPEC-4 OS shell integration test.
//
// Division of labor (see FRICTION.md):
// - Rust side (no capability/ACL cost): tray icon + tray menu, native menubar,
//   global shortcut, notification, clipboard image read/write, file drop,
//   close-to-tray interception, second window, theme-change events.
// - JS side (needs capability permissions): file Open/Save dialogs
//   (window.__TAURI__.dialog) — deliberately routed through the webview to
//   measure the ACL wiring cost of one plugin used from JS.
//
// TRAY_SELFTEST=1 runs a headless self-test ~4 s after launch that exercises
// clipboard-image paste, the About window, close-to-tray, the show/hide
// toggle (same code path as the global shortcut) and save+notification,
// printing results to stdout.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, DragDropEvent, Emitter, Manager, WebviewUrl, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

const GLOBAL_SHORTCUT: &str = "super+shift+9"; // Cmd+Shift+9 on macOS

// ---------------------------------------------------------------- commands

/// Writes the note and fires the "Note saved" system notification.
/// Plain std::fs — Rust commands need no fs plugin and no permissions.
#[tauri::command]
fn write_note(app: AppHandle, path: String, text: String) -> Result<(), String> {
    std::fs::write(&path, &text).map_err(|e| e.to_string())?;
    let notified = app
        .notification()
        .builder()
        .title("Note saved")
        .body(&path)
        .show();
    println!("[tray] saved {path} ({} bytes); notification result: {notified:?}", text.len());
    Ok(())
}

#[tauri::command]
fn read_note(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Reads an image off the clipboard (clipboard-manager → arboard) and returns
/// it as raw bytes: [width: u32 LE][height: u32 LE][RGBA…]. Returning
/// `tauri::ipc::Response` avoids JSON-encoding megabytes of pixels.
/// async so it runs on the async runtime, not the main thread (the plugin
/// documents main-thread clipboard reads as a deadlock risk on Linux).
#[tauri::command]
async fn paste_image(app: AppHandle) -> Result<tauri::ipc::Response, String> {
    let img = app.clipboard().read_image().map_err(|e| e.to_string())?;
    let rgba = img.rgba();
    let mut buf = Vec::with_capacity(8 + rgba.len());
    buf.extend_from_slice(&img.width().to_le_bytes());
    buf.extend_from_slice(&img.height().to_le_bytes());
    buf.extend_from_slice(rgba);
    println!("[tray] clipboard image read: {}x{}", img.width(), img.height());
    Ok(tauri::ipc::Response::new(buf))
}

/// Frontend → stdout, so headless runs can verify in-webview behavior.
#[tauri::command]
fn report(msg: String) {
    println!("[tray] {msg}");
}

// ------------------------------------------------------------ window logic

fn toggle_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
            println!("[tray] main window hidden");
        } else {
            let _ = w.show();
            let _ = w.set_focus();
            println!("[tray] main window shown");
        }
    }
}

fn open_about(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("about") {
        let _ = w.set_focus();
        return;
    }
    let built = tauri::WebviewWindowBuilder::new(app, "about", WebviewUrl::App("about.html".into()))
        .title("About Tray Notes")
        .inner_size(340.0, 230.0)
        .resizable(false)
        .build();
    println!("[tray] about window opened ok={}", built.is_ok());
}

// ------------------------------------------------------------------- menus

/// Native menubar (macOS: real top menu bar; muda underneath).
/// macOS convention: ⌘Q lives on the app menu (PredefinedMenuItem::quit);
/// File keeps a plain "Quit" item per spec, without a duplicate accelerator.
fn build_menubar(app: &tauri::App) -> tauri::Result<()> {
    let about = MenuItem::with_id(app, "about", "About Tray Notes", true, None::<&str>)?;
    let app_menu = Submenu::with_items(
        app,
        "Tray Notes",
        true,
        &[
            &about,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let new_item = MenuItem::with_id(app, "new", "New", true, Some("CmdOrCtrl+N"))?;
    let open_item = MenuItem::with_id(app, "open", "Open…", true, Some("CmdOrCtrl+O"))?;
    let save_item = MenuItem::with_id(app, "save", "Save…", true, Some("CmdOrCtrl+S"))?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &new_item,
            &open_item,
            &save_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;
    // Standard clipboard roles: predefined items map to the native selectors,
    // which is what makes ⌘C/⌘V/⌘A reach the WKWebView on macOS at all.
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let menu = Menu::with_items(app, &[&app_menu, &file_menu, &edit_menu])?;
    app.set_menu(menu)?;
    Ok(())
}

/// System tray (macOS: menu-bar extra). Menu clicks land in the app-wide
/// on_menu_event handler — ids are distinct from the menubar's.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "tray-toggle", "Show/Hide Window", true, None::<&str>)?;
    let new_note = MenuItem::with_id(app, "tray-new", "New Note", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &new_note, &PredefinedMenuItem::separator(app)?, &quit])?;
    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().expect("bundle icon embedded by tauri-build").clone())
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("Tray Notes")
        .build(app)?;
    println!("[tray] tray icon built");
    Ok(())
}

// ---------------------------------------------------------------- selftest

fn selftest(handle: AppHandle) {
    std::thread::sleep(Duration::from_secs(4));

    // 1) Clipboard image round-trip: write a 48x32 RGBA gradient via the
    //    plugin (arboard), then press the real "Paste image" button in JS.
    let (w, h) = (48u32, 32u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            rgba.extend_from_slice(&[(x * 5) as u8, (y * 7) as u8, 160, 255]);
        }
    }
    let wrote = handle.clipboard().write_image(&Image::new(&rgba, w, h));
    println!("[tray] selftest: clipboard write_image ok={}", wrote.is_ok());
    if let Some(win) = handle.get_webview_window("main") {
        let _ = win.eval("document.getElementById('paste-image').click()");
    }
    std::thread::sleep(Duration::from_secs(2));

    // 2) Multi-window: open About (window creation must happen on the main
    //    thread on macOS), verify it exists, close it.
    let h2 = handle.clone();
    let _ = handle.run_on_main_thread(move || open_about(&h2));
    std::thread::sleep(Duration::from_secs(2));
    println!("[tray] selftest: about exists={}", handle.get_webview_window("about").is_some());
    if let Some(about) = handle.get_webview_window("about") {
        let _ = about.close();
    }
    std::thread::sleep(Duration::from_secs(1));
    println!("[tray] selftest: about closed={}", handle.get_webview_window("about").is_none());

    // 3) close-to-tray: request close on main; CloseRequested is intercepted,
    //    the window is hidden, the app stays alive. Then toggle it back via
    //    the exact function the global shortcut handler calls.
    if let Some(win) = handle.get_webview_window("main") {
        let _ = win.close();
        std::thread::sleep(Duration::from_secs(1));
        let vis = handle
            .get_webview_window("main")
            .map(|w| w.is_visible().unwrap_or(true));
        println!(
            "[tray] selftest: after close request, window exists={} visible={vis:?}",
            handle.get_webview_window("main").is_some()
        );
        toggle_main_window(&handle);
        std::thread::sleep(Duration::from_millis(500));
        let vis = handle
            .get_webview_window("main")
            .map(|w| w.is_visible().unwrap_or(false));
        println!("[tray] selftest: after toggle, visible={vis:?}");
    }

    // 4) Save + notification through the real frontend command (fixed path —
    //    the native save dialog cannot be driven headlessly).
    if let Some(win) = handle.get_webview_window("main") {
        let _ = win.eval(
            "window.__TAURI__.core.invoke('write_note', { path: '/tmp/tray-notes-selftest.txt', text: document.getElementById('editor').value })",
        );
    }
}

// -------------------------------------------------------------------- main

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([GLOBAL_SHORTCUT])
                .expect("global shortcut parses")
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        println!("[tray] global shortcut Cmd+Shift+9 fired");
                        toggle_main_window(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![write_note, read_note, paste_image, report])
        .setup(|app| {
            build_menubar(app)?;
            build_tray(app)?;
            let sc: Shortcut = GLOBAL_SHORTCUT.parse().expect("shortcut parses");
            println!(
                "[tray] global shortcut registered={}",
                app.global_shortcut().is_registered(sc)
            );
            app.on_menu_event(|app, event| match event.id().as_ref() {
                "new" | "tray-new" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    let _ = app.emit("menu-new", ());
                }
                "open" => {
                    let _ = app.emit("menu-open", ());
                }
                "save" => {
                    let _ = app.emit("menu-save", ());
                }
                "about" => open_about(app),
                "quit" | "tray-quit" => app.exit(0),
                "tray-toggle" => toggle_main_window(app),
                _ => {}
            });
            if std::env::var("TRAY_SELFTEST").is_ok() {
                let handle = app.handle().clone();
                std::thread::spawn(move || selftest(handle));
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // close-to-tray: only for the main window; About closes for real.
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
                println!("[tray] close intercepted -> hidden to tray");
            }
            // Native drag-drop (dragDropEnabled: true): Finder drops arrive
            // as real file-system paths, no webview sandbox in the way.
            WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                if let Some(p) = paths.iter().find(|p| p.extension().is_some_and(|e| e == "txt")) {
                    match std::fs::read_to_string(p) {
                        Ok(text) => {
                            println!("[tray] file dropped: {} ({} bytes)", p.display(), text.len());
                            let _ = window.emit("note-loaded", serde_json::json!({
                                "path": p.display().to_string(),
                                "text": text,
                            }));
                        }
                        Err(e) => println!("[tray] file drop read error: {e}"),
                    }
                }
            }
            WindowEvent::ThemeChanged(theme) => {
                println!("[tray] ThemeChanged: {theme}");
                let _ = window.emit("theme-changed", theme.to_string());
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // macOS: clicking the dock icon while hidden should bring it back.
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
}
