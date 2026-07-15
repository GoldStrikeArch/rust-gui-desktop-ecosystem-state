//! "Tray Notes" — SPEC-4 OS shell integration test.
//! Dioxus 0.7.9 desktop (wry/tao webview renderer), plain cargo (no dx CLI).
//!
//! Shell integration inventory (see FRICTION.md for ratings):
//! - tray icon + menu:   dioxus-desktop built-in (re-exported `tray-icon` crate,
//!   `init_tray_icon` + `use_tray_menu_event_handler`)
//! - native menubar:     dioxus-desktop built-in (muda via `Config::with_menu`)
//! - global hotkey:      dioxus-desktop built-in (`use_global_shortcut`, global-hotkey crate)
//! - close-to-tray:      dioxus-desktop built-in (`WindowCloseBehaviour::WindowHides`)
//! - multi-window:       dioxus-desktop built-in (`window().new_window(...)`)
//! - file drop:          dioxus-desktop built-in (wry drag-drop -> HTML ondrop with real paths)
//! - dialogs:            rfd (helper crate; dioxus already ships it internally)
//! - clipboard image:    arboard + image + base64 (helper crates)
//! - notification:       notify-rust (helper crate) with osascript fallback
//! - dark mode:          CSS prefers-color-scheme in the webview (live)

use base64::Engine as _;
use dioxus::desktop::muda::{
    accelerator::Accelerator, Menu as MenuBar, MenuItem, PredefinedMenuItem, Submenu,
};
use dioxus::desktop::trayicon::{init_tray_icon, menu as tray_menu};
use dioxus::desktop::{
    use_global_shortcut, use_muda_event_handler, use_tray_menu_event_handler, use_window,
    Config, HotKeyState, LogicalSize, WindowBuilder, WindowCloseBehaviour,
};
use dioxus::html::HasFileData;
use dioxus::prelude::*;

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(
                    WindowBuilder::new()
                        .with_title("Tray Notes (dioxus)")
                        .with_inner_size(LogicalSize::new(500.0, 420.0))
                        .with_resizable(true),
                )
                // Custom native menubar (muda). Replaces the default one.
                .with_menu(build_menubar())
                // Req 2: closing the window hides it to the tray; the app keeps
                // running because the last-window-close exit is disabled too.
                .with_close_behaviour(WindowCloseBehaviour::WindowHides)
                .with_exits_when_last_window_closes(false),
        )
        .launch(App);
}

/// Native menubar: File -> New/Open/Save (⌘N/⌘O/⌘S) + Quit (⌘Q, in the macOS
/// application menu), Edit -> standard clipboard roles (predefined muda items).
fn build_menubar() -> MenuBar {
    let accel = |s: &str| -> Option<Accelerator> { Some(s.parse().unwrap()) };
    let menu = MenuBar::new();

    // On macOS the first submenu becomes the application menu ("Tray Notes").
    let app = Submenu::new("Tray Notes", true);
    app.append_items(&[
        &PredefinedMenuItem::about(None, None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None), // ⌘Q — really exits
    ])
    .unwrap();

    let file = Submenu::new("File", true);
    file.append_items(&[
        &MenuItem::with_id("file-new", "New", true, accel("CmdOrCtrl+N")),
        &MenuItem::with_id("file-open", "Open…", true, accel("CmdOrCtrl+O")),
        &MenuItem::with_id("file-save", "Save…", true, accel("CmdOrCtrl+S")),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::close_window(None), // ⌘W (hides via close behaviour)
    ])
    .unwrap();

    let edit = Submenu::new("Edit", true);
    edit.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::select_all(None),
    ])
    .unwrap();

    menu.append_items(&[&app, &file, &edit]).unwrap();
    menu
}

#[component]
fn App() -> Element {
    let mut content = use_signal(String::new);
    let mut status = use_signal(|| "Ready. Close button hides to tray; ⌘⇧9 toggles.".to_string());
    let mut image_thumb = use_signal(|| None::<String>);
    let desktop = use_window();

    // ---- System tray icon + menu (built-in: dioxus re-exports tray-icon and
    // pumps its events through the tao event loop). Created once on mount.
    // Left click shows the window (dioxus default), right click opens the menu.
    use_hook(|| {
        let menu = tray_menu::Menu::new();
        menu.append_items(&[
            &tray_menu::MenuItem::with_id("tray-toggle", "Show/Hide Window", true, None),
            &tray_menu::MenuItem::with_id("tray-new", "New Note", true, None),
            &tray_menu::PredefinedMenuItem::separator(),
            &tray_menu::PredefinedMenuItem::quit(Some("Quit Tray Notes")), // really exits
        ])
        .unwrap();
        init_tray_icon(menu, None) // None -> dioxus default icon
    });

    // Show/Hide toggle shared by the tray menu and the global hotkey.
    let toggle_window = use_callback({
        let desktop = desktop.clone();
        move |()| {
            if desktop.is_visible() {
                desktop.set_visible(false);
                println!("[tray-notes] toggle -> hidden");
            } else {
                desktop.set_visible(true);
                desktop.set_focus();
                println!("[tray-notes] toggle -> shown");
            }
        }
    });

    // ---- Global hotkey Cmd+Shift+9 (built-in: global-hotkey crate wired into
    // the event loop; fires even when the app is unfocused or hidden).
    _ = use_global_shortcut("super+shift+9", move |state| {
        if state == HotKeyState::Pressed {
            toggle_window.call(());
        }
    });

    // ---- One handler for all native menu ids (menubar + tray menu).
    // NOTE: tray-icon re-exports the same muda crate, and dioxus installs the
    // tray receiver *after* the menubar receiver on the single global
    // muda::MenuEvent handler slot — so in practice menubar events also arrive
    // as TrayMenuEvent. Registering both hooks covers either routing.
    let on_menu = use_callback(move |id: String| {
        println!("[tray-notes] menu event: {id}");
        match id.as_str() {
            "file-new" | "tray-new" => {
                content.set(String::new());
                image_thumb.set(None);
                status.set("New note.".to_string());
            }
            "file-open" => open_note(content, status),
            "file-save" => save_note(content, status),
            "tray-toggle" => toggle_window.call(()),
            _ => {}
        }
    });
    use_muda_event_handler(move |e| on_menu.call(e.id().0.clone()));
    use_tray_menu_event_handler(move |e| on_menu.call(e.id().0.clone()));

    // TRAY_SELFTEST=1: deterministic verification of the non-UI-clickable
    // paths (clipboard image grab, save + notification, second window) so a
    // scripted launch check can observe them via stdout + window list.
    use_future(move || async move {
        if std::env::var_os("TRAY_SELFTEST").is_none() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        paste_image(image_thumb, status);
        println!("[selftest] paste_image -> {}", status.peek());
        let path = std::env::temp_dir().join("dx-tray-selftest.txt");
        content.set("selftest note".to_string());
        match std::fs::write(&path, content.peek().as_bytes()) {
            Ok(()) => {
                println!("[selftest] wrote {}", path.display());
                notify_saved();
            }
            Err(e) => println!("[selftest] write failed: {e}"),
        }
        open_about();
        println!("[selftest] about window requested");
    });

    rsx! {
        style { {CSS} }
        div {
            class: "root",
            // ---- Req 7: file drop. The wry drag-drop handler feeds real
            // native paths into the HTML drop event (evt.files()).
            ondragover: move |evt| evt.prevent_default(),
            ondrop: move |evt| {
                evt.prevent_default();
                let files = evt.files();
                match files.iter().find(|f| f.path().extension().is_some_and(|e| e == "txt")) {
                    Some(f) => {
                        let path = f.path();
                        match std::fs::read_to_string(&path) {
                            Ok(text) => {
                                content.set(text);
                                status.set(format!("Loaded dropped file {}", path.display()));
                            }
                            Err(e) => status.set(format!("Drop failed: {e}")),
                        }
                    }
                    None if !files.is_empty() => status.set("Drop a .txt file.".to_string()),
                    None => {}
                }
            },

            div { class: "toolbar",
                button { onclick: move |_| paste_image(image_thumb, status), "Paste image" }
                button { onclick: move |_| open_about(), "About (2nd window)" }
                span { class: "hint", "⌘⇧9 global toggle · drop a .txt here" }
            }

            textarea {
                class: "editor",
                placeholder: "Type a note… (normal text paste works natively)",
                value: "{content}",
                oninput: move |e| content.set(e.value()),
            }

            if let Some(src) = image_thumb() {
                div { class: "thumbrow",
                    img { class: "thumb", src: "{src}" }
                    button { onclick: move |_| image_thumb.set(None), "✕ clear" }
                }
            }

            div { class: "status", "{status}" }
        }
    }
}

/// Second window (multi-window test) — its own VirtualDom + webview.
fn open_about() {
    spawn(async move {
        let dom = VirtualDom::new(AboutWindow);
        let cfg = Config::new()
            // Do NOT install a menubar for this window: on macOS the menubar is
            // app-global and the default one would clobber our custom File menu.
            .with_menu(None::<dioxus::desktop::muda::Menu>)
            .with_window(
                WindowBuilder::new()
                    .with_title("About Tray Notes")
                    .with_inner_size(LogicalSize::new(340.0, 220.0)),
            );
        // The About window closes for real (default WindowCloses behaviour);
        // the app stays alive because exit-on-last-window-close is off.
        dioxus::desktop::window().new_window(dom, cfg).await;
    });
}

#[component]
fn AboutWindow() -> Element {
    rsx! {
        style { {CSS} }
        div { class: "root about",
            h2 { "Tray Notes" }
            p { "SPEC-4 shell-integration test app, built with Dioxus 0.7.9 (wry/tao)." }
            p { class: "hint", "This is an independent second window with its own webview." }
            button { onclick: move |_| dioxus::desktop::window().close(), "Close" }
        }
    }
}

/// Req 5: native Open dialog (rfd) -> load .txt into the editor.
fn open_note(mut content: Signal<String>, mut status: Signal<String>) {
    spawn(async move {
        if let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Text", &["txt"])
            .pick_file()
            .await
        {
            let path = file.path().to_path_buf();
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    content.set(text);
                    status.set(format!("Opened {}", path.display()));
                }
                Err(e) => status.set(format!("Open failed: {e}")),
            }
        }
    });
}

/// Req 5 + 8: native Save dialog (rfd), then a "Note saved" system notification.
fn save_note(content: Signal<String>, mut status: Signal<String>) {
    spawn(async move {
        if let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Text", &["txt"])
            .set_file_name("note.txt")
            .save_file()
            .await
        {
            let path = file.path().to_path_buf();
            match std::fs::write(&path, content.peek().as_bytes()) {
                Ok(()) => {
                    status.set(format!("Saved {}", path.display()));
                    notify_saved();
                }
                Err(e) => status.set(format!("Save failed: {e}")),
            }
        }
    });
}

/// "Note saved" notification: notify-rust first; if the unbundled dev binary
/// can't post (macOS notifications are bundle-id gated), fall back to
/// osascript so the behaviour is still observable.
fn notify_saved() {
    match notify_rust::Notification::new()
        .summary("Tray Notes")
        .body("Note saved")
        .show()
    {
        Ok(_) => println!("[tray-notes] notification posted via notify-rust"),
        Err(e) => {
            println!("[tray-notes] notify-rust failed ({e}); osascript fallback");
            _ = std::process::Command::new("osascript")
                .arg("-e")
                .arg(r#"display notification "Note saved" with title "Tray Notes""#)
                .spawn();
        }
    }
}

/// Req 6b: image clipboard via arboard; PNG-encode + base64 so the webview can
/// show it as an <img> data URI thumbnail.
fn paste_image(mut image_thumb: Signal<Option<String>>, mut status: Signal<String>) {
    let grab = || -> Result<String, String> {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let img = cb.get_image().map_err(|e| e.to_string())?;
        let (w, h) = (img.width as u32, img.height as u32);
        let rgba = image::RgbaImage::from_raw(w, h, img.bytes.into_owned())
            .ok_or("clipboard image had unexpected buffer size")?;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ))
    };
    match grab() {
        Ok(uri) => {
            println!("[tray-notes] clipboard image grabbed ({} b data uri)", uri.len());
            image_thumb.set(Some(uri));
            status.set("Pasted image from clipboard.".to_string());
        }
        Err(e) => {
            println!("[tray-notes] clipboard image failed: {e}");
            status.set(format!("No clipboard image: {e}"));
        }
    }
}

/// Req 9: live dark mode. WKWebView re-evaluates prefers-color-scheme when the
/// OS theme flips, so this reacts without restart; color-scheme also switches
/// the native rendering of the textarea/scrollbars.
const CSS: &str = r#"
:root { color-scheme: light dark; }
* { box-sizing: border-box; }
body { margin: 0; font-family: system-ui, sans-serif; }
.root {
  display: flex; flex-direction: column; height: 100vh;
  gap: 8px; padding: 10px;
  background: #f2f2f6; color: #1d1d1f;
  transition: background 0.25s, color 0.25s;
}
.toolbar { display: flex; gap: 8px; align-items: center; }
.toolbar button { padding: 4px 10px; }
.hint { font-size: 11.5px; opacity: 0.65; margin-left: auto; }
.editor {
  flex: 1; resize: none; padding: 8px;
  font: 14px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace;
  border: 1px solid #c8c8cc; border-radius: 6px;
  background: #ffffff; color: inherit;
}
.thumbrow { display: flex; gap: 8px; align-items: flex-start; }
.thumb {
  max-height: 90px; max-width: 200px;
  border: 1px solid #c8c8cc; border-radius: 4px;
}
.status { font-size: 12px; opacity: 0.75; min-height: 15px; }
.about { justify-content: center; align-items: center; text-align: center; }
.about h2 { margin: 0; }
.about p { margin: 0 12px; font-size: 13px; }
@media (prefers-color-scheme: dark) {
  .root { background: #1e1e21; color: #e8e8ea; }
  .editor { background: #2a2a2e; border-color: #48484d; }
  .thumb { border-color: #48484d; }
}
"#;
