//! The OS-shell layer: external winit `ApplicationHandler` wrapping
//! `MasonryState`, a wrapper `AppDriver` for hide-to-tray/quit, and the
//! tray icon / native menubar / global hotkey setup.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use global_hotkey::hotkey::{Code as HkCode, HotKey, Modifiers as HkModifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use masonry_winit::app::{AppDriver, DriverCtx, MasonryState, MasonryUserEvent};
use tray_icon::menu::accelerator::{Accelerator, Code, Modifiers};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};
use xilem::masonry::app as masonry_core_app;
use xilem::masonry::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers as KbModifiers};
use xilem::masonry::core::{ErasedAction, TextEvent, WidgetId};
use xilem::winit::application::ApplicationHandler;
use xilem::winit::event::{StartCause, WindowEvent};
use xilem::winit::event_loop::ActiveEventLoop;
use xilem::winit::window::Theme;
use xilem::WindowId;

use crate::{Ev, Shared};

/// Wraps xilem's `MasonryDriver`. Adds:
/// - close-to-tray: main-window close requests hide the window instead;
/// - visibility/quit sync: after every action, applies `Shared` flags to the
///   real winit window handle (the only public path to it is `DriverCtx`).
pub struct WrapperDriver {
    inner: Box<dyn AppDriver>,
    main_window: WindowId,
    shared: Arc<Shared>,
}

impl WrapperDriver {
    fn sync(&mut self, ctx: &mut DriverCtx<'_, '_>) {
        if self.shared.quit.load(Ordering::SeqCst) {
            ctx.exit();
            return;
        }

        // Edit-menu roles: inject synthetic TextEvents into the focused
        // window's RenderRoot (masonry's TextArea implements cut/copy/
        // select-all on ⌘X/⌘C/⌘A key events, and paste via ClipboardPaste).
        let cmds: Vec<Ev> = self.shared.pending_edit.lock().unwrap().drain(..).collect();
        for cmd in cmds {
            let render_root = ctx.render_root(self.main_window);
            match cmd {
                Ev::EditPaste => {
                    if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                        render_root.handle_text_event(TextEvent::ClipboardPaste(text));
                    }
                }
                Ev::EditCut => synth_cmd_key(render_root, "x"),
                Ev::EditCopy => synth_cmd_key(render_root, "c"),
                Ev::EditSelectAll => synth_cmd_key(render_root, "a"),
                _ => {}
            }
        }

        let want = self.shared.want_visible.load(Ordering::SeqCst);
        let handle = ctx.window(self.main_window).handle();
        let is = handle.is_visible().unwrap_or(true);
        if want != is {
            handle.set_visible(want);
            if want {
                handle.focus_window();
            }
        }
    }
}

/// Synthesize a Cmd+<ch> key-down aimed at masonry's focused text widget.
fn synth_cmd_key(render_root: &mut masonry_core_app::RenderRoot, ch: &str) {
    let event = KeyboardEvent {
        state: KeyState::Down,
        key: Key::Character(ch.into()),
        modifiers: KbModifiers::META,
        ..Default::default()
    };
    render_root.handle_text_event(TextEvent::Keyboard(event));
}

impl AppDriver for WrapperDriver {
    fn on_action(
        &mut self,
        window_id: WindowId,
        ctx: &mut DriverCtx<'_, '_>,
        widget_id: WidgetId,
        action: ErasedAction,
    ) {
        self.inner.on_action(window_id, ctx, widget_id, action);
        self.sync(ctx);
    }

    fn on_start(&mut self, state: &mut MasonryState<'_>) {
        self.inner.on_start(state);
    }

    fn on_close_requested(&mut self, window_id: WindowId, ctx: &mut DriverCtx<'_, '_>) {
        if window_id == self.main_window {
            // Close-to-tray: hide, keep running. Never forwarded to xilem.
            self.shared.want_visible.store(false, Ordering::SeqCst);
            self.sync(ctx);
        } else {
            self.inner.on_close_requested(window_id, ctx);
        }
    }
}

/// Keep-alive handles for the shell integrations (dropping them removes the
/// tray icon / unregisters the hotkey).
struct ShellHandles {
    _tray: TrayIcon,
    _hotkey: GlobalHotKeyManager,
    _menubar: Menu,
}

pub struct ShellApp {
    masonry_state: MasonryState<'static>,
    driver: WrapperDriver,
    shared: Arc<Shared>,
    shell: Option<ShellHandles>,
}

impl ShellApp {
    pub fn new(
        masonry_state: MasonryState<'static>,
        inner: Box<dyn AppDriver>,
        main_window: WindowId,
        shared: Arc<Shared>,
    ) -> Self {
        Self {
            masonry_state,
            driver: WrapperDriver {
                inner,
                main_window,
                shared: shared.clone(),
            },
            shared,
            shell: None,
        }
    }
}

impl ApplicationHandler<MasonryUserEvent> for ShellApp {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        // macOS: tray icons and the NSApp menubar must be created on the main
        // thread *after* the event loop has started (tray-icon requirement
        // with winit); StartCause::Init is the sanctioned place.
        if matches!(cause, StartCause::Init) && self.shell.is_none() {
            self.shell = Some(init_shell(self.shared.clone()));
        }
        self.masonry_state.handle_new_events(event_loop, cause);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state
            .handle_resumed(event_loop, &mut self.driver);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_suspended(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: xilem::winit::window::WindowId,
        event: WindowEvent,
    ) {
        // masonry_winit 0.4 drops these on the floor (`_ => ()`); intercept
        // them at the winit layer and route into xilem state via the channel.
        match &event {
            WindowEvent::DroppedFile(path) => {
                self.shared.send(Ev::FileDropped(path.clone()));
            }
            WindowEvent::ThemeChanged(theme) => {
                self.shared.send(Ev::Theme(*theme == Theme::Dark));
            }
            _ => {}
        }
        self.masonry_state
            .handle_window_event(event_loop, window_id, event, &mut self.driver);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: MasonryUserEvent) {
        self.masonry_state
            .handle_user_event(event_loop, event, &mut self.driver);
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: xilem::winit::event::DeviceId,
        event: xilem::winit::event::DeviceEvent,
    ) {
        self.masonry_state
            .handle_device_event(event_loop, device_id, event, &mut self.driver);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_about_to_wait(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_memory_warning(event_loop);
    }
}

/// Create tray icon, native menubar and global hotkey; wire all their event
/// handlers to the shared channel. Main thread, event loop running.
fn init_shell(shared: Arc<Shared>) -> ShellHandles {
    // --- Native menubar (muda, re-exported by tray-icon). On macOS the
    // first submenu becomes the application menu.
    let menubar = Menu::new();

    let app_menu = Submenu::new("Tray Notes", true);
    let about_item = MenuItem::with_id("about", "About Tray Notes", true, None);
    let quit_item = MenuItem::with_id(
        "quit",
        "Quit Tray Notes",
        true,
        Some(Accelerator::new(Some(Modifiers::META), Code::KeyQ)),
    );
    app_menu
        .append_items(&[&about_item, &PredefinedMenuItem::separator(), &quit_item])
        .unwrap();

    let file_menu = Submenu::new("File", true);
    let new_item = MenuItem::with_id(
        "new",
        "New",
        true,
        Some(Accelerator::new(Some(Modifiers::META), Code::KeyN)),
    );
    let open_item = MenuItem::with_id(
        "open",
        "Open…",
        true,
        Some(Accelerator::new(Some(Modifiers::META), Code::KeyO)),
    );
    let save_item = MenuItem::with_id(
        "save",
        "Save…",
        true,
        Some(Accelerator::new(Some(Modifiers::META), Code::KeyS)),
    );
    file_menu
        .append_items(&[&new_item, &open_item, &save_item])
        .unwrap();

    // Standard clipboard roles. muda's `PredefinedMenuItem::{cut,copy,paste}`
    // are NSResponder-selector based AND carry ⌘X/⌘C/⌘V key equivalents;
    // masonry is not an NSResponder text view, so the items do nothing — but
    // the menu still CONSUMES the key equivalents, which broke masonry's own
    // built-in ⌘C/⌘V handling (verified). So: custom items *without*
    // accelerators, wired to synthetic TextEvent injection via the wrapper
    // driver; the keyboard shortcuts stay with masonry's native handling.
    let edit_menu = Submenu::new("Edit", true);
    edit_menu
        .append_items(&[
            &MenuItem::with_id("cut", "Cut", true, None),
            &MenuItem::with_id("copy", "Copy", true, None),
            &MenuItem::with_id("paste", "Paste", true, None),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id("select-all", "Select All", true, None),
        ])
        .unwrap();

    menubar
        .append_items(&[&app_menu, &file_menu, &edit_menu])
        .unwrap();
    #[cfg(target_os = "macos")]
    menubar.init_for_nsapp();

    // --- Tray icon (macOS menu-bar extra) with its own menu.
    let tray_menu = Menu::new();
    let t_toggle = MenuItem::with_id("tray-toggle", "Show/Hide Window", true, None);
    let t_new = MenuItem::with_id("tray-new", "New Note", true, None);
    let t_quit = MenuItem::with_id("tray-quit", "Quit", true, None);
    tray_menu
        .append_items(&[
            &t_toggle,
            &t_new,
            &PredefinedMenuItem::separator(),
            &t_quit,
        ])
        .unwrap();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Tray Notes")
        .with_icon(tray_icon_image())
        .with_icon_as_template(true)
        .build()
        .unwrap();

    // --- One muda event handler serves both the menubar and the tray menu.
    let sh = shared.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let ev = match event.id().0.as_str() {
            "about" => Ev::About,
            "quit" | "tray-quit" => Ev::Quit,
            "new" | "tray-new" => Ev::NewNote,
            "open" => Ev::OpenRequested,
            "save" => Ev::SaveRequested,
            "tray-toggle" => Ev::ToggleWindow,
            "cut" => Ev::EditCut,
            "copy" => Ev::EditCopy,
            "paste" => Ev::EditPaste,
            "select-all" => Ev::EditSelectAll,
            _ => return,
        };
        sh.send(ev);
    }));

    // --- Global hotkey Cmd+Shift+9 (works while unfocused/hidden).
    let hotkey_mgr = GlobalHotKeyManager::new().unwrap();
    hotkey_mgr
        .register(HotKey::new(
            Some(HkModifiers::META | HkModifiers::SHIFT),
            HkCode::Digit9,
        ))
        .unwrap();
    let sh = shared.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.state() == HotKeyState::Pressed {
            sh.send(Ev::ToggleWindow);
        }
    }));

    ShellHandles {
        _tray: tray,
        _hotkey: hotkey_mgr,
        _menubar: menubar,
    }
}

/// 22x22 template icon (black + alpha): a rounded "note" outline with lines.
fn tray_icon_image() -> tray_icon::Icon {
    const S: usize = 22;
    let mut rgba = vec![0u8; S * S * 4];
    let mut set = |x: usize, y: usize| {
        let i = (y * S + x) * 4;
        rgba[i] = 0;
        rgba[i + 1] = 0;
        rgba[i + 2] = 0;
        rgba[i + 3] = 255;
    };
    // outline rectangle 4..18 x 3..19
    for x in 4..18 {
        set(x, 3);
        set(x, 18);
    }
    for y in 3..19 {
        set(4, y);
        set(17, y);
    }
    // "text" lines
    for x in 7..15 {
        set(x, 7);
        set(x, 10);
        set(x, 13);
    }
    tray_icon::Icon::from_rgba(rgba, S as u32, S as u32).unwrap()
}
