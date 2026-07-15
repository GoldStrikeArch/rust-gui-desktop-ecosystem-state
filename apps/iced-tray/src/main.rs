//! "Tray Notes" — OS shell integration test (SPEC-4), iced 0.14.
//!
//! Architecture notes (research-relevant):
//! - `iced::daemon` (0.13+) is the load-bearing piece: it keeps the process
//!   alive with zero windows, which is what makes close-to-tray and
//!   Quit-from-tray semantics possible at all in iced.
//! - iced owns the winit event loop, so tray-icon / muda / global-hotkey
//!   (which all require main-thread creation on macOS *after* the NSApp run
//!   loop exists) are created lazily in `update` via a `Task::done(SetupShell)`
//!   fired from `boot` — `boot`/`update`/`view` all run on the main thread
//!   (verified in iced_winit 0.14 source; `State` has no `Send` bound).
//! - Their events arrive on global crossbeam channels; iced has no way to
//!   inject foreign events into its loop, so a 100 ms `time::every`
//!   subscription polls the three channels (`Message::Poll`).
//! - Dark mode is built-in: with no `.theme()` set, iced 0.14 resolves
//!   `Theme::default(system mode)` per window and live-updates it
//!   (dark-mode PR #3051). We additionally subscribe to
//!   `system::theme_changes()` just to surface the mode in the status bar.

use std::path::PathBuf;

use global_hotkey::hotkey::{Code, HotKey, Modifiers as HotKeyModifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::accelerator::{Accelerator, Code as MenuCode, Modifiers as MenuModifiers};
use tray_icon::menu::{
    Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

use iced::event;
use iced::time;
use iced::widget::{button, column, container, image, row, space, text, text_editor};
use iced::window;
use iced::{Center, Element, Event, Fill, Size, Subscription, Task, theme};

use std::time::Duration;

pub fn main() -> iced::Result {
    iced::daemon(TrayNotes::boot, TrayNotes::update, TrayNotes::view)
        .title(TrayNotes::title)
        .subscription(TrayNotes::subscription)
        .run()
}

// ---------------------------------------------------------------------------
// Shell integration objects (all !Send; must live on the main thread)
// ---------------------------------------------------------------------------

struct Shell {
    // Kept alive for the lifetime of the app; dropping the tray icon
    // removes it from the menu bar.
    _tray: TrayIcon,
    _menubar: Menu,
    _hotkeys: GlobalHotKeyManager,
    hotkey_id: u32,
    // Tray menu items.
    tray_toggle: MenuId,
    tray_new: MenuId,
    tray_quit: MenuId,
    // Native menubar items.
    menu_new: MenuId,
    menu_open: MenuId,
    menu_save: MenuId,
    menu_about: MenuId,
    menu_cut: MenuId,
    menu_copy: MenuId,
    menu_paste: MenuId,
    menu_select_all: MenuId,
}

/// 22×22 template icon (black + alpha): a rounded "note" square with lines.
fn tray_icon_rgba() -> tray_icon::Icon {
    const S: usize = 22;
    let mut rgba = vec![0u8; S * S * 4];
    let mut put = |x: usize, y: usize, a: u8| {
        let i = (y * S + x) * 4;
        rgba[i] = 0;
        rgba[i + 1] = 0;
        rgba[i + 2] = 0;
        rgba[i + 3] = a;
    };
    for y in 3..19 {
        for x in 4..18 {
            let corner = (x <= 5 || x >= 16) && (y <= 4 || y >= 17);
            let border = x == 4 || x == 17 || y == 3 || y == 18;
            let line = (y == 7 || y == 10 || y == 13) && (6..16).contains(&x);
            if corner {
                continue;
            }
            if border {
                put(x, y, 255);
            } else if line {
                put(x, y, 200);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, S as u32, S as u32).expect("valid rgba icon")
}

impl Shell {
    /// Must run on the main thread, after the winit/NSApp run loop started
    /// (tray-icon issue #90: creating the NSStatusItem before the loop runs
    /// breaks menu display on macOS).
    fn create() -> Result<Self, String> {
        // --- Tray icon + menu -------------------------------------------
        let tray_toggle = MenuItem::new("Show/Hide Window", true, None);
        let tray_new = MenuItem::new("New Note", true, None);
        let tray_quit = MenuItem::new("Quit Tray Notes", true, None);
        let tray_menu = Menu::new();
        tray_menu
            .append_items(&[
                &tray_toggle,
                &tray_new,
                &PredefinedMenuItem::separator(),
                &tray_quit,
            ])
            .map_err(|e| format!("tray menu: {e}"))?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Tray Notes (iced)")
            .with_icon(tray_icon_rgba())
            .with_icon_as_template(true) // adapts to menu-bar dark/light
            .build()
            .map_err(|e| format!("tray icon: {e}"))?;

        // --- Native menubar (macOS main menu) ---------------------------
        let cmd = Some(MenuModifiers::META);
        let menu_new = MenuItem::new("New", true, Some(Accelerator::new(cmd, MenuCode::KeyN)));
        // (see Edit menu below for the cut/copy/paste story)
        let menu_open = MenuItem::new("Open…", true, Some(Accelerator::new(cmd, MenuCode::KeyO)));
        let menu_save = MenuItem::new("Save…", true, Some(Accelerator::new(cmd, MenuCode::KeyS)));
        let menu_about = MenuItem::new("About Tray Notes", true, None);

        let app_menu = Submenu::new("Tray Notes", true);
        app_menu
            .append_items(&[
                &menu_about,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::separator(),
                // Native ⌘Q — exits via NSApp terminate:.
                &PredefinedMenuItem::quit(None),
            ])
            .map_err(|e| format!("app menu: {e}"))?;

        let file_menu = Submenu::new("File", true);
        file_menu
            .append_items(&[
                &menu_new,
                &menu_open,
                &menu_save,
                &PredefinedMenuItem::separator(),
                // ⌘W → performClose: → winit CloseRequested → our hide-to-tray.
                &PredefinedMenuItem::close_window(Some("Close Window")),
            ])
            .map_err(|e| format!("file menu: {e}"))?;

        // Clipboard roles. RESEARCH FINDING: muda's PredefinedMenuItem::
        // cut/copy/paste use the Cocoa responder chain (cut:/copy:/paste:
        // selectors) which iced's winit NSView does not implement — the menu
        // claims the ⌘X/⌘C/⌘V key equivalents, the items no-op, AND the
        // keystrokes never reach iced's own text_editor bindings (verified:
        // ⌘V pasted nothing with predefined items installed). Workaround:
        // custom items routed explicitly to the editor via MenuEvent +
        // iced::clipboard tasks. Undo/redo omitted: iced text_editor has no
        // undo stack at all.
        let menu_cut = MenuItem::new("Cut", true, Some(Accelerator::new(cmd, MenuCode::KeyX)));
        let menu_copy = MenuItem::new("Copy", true, Some(Accelerator::new(cmd, MenuCode::KeyC)));
        let menu_paste = MenuItem::new("Paste", true, Some(Accelerator::new(cmd, MenuCode::KeyV)));
        let menu_select_all =
            MenuItem::new("Select All", true, Some(Accelerator::new(cmd, MenuCode::KeyA)));
        let edit_menu = Submenu::new("Edit", true);
        edit_menu
            .append_items(&[&menu_cut, &menu_copy, &menu_paste, &menu_select_all])
            .map_err(|e| format!("edit menu: {e}"))?;

        let menubar = Menu::new();
        menubar
            .append_items(&[&app_menu, &file_menu, &edit_menu])
            .map_err(|e| format!("menubar: {e}"))?;

        #[cfg(target_os = "macos")]
        menubar.init_for_nsapp();

        // --- Global hotkey ⌘⇧9 ------------------------------------------
        let hotkeys =
            GlobalHotKeyManager::new().map_err(|e| format!("hotkey manager: {e}"))?;
        let hotkey = HotKey::new(
            Some(HotKeyModifiers::META | HotKeyModifiers::SHIFT),
            Code::Digit9,
        );
        hotkeys
            .register(hotkey)
            .map_err(|e| format!("hotkey register: {e}"))?;

        Ok(Self {
            _tray: tray,
            _menubar: menubar,
            _hotkeys: hotkeys,
            hotkey_id: hotkey.id(),
            tray_toggle: tray_toggle.id().clone(),
            tray_new: tray_new.id().clone(),
            tray_quit: tray_quit.id().clone(),
            menu_new: menu_new.id().clone(),
            menu_open: menu_open.id().clone(),
            menu_save: menu_save.id().clone(),
            menu_about: menu_about.id().clone(),
            menu_cut: menu_cut.id().clone(),
            menu_copy: menu_copy.id().clone(),
            menu_paste: menu_paste.id().clone(),
            menu_select_all: menu_select_all.id().clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

struct TrayNotes {
    main_window: window::Id,
    about_window: Option<window::Id>,
    main_visible: bool,
    shell: Option<Shell>,
    content: text_editor::Content,
    file: Option<PathBuf>,
    pasted_image: Option<image::Handle>,
    status: String,
    theme_mode: theme::Mode,
}

#[derive(Debug, Clone)]
enum Message {
    SetupShell,
    Poll,
    EditorAction(text_editor::Action),
    EditCut,
    EditCopy,
    EditPaste(Option<String>),
    EditSelectAll,
    NewNote,
    OpenRequested,
    OpenPicked(Option<PathBuf>),
    SaveRequested,
    SavePicked(Option<PathBuf>),
    PasteImage,
    ClearImage,
    ToggleWindow,
    OpenAbout,
    CloseAbout,
    CloseRequested(window::Id),
    WindowClosed(window::Id),
    FileDropped(window::Id, PathBuf),
    ThemeChanged(theme::Mode),
    Quit,
    Screenshot(window::Id),
    ScreenshotTaken(window::Screenshot),
}

fn main_window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(500.0, 420.0),
        // Deliver CloseRequested to us instead of closing: close-to-tray.
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

impl TrayNotes {
    fn boot() -> (Self, Task<Message>) {
        let (main_window, open) = window::open(main_window_settings());

        let state = Self {
            main_window,
            about_window: None,
            main_visible: true,
            shell: None,
            content: text_editor::Content::new(),
            file: None,
            pasted_image: None,
            status: String::from("Ready — tray menu, ⌘⇧9, drop a .txt here"),
            theme_mode: theme::Mode::None,
        };

        (
            state,
            Task::batch([open.discard(), Task::done(Message::SetupShell)]),
        )
    }

    fn title(&self, window: window::Id) -> String {
        if Some(window) == self.about_window {
            String::from("About — Tray Notes (iced)")
        } else {
            String::from("Tray Notes (iced)")
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SetupShell => {
                match Shell::create() {
                    Ok(shell) => {
                        eprintln!("shell: tray + menubar + hotkey OK");
                        self.shell = Some(shell);
                    }
                    Err(error) => {
                        self.status = format!("shell setup FAILED: {error}");
                        eprintln!("shell setup FAILED: {error}");
                    }
                }
                // Optional scripted verification hooks (research only).
                let mut tasks = Vec::new();
                if std::env::var("TRAY_SELFTEST_IMAGE").is_ok() {
                    tasks.push(Task::done(Message::PasteImage));
                }
                if let Ok(path) = std::env::var("TRAY_SELFTEST_SAVE") {
                    tasks.push(Task::done(Message::SavePicked(Some(path.into()))));
                }
                if std::env::var("TRAY_SELFTEST_SHOT").is_ok() {
                    tasks.push(
                        Task::future(async {
                            smol::Timer::after(Duration::from_secs(3)).await;
                        })
                        .then({
                            let id = self.main_window;
                            move |()| Task::done(Message::Screenshot(id))
                        }),
                    );
                }
                Task::batch(tasks)
            }
            Message::Poll => {
                let mut tasks = Vec::new();

                while let Ok(event) = MenuEvent::receiver().try_recv() {
                    if let Some(shell) = &self.shell {
                        let id = event.id();
                        if *id == shell.tray_toggle {
                            tasks.push(Task::done(Message::ToggleWindow));
                        } else if *id == shell.tray_new || *id == shell.menu_new {
                            tasks.push(Task::done(Message::NewNote));
                        } else if *id == shell.tray_quit {
                            tasks.push(Task::done(Message::Quit));
                        } else if *id == shell.menu_open {
                            tasks.push(Task::done(Message::OpenRequested));
                        } else if *id == shell.menu_save {
                            tasks.push(Task::done(Message::SaveRequested));
                        } else if *id == shell.menu_about {
                            tasks.push(Task::done(Message::OpenAbout));
                        } else if *id == shell.menu_cut {
                            tasks.push(Task::done(Message::EditCut));
                        } else if *id == shell.menu_copy {
                            tasks.push(Task::done(Message::EditCopy));
                        } else if *id == shell.menu_paste {
                            tasks.push(iced::clipboard::read().map(Message::EditPaste));
                        } else if *id == shell.menu_select_all {
                            tasks.push(Task::done(Message::EditSelectAll));
                        }
                    }
                }

                // Tray icon click/hover events (macOS: menu opens natively;
                // these are informational only).
                while let Ok(_event) = TrayIconEvent::receiver().try_recv() {}

                while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                    let registered =
                        self.shell.as_ref().is_some_and(|s| s.hotkey_id == event.id());
                    if registered && event.state() == HotKeyState::Pressed {
                        tasks.push(Task::done(Message::ToggleWindow));
                    }
                }

                Task::batch(tasks)
            }
            Message::EditorAction(action) => {
                self.content.perform(action);
                Task::none()
            }
            // Menubar Edit → routed explicitly to the note editor (iced has
            // no focus-based responder chain to dispatch these natively).
            Message::EditCut => match self.content.selection() {
                Some(selection) => {
                    self.content
                        .perform(text_editor::Action::Edit(text_editor::Edit::Backspace));
                    iced::clipboard::write(selection)
                }
                None => Task::none(),
            },
            Message::EditCopy => match self.content.selection() {
                Some(selection) => iced::clipboard::write(selection),
                None => Task::none(),
            },
            Message::EditPaste(Some(text)) => {
                eprintln!("edit-paste: {} chars", text.len());
                self.content.perform(text_editor::Action::Edit(
                    text_editor::Edit::Paste(std::sync::Arc::new(text)),
                ));
                Task::none()
            }
            Message::EditPaste(None) => Task::none(),
            Message::EditSelectAll => {
                self.content.perform(text_editor::Action::SelectAll);
                Task::none()
            }
            Message::NewNote => {
                eprintln!("new-note");
                self.content = text_editor::Content::new();
                self.file = None;
                self.status = String::from("New note");
                self.show_main()
            }
            Message::OpenRequested => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .add_filter("Text", &["txt"])
                        .pick_file()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                },
                Message::OpenPicked,
            ),
            Message::OpenPicked(Some(path)) => {
                self.load_file(path);
                Task::none()
            }
            Message::OpenPicked(None) => Task::none(),
            Message::SaveRequested => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .add_filter("Text", &["txt"])
                        .set_file_name("note.txt")
                        .save_file()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                },
                Message::SavePicked,
            ),
            Message::SavePicked(Some(path)) => {
                match std::fs::write(&path, self.content.text()) {
                    Ok(()) => {
                        self.status = format!("Saved {}", path.display());
                        self.file = Some(path.clone());

                        // System notification: "Note saved".
                        let result = notify_rust::Notification::new()
                            .summary("Note saved")
                            .body(&format!("{}", path.display()))
                            .show();
                        match result {
                            Ok(_) => eprintln!("notification: OK"),
                            Err(error) => {
                                eprintln!("notification: FAILED: {error}");
                                self.status =
                                    format!("Saved; notification failed: {error}");
                            }
                        }
                    }
                    Err(error) => self.status = format!("Save failed: {error}"),
                }
                Task::none()
            }
            Message::SavePicked(None) => Task::none(),
            Message::PasteImage => {
                match arboard::Clipboard::new().and_then(|mut cb| cb.get_image()) {
                    Ok(img) => {
                        let (w, h) = (img.width as u32, img.height as u32);
                        self.pasted_image = Some(image::Handle::from_rgba(
                            w,
                            h,
                            img.bytes.into_owned(),
                        ));
                        self.status = format!("Pasted image {w}×{h}");
                        eprintln!("paste-image: OK {w}x{h}");
                    }
                    Err(error) => {
                        self.status = format!("No image on clipboard: {error}");
                        eprintln!("paste-image: FAILED: {error}");
                    }
                }
                Task::none()
            }
            Message::ClearImage => {
                self.pasted_image = None;
                Task::none()
            }
            Message::ToggleWindow => {
                if self.main_visible {
                    eprintln!("toggle-window: hiding");
                    self.main_visible = false;
                    self.status = String::from("Hidden to tray");
                    window::set_mode(self.main_window, window::Mode::Hidden)
                } else {
                    eprintln!("toggle-window: showing");
                    self.show_main()
                }
            }
            Message::OpenAbout => {
                if let Some(id) = self.about_window {
                    window::gain_focus(id)
                } else {
                    let (id, open) = window::open(window::Settings {
                        size: Size::new(320.0, 200.0),
                        resizable: false,
                        ..window::Settings::default()
                    });
                    self.about_window = Some(id);
                    open.discard()
                }
            }
            Message::CloseAbout => match self.about_window {
                Some(id) => window::close(id),
                None => Task::none(),
            },
            Message::CloseRequested(id) => {
                if id == self.main_window {
                    // Close-to-tray: hide, keep running.
                    self.main_visible = false;
                    self.status = String::from("Hidden to tray (⌘⇧9 or tray to show)");
                    eprintln!("close-to-tray: hidden");
                    window::set_mode(id, window::Mode::Hidden)
                } else {
                    window::close(id)
                }
            }
            Message::WindowClosed(id) => {
                if Some(id) == self.about_window {
                    self.about_window = None;
                }
                Task::none()
            }
            Message::FileDropped(_window, path) => {
                if path.extension().is_some_and(|ext| ext == "txt") {
                    self.load_file(path);
                } else {
                    self.status = format!("Ignored drop (not .txt): {}", path.display());
                }
                Task::none()
            }
            Message::ThemeChanged(mode) => {
                self.theme_mode = mode;
                eprintln!("theme-changed: {mode:?}");
                Task::none()
            }
            Message::Quit => iced::exit(),
            Message::Screenshot(id) => {
                eprintln!("screenshot: capturing window {id:?}");
                window::screenshot(id).map(Message::ScreenshotTaken)
            }
            Message::ScreenshotTaken(shot) => {
                let path = std::env::var("TRAY_SELFTEST_SHOT")
                    .unwrap_or_else(|_| String::from("screenshot.png"));
                match image_rs_save(&shot, &path) {
                    Ok(()) => eprintln!("screenshot: saved {path}"),
                    Err(error) => eprintln!("screenshot: FAILED: {error}"),
                }
                Task::none()
            }
        }
    }

    fn show_main(&mut self) -> Task<Message> {
        self.main_visible = true;
        Task::batch([
            window::set_mode(self.main_window, window::Mode::Windowed),
            window::gain_focus(self.main_window),
        ])
    }

    fn load_file(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.content = text_editor::Content::with_text(&text);
                self.status = format!("Loaded {}", path.display());
                eprintln!("file-loaded: {}", path.display());
                self.file = Some(path);
            }
            Err(error) => self.status = format!("Load failed: {error}"),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            // Poll tray / menubar / hotkey crossbeam channels.
            time::every(Duration::from_millis(100)).map(|_| Message::Poll),
            window::close_requests().map(Message::CloseRequested),
            window::close_events().map(Message::WindowClosed),
            iced::system::theme_changes().map(Message::ThemeChanged),
            event::listen_with(|event, _status, window| match event {
                Event::Window(window::Event::FileDropped(path)) => {
                    Some(Message::FileDropped(window, path))
                }
                _ => None,
            }),
        ])
    }

    fn view(&self, window: window::Id) -> Element<'_, Message> {
        if Some(window) == self.about_window {
            return self.view_about();
        }

        let toolbar = row![
            button("New").on_press(Message::NewNote),
            button("Open…").on_press(Message::OpenRequested),
            button("Save…").on_press(Message::SaveRequested),
            button("Paste image").on_press(Message::PasteImage),
            space::horizontal(),
            button("About").on_press(Message::OpenAbout),
        ]
        .spacing(8);

        let editor = text_editor(&self.content)
            .placeholder("Type your note… (⌘V pastes, drop a .txt to load)")
            .on_action(Message::EditorAction)
            .height(Fill);

        let mut body = column![toolbar, editor].spacing(10).padding(12);

        if let Some(handle) = &self.pasted_image {
            body = body.push(
                row![
                    image(handle.clone()).height(80),
                    button("Clear").on_press(Message::ClearImage),
                ]
                .spacing(8)
                .align_y(Center),
            );
        }

        let mode = match self.theme_mode {
            theme::Mode::Dark => "dark",
            theme::Mode::Light => "light",
            theme::Mode::None => "unknown",
        };
        body = body.push(
            row![
                text(&self.status).size(12),
                space::horizontal(),
                text(format!("system theme: {mode}")).size(12),
            ]
            .spacing(8),
        );

        body.into()
    }

    fn view_about(&self) -> Element<'_, Message> {
        container(
            column![
                text("Tray Notes").size(24),
                text("iced 0.14.0 — SPEC-4 shell integration test").size(13),
                text("tray-icon + muda + global-hotkey + rfd + arboard").size(12),
                button("Close").on_press(Message::CloseAbout),
            ]
            .spacing(12)
            .align_x(Center),
        )
        .center(Fill)
        .into()
    }
}

/// Encode an iced window screenshot (RGBA8) as PNG.
fn image_rs_save(shot: &window::Screenshot, path: &str) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        shot.size.width,
        shot.size.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&shot.rgba)
        .map_err(|e| e.to_string())?;
    Ok(())
}
