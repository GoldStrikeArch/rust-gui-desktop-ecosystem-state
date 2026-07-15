//! "Tray Notes" — OS shell integration test in gpui 0.2.2 (SPEC-4).
//!
//! Built-in gpui pieces: native macOS menubar (`cx.set_menus` + actions +
//! key bindings), native file dialogs (`cx.prompt_for_paths` /
//! `prompt_for_new_path`), text + image clipboard (`ClipboardItem` /
//! `ClipboardEntry::Image`), external file drop (`on_drop::<ExternalPaths>`),
//! live dark mode (`window.observe_window_appearance`), multi-window
//! (`cx.open_window`), and close-interception (`window.on_window_should_close`).
//!
//! Helper crates for what gpui does NOT have:
//! - `tray-icon` — NSStatusItem menu-bar extra. gpui runs the real AppKit
//!   main runloop, so creating the status item on the main thread inside
//!   `Application::run` just works. Its menu events arrive on a global
//!   crossbeam channel which we drain from a gpui timer task (80 ms poll).
//! - `global-hotkey` — Carbon RegisterEventHotKey-based ⌘⇧9, same polling.
//! - `notify-rust` — "Note saved" notification (NSUserNotification via
//!   mac-notification-sys); falls back to `osascript display notification`
//!   if it errors.
//!
//! The multiline editor is hand-rolled (editor.rs) — gpui has no text widget.

mod editor;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use gpui::{
    App, Application, Bounds, ClipboardEntry, Context, Entity, ExternalPaths, Global, Image,
    Menu, MenuItem, OsAction, PathPromptOptions, SharedString, Subscription, TitlebarOptions,
    Window, WindowAppearance, WindowBounds, WindowHandle, WindowOptions, actions, div, img,
    prelude::*, px, rgb, size,
};
use tray_icon::{
    TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu as TrayMenu, MenuEvent, MenuItem as TrayMenuItem, PredefinedMenuItem},
};

use editor::Editor;

actions!(
    tray_notes,
    [NewNote, OpenNote, SaveNote, Quit, ShowAbout, ToggleWindow]
);

// ---------------------------------------------------------------------------
// Globals
// ---------------------------------------------------------------------------

/// App-wide shell state: window handles + visibility bookkeeping.
struct Shell {
    main: WindowHandle<NotesApp>,
    about: Option<WindowHandle<AboutView>>,
    /// Whether the app is currently shown (false after hide-to-tray).
    visible: bool,
}
impl Global for Shell {}

/// Keeps the NSStatusItem and the Carbon hotkey registration alive.
struct ShellIntegrations {
    _tray: Option<TrayIcon>,
    _hotkeys: Option<GlobalHotKeyManager>,
}
impl Global for ShellIntegrations {}

// ---------------------------------------------------------------------------
// Theme (live dark mode)
// ---------------------------------------------------------------------------

struct Theme {
    bg: gpui::Rgba,
    panel: gpui::Rgba,
    border: gpui::Rgba,
    text: gpui::Rgba,
    dim: gpui::Rgba,
    accent: gpui::Rgba,
}

fn theme(appearance: WindowAppearance) -> Theme {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme {
            bg: rgb(0x1e1e21),
            panel: rgb(0x2a2a2e),
            border: rgb(0x3f3f46),
            text: rgb(0xe4e4e7),
            dim: rgb(0x9f9fa8),
            accent: rgb(0x60a5fa),
        },
        _ => Theme {
            bg: rgb(0xf4f4f5),
            panel: rgb(0xffffff),
            border: rgb(0xd4d4d8),
            text: rgb(0x18181b),
            dim: rgb(0x6b7280),
            accent: rgb(0x2563eb),
        },
    }
}

// ---------------------------------------------------------------------------
// Main notes window
// ---------------------------------------------------------------------------

struct NotesApp {
    editor: Entity<Editor>,
    /// Thumbnail from "Paste image" (gpui ClipboardEntry::Image).
    image: Option<Arc<Image>>,
    file_path: Option<PathBuf>,
    status: SharedString,
    drag_over: bool,
    _appearance_sub: Subscription,
}

impl NotesApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            Editor::new(
                cx,
                "Welcome to Tray Notes.\nClose the window: it hides to the menu-bar extra.\n\u{2318}\u{21e7}9 toggles it globally.",
                "Type a note…",
            )
        });
        editor.read(cx).focus_handle.focus(window);
        // Live dark mode: repaint every window whenever macOS switches themes.
        let sub = window.observe_window_appearance(|_, cx| cx.refresh_windows());
        Self {
            editor,
            image: None,
            file_path: None,
            status: "Unsaved note".into(),
            drag_over: false,
            _appearance_sub: sub,
        }
    }

    fn load_file(&mut self, path: PathBuf, text: String, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.set_content(&text, cx));
        self.status = format!("Loaded {}", path.display()).into();
        self.file_path = Some(path);
        cx.notify();
    }

    fn new_note(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.set_content("", cx));
        self.file_path = None;
        self.status = "New note".into();
        cx.notify();
    }

    fn paste_image(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            self.status = "Clipboard is empty".into();
            cx.notify();
            return;
        };
        let image = item.into_entries().find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            _ => None,
        });
        match image {
            Some(image) => {
                self.status = format!(
                    "Pasted {:?} image ({} KiB)",
                    image.format(),
                    image.bytes().len() / 1024
                )
                .into();
                self.image = Some(Arc::new(image));
            }
            None => self.status = "No image on the clipboard".into(),
        }
        cx.notify();
    }

    fn dropped_files(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        self.drag_over = false;
        if let Some(path) = paths
            .paths()
            .iter()
            .find(|p| p.extension().is_some_and(|e| e == "txt"))
        {
            match std::fs::read_to_string(path) {
                Ok(text) => self.load_file(path.clone(), text, cx),
                Err(err) => {
                    self.status = format!("Drop failed: {err}").into();
                }
            }
        } else {
            self.status = "Drop a .txt file to load it".into();
        }
        cx.notify();
    }
}

impl Render for NotesApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(window.appearance());

        let thumbnail: Option<gpui::AnyElement> = self.image.clone().map(|image| {
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(t.border)
                .child(
                    img(image)
                        .h(px(72.))
                        .rounded_md()
                        .border_1()
                        .border_color(t.border),
                )
                .child(
                    div()
                        .id("clear-image")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_sm()
                        .text_color(t.dim)
                        .hover(|s| s.text_color(t.text).bg(t.border))
                        .cursor_pointer()
                        .child("Clear image")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.image = None;
                            cx.notify();
                        })),
                )
                .into_any_element()
        });

        div()
            .id("notes-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.text)
            .text_size(px(14.))
            .line_height(px(21.))
            // File drop from Finder (gpui built-in ExternalPaths drag payload).
            .on_drag_move::<ExternalPaths>(cx.listener(|this, _, _, cx| {
                if !this.drag_over {
                    this.drag_over = true;
                    cx.notify();
                }
            }))
            .on_drop::<ExternalPaths>(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.dropped_files(paths, cx);
            }))
            .child(
                // Editor pane (scrollable).
                div()
                    .id("editor-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .m_3()
                    .rounded_md()
                    .bg(t.panel)
                    .border_1()
                    .border_color(if self.drag_over { t.accent } else { t.border })
                    .child(div().p_3().child(self.editor.clone())),
            )
            .children(thumbnail)
            .child(
                // Bottom bar: paste-image button + status + hotkey hint.
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(t.border)
                    .child(
                        div()
                            .id("paste-image")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_sm()
                            .bg(t.accent)
                            .text_color(gpui::white())
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .child("Paste image")
                            .on_click(cx.listener(|this, _, _, cx| this.paste_image(cx))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(t.dim)
                            .truncate()
                            .child(self.status.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_sm()
                            .text_color(t.dim)
                            .child("\u{2318}\u{21e7}9 toggle · close hides to tray"),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// About window (multi-window test)
// ---------------------------------------------------------------------------

struct AboutView {
    _appearance_sub: Subscription,
}

impl AboutView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sub = window.observe_window_appearance(|_, cx| cx.refresh_windows());
        let _ = cx;
        Self {
            _appearance_sub: sub,
        }
    }
}

impl Render for AboutView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(window.appearance());
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(t.bg)
            .text_color(t.text)
            .child(div().text_xl().child("Tray Notes"))
            .child(div().text_sm().text_color(t.dim).child("gpui 0.2.2 — SPEC-4 shell-integration probe"))
            .child(div().text_sm().text_color(t.dim).child("tray-icon · global-hotkey · notify-rust"))
            .child(
                div()
                    .text_sm()
                    .text_color(t.accent)
                    .child("This is an independent second window."),
            )
    }
}

// ---------------------------------------------------------------------------
// Shell plumbing (visibility, tray, hotkey, menubar actions)
// ---------------------------------------------------------------------------

fn set_visible(cx: &mut App, visible: bool) {
    cx.global_mut::<Shell>().visible = visible;
    if visible {
        // Unhides the app (the inverse of `cx.hide()`) and raises the window.
        cx.activate(true);
        let main = cx.global::<Shell>().main;
        main.update(cx, |_, window, _| window.activate_window()).ok();
    } else {
        // gpui exposes no per-window hide; NSApp-level hide is the closest.
        cx.hide();
    }
}

fn toggle_window(cx: &mut App) {
    let visible = cx.global::<Shell>().visible;
    set_visible(cx, !visible);
}

fn new_note(cx: &mut App) {
    set_visible(cx, true);
    let main = cx.global::<Shell>().main;
    main.update(cx, |notes, _, cx| notes.new_note(cx)).ok();
}

fn open_note(cx: &mut App) {
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: None,
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = rx.await
            && let Some(path) = paths.into_iter().next()
        {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            cx.update(|cx| {
                let main = cx.global::<Shell>().main;
                main.update(cx, |notes, _, cx| notes.load_file(path, text, cx))
                    .ok();
            })
            .ok();
        }
    })
    .detach();
}

fn save_note(cx: &mut App) {
    let start_dir = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
    let rx = cx.prompt_for_new_path(&start_dir, Some("note.txt"));
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(path))) = rx.await {
            cx.update(|cx| {
                let main = cx.global::<Shell>().main;
                let text = main
                    .update(cx, |notes, _, cx| notes.editor.read(cx).content.clone())
                    .unwrap_or_default();
                match std::fs::write(&path, text) {
                    Ok(()) => {
                        main.update(cx, |notes, _, cx| {
                            notes.status = format!("Saved {}", path.display()).into();
                            notes.file_path = Some(path.clone());
                            cx.notify();
                        })
                        .ok();
                        notify_saved(&path);
                    }
                    Err(err) => {
                        main.update(cx, |notes, _, cx| {
                            notes.status = format!("Save failed: {err}").into();
                            cx.notify();
                        })
                        .ok();
                    }
                }
            })
            .ok();
        }
    })
    .detach();
}

/// System notification: try notify-rust (NSUserNotification), fall back to
/// `osascript` if it errors (e.g. bundle-identifier problems on unbundled
/// binaries).
fn notify_saved(path: &std::path::Path) {
    let body = format!("Saved to {}", path.display());
    match notify_rust::Notification::new()
        .summary("Note saved")
        .body(&body)
        .show()
    {
        Ok(_) => println!("[tray-notes] notification shown via notify-rust"),
        Err(err) => {
            println!("[tray-notes] notify-rust failed ({err}); falling back to osascript");
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(format!(
                    "display notification \"{}\" with title \"Note saved\"",
                    body.replace('"', "'")
                ))
                .spawn()
                .ok();
        }
    }
}

fn show_about(cx: &mut App) {
    // Reuse the About window if it is still open, otherwise create it.
    if let Some(about) = cx.global::<Shell>().about
        && about
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let bounds = Bounds::centered(None, size(px(360.), px(220.)), cx);
    let about = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("About Tray Notes")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| AboutView::new(window, cx)),
        )
        .ok();
    cx.global_mut::<Shell>().about = about;
}

/// 22×22 template icon (black + alpha; macOS recolors it for the menu bar):
/// a note sheet with three text lines.
fn tray_icon_rgba() -> (Vec<u8>, u32, u32) {
    const S: usize = 22;
    let mut data = vec![0u8; S * S * 4];
    let mut set = |x: usize, y: usize| {
        let i = (y * S + x) * 4;
        data[i] = 0;
        data[i + 1] = 0;
        data[i + 2] = 0;
        data[i + 3] = 255;
    };
    for y in 3..19 {
        for x in 5..17 {
            let border = y == 3 || y == 18 || x == 5 || x == 16;
            let line = (y == 7 || y == 10 || y == 13) && (7..15).contains(&x);
            if border || line {
                set(x, y);
            }
        }
    }
    (data, S as u32, S as u32)
}

/// Create the macOS menu-bar extra. Must run on the main thread with an
/// AppKit runloop — which is exactly what gpui's Application::run provides.
fn build_tray() -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let (rgba, w, h) = tray_icon_rgba();
    let icon = tray_icon::Icon::from_rgba(rgba, w, h)?;
    let menu = TrayMenu::new();
    menu.append_items(&[
        &TrayMenuItem::with_id("toggle", "Show/Hide Window", true, None),
        &TrayMenuItem::with_id("new", "New Note", true, None),
        &PredefinedMenuItem::separator(),
        &TrayMenuItem::with_id("quit", "Quit Tray Notes", true, None),
    ])?;
    Ok(TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_icon_as_template(true)
        .with_tooltip("Tray Notes (gpui)")
        .build()?)
}

fn main() {
    Application::new().run(|cx: &mut App| {
        editor::bind_editor_keys(cx);

        // --- Native macOS menubar (gpui built-in) --------------------------
        // NOTE: handlers that call `WindowHandle::update` must be deferred —
        // a menu/keystroke action dispatches *through* the focused window, and
        // re-entering that window from inside the dispatch fails (silently,
        // if you `.ok()` it). Discovered the hard way; see FRICTION.md.
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &NewNote, cx| cx.defer(new_note));
        cx.on_action(|_: &OpenNote, cx| cx.defer(open_note));
        cx.on_action(|_: &SaveNote, cx| cx.defer(save_note));
        cx.on_action(|_: &ShowAbout, cx| cx.defer(show_about));
        cx.on_action(|_: &ToggleWindow, cx| cx.defer(toggle_window));
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-n", NewNote, None),
            gpui::KeyBinding::new("cmd-o", OpenNote, None),
            gpui::KeyBinding::new("cmd-s", SaveNote, None),
            gpui::KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.set_menus(vec![
            Menu {
                name: "Tray Notes".into(),
                items: vec![
                    MenuItem::action("About Tray Notes", ShowAbout),
                    MenuItem::separator(),
                    MenuItem::action("Quit Tray Notes", Quit),
                ],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("New", NewNote),
                    MenuItem::action("Open…", OpenNote),
                    MenuItem::action("Save…", SaveNote),
                    MenuItem::separator(),
                    MenuItem::action("Quit", Quit),
                ],
            },
            Menu {
                name: "Edit".into(),
                items: vec![
                    MenuItem::os_action("Cut", editor::Cut, OsAction::Cut),
                    MenuItem::os_action("Copy", editor::Copy, OsAction::Copy),
                    MenuItem::os_action("Paste", editor::Paste, OsAction::Paste),
                    MenuItem::separator(),
                    MenuItem::os_action("Select All", editor::SelectAll, OsAction::SelectAll),
                ],
            },
        ]);

        // --- Main window ----------------------------------------------------
        let bounds = Bounds::centered(None, size(px(500.), px(420.)), cx);
        let main = cx
            .open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(SharedString::from("Tray Notes (gpui)")),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| NotesApp::new(window, cx)),
            )
            .unwrap();
        cx.set_global(Shell {
            main,
            about: None,
            visible: true,
        });

        // --- Close-to-tray: intercept the close button ----------------------
        main.update(cx, |_, window, cx| {
            window.on_window_should_close(cx, |_, cx| {
                set_visible(cx, false);
                false // veto the close; the window lives on, hidden
            });
        })
        .ok();

        // --- Menu-bar extra (tray-icon) & global hotkey (global-hotkey) -----
        let tray = match build_tray() {
            Ok(tray) => {
                println!("[tray-notes] tray icon created (NSStatusItem)");
                Some(tray)
            }
            Err(err) => {
                println!("[tray-notes] tray icon FAILED: {err}");
                None
            }
        };
        let hotkeys = match GlobalHotKeyManager::new() {
            Ok(mgr) => {
                let hk = HotKey::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Digit9);
                match mgr.register(hk) {
                    Ok(()) => println!("[tray-notes] global hotkey cmd-shift-9 registered"),
                    Err(err) => println!("[tray-notes] hotkey register FAILED: {err}"),
                }
                Some(mgr)
            }
            Err(err) => {
                println!("[tray-notes] GlobalHotKeyManager FAILED: {err}");
                None
            }
        };
        cx.set_global(ShellIntegrations {
            _tray: tray,
            _hotkeys: hotkeys,
        });

        // --- Event pump: drain tray/menu/hotkey crossbeam channels ----------
        // tray-icon and global-hotkey deliver events on global channels with
        // no waker integration, so poll them from a gpui timer task.
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let mut do_toggle = false;
                let mut do_new = false;
                let mut do_quit = false;
                while let Ok(ev) = MenuEvent::receiver().try_recv() {
                    println!("[tray-notes] tray menu event: {:?}", ev.id.0);
                    match ev.id.0.as_str() {
                        "toggle" => do_toggle = true,
                        "new" => do_new = true,
                        "quit" => do_quit = true,
                        _ => {}
                    }
                }
                while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                    println!("[tray-notes] global hotkey event: {ev:?}");
                    if ev.state == HotKeyState::Pressed {
                        do_toggle = true;
                    }
                }
                // Drain plain icon click/hover events (unused).
                while TrayIconEvent::receiver().try_recv().is_ok() {}

                if do_toggle || do_new || do_quit {
                    // Let the NSMenu tracking session fully unwind first:
                    // [NSApp hide:] is silently ignored while the status-item
                    // menu is still dismissing (found empirically).
                    cx.background_executor()
                        .timer(Duration::from_millis(300))
                        .await;
                    cx.update(|cx| {
                        if do_quit {
                            cx.quit();
                        } else if do_new {
                            new_note(cx);
                        } else if do_toggle {
                            toggle_window(cx);
                        }
                    })
                    .ok();
                }
            }
        })
        .detach();

        // --- Scriptable probes for verification (env-gated) -----------------
        match std::env::var("TRAY_PROBE").as_deref() {
            Ok("clipboard") => {
                let entries: Vec<String> = cx
                    .read_from_clipboard()
                    .map(|item| {
                        item.into_entries()
                            .map(|e| match e {
                                ClipboardEntry::String(s) => {
                                    format!("String({} chars)", s.text().len())
                                }
                                ClipboardEntry::Image(i) => format!(
                                    "Image({:?}, {} bytes)",
                                    i.format(),
                                    i.bytes().len()
                                ),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                println!("[tray-notes] clipboard probe: {entries:?}");
            }
            Ok("notify") => notify_saved(std::path::Path::new("/tmp/probe.txt")),
            Ok("image-thumb") => {
                // Simulate clicking "Paste image" right after launch.
                let main = cx.global::<Shell>().main;
                main.update(cx, |notes, _, cx| {
                    notes.paste_image(cx);
                    println!("[tray-notes] image-thumb probe: {}", notes.status);
                })
                .ok();
            }
            _ => {}
        }

        cx.activate(true);
    });
}
