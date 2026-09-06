//! The shell layer.
//!
//! xilem 0.4's app model (`Xilem::new` + `run_in`) is a closed loop: you hand
//! it state and a window-list function and never see the OS again. Three of
//! SPEC-9's requirements live below that line, so this app uses the
//! external-event-loop embedding (upstream `external_event_loop.rs`) and owns
//! the winit `ApplicationHandler`:
//!
//! 1. **Modality.** Nothing in xilem/masonry/winit can disable a window on
//!    macOS. While the modal window exists we drop every input event addressed
//!    to the main window *before* `MasonryState` sees it — a hand-rolled
//!    equivalent of Win32 owner-disabling.
//! 2. **Close veto.** `WindowOptions::on_close` is `Fn(&mut State)` with no
//!    return value, so the view layer cannot answer "no". The *driver* can:
//!    `AppDriver::on_close_requested` is the interception point, and simply
//!    not forwarding it leaves the window open.
//! 3. **Window handles.** `DriverCtx::window(id).handle()` is the only public
//!    path from app code to a winit `Window` — needed for `focus_window()`
//!    (focus-return, singleton focus) and for reading position/size to persist.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use masonry_winit::app::{AppDriver, DriverCtx, MasonryState, MasonryUserEvent};
use serde::{Deserialize, Serialize};
use xilem::WindowId;
use xilem::masonry::core::{ErasedAction, WidgetId};
use xilem::winit::application::ApplicationHandler;
use xilem::winit::event::{ElementState, StartCause, WindowEvent};
use xilem::winit::event_loop::ActiveEventLoop;
use xilem::winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::{Answer, Ev, Shared, Win};

// --- MARK: persistence

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Persist {
    /// (window key, [x, y, w, h]) in *logical* pixels.
    pub geom: Vec<(String, [f64; 4])>,
    pub inspector_open: bool,
    pub prefs_open: bool,
    pub compact: bool,
    pub theme: u8,
}

fn beside_exe(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

pub fn state_path() -> PathBuf {
    // WINDOWS_STATE lets the self-test and the scripted persistence check use
    // a scratch file instead of the real one next to the binary.
    match std::env::var("WINDOWS_STATE") {
        Ok(p) => PathBuf::from(p),
        Err(_) => beside_exe("xilem-windows-state.json"),
    }
}

/// Where "Save" writes the project list.
pub fn data_path() -> PathBuf {
    beside_exe("xilem-windows-projects.tsv")
}

/// Read once; app logic calls this on every rebuild.
pub fn load() -> &'static Persist {
    static CACHE: std::sync::OnceLock<Persist> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        std::fs::read_to_string(state_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    })
}

// --- MARK: wrapper driver

pub struct WrapperDriver {
    inner: Box<dyn AppDriver>,
    ids: [WindowId; 4],
    shared: Arc<Shared>,
}

impl WrapperDriver {
    fn wid(&self, w: Win) -> WindowId {
        match w {
            Win::Main => self.ids[0],
            Win::Inspector => self.ids[1],
            Win::Prefs => self.ids[2],
            Win::Modal => self.ids[3],
        }
    }

    /// Publish everything the winit layer and app logic need to know about the
    /// real OS windows, then apply pending focus / quit / close requests.
    fn sync(&mut self, ctx: &mut DriverCtx<'_, '_>) {
        let open: Vec<Win> = self.shared.open.lock().unwrap().clone();

        let mut ids = Vec::with_capacity(open.len());
        let mut geom = Vec::with_capacity(open.len());
        for w in &open {
            let handle = ctx.window(self.wid(*w)).handle();
            ids.push((handle.id(), *w));
            let sf = handle.scale_factor();
            if let Ok(p) = handle.outer_position() {
                let s = handle.inner_size();
                geom.push((
                    *w,
                    [
                        f64::from(p.x) / sf,
                        f64::from(p.y) / sf,
                        f64::from(s.width) / sf,
                        f64::from(s.height) / sf,
                    ],
                ));
            }
        }
        *self.shared.ids.lock().unwrap() = ids;
        *self.shared.geom.lock().unwrap() = geom;

        let want_focus = match self.shared.focus.swap(0, Ordering::SeqCst) {
            1 => Some(Win::Main),
            2 => Some(Win::Inspector),
            _ => None,
        };
        if let Some(w) = want_focus
            && open.contains(&w)
        {
            ctx.window(self.wid(w)).handle().focus_window();
            self.shared.focus_applied.store(
                match w {
                    Win::Main => 1,
                    _ => 2,
                },
                Ordering::SeqCst,
            );
        }

        if self.shared.quit.load(Ordering::SeqCst) {
            self.persist(ctx);
            ctx.exit();
            return;
        }
        if self.shared.request_close_main.swap(false, Ordering::SeqCst) {
            let main = self.wid(Win::Main);
            self.on_close_requested(main, ctx);
        }
    }

    fn persist(&mut self, ctx: &mut DriverCtx<'_, '_>) {
        let _ = ctx;
        let p = Persist {
            geom: self
                .shared
                .geom
                .lock()
                .unwrap()
                .iter()
                .filter(|(w, _)| *w != Win::Modal)
                .map(|(w, g)| (w.key().to_string(), *g))
                .collect(),
            inspector_open: self.shared.insp_open.load(Ordering::SeqCst),
            prefs_open: self.shared.prefs_open.load(Ordering::SeqCst),
            compact: self.shared.compact.load(Ordering::SeqCst),
            theme: self.shared.theme.load(Ordering::SeqCst),
        };
        if let Ok(s) = serde_json::to_string_pretty(&p) {
            let _ = std::fs::write(state_path(), s);
        }
    }

    /// The native "Save changes?" box. `set_parent` makes rfd use
    /// `beginSheetModalForWindow:` (a real macOS sheet) instead of a free
    /// `runModal` alert; it is behind an env var because running AppKit's
    /// nested modal loop from inside a winit callback is exactly the shape
    /// that panic-aborted winit in apps/xilem-tray.
    fn ask_save(&mut self, ctx: &mut DriverCtx<'_, '_>) -> Answer {
        if let Some(a) = self.shared.scripted_answer() {
            return a;
        }
        let mut dlg = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Windows (xilem)")
            .set_description("Save changes before closing?")
            .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                "Save".into(),
                "Discard".into(),
                "Cancel".into(),
            ));
        if std::env::var("WINDOWS_SHEET").is_ok() {
            dlg = dlg.set_parent(ctx.window(self.wid(Win::Main)).handle());
        }
        match dlg.show() {
            rfd::MessageDialogResult::Custom(s) if s == "Save" => Answer::Save,
            rfd::MessageDialogResult::Custom(s) if s == "Discard" => Answer::Discard,
            _ => Answer::Cancel,
        }
    }
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
        if window_id != self.wid(Win::Main) {
            // Child windows: hand it to xilem, whose `on_close` callback flips
            // the state flag; the next app-logic pass stops yielding the window
            // and MasonryDriver closes it. `keep_running()` stays true, so the
            // app survives.
            self.inner.on_close_requested(window_id, ctx);
            self.sync(ctx);
            return;
        }
        match if self.shared.dirty.load(Ordering::SeqCst) {
            self.ask_save(ctx)
        } else {
            Answer::Discard
        } {
            Answer::Cancel => {
                // VETO. Not forwarding is the whole mechanism: xilem never
                // learns the window was asked to close, so it keeps yielding
                // it and the window stays up.
                let n = self.shared.vetoed.fetch_add(1, Ordering::SeqCst) + 1;
                if crate::logging() {
                    println!("VETOED #{n} CloseRequested on main window (Cancel)");
                    use std::io::Write as _;
                    let _ = std::io::stdout().flush();
                }
            }
            Answer::Save => {
                // Round-trip through app state (it owns the projects), which
                // then sets `quit`; the next sync() persists and exits.
                self.shared.send(Ev::SaveAll);
            }
            Answer::Discard => {
                self.persist(ctx);
                ctx.exit();
            }
        }
    }
}

// --- MARK: winit application handler

fn is_input(e: &WindowEvent) -> bool {
    matches!(
        e,
        WindowEvent::MouseInput { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::KeyboardInput { .. }
            | WindowEvent::Ime(..)
            | WindowEvent::ModifiersChanged(..)
            | WindowEvent::TouchpadPressure { .. }
    )
}

fn short(e: &WindowEvent) -> &'static str {
    match e {
        WindowEvent::MouseInput { .. } => "MouseInput",
        WindowEvent::CursorMoved { .. } => "CursorMoved",
        WindowEvent::CursorEntered { .. } => "CursorEntered",
        WindowEvent::CursorLeft { .. } => "CursorLeft",
        WindowEvent::MouseWheel { .. } => "MouseWheel",
        WindowEvent::KeyboardInput { .. } => "KeyboardInput",
        WindowEvent::Ime(..) => "Ime",
        WindowEvent::ModifiersChanged(..) => "ModifiersChanged",
        _ => "other",
    }
}

pub struct ShellApp {
    masonry_state: MasonryState<'static>,
    driver: WrapperDriver,
    shared: Arc<Shared>,
    mods: ModifiersState,
}

impl ShellApp {
    pub fn new(
        masonry_state: MasonryState<'static>,
        inner: Box<dyn AppDriver>,
        ids: [WindowId; 4],
        shared: Arc<Shared>,
    ) -> Self {
        Self {
            masonry_state,
            driver: WrapperDriver {
                inner,
                ids,
                shared: shared.clone(),
            },
            shared,
            mods: ModifiersState::empty(),
        }
    }

    fn close_request(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: xilem::winit::window::WindowId,
    ) {
        self.masonry_state.handle_window_event(
            event_loop,
            window_id,
            WindowEvent::CloseRequested,
            &mut self.driver,
        );
    }
}

impl ApplicationHandler<MasonryUserEvent> for ShellApp {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
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
        if let WindowEvent::ModifiersChanged(m) = &event {
            self.mods = m.state();
        }
        let tag = self.shared.tag_of(window_id);

        // Esc cancels the modal from any window, so it has to be tested
        // before the input block below swallows it.
        if self.shared.modal_open.load(Ordering::SeqCst)
            && let WindowEvent::KeyboardInput { event: ke, .. } = &event
            && ke.state == ElementState::Pressed
            && matches!(&ke.logical_key, Key::Named(NamedKey::Escape))
        {
            self.shared.send(Ev::ModalCancel);
            return;
        }

        // --- Hand-rolled modality (there is no other kind here).
        if self.shared.modal_open.load(Ordering::SeqCst)
            && tag == Some(Win::Main)
            && is_input(&event)
        {
            let n = self.shared.blocked.fetch_add(1, Ordering::SeqCst) + 1;
            if crate::logging() {
                println!("BLOCKED #{n} {:?} -> main window (modal open)", short(&event));
                use std::io::Write as _;
                let _ = std::io::stdout().flush();
            }
            return;
        }

        // --- Per-window shortcuts. xilem 0.4 has no menu or accelerator API,
        // and masonry only routes keys to the focused widget.
        if let WindowEvent::KeyboardInput {
            event: ke,
            is_synthetic: false,
            ..
        } = &event
            && ke.state == ElementState::Pressed
        {
            if crate::logging() {
                println!(
                    "KEY {:?} mods(super={} shift={} ctrl={}) window={:?}",
                    ke.logical_key,
                    self.mods.super_key(),
                    self.mods.shift_key(),
                    self.mods.control_key(),
                    tag
                );
                use std::io::Write as _;
                let _ = std::io::stdout().flush();
            }
            let ch = match &ke.logical_key {
                Key::Character(s) => Some(s.to_lowercase()),
                _ => None,
            };
            let esc = matches!(&ke.logical_key, Key::Named(NamedKey::Escape));
            let cmd = self.mods.super_key() || self.mods.control_key();
            if cmd && ch.as_deref() == Some(",") {
                self.shared.send(Ev::OpenPrefs);
                return;
            }
            if cmd && self.mods.shift_key() && ch.as_deref() == Some("i") {
                self.shared.send(Ev::ToggleInspector);
                return;
            }
            if cmd && ch.as_deref() == Some("w") {
                // "closes the focused window only": the event carries the
                // winit window id, so this is per-window by construction.
                self.close_request(event_loop, window_id);
                return;
            }
            if esc {
                if self.shared.modal_open.load(Ordering::SeqCst) {
                    self.shared.send(Ev::ModalCancel);
                    return;
                }
                if matches!(tag, Some(Win::Inspector) | Some(Win::Prefs)) {
                    self.close_request(event_loop, window_id);
                    return;
                }
            }
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
