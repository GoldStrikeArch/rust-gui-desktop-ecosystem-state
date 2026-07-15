//! "Tray Notes" per apps/SPEC-4.md — OS shell integration test for
//! egui 0.35 / eframe 0.35 on macOS.
//!
//! Everything *around* the window is assembled from helper crates:
//! tray-icon (tray + muda re-export for the native menubar), global-hotkey,
//! rfd (dialogs), arboard (image clipboard), and an `osascript` subprocess
//! for notifications. `notify-rust` was tried and rejected; see FRICTION.md.
//!
//! Integration model: tray/menubar/hotkey callbacks fire on the macOS main
//! thread (muda/tray-icon dispatch through NSApplication's target-action,
//! global-hotkey through Carbon RegisterEventHotKey). Each callback pushes a
//! semantic `Action` onto a shared queue and calls
//! `egui::Context::request_repaint()`; the eframe frame then drains the
//! queue. eframe 0.35 skips `App::ui` for hidden viewports; `App::logic`
//! still runs and is where the queue is drained. That is what makes
//! close-to-tray + reopen work with viewport commands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::accelerator::{
    Accelerator, Code as MenuCode, Modifiers as MenuModifiers,
};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Semantic actions produced by tray menu / native menubar / global hotkey
/// callbacks, drained once per egui frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    ToggleWindow,
    NewNote,
    FileOpen,
    FileSave,
    EditCut,
    EditCopy,
    EditPaste,
    About,
    Quit,
}

type ActionQueue = Arc<Mutex<Vec<Action>>>;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 420.0])
            .with_position([80.0, 60.0]) // deterministic position for scripted screenshots
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Tray Notes (egui)",
        options,
        // The creator closure runs on the main thread after winit has
        // initialized NSApplication — the only point where tray-icon,
        // muda's `init_for_nsapp` and GlobalHotKeyManager may be created.
        Box::new(|cc| Ok(Box::new(TrayNotesApp::new(cc)))),
    )
}

struct TrayNotesApp {
    // --- note state ---
    text: String,
    file_path: Option<PathBuf>,
    status: String,
    pasted_image: Option<egui::TextureHandle>,

    // --- shell integration state ---
    actions: ActionQueue,
    window_visible: bool,
    really_quit: bool,
    about_open: bool,

    // Owned so they are not dropped (dropping unregisters them). All are
    // !Send; the eframe App lives on the main thread, which is exactly
    // where macOS wants them.
    _tray: TrayIcon,
    _hotkey_manager: GlobalHotKeyManager,
    _menubar: Menu,
}

impl TrayNotesApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let actions: ActionQueue = Arc::new(Mutex::new(Vec::new()));
        let ctx = cc.egui_ctx.clone();

        // ---- Native menubar (muda via tray_icon::menu) ----
        // macOS: the first submenu becomes the application menu.
        let menubar = Menu::new();
        let app_menu = Submenu::new("Tray Notes", true);
        app_menu
            .append_items(&[
                &MenuItem::with_id("about", "About Tray Notes", true, None),
                &PredefinedMenuItem::separator(),
                // Quit lives in File per SPEC-4; app menu gets a plain one.
                &MenuItem::with_id("quit2", "Quit Tray Notes", true, None),
            ])
            .unwrap();
        let file_menu = Submenu::new("File", true);
        file_menu
            .append_items(&[
                &MenuItem::with_id(
                    "new",
                    "New",
                    true,
                    Some(Accelerator::new(Some(MenuModifiers::META), MenuCode::KeyN)),
                ),
                &MenuItem::with_id(
                    "open",
                    "Open…",
                    true,
                    Some(Accelerator::new(Some(MenuModifiers::META), MenuCode::KeyO)),
                ),
                &MenuItem::with_id(
                    "save",
                    "Save…",
                    true,
                    Some(Accelerator::new(Some(MenuModifiers::META), MenuCode::KeyS)),
                ),
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id(
                    "quit",
                    "Quit",
                    true,
                    Some(Accelerator::new(Some(MenuModifiers::META), MenuCode::KeyQ)),
                ),
            ])
            .unwrap();
        // Edit menu: egui draws its own text widgets (the winit NSView does
        // not implement the cut:/copy:/paste: responder-chain selectors), so
        // PredefinedMenuItem clipboard roles would be permanently disabled.
        // Instead: custom items whose MenuEvents are bridged to synthetic
        // egui::Event::{Cut,Copy,Paste} at the top of the next frame.
        let edit_menu = Submenu::new("Edit", true);
        edit_menu
            .append_items(&[
                &MenuItem::with_id(
                    "cut",
                    "Cut",
                    true,
                    Some(Accelerator::new(Some(MenuModifiers::META), MenuCode::KeyX)),
                ),
                &MenuItem::with_id(
                    "copy",
                    "Copy",
                    true,
                    Some(Accelerator::new(Some(MenuModifiers::META), MenuCode::KeyC)),
                ),
                &MenuItem::with_id(
                    "paste",
                    "Paste",
                    true,
                    Some(Accelerator::new(Some(MenuModifiers::META), MenuCode::KeyV)),
                ),
            ])
            .unwrap();
        menubar
            .append_items(&[&app_menu, &file_menu, &edit_menu])
            .unwrap();
        #[cfg(target_os = "macos")]
        menubar.init_for_nsapp();

        // ---- Tray icon (menu-bar extra) ----
        let tray_menu = Menu::new();
        tray_menu
            .append_items(&[
                &MenuItem::with_id("tray-toggle", "Show/Hide Window", true, None),
                &MenuItem::with_id("tray-new", "New Note", true, None),
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id("tray-quit", "Quit", true, None),
            ])
            .unwrap();
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Tray Notes (egui)")
            .with_icon(note_icon())
            .with_icon_as_template(true) // auto light/dark tint in the macOS menu bar
            .build()
            .expect("failed to create tray icon");

        // ---- One global handler for BOTH menus (muda has a single channel) ----
        {
            let actions = actions.clone();
            let ctx = ctx.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let action = match event.id().0.as_str() {
                    "tray-toggle" => Some(Action::ToggleWindow),
                    "tray-new" | "new" => Some(Action::NewNote),
                    "tray-quit" | "quit" | "quit2" => Some(Action::Quit),
                    "open" => Some(Action::FileOpen),
                    "save" => Some(Action::FileSave),
                    "cut" => Some(Action::EditCut),
                    "copy" => Some(Action::EditCopy),
                    "paste" => Some(Action::EditPaste),
                    "about" => Some(Action::About),
                    _ => None,
                };
                if let Some(action) = action {
                    eprintln!("[tray-notes] menu action: {action:?}");
                    actions.lock().unwrap().push(action);
                    ctx.request_repaint(); // wake eframe even while hidden
                }
            }));
        }

        // ---- Global hotkey Cmd+Shift+9 ----
        let hotkey_manager = GlobalHotKeyManager::new().expect("hotkey manager");
        let hotkey = HotKey::new(Some(Modifiers::META | Modifiers::SHIFT), Code::Digit9);
        hotkey_manager.register(hotkey).expect("register Cmd+Shift+9");
        {
            let actions = actions.clone();
            let ctx = ctx.clone();
            GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
                if event.state() == HotKeyState::Pressed {
                    eprintln!("[tray-notes] global hotkey Cmd+Shift+9");
                    actions.lock().unwrap().push(Action::ToggleWindow);
                    ctx.request_repaint();
                }
            }));
        }

        eprintln!("[tray-notes] started: tray + menubar + hotkey installed");

        // Optional: open a .txt passed on the command line (also makes the
        // save-path + notification flow scriptable without a dialog).
        let mut app = Self {
            text: String::new(),
            file_path: None,
            status: "New note".to_owned(),
            pasted_image: None,
            actions,
            window_visible: true,
            really_quit: false,
            about_open: false,
            _tray: tray,
            _hotkey_manager: hotkey_manager,
            _menubar: menubar,
        };
        if let Some(path) = std::env::args().nth(1) {
            app.load_file(Path::new(&path));
        }
        app
    }

    fn handle_action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::ToggleWindow => self.set_window_visible(ctx, !self.window_visible),
            Action::NewNote => {
                self.text.clear();
                self.file_path = None;
                self.status = "New note".to_owned();
                self.set_window_visible(ctx, true);
            }
            Action::FileOpen => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Text", &["txt"])
                    .pick_file()
                {
                    self.load_file(&path);
                }
            }
            Action::FileSave => self.save(ctx),
            // Bridge native Edit-menu roles onto egui's focused widget by
            // injecting the events egui's TextEdit already understands.
            Action::EditCut => ctx.input_mut(|i| i.events.push(egui::Event::Cut)),
            Action::EditCopy => ctx.input_mut(|i| i.events.push(egui::Event::Copy)),
            Action::EditPaste => {
                if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                    ctx.input_mut(|i| i.events.push(egui::Event::Paste(text)));
                }
            }
            Action::About => {
                self.about_open = true;
                // The About viewport is painted from `ui`, which only runs
                // while the main window is visible.
                self.set_window_visible(ctx, true);
            }
            Action::Quit => {
                self.really_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn set_window_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.window_visible = visible;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        if visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        eprintln!("[tray-notes] window visible: {visible}");
    }

    fn load_file(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                self.text = contents;
                self.file_path = Some(path.to_owned());
                self.status = format!("Loaded {}", path.display());
            }
            Err(err) => self.status = format!("Failed to read {}: {err}", path.display()),
        }
        eprintln!("[tray-notes] {}", self.status);
    }

    fn save(&mut self, _ctx: &egui::Context) {
        let path = self.file_path.clone().or_else(|| {
            rfd::FileDialog::new()
                .add_filter("Text", &["txt"])
                .set_file_name("note.txt")
                .save_file()
        });
        let Some(path) = path else {
            self.status = "Save cancelled".to_owned();
            return;
        };
        match std::fs::write(&path, &self.text) {
            Ok(()) => {
                self.file_path = Some(path.clone());
                self.status = format!("Saved {}", path.display());
                notify_saved(&path);
            }
            Err(err) => self.status = format!("Failed to save: {err}"),
        }
        eprintln!("[tray-notes] {}", self.status);
    }

    fn paste_image(&mut self, ctx: &egui::Context) {
        match arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
            Ok(image) => {
                let size = [image.width, image.height];
                let color_image =
                    egui::ColorImage::from_rgba_unmultiplied(size, &image.bytes);
                self.pasted_image = Some(ctx.load_texture(
                    "clipboard-image",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
                self.status = format!("Pasted image {}x{}", size[0], size[1]);
            }
            Err(err) => self.status = format!("No image on clipboard: {err}"),
        }
        eprintln!("[tray-notes] {}", self.status);
    }

    fn show_main_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui.button("Save (⌘S)").clicked() {
                self.save(ctx);
            }
            if ui.button("Paste image").clicked() {
                self.paste_image(ctx);
            }
            ui.label(egui::RichText::new(&self.status).weak());
        });
        ui.separator();

        let image_height = if self.pasted_image.is_some() { 110.0 } else { 0.0 };
        let editor_height = ui.available_height() - image_height;
        egui::ScrollArea::vertical()
            .max_height(editor_height)
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add_sized(
                    [ui.available_width(), editor_height - 10.0],
                    egui::TextEdit::multiline(&mut self.text)
                        .hint_text("Type a note… (file drop loads .txt)"),
                );
            });

        if let Some(texture) = &self.pasted_image {
            ui.separator();
            let size = texture.size_vec2();
            let scale = (100.0 / size.y).min(1.0);
            ui.image((texture.id(), size * scale));
        }
    }
}

impl eframe::App for TrayNotesApp {
    // KEY INTEGRATION DETAIL: eframe 0.35 does NOT run `App::ui` for hidden
    // viewports (`run_ui = is_visible || …` in wgpu_integration.rs), so a
    // window hidden to the tray would never drain the action queue if that
    // happened in `ui`. `App::logic` is the documented escape hatch: it runs
    // on every pass, "additionally also called when the UI is hidden, but
    // request_repaint was called" — and viewport commands (Visible(true))
    // queued from it are still applied by these UI-less passes.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain tray/menubar/hotkey actions queued since the last pass.
        let pending: Vec<Action> = std::mem::take(&mut *self.actions.lock().unwrap());
        for action in pending {
            self.handle_action(action, ctx);
        }

        // Close-to-tray: intercept the red close button on the main window.
        if ctx.input(|i| i.viewport().close_requested()) && !self.really_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.set_window_visible(ctx, false);
        }

        // Watchdog heartbeat (2 Hz): `ctx.request_repaint()` issued from the
        // native callbacks was observed (once) to be dropped as an "outdated
        // UserEvent::RequestRepaint" by eframe's lost-wakeup guard, after
        // which queued actions were never drained again. A slow self-repaint
        // scheduled from inside the pass keeps eframe's own RepaintAt timer
        // chain alive so any lost wake self-heals within 500 ms. Hidden
        // windows are additionally throttled by eframe to >= 100 ms.
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // File drop: load the first dropped .txt.
        let dropped: Vec<egui::DroppedFile> =
            ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.into_iter().find_map(|f| f.path) {
            self.load_file(&path);
        }

        // In-app shortcut Cmd+Shift+V = "Paste image" (button equivalent).
        if ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::V,
            ))
        }) {
            self.paste_image(&ctx);
        }

        egui::CentralPanel::default().show(ui, |ui| self.show_main_ui(ui, &ctx));

        // Second window (multi-window test): immediate viewport.
        if self.about_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("about"),
                egui::ViewportBuilder::default()
                    .with_title("About Tray Notes")
                    .with_inner_size([320.0, 160.0]),
                |ui, _class| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.heading("Tray Notes");
                        ui.label("egui 0.35 / eframe 0.35 OS-shell integration demo.");
                        ui.label("tray-icon + muda + global-hotkey + rfd + arboard + osascript");
                        if ui.button("Close").clicked() {
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    if ui.input(|i| i.viewport().close_requested()) {
                        self.about_open = false;
                    }
                },
            );
        }
    }
}

/// "Note saved" system notification.
///
/// notify-rust was tried and REJECTED in the full app. In two of three runs,
/// the first notification appeared and eframe then stopped running frames
/// while tray/hotkey callbacks remained alive. Delegate replacement by
/// mac-notification-sys is a plausible cause, but no minimized reproduction
/// was preserved, so it is not treated as proven. This implementation uses an
/// `osascript` subprocess instead and touches no in-process notification API.
fn notify_saved(path: &Path) {
    let script = format!(
        "display notification \"{}\" with title \"Note saved\"",
        path.display().to_string().replace('"', "'")
    );
    match std::process::Command::new("osascript")
        .args(["-e", &script])
        .spawn()
    {
        Ok(_) => eprintln!("[tray-notes] notification sent via osascript"),
        Err(err) => eprintln!("[tray-notes] notification failed: {err}"),
    }
}

/// 32x32 RGBA "note" glyph drawn in code (black + alpha only, so the macOS
/// template-image machinery tints it for light/dark menu bars).
fn note_icon() -> tray_icon::Icon {
    const N: usize = 32;
    let mut rgba = vec![0u8; N * N * 4];
    let mut set = |x: usize, y: usize| {
        let i = 4 * (y * N + x);
        rgba[i] = 0;
        rgba[i + 1] = 0;
        rgba[i + 2] = 0;
        rgba[i + 3] = 255;
    };
    // Page outline with a dog-ear at top-right.
    for y in 4..28 {
        for x in 6..26 {
            let dog_ear = x >= 20 && y <= 10 && (x - 20) + (10 - y) > 6;
            if dog_ear {
                continue;
            }
            let border = x == 6 || x == 25 || y == 4 || y == 27
                || (x >= 20 && y <= 10 && (x - 20) + (10 - y) == 6);
            // Three "text lines" on the page.
            let text_line = (y == 12 || y == 16 || y == 20) && (9..=22).contains(&x);
            if border || text_line {
                set(x, y);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, N as u32, N as u32).expect("icon")
}
