//! "Tray Notes" — OS shell integration on Freya 0.4 (SPEC-4).
//!
//! Everything *around* the window: menu-bar extra, native menubar, global
//! hotkey, native dialogs, image clipboard, file drop, notifications, live
//! dark mode, a second window, and close-to-tray.
//!
//! Freya owns the muda/tray-icon global event channels (the `tray` feature),
//! so menu + tray events arrive through one `LaunchConfig::with_tray` handler.
//! That handler runs on the renderer thread *outside* the reactive runtime, so
//! it pushes into a small queue that the UI drains on an 80 ms timer — the same
//! loop that polls `global-hotkey`'s crossbeam channel.

use std::{
    cell::RefCell,
    collections::VecDeque,
    path::PathBuf,
    rc::Rc,
    sync::Mutex,
    time::Duration,
};

use async_io::Timer;
use freya::{
    elements::image::ImageHandle,
    engine::prelude::AlphaType,
    prelude::*,
    text_edit::{
        EditableConfig,
        EditableEvent,
        EditorLine,
        TextEditor,
        UseEditable,
        use_editable,
    },
    tray::{
        Icon as TrayIconImage,
        TrayEvent,
        TrayIconBuilder,
        menu::{
            Menu,
            MenuItem,
            PredefinedMenuItem,
            Submenu,
            accelerator::{
                Accelerator,
                Code,
                Modifiers as AccelModifiers,
            },
        },
    },
};
use global_hotkey::{
    GlobalHotKeyEvent,
    GlobalHotKeyManager,
    HotKeyState,
    hotkey::{
        Code as HotCode,
        HotKey,
        Modifiers as HotModifiers,
    },
};

// ---------------------------------------------------------------- shell bridge

/// Menu/tray ids pushed by the `with_tray` handler (renderer thread) and
/// drained by the UI poll loop. A plain `Mutex<VecDeque<..>>` is enough:
/// Freya is single-threaded, this is a hand-off, not real concurrency.
static SHELL_QUEUE: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

fn push_action(id: &str) {
    SHELL_QUEUE.lock().unwrap().push_back(id.to_owned());
}

fn drain_actions() -> Vec<String> {
    SHELL_QUEUE.lock().unwrap().drain(..).collect()
}

const ID_TRAY_TOGGLE: &str = "tray-toggle";
const ID_TRAY_NEW: &str = "tray-new";
const ID_TRAY_QUIT: &str = "tray-quit";
const ID_FILE_NEW: &str = "file-new";
const ID_FILE_OPEN: &str = "file-open";
const ID_FILE_SAVE: &str = "file-save";
const ID_FILE_ABOUT: &str = "file-about";
const ID_EDIT_CUT: &str = "edit-cut";
const ID_EDIT_COPY: &str = "edit-copy";
const ID_EDIT_PASTE: &str = "edit-paste";
const ID_EDIT_SELECT_ALL: &str = "edit-select-all";

/// Window close was requested; the poll loop turns this into "hide to tray".
static CLOSE_REQUESTED: Mutex<bool> = Mutex::new(false);

// ---------------------------------------------------------------- main

fn main() {
    // Debug builds only: Freya installs its own release-mode panic hook that
    // shows a modal rfd "Fatal Error" dialog and calls `exit(1)` *before*
    // chaining to the previous hook, so a panic never reaches stderr in a
    // release run. This keeps panics greppable while developing.
    #[cfg(debug_assertions)]
    std::panic::set_hook(Box::new(|info| eprintln!("PANIC: {info}")));
    let selftest = std::env::var("TRAY_SELFTEST").is_ok();

    launch(
        LaunchConfig::new()
            // Keep the process (and the menu-bar extra) alive with no windows.
            .with_exit_on_close(false)
            .with_tray(build_tray, |event: TrayEvent, _ctx| {
                if let TrayEvent::Menu(menu_event) = event {
                    push_action(menu_event.id().as_ref());
                }
            })
            .with_window(
                WindowConfig::new(app)
                    .with_title("Tray Notes (freya)")
                    .with_size(500.0, 420.0)
                    .with_on_close(move |_ctx, _id| {
                        *CLOSE_REQUESTED.lock().unwrap() = true;
                        if selftest {
                            eprintln!("close-to-tray: keeping process alive");
                        }
                        CloseDecision::KeepOpen
                    }),
            ),
    )
}

/// Build the menu-bar extra. Called by Freya on the main thread once the event
/// loop is running (tray-icon requires exactly that).
fn build_tray() -> freya::tray::TrayIcon {
    let menu = Menu::new();
    let toggle = MenuItem::with_id(ID_TRAY_TOGGLE, "Show/Hide window", true, None);
    let new_note = MenuItem::with_id(ID_TRAY_NEW, "New note", true, None);
    let sep = PredefinedMenuItem::separator();
    let quit = MenuItem::with_id(ID_TRAY_QUIT, "Quit", true, None);
    let _ = menu.append_items(&[&toggle, &new_note, &sep, &quit]);
    retain_menu_items(vec![
        Box::new(toggle),
        Box::new(new_note),
        Box::new(sep),
        Box::new(quit),
    ]);

    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Tray Notes (freya)")
        .with_icon(tray_image())
        .with_icon_as_template(true)
        .build()
        .expect("failed to create the tray icon")
}

/// A 20×20 rounded-square glyph, generated so the crate needs no image asset
/// and no image decoder.
fn tray_image() -> TrayIconImage {
    const S: u32 = 20;
    let mut rgba = vec![0u8; (S * S * 4) as usize];
    for y in 0..S {
        for x in 0..S {
            let border = x < 2 || y < 2 || x >= S - 2 || y >= S - 2;
            let corner = (x < 4 || x >= S - 4) && (y < 4 || y >= S - 4);
            let line = (y == 7 || y == 11 || y == 15) && (5..15).contains(&x);
            let on = (border && !corner) || line;
            let i = ((y * S + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(if on {
                &[0, 0, 0, 255]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    TrayIconImage::from_rgba(rgba, S, S).expect("tray icon")
}

/// Native menubar (muda). Must be built on the main thread after the event loop
/// starts — the root component's first render is exactly that moment.
fn install_menubar() {
    let menu = Menu::new();

    let app_menu = Submenu::new("Tray Notes", true);
    let hide = PredefinedMenuItem::hide(None);
    let app_sep = PredefinedMenuItem::separator();
    let quit = PredefinedMenuItem::quit(None);
    let _ = app_menu.append_items(&[
        // NOTE: `PredefinedMenuItem::about(..)` panics inside muda's macOS icon
        // conversion (`png ... ZeroWidth`) as soon as AppKit realises the menu,
        // with or without `AboutMetadata`; in a release build Freya turns that
        // panic into a modal dialog and exits. Omitted. See FRICTION.md.
        &hide,
        &app_sep,
        &quit,
    ]);

    let file_menu = Submenu::new("File", true);
    let file_new = accel_item(ID_FILE_NEW, "New", Code::KeyN);
    let file_open = accel_item(ID_FILE_OPEN, "Open…", Code::KeyO);
    let file_save = accel_item(ID_FILE_SAVE, "Save…", Code::KeyS);
    let file_sep = PredefinedMenuItem::separator();
    let file_about = MenuItem::with_id(ID_FILE_ABOUT, "About window", true, None);
    let _ = file_menu.append_items(&[
        &file_new,
        &file_open,
        &file_save,
        &file_sep,
        &file_about,
    ]);

    // NOTE: these are custom items, not `PredefinedMenuItem::copy/paste/...`.
    // A menu key equivalent always wins over the focused view, and Freya's
    // `use_editable` implements ⌘X/⌘C/⌘V/⌘A itself — so predefined roles would
    // both no-op (no Cocoa responder implements them in the winit view) *and*
    // shadow the editor's own bindings. Routing them by hand keeps both the
    // menu items and the keystrokes working; see FRICTION.md.
    let edit_menu = Submenu::new("Edit", true);
    let cut = accel_item(ID_EDIT_CUT, "Cut", Code::KeyX);
    let copy = accel_item(ID_EDIT_COPY, "Copy", Code::KeyC);
    let paste = accel_item(ID_EDIT_PASTE, "Paste", Code::KeyV);
    let select_all = accel_item(ID_EDIT_SELECT_ALL, "Select All", Code::KeyA);
    let _ = edit_menu.append_items(&[&cut, &copy, &paste, &select_all]);

    let _ = menu.append_items(&[&app_menu, &file_menu, &edit_menu]);
    menu.init_for_nsapp();

    retain_menu_items(vec![
        Box::new(menu),
        Box::new(app_menu),
        Box::new(hide),
        Box::new(app_sep),
        Box::new(quit),
        Box::new(file_menu),
        Box::new(file_new),
        Box::new(file_open),
        Box::new(file_save),
        Box::new(file_sep),
        Box::new(file_about),
        Box::new(edit_menu),
        Box::new(cut),
        Box::new(copy),
        Box::new(paste),
        Box::new(select_all),
    ]);
}

fn accel_item(id: &str, text: &str, code: Code) -> MenuItem {
    MenuItem::with_id(
        id,
        text,
        true,
        Some(Accelerator::new(Some(AccelModifiers::META), code)),
    )
}

thread_local! {
    /// muda stores a **raw** `*const MenuChild` inside each `NSMenuItem` and does
    /// not retain it (there is a `FIXME` about exactly this in muda's source).
    /// If the Rust-side `Menu`/`Submenu`/`MenuItem` values are dropped — the
    /// natural thing to do after `init_for_nsapp()` / `TrayIconBuilder::with_menu`
    /// — those pointers dangle, and the *first* click on any menu item reads
    /// freed memory. Observed failure: the freed `MenuChild` was reinterpreted as
    /// a `PredefinedMenuItemType::About` with a zero-sized icon, panicking inside
    /// muda's PNG encoder, which Freya's release-mode panic hook turned into a
    /// modal "Fatal Error" dialog and `exit(1)`. Keeping every item alive for the
    /// process lifetime is the fix. See FRICTION.md.
    static MENU_ITEMS: RefCell<Vec<Box<dyn std::any::Any>>> = const { RefCell::new(Vec::new()) };
}

fn retain_menu_items(items: Vec<Box<dyn std::any::Any>>) {
    MENU_ITEMS.with(|store| store.borrow_mut().extend(items));
}

// ---------------------------------------------------------------- root app

fn app() -> impl IntoElement {
    let platform = Platform::get();
    let preferred_theme = platform.preferred_theme;
    let selftest = use_hook(|| std::env::var("TRAY_SELFTEST").is_ok());

    let mut editable = use_editable(String::new, || {
        EditableConfig::new().with_allow_changes(true)
    });
    let mut status = use_state(|| String::from("Ready — tray menu, ⌘⇧9, drop a .txt here"));
    let mut file: State<Option<PathBuf>> = use_state(|| None);
    let thumbnail: State<Option<(ImageHandle, u32, u32)>> = use_state(|| None);
    let mut visible = use_state(|| true);
    let about_open = use_state(|| false);

    let dark = *preferred_theme.read() == PreferredTheme::Dark;
    let mut theme = use_init_theme(light_theme);
    use_side_effect(move || {
        let dark = *preferred_theme.read() == PreferredTheme::Dark;
        theme.set(if dark { dark_theme() } else { light_theme() });
        eprintln!(
            "theme-changed: {}",
            if dark { "Dark" } else { "Light" }
        );
    });

    // ---- shell setup + the single poll loop -------------------------------
    use_hook(move || {
        install_menubar();

        // The manager must stay alive for the hotkey to stay registered.
        let manager = GlobalHotKeyManager::new();
        let registered = match &manager {
            Ok(manager) => manager
                .register(HotKey::new(
                    Some(HotModifiers::META | HotModifiers::SHIFT),
                    HotCode::Digit9,
                ))
                .is_ok(),
            Err(_) => false,
        };
        eprintln!(
            "shell: tray + menubar OK, global-hotkey {}",
            if registered { "OK" } else { "FAILED" }
        );

        spawn(async move {
            // Keep the manager owned by this task so it lives as long as the app.
            let _manager = manager;
            loop {
                Timer::after(Duration::from_millis(80)).await;

                if std::mem::replace(&mut *CLOSE_REQUESTED.lock().unwrap(), false) {
                    set_window_visible(false);
                    visible.set(false);
                    status.set("Hidden to the menu bar — ⌘⇧9 or the tray brings it back".into());
                }

                while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                    // The channel reports both Pressed and Released; toggling on
                    // both would net out to no change.
                    if event.state != HotKeyState::Pressed {
                        continue;
                    }
                    let now = !*visible.peek();
                    set_window_visible(now);
                    visible.set(now);
                    eprintln!("hotkey: toggled window -> visible={now}");
                }

                for id in drain_actions() {
                    handle_action(&id, editable, status, file, visible, about_open);
                }
            }
        });

        // ---- scripted verification hooks (research only) ------------------
        if std::env::var("TRAY_SELFTEST_IMAGE").is_ok() {
            spawn(async move {
                Timer::after(Duration::from_millis(600)).await;
                paste_image(thumbnail, status);
            });
        }
        if let Ok(path) = std::env::var("TRAY_SELFTEST_SAVE") {
            spawn(async move {
                Timer::after(Duration::from_millis(3500)).await;
                save_to(PathBuf::from(path), editable, status, file);
            });
        }
        if let Ok(path) = std::env::var("TRAY_SELFTEST_SHOT") {
            spawn(async move {
                Timer::after(Duration::from_millis(2000)).await;
                screenshot_window(path);
            });
        }
        if selftest {
            spawn(async move {
                Timer::after(Duration::from_secs(6)).await;
                eprintln!("selftest: done");
            });
        }
    });

    // ---- colours ----------------------------------------------------------
    let bg = if dark {
        Color::from_argb(255, 28, 30, 36)
    } else {
        Color::from_argb(255, 246, 247, 249)
    };
    let panel = if dark {
        Color::from_argb(255, 38, 41, 48)
    } else {
        Color::WHITE
    };
    let text = if dark {
        Color::from_argb(255, 228, 231, 238)
    } else {
        Color::from_argb(255, 26, 28, 33)
    };
    let muted = if dark {
        Color::from_argb(255, 140, 148, 163)
    } else {
        Color::from_argb(255, 106, 112, 124)
    };

    let thumb = thumbnail.read().clone();

    rect()
        .expanded()
        .content(Content::flex())
        .background(bg)
        .color(text)
        .padding(Gaps::new_all(10.))
        .spacing(8.)
        // File drop: a first-class element event, no winit plumbing.
        .on_file_drop(move |e: Event<FileEventData>| {
            let Some(path) = e.data().file_path.clone() else {
                return;
            };
            if path.extension().is_some_and(|ext| ext == "txt") {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        editable.editor_mut().write().set(&content);
                        status.set(format!("Loaded {}", path.display()));
                        eprintln!("file-dropped: {}", path.display());
                        file.set(Some(path));
                    }
                    Err(error) => status.set(format!("Load failed: {error}")),
                }
            } else {
                status.set(format!("Ignored drop (not .txt): {}", path.display()));
            }
        })
        .child(
            rect()
                .horizontal()
                .spacing(8.)
                .cross_align(Alignment::Center)
                .child(
                    Button::new()
                        .compact()
                        .on_press(move |_| {
                            push_action(ID_FILE_OPEN);
                        })
                        .child("Open…"),
                )
                .child(
                    Button::new()
                        .compact()
                        .on_press(move |_| {
                            push_action(ID_FILE_SAVE);
                        })
                        .child("Save…"),
                )
                .child(
                    Button::new()
                        .compact()
                        .on_press(move |_| paste_image(thumbnail, status))
                        .child("Paste image"),
                )
                .child(
                    Button::new()
                        .compact()
                        .on_press(move |_| {
                            push_action(ID_FILE_ABOUT);
                        })
                        .child("About"),
                ),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.))
                .content(Content::flex())
                .background(panel)
                .rounded_md()
                .border(Border::new().fill(muted.with_a(70)).width(1.))
                .child(TextArea {
                    editable,
                    color: text,
                }),
        )
        .maybe_child(thumb.map(|(handle, w, h)| {
            rect()
                .horizontal()
                .spacing(8.)
                .cross_align(Alignment::Center)
                .child(
                    image(handle)
                        .width(Size::px(96.))
                        .height(Size::px(64.))
                        .aspect_ratio(AspectRatio::Fit),
                )
                .child(
                    label()
                        .text(format!("clipboard image {w}×{h}"))
                        .font_size(11.)
                        .color(muted),
                )
        }))
        .child(
            label()
                .text(if about_open() {
                    format!("{} · About window open", status.read())
                } else {
                    status.read().clone()
                })
                .font_size(11.)
                .color(muted)
                .max_lines(2),
        )
}

/// The About window's root component (a second, independent window).
fn about_app() -> impl IntoElement {
    rect()
        .expanded()
        .center()
        .spacing(6.)
        .background(Color::from_argb(255, 246, 247, 249))
        .color(Color::from_argb(255, 26, 28, 33))
        .child(label().text("Tray Notes").font_size(18.))
        .child(label().text("Freya 0.4.0 · SPEC-4").font_size(12.))
        .child(
            label()
                .text("Second window, opened at runtime.")
                .font_size(11.),
        )
}

// ---------------------------------------------------------------- actions

#[allow(clippy::too_many_arguments)]
fn handle_action(
    id: &str,
    mut editable: UseEditable,
    mut status: State<String>,
    mut file: State<Option<PathBuf>>,
    mut visible: State<bool>,
    mut about_open: State<bool>,
) {
    match id {
        ID_TRAY_TOGGLE => {
            let now = !*visible.peek();
            set_window_visible(now);
            visible.set(now);
            eprintln!("tray: toggled window -> visible={now}");
        }
        ID_TRAY_NEW | ID_FILE_NEW => {
            editable.editor_mut().write().set("");
            file.set(None);
            status.set("New note".into());
            set_window_visible(true);
            visible.set(true);
            eprintln!("menu: new note");
        }
        ID_TRAY_QUIT => {
            eprintln!("tray: quit");
            std::process::exit(0);
        }
        ID_FILE_OPEN => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Plain text", &["txt"])
                .pick_file()
            {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        editable.editor_mut().write().set(&content);
                        status.set(format!("Loaded {}", path.display()));
                        eprintln!("dialog: opened {}", path.display());
                        file.set(Some(path));
                    }
                    Err(error) => status.set(format!("Load failed: {error}")),
                }
            } else {
                status.set("Open cancelled".into());
            }
        }
        ID_FILE_SAVE => {
            let suggested = file
                .peek()
                .as_ref()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| String::from("note.txt"));
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Plain text", &["txt"])
                .set_file_name(suggested)
                .save_file()
            {
                save_to(path, editable, status, file);
            } else {
                status.set("Save cancelled".into());
            }
        }
        ID_FILE_ABOUT => {
            if !*about_open.peek() {
                about_open.set(true);
                spawn(async move {
                    let id = Platform::get()
                        .launch_window(
                            WindowConfig::new(about_app)
                                .with_title("About — Tray Notes (freya)")
                                .with_size(360.0, 220.0),
                        )
                        .await;
                    eprintln!("multi-window: about window opened ({id:?})");
                });
            }
        }
        ID_EDIT_CUT | ID_EDIT_COPY | ID_EDIT_PASTE | ID_EDIT_SELECT_ALL => {
            // Replay the keystroke the editor already understands rather than
            // reimplementing clipboard handling.
            let ch = match id {
                ID_EDIT_CUT => "x",
                ID_EDIT_COPY => "c",
                ID_EDIT_PASTE => "v",
                _ => "a",
            };
            editable.process_event(EditableEvent::KeyDown {
                key: &Key::Character(ch.into()),
                modifiers: Modifiers::META,
            });
            eprintln!("menu: edit-{ch}");
        }
        other => eprintln!("menu: unhandled id {other}"),
    }
}

fn save_to(
    path: PathBuf,
    editable: UseEditable,
    mut status: State<String>,
    mut file: State<Option<PathBuf>>,
) {
    let content = editable.editor().peek().rope().to_string();
    match std::fs::write(&path, content) {
        Ok(()) => {
            status.set(format!("Saved {}", path.display()));
            eprintln!("dialog: saved {}", path.display());
            file.set(Some(path));
            // An *unbundled* cargo binary has no bundle identifier, and
            // mac-notification-sys then asks the OS to pick one — which pops a
            // modal "Choose Application" panel that blocks the UI thread
            // (observed). Borrowing an existing bundle id keeps the notification
            // path non-interactive; see FRICTION.md.
            let _ = notify_rust::set_application("com.apple.Terminal");
            match notify_rust::Notification::new()
                .summary("Tray Notes")
                .body("Note saved")
                .show()
            {
                Ok(_) => eprintln!("notification: OK"),
                Err(error) => eprintln!("notification: FAILED: {error}"),
            }
        }
        Err(error) => {
            status.set(format!("Save failed: {error}"));
            eprintln!("dialog: save FAILED: {error}");
        }
    }
}

fn paste_image(mut thumbnail: State<Option<(ImageHandle, u32, u32)>>, mut status: State<String>) {
    match arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
        Ok(img) => {
            let (w, h) = (img.width as u32, img.height as u32);
            match ImageHandle::from_rgba(w, h, Bytes::from(img.bytes.into_owned()), AlphaType::Unpremul)
            {
                Some(handle) => {
                    thumbnail.set(Some((handle, w, h)));
                    status.set(format!("Pasted image {w}×{h}"));
                    eprintln!("paste-image: OK {w}x{h}");
                }
                None => {
                    status.set("Clipboard image could not be decoded".into());
                    eprintln!("paste-image: FAILED (decode)");
                }
            }
        }
        Err(error) => {
            status.set(format!("No image on the clipboard ({error})"));
            eprintln!("paste-image: FAILED: {error}");
        }
    }
}

fn set_window_visible(visible: bool) {
    Platform::get().with_window(None, move |window| {
        window.set_visible(visible);
        if visible {
            window.focus_window();
        }
    });
}

/// Freya has no window-screenshot API, so the self-test asks the OS for the
/// window's frame and shells out to `screencapture`.
fn screenshot_window(path: String) {
    Platform::get().with_window(None, move |window| {
        let scale = window.scale_factor();
        let Ok(pos) = window.outer_position() else {
            eprintln!("screenshot: FAILED (no outer_position)");
            return;
        };
        let size = window.outer_size();
        let region = format!(
            "{},{},{},{}",
            (pos.x as f64 / scale).round() as i32,
            (pos.y as f64 / scale).round() as i32,
            (size.width as f64 / scale).round() as i32,
            (size.height as f64 / scale).round() as i32,
        );
        match std::process::Command::new("screencapture")
            .args(["-x", "-o", &format!("-R{region}"), &path])
            .status()
        {
            Ok(s) if s.success() => eprintln!("screenshot: saved {path}"),
            Ok(s) => eprintln!("screenshot: FAILED (status {s})"),
            Err(error) => eprintln!("screenshot: FAILED: {error}"),
        }
    });
}

// ---------------------------------------------------------------- text area

/// Multi-line plain-text editor.
///
/// Freya's `Input` component is single-line (`max_lines(1)`), so a multi-line
/// editor has to be assembled from the low-level `use_editable` hook: one
/// `paragraph` element per line, each with its own `ParagraphHolder` so hit
/// testing can map a click back to a character offset.
#[derive(Clone, PartialEq)]
struct TextArea {
    editable: UseEditable,
    color: Color,
}

impl Component for TextArea {
    fn render(&self) -> impl IntoElement {
        let mut editable = self.editable;
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut area = use_state(Area::default);
        let mut dragging = use_state(|| false);
        // Holders must survive across renders (the paragraph element fills them
        // during layout) but must NOT be reactive, or every layout pass would
        // schedule another render.
        let holders = use_hook(|| Rc::new(RefCell::new(Vec::<ParagraphHolder>::new())));

        let is_focused = focus().is_focused();

        let (lines, cursor_row, cursor_col) = {
            let editor = editable.editor().read();
            let n = editor.len_lines();
            holders
                .borrow_mut()
                .resize_with(n, ParagraphHolder::default);
            let lines: Vec<(String, Option<(usize, usize)>)> = (0..n)
                .map(|i| {
                    (
                        editor
                            .line(i)
                            .map(|l| l.text.trim_end_matches('\n').to_string())
                            .unwrap_or_default(),
                        editor.get_visible_selection(EditorLine::Paragraph(i)),
                    )
                })
                .collect();
            (lines, editor.cursor_row(), editor.cursor_col())
        };

        let color = self.color;
        let holders_for_down = holders.clone();
        let holders_for_move = holders.clone();

        let line_elements: Vec<Element> = lines
            .into_iter()
            .enumerate()
            .map(|(index, (text, selection))| {
                let holders = holders_for_down.clone();
                let holder = holders.borrow()[index].clone();
                paragraph()
                    .key(index)
                    .width(Size::fill())
                    .holder(holder)
                    .color(color)
                    .font_size(14.)
                    .line_height(1.3)
                    .cursor_index((is_focused && index == cursor_row).then_some(cursor_col))
                    .cursor_color(color)
                    .highlights(selection.map(|s| vec![s]))
                    .span(if text.is_empty() {
                        String::from(" ")
                    } else {
                        text
                    })
                    .on_focus_press(move |e: Event<FocusPressEventData>| {
                        e.stop_propagation();
                        e.prevent_default();
                        dragging.set(true);
                        editable.process_event(EditableEvent::Down {
                            location: e.element_location(),
                            editor_line: EditorLine::Paragraph(index),
                            holder: &holders.borrow()[index],
                        });
                        a11y_id.request_focus();
                    })
                    .into()
            })
            .collect();

        rect()
            .expanded()
            .content(Content::flex())
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::MultilineTextInput)
            .a11y_alt("Note text")
            // Clicking anywhere in the editor focuses it; clicking directly on
            // a line stops propagation and also positions the caret.
            .on_focus_press(move |e: Event<FocusPressEventData>| {
                e.stop_propagation();
                e.prevent_default();
                a11y_id.request_focus();
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                e.stop_propagation();
                editable.process_event(EditableEvent::KeyDown {
                    key: &e.key,
                    modifiers: e.modifiers,
                });
            })
            .on_key_up(move |e: Event<KeyboardEventData>| {
                editable.process_event(EditableEvent::KeyUp { key: &e.key });
            })
            .on_global_pointer_move(move |e: Event<PointerEventData>| {
                if !*dragging.peek() {
                    return;
                }
                let origin = area.peek().origin;
                let mut location = e.global_location();
                location.x -= origin.x as f64;
                location.y -= origin.y as f64;
                let row = {
                    let editor = editable.editor().peek();
                    editor.cursor_row().min(editor.len_lines().saturating_sub(1))
                };
                let holder = holders_for_move.borrow()[row].clone();
                editable.process_event(EditableEvent::Move {
                    location,
                    editor_line: EditorLine::Paragraph(row),
                    holder: &holder,
                });
            })
            .on_global_pointer_press(move |_: Event<PointerEventData>| {
                if *dragging.peek() {
                    dragging.set(false);
                    editable.process_event(EditableEvent::Release);
                }
            })
            .child(
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::flex(1.))
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new_all(8.))
                            .on_sized(move |e: Event<SizedEventData>| area.set(e.visible_area))
                            .children(line_elements),
                    ),
            )
    }
}
