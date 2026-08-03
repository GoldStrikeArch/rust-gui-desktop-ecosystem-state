//! "Tray Notes" — OS shell integration (SPEC-4), vizia 0.4.
//!
//! Architecture notes (research-relevant):
//! - vizia's builder closure and `Model::event` both run on the **main
//!   thread**, and a `Model` has no `Send` bound, so the `!Send` `TrayIcon`
//!   and `GlobalHotKeyManager` can simply live in application state. The
//!   shell is created on the first tick of a 100 ms timer rather than in the
//!   builder, so it happens after the winit run loop is up (tray-icon #90).
//! - tray / menubar / hotkey all deliver through **global crossbeam
//!   channels** with no waker integration, so the same 100 ms
//!   `cx.add_timer` drains all three.
//! - **Close-to-tray** works because vizia dispatches an event to a model
//!   *before* the view on the same entity: the app model sits on the window
//!   entity, sees `WindowEvent::WindowClose` first, calls `meta.consume()`
//!   and answers with `WindowEvent::SetVisible(false)`. The `Window` view
//!   never runs its close path, so the process stays alive.
//! - **File drop** is free: winit's `DroppedFile` is surfaced as
//!   `WindowEvent::Drop(DropData::File(path))`.
//!
//! Verification hooks (research only, all opt-in via env vars):
//!   TRAY_SELFTEST=1            evidence lines on stderr
//!   TRAY_SELFTEST_IMAGE=1      run the clipboard-image paste at startup
//!   TRAY_SELFTEST_SAVE=<path>  write the note to <path> + fire the notification
//!   TRAY_SELFTEST_SHOT=<path>  park the window at a known position and
//!                              `screencapture -R` it after 3 s

use std::path::PathBuf;

use global_hotkey::hotkey::{Code as HotCode, HotKey, Modifiers as HotMods};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use tray_icon::menu::accelerator::{Accelerator, Code as AccelCode, Modifiers as AccelMods};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use vizia::prelude::*;

/// Fixed position used by the screenshot hook so the capture rect is known.
const SHOT_RECT: (i32, i32, u32, u32) = (120, 120, 500, 452);

fn main() -> Result<(), ApplicationError> {
    let selftest = std::env::var_os("TRAY_SELFTEST").is_some();

    Application::new(move |cx| {
        cx.add_stylesheet(STYLE).expect("failed to add stylesheet");

        let note = Signal::new(String::from(
            "Tray Notes (vizia)\n\nType here. ⌘S saves, ⌘O opens, ⌘N clears.\n\
             Closing the window hides it to the menu-bar extra; ⌘⇧9 toggles it \
             from anywhere.",
        ));
        let status = Signal::new(String::from("ready"));
        let theme = Signal::new(String::from("system"));
        let thumbnail = Signal::new(None::<String>);
        let show_about = Signal::new(false);
        let editor = Signal::new(None::<Entity>);

        // 100 ms poll of the three global channels (tray / menu / hotkey).
        let timer = cx.add_timer(Duration::from_millis(100), None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(TrayEvent::Poll);
            }
        });

        TrayNotes {
            note,
            status,
            theme,
            thumbnail,
            show_about,
            editor,
            file: None,
            shell: None,
            selftest,
            booted: false,
        }
        .build(cx);

        cx.start_timer(timer);

        VStack::new(cx, move |cx| {
            let editor_handle = Textbox::new_multiline(cx, note, true)
                .class("editor")
                .width(Stretch(1.0))
                .height(Stretch(1.0))
                .on_edit(|cx, text| cx.emit(TrayEvent::SetNote(text)));
            let entity = editor_handle.entity();
            editor.set(Some(entity));

            // Clipboard-image thumbnail (arboard -> PNG -> Skia image).
            Binding::new(cx, thumbnail, move |cx| {
                if let Some(key) = thumbnail.get() {
                    Image::new(cx, key).class("thumb");
                }
            });

            HStack::new(cx, |cx| {
                Button::new(cx, |cx| Label::new(cx, "Paste image"))
                    .on_press(|cx| cx.emit(TrayEvent::PasteImage));
                Button::new(cx, |cx| Label::new(cx, "About"))
                    .variant(ButtonVariant::Outline)
                    .on_press(|cx| cx.emit(TrayEvent::ShowAbout));
                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                Label::new(cx, theme).class("dim");
            })
            .class("toolbar");

            Label::new(cx, status).class("status");
        })
        .class("app")
        // File drop straight from Finder: winit -> vizia, no helper crate.
        .on_drop(|cx, data| {
            if let DropData::File(path) = data {
                cx.emit(TrayEvent::LoadFile(path));
            }
        });

        // Second window (multi-window test).
        Binding::new(cx, show_about, move |cx| {
            if show_about.get() {
                Window::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(cx, "Tray Notes").class("about-title");
                        Label::new(cx, "vizia 0.4.0 · SPEC-4 shell-integration probe")
                            .class("dim");
                        Button::new(cx, |cx| Label::new(cx, "Close"))
                            .on_press(|cx| cx.emit(WindowEvent::WindowClose));
                    })
                    .class("about");
                })
                .on_close(|cx| cx.emit(TrayEvent::AboutClosed))
                .title("About Tray Notes")
                .inner_size((360, 180));
            }
        });
    })
    .title("Tray Notes (vizia)")
    .inner_size((500, 420))
    .run()
}

// ---------------------------------------------------------------------------
// Shell (tray + native menubar + global hotkey)
// ---------------------------------------------------------------------------

struct Shell {
    _tray: TrayIcon,
    _hotkeys: GlobalHotKeyManager,
    _menubar: Menu,
    tray_toggle: MenuId,
    tray_new: MenuId,
    tray_quit: MenuId,
    menu_new: MenuId,
    menu_open: MenuId,
    menu_save: MenuId,
    menu_about: MenuId,
    menu_cut: MenuId,
    menu_copy: MenuId,
    menu_paste: MenuId,
    menu_select_all: MenuId,
    hotkey_id: u32,
}

fn tray_icon_rgba() -> tray_icon::Icon {
    // A 22x22 rounded-ish note glyph drawn by hand — no asset file.
    const SIZE: u32 = 22;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let edge = x < 3 || x >= SIZE - 3 || y < 2 || y >= SIZE - 2;
            let rule = !edge && (y % 5 == 0);
            let (r, g, b, a) = if edge {
                (0, 0, 0, 0)
            } else if rule {
                (40, 40, 40, 255)
            } else {
                (235, 235, 235, 255)
            };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).expect("tray icon")
}

impl Shell {
    fn build() -> Result<Self, String> {
        // ---- tray menu -------------------------------------------------
        let tray_toggle = MenuItem::new("Show/Hide window", true, None);
        let tray_new = MenuItem::new("New note", true, None);
        let tray_quit = MenuItem::new("Quit", true, None);
        let tray_menu = Menu::new();
        tray_menu
            .append_items(&[
                &tray_toggle,
                &tray_new,
                &PredefinedMenuItem::separator(),
                &tray_quit,
            ])
            .map_err(|e| e.to_string())?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Tray Notes (vizia)")
            .with_icon(tray_icon_rgba())
            .build()
            .map_err(|e| e.to_string())?;

        // ---- native menubar --------------------------------------------
        let menu_new = MenuItem::new(
            "New",
            true,
            Some(Accelerator::new(Some(AccelMods::META), AccelCode::KeyN)),
        );
        let menu_open = MenuItem::new(
            "Open…",
            true,
            Some(Accelerator::new(Some(AccelMods::META), AccelCode::KeyO)),
        );
        let menu_save = MenuItem::new(
            "Save…",
            true,
            Some(Accelerator::new(Some(AccelMods::META), AccelCode::KeyS)),
        );
        let menu_about = MenuItem::new("About Tray Notes", true, None);
        // Deliberately NOT `PredefinedMenuItem::cut/copy/paste/select_all`:
        // those install Cocoa responder-chain selectors that winit's NSView
        // does not implement, so they no-op AND their key equivalents
        // swallow ⌘X/⌘C/⌘V/⌘A before the app's own text bindings see them.
        // Custom items routed by hand to the editor keep the shortcuts alive.
        let menu_cut = MenuItem::new(
            "Cut",
            true,
            Some(Accelerator::new(Some(AccelMods::META), AccelCode::KeyX)),
        );
        let menu_copy = MenuItem::new(
            "Copy",
            true,
            Some(Accelerator::new(Some(AccelMods::META), AccelCode::KeyC)),
        );
        let menu_paste = MenuItem::new(
            "Paste",
            true,
            Some(Accelerator::new(Some(AccelMods::META), AccelCode::KeyV)),
        );
        let menu_select_all = MenuItem::new(
            "Select All",
            true,
            Some(Accelerator::new(Some(AccelMods::META), AccelCode::KeyA)),
        );

        let app_menu = Submenu::new("Tray Notes", true);
        app_menu
            .append_items(&[
                &menu_about,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(Some("Quit Tray Notes")),
            ])
            .map_err(|e| e.to_string())?;

        let file_menu = Submenu::new("File", true);
        file_menu
            .append_items(&[&menu_new, &menu_open, &menu_save])
            .map_err(|e| e.to_string())?;

        let edit_menu = Submenu::new("Edit", true);
        edit_menu
            .append_items(&[
                &menu_cut,
                &menu_copy,
                &menu_paste,
                &PredefinedMenuItem::separator(),
                &menu_select_all,
            ])
            .map_err(|e| e.to_string())?;

        let menubar = Menu::new();
        menubar
            .append_items(&[&app_menu, &file_menu, &edit_menu])
            .map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        menubar.init_for_nsapp();

        // ---- global hotkey ---------------------------------------------
        let hotkeys = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        let hotkey = HotKey::new(Some(HotMods::META | HotMods::SHIFT), HotCode::Digit9);
        hotkeys.register(hotkey).map_err(|e| e.to_string())?;

        Ok(Self {
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
            hotkey_id: hotkey.id(),
            _tray: tray,
            _hotkeys: hotkeys,
            _menubar: menubar,
        })
    }
}


/// Fires the "Note saved" system notification.
///
/// Two macOS traps live in these six lines, both found the hard way:
///
/// 1. `mac-notification-sys` (under `notify-rust`) defaults to the bundle
///    identifier `"use_default"`. From an UNBUNDLED cargo binary that id
///    does not resolve, and macOS 26 pops a modal *"Choose Application —
///    Where is use_default?"* panel in front of the app while `.show()`
///    still returns `Ok`. `set_application` with a real, already-approved
///    bundle id is the documented workaround.
/// 2. The notification MUST NOT be sent from inside vizia's event dispatch.
///    `NotificationHandle::drop` calls into NSUserNotification, which spins
///    the Cocoa run loop and re-enters winit's event handler; winit then
///    panics with *"tried to handle event while another event is currently
///    being handled"* inside a non-unwinding block and the process aborts.
///    Sending from a plain background thread avoids the re-entrancy.
fn notify_saved(selftest: bool) {
    std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        let _ = notify_rust::set_application("com.apple.Terminal");
        let result = notify_rust::Notification::new()
            .summary("Tray Notes")
            .body("Note saved")
            .show();
        if selftest {
            match result {
                Ok(_) => eprintln!("notification: OK"),
                Err(error) => eprintln!("notification: FAILED: {error}"),
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct TrayNotes {
    note: Signal<String>,
    status: Signal<String>,
    theme: Signal<String>,
    thumbnail: Signal<Option<String>>,
    show_about: Signal<bool>,
    editor: Signal<Option<Entity>>,
    file: Option<PathBuf>,
    shell: Option<Shell>,
    selftest: bool,
    booted: bool,
}

enum TrayEvent {
    Poll,
    SetNote(String),
    ToggleWindow,

    NewNote,
    OpenRequested,
    SaveRequested,
    SaveTo(PathBuf),
    LoadFile(PathBuf),
    PasteImage,
    ShowAbout,
    AboutClosed,
    Quit,
}

impl TrayNotes {
    fn trace(&self, line: impl AsRef<str>) {
        if self.selftest {
            eprintln!("{}", line.as_ref());
        }
    }

    fn save_to(&mut self, cx: &mut EventContext, path: PathBuf) {
        match std::fs::write(&path, self.note.get()) {
            Ok(()) => {
                self.status.set(format!("Saved {}", path.display()));
                self.trace(format!("file-saved: {}", path.display()));
                self.file = Some(path);
                notify_saved(self.selftest);
            }
            Err(error) => {
                self.status.set(format!("Save failed: {error}"));
                self.trace(format!("file-saved: FAILED: {error}"));
            }
        }
        let _ = cx;
    }

    fn load_file(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.note.set(text);
                self.status.set(format!("Loaded {}", path.display()));
                self.trace(format!("file-loaded: {}", path.display()));
                self.file = Some(path);
            }
            Err(error) => {
                self.status.set(format!("Load failed: {error}"));
                self.trace(format!("file-loaded: FAILED: {error}"));
            }
        }
    }

    fn paste_image(&mut self, cx: &mut EventContext) {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(error) => {
                self.status.set(format!("Clipboard unavailable: {error}"));
                self.trace(format!("paste-image: FAILED: {error}"));
                return;
            }
        };

        match clipboard.get_image() {
            Ok(img) => {
                let (width, height) = (img.width as u32, img.height as u32);
                let buffer =
                    match image::RgbaImage::from_raw(width, height, img.bytes.into_owned()) {
                        Some(buffer) => buffer,
                        None => {
                            self.trace("paste-image: FAILED: bad RGBA buffer");
                            return;
                        }
                    };
                let mut png = std::io::Cursor::new(Vec::new());
                if let Err(error) =
                    image::DynamicImage::ImageRgba8(buffer).write_to(&mut png, image::ImageFormat::Png)
                {
                    self.trace(format!("paste-image: FAILED: {error}"));
                    return;
                }
                // `Context::load_image` is not reachable from an
                // `EventContext`; the route that *is* reachable at event
                // time is `ContextProxy::load_image`, which decodes the PNG
                // into a Skia image and queues an internal LoadImage event.
                let key = format!("clipboard-image-{width}x{height}");
                let mut proxy = cx.get_proxy();
                if let Err(error) = proxy.load_image(
                    key.clone(),
                    &png.into_inner(),
                    ImageRetentionPolicy::Forever,
                ) {
                    self.trace(format!("paste-image: FAILED: {error}"));
                    return;
                }
                self.thumbnail.set(Some(key));
                self.status.set(format!("Pasted image {width}x{height}"));
                self.trace(format!("paste-image: OK {width}x{height}"));
            }
            Err(error) => {
                self.status.set(format!("No image on clipboard: {error}"));
                self.trace(format!("paste-image: FAILED: {error}"));
            }
        }
    }

    fn to_editor(&self, cx: &mut EventContext, event: TextEvent) {
        if let Some(entity) = self.editor.get() {
            cx.emit_to(entity, event);
        }
    }
}

impl Model for TrayNotes {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.take(|tray_event, _| match tray_event {
            TrayEvent::Poll => {
                if !self.booted {
                    self.booted = true;
                    // Created on the first tick: the winit run loop is up by
                    // now, which NSStatusItem creation requires.
                    match Shell::build() {
                        Ok(shell) => {
                            self.shell = Some(shell);
                            self.status.set(String::from(
                                "tray + native menubar + ⌘⇧9 hotkey registered",
                            ));
                            self.trace("shell: tray + menubar + hotkey OK");
                        }
                        Err(error) => {
                            self.status.set(format!("Shell setup failed: {error}"));
                            self.trace(format!("shell: FAILED: {error}"));
                        }
                    }

                    // Scripted verification hooks.
                    if std::env::var_os("TRAY_SELFTEST_IMAGE").is_some() {
                        cx.emit(TrayEvent::PasteImage);
                    }
                    if let Ok(path) = std::env::var("TRAY_SELFTEST_SAVE") {
                        cx.emit(TrayEvent::SaveTo(PathBuf::from(path)));
                    }
                    if let Ok(path) = std::env::var("TRAY_SELFTEST_SHOT") {
                        // Park the window at a known position so the capture
                        // rect is deterministic (vizia exposes no window-id
                        // or screenshot API), then shoot from a plain thread.
                        cx.emit(WindowEvent::SetPosition(WindowPosition::new(
                            SHOT_RECT.0,
                            SHOT_RECT.1,
                        )));
                        let selftest = self.selftest;
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            let rect = format!(
                                "{},{},{},{}",
                                SHOT_RECT.0, SHOT_RECT.1, SHOT_RECT.2, SHOT_RECT.3
                            );
                            let result = std::process::Command::new("/usr/sbin/screencapture")
                                .args(["-x", "-o", "-R", &rect, &path])
                                .status();
                            if selftest {
                                match result {
                                    Ok(status) if status.success() => {
                                        eprintln!("screenshot: saved {path}")
                                    }
                                    Ok(status) => {
                                        eprintln!("screenshot: FAILED status {status}")
                                    }
                                    Err(error) => eprintln!("screenshot: FAILED {error}"),
                                }
                            }
                        });
                    }
                }

                let Some(shell) = &self.shell else { return };

                // Three global channels, no waker integration -> polled.
                while let Ok(menu_event) = MenuEvent::receiver().try_recv() {
                    let id = menu_event.id();
                    if *id == shell.tray_toggle {
                        cx.emit(TrayEvent::ToggleWindow);
                    } else if *id == shell.tray_new || *id == shell.menu_new {
                        cx.emit(TrayEvent::NewNote);
                    } else if *id == shell.tray_quit {
                        cx.emit(TrayEvent::Quit);
                    } else if *id == shell.menu_open {
                        cx.emit(TrayEvent::OpenRequested);
                    } else if *id == shell.menu_save {
                        cx.emit(TrayEvent::SaveRequested);
                    } else if *id == shell.menu_about {
                        cx.emit(TrayEvent::ShowAbout);
                    } else if *id == shell.menu_cut {
                        self.to_editor(cx, TextEvent::Cut);
                        self.trace("edit-menu: cut");
                    } else if *id == shell.menu_copy {
                        self.to_editor(cx, TextEvent::Copy);
                        self.trace("edit-menu: copy");
                    } else if *id == shell.menu_paste {
                        self.to_editor(cx, TextEvent::Paste);
                        self.trace("edit-menu: paste");
                    } else if *id == shell.menu_select_all {
                        self.to_editor(cx, TextEvent::SelectAll);
                        self.trace("edit-menu: select-all");
                    }
                }

                while let Ok(tray) = TrayIconEvent::receiver().try_recv() {
                    if let TrayIconEvent::DoubleClick { .. } = tray {
                        cx.emit(TrayEvent::ToggleWindow);
                    }
                }

                while let Ok(hotkey) = GlobalHotKeyEvent::receiver().try_recv() {
                    if hotkey.id == shell.hotkey_id
                        && hotkey.state == global_hotkey::HotKeyState::Pressed
                    {
                        self.trace("hotkey: cmd+shift+9");
                        cx.emit(TrayEvent::ToggleWindow);
                    }
                }
            }

            TrayEvent::SetNote(text) => self.note.set(text),

            TrayEvent::ToggleWindow => {
                let visible = cx.window_is_visible();
                self.trace(format!(
                    "toggle-window: {}",
                    if visible { "hiding" } else { "showing" }
                ));
                cx.set_window_visible(!visible);
            }

            TrayEvent::NewNote => {
                self.note.set(String::new());
                self.file = None;
                self.thumbnail.set(None);
                self.status.set(String::from("New note"));
                self.trace("new-note");
                cx.set_window_visible(true);
            }

            TrayEvent::OpenRequested => {
                if let Some(path) =
                    rfd::FileDialog::new().add_filter("Text", &["txt"]).pick_file()
                {
                    self.load_file(path);
                } else {
                    self.trace("file-open: cancelled");
                }
            }

            TrayEvent::SaveRequested => {
                let path = self.file.clone().or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("Text", &["txt"])
                        .set_file_name("note.txt")
                        .save_file()
                });
                match path {
                    Some(path) => self.save_to(cx, path),
                    None => self.trace("file-save: cancelled"),
                }
            }

            TrayEvent::SaveTo(path) => self.save_to(cx, path),
            TrayEvent::LoadFile(path) => self.load_file(path),
            TrayEvent::PasteImage => self.paste_image(cx),

            TrayEvent::ShowAbout => {
                self.show_about.set(true);
                self.trace("about-window: opened");
            }
            TrayEvent::AboutClosed => {
                self.show_about.set(false);
                self.trace("about-window: closed");
            }

            TrayEvent::Quit => {
                self.trace("quit");
                std::process::exit(0);
            }

        });

        event.map(|window_event, meta| match window_event {
            // Close-to-tray. A model attached to the window entity is
            // visited BEFORE the `Window` view on the same entity, so
            // consuming here stops vizia's own close path (which would
            // remove the last window and exit the run loop).
            WindowEvent::WindowClose if meta.target == cx.current() => {
                self.trace("close-to-tray: hidden");
                self.status.set(String::from("Hidden to the menu bar — ⌘⇧9 to return"));
                meta.consume();
                cx.set_window_visible(false);
            }

            WindowEvent::ThemeChanged(mode) => {
                let name = format!("{mode:?}");
                self.theme.set(name.clone());
                self.trace(format!("theme-changed: {name}"));
            }

            // Finder file drop (winit DroppedFile -> vizia DropData::File).
            WindowEvent::Drop(DropData::File(path)) => {
                self.trace(format!("file-drop: {}", path.display()));
                self.load_file(path.clone());
            }

            _ => {}
        });
    }
}

// ---------------------------------------------------------------------------
// Small helpers over the window entity
// ---------------------------------------------------------------------------

trait WindowVisibility {
    fn window_is_visible(&self) -> bool;
    fn set_window_visible(&mut self, visible: bool);
}

/// vizia has no getter for window visibility, so the app tracks it itself.
static VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

impl WindowVisibility for EventContext<'_> {
    fn window_is_visible(&self) -> bool {
        VISIBLE.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn set_window_visible(&mut self, visible: bool) {
        VISIBLE.store(visible, std::sync::atomic::Ordering::Relaxed);
        self.emit(WindowEvent::SetVisible(visible));
        if visible {
            self.emit(WindowEvent::SetMinimized(false));
        }
    }
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.app {
    width: 1s;
    height: 1s;
    padding: 10px;
    vertical-gap: 8px;
}

.editor { font-size: 13px; }
.thumb { width: 1s; height: 110px; corner-radius: 6px; background-size: contain; }

.toolbar { height: auto; horizontal-gap: 8px; alignment: center; }
.dim { color: #8a8a8a; font-size: 12px; height: auto; }
.status { height: auto; font-size: 12px; color: #8a8a8a; }

.about { width: 1s; height: 1s; padding: 16px; vertical-gap: 10px; alignment: center; }
.about-title { font-size: 18px; height: auto; }
"#;
