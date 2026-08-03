//! "Tray Notes" — OS shell integration test (SPEC-4), floem git @ 778bb5f2.
//!
//! Architecture notes (research-relevant):
//! - floem covers a surprising amount of SPEC-4 natively: native menubar
//!   (`floem::Menu` + `set_window_menu`, action CLOSURES — no channels),
//!   file dialogs (`floem::open_file/save_as`, rfd inside), text clipboard
//!   (`floem::Clipboard`), typed `FileDragDrop` events, live dark-mode
//!   restyling + a `ThemeChanged` listener, `new_window` multi-window,
//!   `WindowIdExt::set_visible` for hide-to-tray, and `AppEvent::Reopen`
//!   for Dock-icon reopen (which iced could not express at all).
//! - The tray icon itself needs `tray-icon`. muda-version choreography is
//!   load-bearing: floem pins muda =0.17 and owns that instance's ONE global
//!   `MenuEvent` handler slot; tray-icon 0.24 bundles its own muda 0.19, so
//!   this app hooks THAT instance's handler and forwards events to the UI
//!   thread with an `ExtSendTrigger` (floem's cross-thread wake primitive) —
//!   zero polling anywhere, unlike the iced port's 100 ms channel poll.
//! - Edit-menu clipboard roles: muda's PredefinedMenuItem cut/copy/paste use
//!   Cocoa responder-chain selectors that floem's winit-fork NSView does not
//!   implement (same trap as iced). Custom items are routed by hand into the
//!   editor through `Document::run_command(ClipboardCut/Copy/Paste)` — the
//!   Lapce editor core executes them against the real system clipboard.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use floem::action::{exec_after, set_window_menu};
use floem::ext_event::{ExtSendTrigger, create_trigger, register_ext_trigger};
use floem::kurbo::Size;
use floem::muda::PredefinedMenuItem;
use floem::muda::accelerator::{Accelerator, Code as AccelCode, Modifiers as AccelModifiers};
use floem::prelude::*;
use floem::reactive::Effect;
use floem::views::editor::command::Command;
use floem::views::editor::core::command::{EditCommand, MultiSelectionCommand};
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::core::editor::EditType;
use floem::views::editor::core::selection::Selection;
use floem::window::{WindowConfig, WindowId};
use floem::{
    AppConfig, AppEvent, Application, FileDialogOptions, FileSpec, Menu, WindowIdExt,
    close_window, new_window, open_file, quit_app, save_as,
};

use global_hotkey::hotkey::{Code as HotKeyCode, HotKey, Modifiers as HotKeyModifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{Menu as TrayMenu, MenuEvent, MenuId, MenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

// ---------------------------------------------------------------------------
// Cross-thread shell-event plumbing (tray menu + global hotkey → UI thread)
// ---------------------------------------------------------------------------

enum ShellMsg {
    TrayMenu(MenuId),
    Hotkey(u32),
}

static SHELL_EVENTS: Mutex<VecDeque<ShellMsg>> = Mutex::new(VecDeque::new());

thread_local! {
    /// !Send shell objects; dropping the TrayIcon removes it from the bar.
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
    static MAIN_WINDOW: Cell<Option<WindowId>> = const { Cell::new(None) };
}

struct Shell {
    _tray: TrayIcon,
    _hotkeys: GlobalHotKeyManager,
    hotkey_id: u32,
    tray_toggle: MenuId,
    tray_new: MenuId,
    tray_quit: MenuId,
}

/// 22×22 template icon (black + alpha): a rounded "note" square with lines.
fn tray_icon_rgba() -> tray_icon::Icon {
    const S: usize = 22;
    let mut rgba = vec![0u8; S * S * 4];
    let mut put = |x: usize, y: usize, a: u8| {
        let i = (y * S + x) * 4;
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
    /// Must run on the main thread after the NSApp run loop started
    /// (tray-icon issue #90). floem's timer callbacks satisfy both.
    fn create(trigger: ExtSendTrigger) -> Result<Self, String> {
        let tray_toggle = MenuItem::new("Show/Hide Window", true, None);
        let tray_new = MenuItem::new("New Note", true, None);
        let tray_quit = MenuItem::new("Quit Tray Notes", true, None);
        let tray_menu = TrayMenu::new();
        tray_menu
            .append_items(&[
                &tray_toggle,
                &tray_new,
                &tray_icon::menu::PredefinedMenuItem::separator(),
                &tray_quit,
            ])
            .map_err(|e| format!("tray menu: {e}"))?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Tray Notes (floem)")
            .with_icon(tray_icon_rgba())
            .with_icon_as_template(true)
            .build()
            .map_err(|e| format!("tray icon: {e}"))?;

        // tray-icon's muda 0.19 handler slot is free (floem owns muda 0.17's).
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            SHELL_EVENTS.lock().unwrap().push_back(ShellMsg::TrayMenu(event.id().clone()));
            register_ext_trigger(trigger);
        }));

        let hotkeys = GlobalHotKeyManager::new().map_err(|e| format!("hotkey manager: {e}"))?;
        let hotkey = HotKey::new(
            Some(HotKeyModifiers::META | HotKeyModifiers::SHIFT),
            HotKeyCode::Digit9,
        );
        hotkeys.register(hotkey).map_err(|e| format!("hotkey register: {e}"))?;

        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state() == HotKeyState::Pressed {
                SHELL_EVENTS.lock().unwrap().push_back(ShellMsg::Hotkey(event.id()));
                register_ext_trigger(trigger);
            }
        }));

        Ok(Self {
            _tray: tray,
            _hotkeys: hotkeys,
            hotkey_id: hotkey.id(),
            tray_toggle: tray_toggle.id().clone(),
            tray_new: tray_new.id().clone(),
            tray_quit: tray_quit.id().clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

fn main() {
    // exit_on_close already defaults to false on macOS; make it explicit —
    // the process must survive with the window hidden (close-to-tray).
    Application::new_with_config(AppConfig::default().exit_on_close(false))
        // Dock-icon reopen (macOS `applicationShouldHandleReopen`): floem
        // surfaces it as AppEvent::Reopen — re-show the hidden main window.
        .on_event(|event| {
            if let AppEvent::Reopen { has_visible_windows } = event
                && !has_visible_windows
                && let Some(id) = MAIN_WINDOW.with(|w| w.get())
            {
                eprintln!("reopen: showing main window");
                id.set_visible(true);
            }
        })
        .window(
            main_view,
            Some(
                WindowConfig::default()
                    .title("Tray Notes (floem)")
                    .size(Size::new(500.0, 420.0)),
            ),
        )
        .run();
}

fn main_view(window_id: WindowId) -> impl IntoView {
    MAIN_WINDOW.with(|w| w.set(Some(window_id)));

    let status = RwSignal::new(String::from("Ready — tray menu, ⌘⇧9, drop a .txt here"));
    let theme_mode = RwSignal::new(String::from("unknown"));
    let image_png: RwSignal<Option<Vec<u8>>> = RwSignal::new(None);
    let file: RwSignal<Option<PathBuf>> = RwSignal::new(None);
    let about_window: RwSignal<Option<WindowId>> = RwSignal::new(None);

    // ----- editor (Lapce editor core; multiline, selection, clipboard) -----
    let editor_view = text_editor("");
    let editor = editor_view.editor().clone();
    let doc = editor_view.doc();

    let editor_text = {
        let doc = doc.clone();
        move || {
            let rope = doc.text();
            rope.slice_to_cow(0..rope.len()).into_owned()
        }
    };
    let set_editor_text = {
        let doc = doc.clone();
        move |text: &str| {
            let len = doc.text().len();
            doc.edit_single(
                Selection::region(0, len, CursorAffinity::Backward),
                text,
                EditType::Paste,
            );
        }
    };
    let run_editor_command = {
        let doc = doc.clone();
        let editor = editor.clone();
        move |cmd: Command| {
            doc.run_command(&editor, &cmd, None, floem::ui_events::keyboard::Modifiers::default());
        }
    };

    // ----- shared actions ---------------------------------------------------
    let toggle_window = move || {
        if window_id.is_visible() {
            eprintln!("toggle-window: hiding");
            status.set(String::from("Hidden to tray (⌘⇧9 or tray to show)"));
            window_id.set_visible(false);
        } else {
            eprintln!("toggle-window: showing");
            window_id.set_visible(true);
        }
    };

    let new_note = {
        let set_editor_text = set_editor_text.clone();
        move || {
            eprintln!("new-note");
            set_editor_text("");
            file.set(None);
            status.set(String::from("New note"));
            window_id.set_visible(true);
        }
    };

    let load_path = {
        let set_editor_text = set_editor_text.clone();
        move |path: &Path| match std::fs::read_to_string(path) {
            Ok(text) => {
                set_editor_text(&text);
                status.set(format!("Loaded {}", path.display()));
                eprintln!("file-loaded: {}", path.display());
                file.set(Some(path.to_path_buf()));
            }
            Err(error) => status.set(format!("Load failed: {error}")),
        }
    };

    let open_dialog = {
        let load_path = load_path.clone();
        move || {
            let load_path = load_path.clone();
            open_file(
                FileDialogOptions::new().allowed_types(vec![FileSpec {
                    name: "Text",
                    extensions: &["txt"],
                }]),
                move |info| {
                    if let Some(info) = info
                        && let Some(path) = info.path.first()
                    {
                        load_path(path);
                    }
                },
            );
        }
    };

    let save_to = {
        let editor_text = editor_text.clone();
        move |path: &Path| {
            match std::fs::write(path, editor_text()) {
                Ok(()) => {
                    status.set(format!("Saved {}", path.display()));
                    file.set(Some(path.to_path_buf()));
                    // System notification: "Note saved".
                    let result = notify_rust::Notification::new()
                        .summary("Note saved")
                        .body(&format!("{}", path.display()))
                        .show();
                    match result {
                        Ok(_) => eprintln!("notification: OK"),
                        Err(error) => {
                            eprintln!("notification: FAILED: {error}");
                            status.set(format!("Saved; notification failed: {error}"));
                        }
                    }
                }
                Err(error) => status.set(format!("Save failed: {error}")),
            }
        }
    };

    let save_dialog = {
        let save_to = save_to.clone();
        move || {
            let save_to = save_to.clone();
            save_as(
                FileDialogOptions::new()
                    .default_name("note.txt")
                    .allowed_types(vec![FileSpec { name: "Text", extensions: &["txt"] }]),
                move |info| {
                    if let Some(info) = info
                        && let Some(path) = info.path.first()
                    {
                        save_to(path);
                    }
                },
            );
        }
    };

    let paste_image = move || {
        match arboard::Clipboard::new().and_then(|mut cb| cb.get_image()) {
            Ok(img) => {
                let (w, h) = (img.width as u32, img.height as u32);
                match encode_png(w, h, &img.bytes) {
                    Ok(png) => {
                        image_png.set(Some(png));
                        status.set(format!("Pasted image {w}×{h}"));
                        eprintln!("paste-image: OK {w}x{h}");
                    }
                    Err(error) => status.set(format!("PNG encode failed: {error}")),
                }
            }
            Err(error) => {
                status.set(format!("No image on clipboard: {error}"));
                eprintln!("paste-image: FAILED: {error}");
            }
        }
    };

    let open_about = move || {
        if about_window.get_untracked().is_some() {
            return;
        }
        new_window(
            move |id| {
                about_window.set(Some(id));
                about_view(id)
            },
            Some(
                WindowConfig::default()
                    .title("About — Tray Notes (floem)")
                    .size(Size::new(340.0, 200.0))
                    .resizable(false),
            ),
        );
    };

    // ----- native menubar (floem BUILT-IN: action closures, no channels) ---
    let cmd = Some(AccelModifiers::META);
    let menubar = Menu::new()
        .submenu("Tray Notes", |m| {
            m.item("About Tray Notes", |i| i.action(open_about))
                .separator()
                .predefined(&PredefinedMenuItem::hide(None))
                .predefined(&PredefinedMenuItem::hide_others(None))
                .separator()
                .predefined(&PredefinedMenuItem::quit(None))
        })
        .submenu("File", |m| {
            let new_note = new_note.clone();
            let open_dialog = open_dialog.clone();
            let save_dialog = save_dialog.clone();
            m.item("New", |i| {
                i.accelerator(Accelerator::new(cmd, AccelCode::KeyN)).action(move || new_note())
            })
            .item("Open…", |i| {
                i.accelerator(Accelerator::new(cmd, AccelCode::KeyO)).action(move || open_dialog())
            })
            .item("Save…", |i| {
                i.accelerator(Accelerator::new(cmd, AccelCode::KeyS)).action(move || save_dialog())
            })
            .separator()
            .predefined(&PredefinedMenuItem::close_window(Some("Close Window")))
        })
        .submenu("Edit", |m| {
            // Predefined cut/copy/paste use responder-chain selectors floem's
            // NSView doesn't implement (same as iced) — route by hand into
            // the Lapce editor core instead.
            let cut = run_editor_command.clone();
            let copy = run_editor_command.clone();
            let paste = run_editor_command.clone();
            let select_all = run_editor_command.clone();
            m.item("Cut", |i| {
                i.accelerator(Accelerator::new(cmd, AccelCode::KeyX))
                    .action(move || cut(Command::Edit(EditCommand::ClipboardCut)))
            })
            .item("Copy", |i| {
                i.accelerator(Accelerator::new(cmd, AccelCode::KeyC))
                    .action(move || copy(Command::Edit(EditCommand::ClipboardCopy)))
            })
            .item("Paste", |i| {
                i.accelerator(Accelerator::new(cmd, AccelCode::KeyV))
                    .action(move || paste(Command::Edit(EditCommand::ClipboardPaste)))
            })
            .item("Select All", |i| {
                i.accelerator(Accelerator::new(cmd, AccelCode::KeyA)).action(move || {
                    select_all(Command::MultiSelection(MultiSelectionCommand::SelectAll))
                })
            })
        });
    set_window_menu(menubar);

    // ----- tray + hotkey setup & event dispatch ----------------------------
    let trigger = create_trigger();

    {
        let new_note = new_note.clone();
        Effect::new(move |_| {
            trigger.track();
            let mut queue = SHELL_EVENTS.lock().unwrap();
            while let Some(msg) = queue.pop_front() {
                SHELL.with(|shell| {
                    let shell = shell.borrow();
                    let Some(shell) = shell.as_ref() else { return };
                    match &msg {
                        ShellMsg::TrayMenu(id) => {
                            if *id == shell.tray_toggle {
                                toggle_window();
                            } else if *id == shell.tray_new {
                                new_note();
                            } else if *id == shell.tray_quit {
                                eprintln!("quit: from tray");
                                quit_app();
                            }
                        }
                        ShellMsg::Hotkey(id) => {
                            if *id == shell.hotkey_id {
                                eprintln!("hotkey: ⌘⇧9");
                                toggle_window();
                            }
                        }
                    }
                });
            }
        });
    }

    // Create the !Send shell objects on the main thread once the run loop is
    // pumping (floem timers run there), then fire self-test hooks if any.
    exec_after(Duration::from_millis(200), move |_| {
        match Shell::create(trigger) {
            Ok(shell) => {
                eprintln!("shell: tray + menubar + hotkey OK");
                SHELL.with(|s| *s.borrow_mut() = Some(shell));
            }
            Err(error) => {
                eprintln!("shell setup FAILED: {error}");
                status.set(format!("shell setup FAILED: {error}"));
            }
        }

        // Optional scripted verification hooks (research only).
        if std::env::var("TRAY_SELFTEST_IMAGE").is_ok() {
            paste_image();
        }
        if let Ok(path) = std::env::var("TRAY_SELFTEST_SAVE") {
            save_to(Path::new(&path));
        }
        if let Ok(path) = std::env::var("TRAY_SELFTEST_SHOT") {
            exec_after(Duration::from_secs(3), move |_| {
                take_screenshot(window_id, &path);
            });
        }
    });

    // ----- UI ---------------------------------------------------------------
    let toolbar = Stack::horizontal((
        Button::new("New").action({
            let new_note = new_note.clone();
            move || new_note()
        }),
        Button::new("Open…").action({
            let open_dialog = open_dialog.clone();
            move || open_dialog()
        }),
        Button::new("Save…").action({
            let save_dialog = save_dialog.clone();
            move || save_dialog()
        }),
        Button::new("Paste image").action(paste_image),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Button::new("About").action(open_about),
    ))
    .style(|s| s.gap(8.0).items_center().width_full());

    let editor_area = editor_view
        .placeholder("Type your note… (⌘V pastes, drop a .txt to load)")
        .style(|s| s.flex_grow(1.0).width_full().border(1.0).border_radius(6.0));

    let thumbnail = dyn_container(
        move || image_png.get(),
        move |png| match png {
            Some(bytes) => Stack::horizontal((
                img(move || bytes.clone()).style(|s| s.height(80.0)),
                Button::new("Clear").action(move || image_png.set(None)),
            ))
            .style(|s| s.gap(8.0).items_center())
            .into_any(),
            None => Empty::new().into_any(),
        },
    );

    let status_bar = Stack::horizontal((
        Label::derived(move || status.get()).style(|s| s.font_size(12.0)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Label::derived(move || format!("system theme: {}", theme_mode.get()))
            .style(|s| s.font_size(12.0)),
    ))
    .style(|s| s.gap(8.0).items_center().width_full());

    Stack::vertical((toolbar, editor_area, thumbnail, status_bar))
        .style(|s| s.flex_col().gap(10.0).padding(12.0).size_full())
        // Close-to-tray: swallow the OS close request, hide instead.
        .on_event_cont(listener::WindowCloseRequested, move |cx, _| {
            cx.prevent_default();
            eprintln!("close-to-tray: hidden");
            status.set(String::from("Hidden to tray (⌘⇧9 or tray to show)"));
            window_id.set_visible(false);
        })
        // Live dark mode: floem restyles automatically; we surface the mode.
        .on_event_cont(listener::ThemeChanged, move |_, theme| {
            let mode = format!("{theme:?}").to_lowercase();
            eprintln!("theme-changed: {mode}");
            theme_mode.set(mode);
        })
        // Finder file drop: typed event with the dropped paths.
        .on_event_stop(listener::FileDragDrop, move |_, drop| {
            for path in drop.paths.iter() {
                if path.extension().is_some_and(|ext| ext == "txt") {
                    load_path(path);
                } else {
                    status.set(format!("Ignored drop (not .txt): {}", path.display()));
                }
            }
        })
}

fn about_view(id: WindowId) -> impl IntoView {
    Stack::vertical((
        Label::new("Tray Notes").style(|s| s.font_size(24.0)),
        Label::new("floem git 778bb5f2 — SPEC-4 shell integration test").style(|s| s.font_size(13.0)),
        Label::new("tray-icon + global-hotkey + arboard + notify-rust").style(|s| s.font_size(12.0)),
        Button::new("Close").action(move || close_window(id)),
    ))
    .style(|s| {
        s.flex_col()
            .gap(12.0)
            .items_center()
            .justify_center()
            .size_full()
    })
}

/// Encode RGBA8 into PNG (floem's `img` view only accepts encoded bytes).
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Self-test screenshot: floem has no window-capture API (gap vs iced's
/// `window::screenshot`), so capture the window's on-screen rect with the
/// macOS `screencapture` tool instead.
fn take_screenshot(window_id: WindowId, path: &str) {
    let Some(bounds) = window_id.bounds_on_screen_including_frame() else {
        eprintln!("screenshot: FAILED: no window bounds");
        return;
    };
    let region = format!(
        "-R{},{},{},{}",
        bounds.x0,
        bounds.y0,
        bounds.width(),
        bounds.height()
    );
    match std::process::Command::new("screencapture")
        .args(["-x", &region, path])
        .status()
    {
        Ok(s) if s.success() => eprintln!("screenshot: saved {path}"),
        Ok(s) => eprintln!("screenshot: FAILED: screencapture exited {s}"),
        Err(error) => eprintln!("screenshot: FAILED: {error}"),
    }
}
