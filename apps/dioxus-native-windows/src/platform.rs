//! SPEC-9 "Windows" — the *Blitz-only* half.
//!
//! Same public surface as `apps/dioxus-windows/src/platform.rs`, so the shared
//! `app.rs` compiles and runs unchanged. Everything here talks to
//! Blitz/winit/rfd instead of tao/wry.
//!
//! WHY THIS FILE HOSTS ITS OWN EVENT LOOP
//! `dioxus_native::launch_cfg` builds an event loop, a `DioxusNativeApplication`
//! with exactly ONE `pending_window`, and runs it. `DioxusNativeApplication`
//! does expose `add_window`, but (a) nothing hands a component a reference to
//! the application, and (b) `BlitzApplication::can_create_surfaces` initialises
//! an added window WITHOUT the dioxus context injection and without
//! `initial_build()`, so such a window would render nothing. There is also no
//! close veto anywhere: `BlitzApplication::window_event` destroys the window on
//! `CloseRequested` unconditionally, and `dioxus_native::use_window_event`
//! handlers run *before* it with no way to cancel.
//! So `launch` here re-implements `launch_cfg` on top of the pieces blitz-shell
//! exports (`create_default_event_loop`, `BlitzShellProxy`, `BlitzApplication`,
//! `View`, `WindowConfig`) plus `DioxusDocument` — ~120 lines — which buys real
//! multi-window *and* a real `CloseRequested` veto.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use blitz_shell::{
    BlitzApplication, BlitzShellEvent, BlitzShellProxy, View, WindowConfig,
    create_default_event_loop,
};
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;
use dioxus_native::winit::application::ApplicationHandler;
use dioxus_native::winit::dpi::{LogicalPosition, LogicalSize};
use dioxus_native::winit::event::{StartCause, WindowEvent};
use dioxus_native::winit::event_loop::ActiveEventLoop;
use dioxus_native::winit::window::{Window, WindowAttributes, WindowId, WindowLevel};
use dioxus_native::{DioxusDocument, DioxusNativeWindowRenderer, DocumentConfig};

/// Logical (x, y, w, h) of a window.
pub type Bounds = (f64, f64, f64, f64);

/// What `open_or_focus` needs to know about a window it may have to create.
pub struct WinSpec {
    pub key: &'static str,
    pub title: String,
    pub size: (f64, f64),
    pub pos: Option<(f64, f64)>,
    /// Ask the OS for a real parent/child relationship with the main window.
    pub child_of_main: bool,
}

/// A window the UI has asked for but the event loop has not created yet.
/// It carries a fully-built `VirtualDom` (root context already injected).
struct Pending {
    key: &'static str,
    attrs: WindowAttributes,
    dom: VirtualDom,
}

thread_local! {
    /// key -> live winit window. Every window lives on the main thread.
    static WINDOWS: RefCell<HashMap<&'static str, Arc<dyn Window>>> =
        RefCell::new(HashMap::new());
    /// winit id -> key, so a destroyed window can be forgotten.
    static IDS: RefCell<HashMap<WindowId, &'static str>> = RefCell::new(HashMap::new());
    /// Windows requested from a component, drained by the event loop.
    static PENDING: RefCell<Vec<Pending>> = RefCell::new(Vec::new());
    /// Lets a component wake the event loop.
    static PROXY: RefCell<Option<BlitzShellProxy>> = RefCell::new(None);
    /// window id -> "may I close?" (true = keep the window, i.e. veto).
    static VETO: RefCell<HashMap<WindowId, Rc<dyn Fn() -> bool>>> =
        RefCell::new(HashMap::new());
}

// ------------------------------------------------------------ verification knobs

pub fn selftest() -> bool {
    std::env::var_os("WINDOWS_SELFTEST").is_some()
}

/// Verification-only: place every window at a fixed spot so screenshots and
/// synthetic clicks are reproducible on this shared desktop.
pub fn place() -> bool {
    selftest() || on_top() || std::env::var_os("WINDOWS_PLACE").is_some()
}

/// Verification-only: float above the *other* agents' windows on this shared
/// desktop. Separate knob from `place` because a raised window level puts the
/// CGWindow layer above 0, which hides the window from
/// `scripts/window-count.swift`.
pub fn on_top() -> bool {
    std::env::var_os("WINDOWS_TOPMOST").is_some()
}

// ------------------------------------------------------------------- helpers

fn my_window() -> Arc<dyn Window> {
    // dioxus-native provides this context per window; `launch`/`open_or_focus`
    // below provide the same one for the windows they create.
    consume_context::<Arc<dyn Window>>()
}

fn get(key: &str) -> Option<Arc<dyn Window>> {
    WINDOWS.with(|m| m.borrow().get(key).cloned())
}

fn wake() {
    PROXY.with(|p| {
        if let Some(p) = p.borrow().as_ref() {
            p.wake_up();
        }
    });
}

fn attrs_for(title: &str, size: (f64, f64), pos: Option<(f64, f64)>) -> WindowAttributes {
    let mut a = WindowAttributes::default()
        .with_title(title)
        .with_surface_size(LogicalSize::new(size.0, size.1))
        .with_resizable(true);
    if let Some((x, y)) = pos {
        a = a.with_position(LogicalPosition::new(x, y));
    }
    if on_top() {
        a = a.with_window_level(WindowLevel::AlwaysOnTop);
    }
    a
}

// -------------------------------------------------------------- public surface

/// `main.rs` calls exactly this.
pub fn launch(root: fn() -> Element) {
    let event_loop = create_default_event_loop();
    let (proxy, queue) = BlitzShellProxy::new(event_loop.create_proxy());
    PROXY.with(|p| *p.borrow_mut() = Some(proxy.clone()));

    // `launch_cfg` would have done this for us. `tokio::time` (used by the
    // self-test in app.rs) needs a runtime with the timer driver enabled, and
    // the guard must outlive `run_app`.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    PENDING.with(|p| {
        p.borrow_mut().push(Pending {
            key: "main",
            attrs: attrs_for("Windows (dioxus-native)", (720.0, 480.0), None),
            dom: VirtualDom::new(root),
        })
    });

    let app = NativeApp {
        inner: BlitzApplication::new(proxy, queue),
    };
    event_loop.run_app(app).unwrap();
}

/// Register the window this component is mounted in under `key`.
/// (`open_or_focus` already registers the windows it creates; this makes the
/// main window — and a re-render of any child — idempotently consistent.)
pub fn register(key: &'static str) {
    let w = my_window();
    IDS.with(|m| m.borrow_mut().insert(w.id(), key));
    WINDOWS.with(|m| m.borrow_mut().insert(key, w));
}

pub fn is_open(key: &str) -> bool {
    WINDOWS.with(|m| m.borrow().contains_key(key))
}

/// Number of live windows this process owns.
pub fn open_count() -> usize {
    WINDOWS.with(|m| m.borrow().len())
}

/// Singleton open: focus the existing window if there is one, else queue a new
/// one whose `VirtualDom` already carries `ctx` as a root context.
pub fn open_or_focus<T: Clone + 'static>(spec: WinSpec, root: fn() -> Element, ctx: T) {
    if let Some(w) = get(spec.key) {
        w.set_visible(true);
        w.focus_window();
        return;
    }
    if PENDING.with(|p| p.borrow().iter().any(|q| q.key == spec.key)) {
        return; // already queued this frame
    }
    if spec.child_of_main {
        // winit 0.31-beta has no `with_parent_window`/`with_owner_window` on
        // WindowAttributes and no macOS extension for it, so the OS
        // parent/child relationship cannot be expressed at all here.
        println!("[windows] {}: OS parenting unavailable on winit 0.31", spec.key);
    }

    // *** The cross-window state mechanism, unchanged from the webview build. ***
    // A second VirtualDom, handed the same `Signal` handles before it mounts.
    let dom = VirtualDom::new(root).with_root_context(ctx);
    PENDING.with(|p| {
        p.borrow_mut().push(Pending {
            key: spec.key,
            attrs: attrs_for(&spec.title, spec.size, spec.pos),
            dom,
        })
    });
    wake();
}

/// Close a window by key (no-op if it is not open).
pub fn close_key(key: &str) {
    if let Some(w) = get(key) {
        PROXY.with(|p| {
            if let Some(p) = p.borrow().as_ref() {
                p.send_event(BlitzShellEvent::CloseWindow {
                    window_id: w.id(),
                });
            }
        });
    }
}

/// Close the window this component lives in (⌘W / Esc from inside a child).
pub fn close_self() {
    let w = my_window();
    PROXY.with(|p| {
        if let Some(p) = p.borrow().as_ref() {
            p.send_event(BlitzShellEvent::CloseWindow { window_id: w.id() });
        }
    });
}

pub fn focus(key: &str) {
    if let Some(w) = get(key) {
        w.set_visible(true);
        w.focus_window();
    }
}

pub fn bounds(key: &str) -> Option<Bounds> {
    let w = get(key)?;
    let sf = w.scale_factor();
    let p = w.outer_position().ok()?;
    let s = w.surface_size();
    Some((
        p.x as f64 / sf,
        p.y as f64 / sf,
        s.width as f64 / sf,
        s.height as f64 / sf,
    ))
}

pub fn set_bounds(key: &str, b: Bounds) {
    if let Some(w) = get(key) {
        w.set_outer_position(LogicalPosition::new(b.0, b.1).into());
        let _ = w.request_surface_size(LogicalSize::new(b.2, b.3).into());
    }
}

/// DPI scale of the window this component lives in.
pub fn scale_factor() -> f64 {
    my_window().scale_factor()
}

/// Native message box (rfd), async so it never runs a nested modal run-loop
/// inside a winit event callback.
pub fn confirm(title: String, message: String, answer: Callback<bool>) {
    spawn(async move {
        let r = rfd::AsyncMessageDialog::new()
            .set_title(title)
            .set_description(message)
            .set_level(rfd::MessageLevel::Warning)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            .await;
        answer.call(r == rfd::MessageDialogResult::Yes);
    });
}

/// Exit the process. Declared `-> ()` rather than `-> !` on purpose (see the
/// webview platform.rs: a `!`-returning call breaks `SpawnIfAsync` inference).
pub fn quit() {
    std::process::exit(0)
}

/// Close interception for the window this component lives in.
///
/// This is a **genuine veto**: our `ApplicationHandler::window_event` asks the
/// callback first and, if it says "keep", never forwards `CloseRequested` to
/// `BlitzApplication` (which would drop the `View` and destroy the window).
/// `on_close` returns true to keep the window alive.
pub fn use_close_guard(on_close: Callback<(), bool>) {
    use_hook(move || {
        let w = my_window();
        let id = w.id();
        // A winit callback is not inside a dioxus runtime, so the guard has to
        // re-enter the owning scope itself (same trick dioxus-native's own
        // `use_window_event` uses).
        let rt = Runtime::current();
        let scope = current_scope_id();
        let f: Rc<dyn Fn() -> bool> = Rc::new(move || rt.in_scope(scope, || on_close.call(())));
        VETO.with(|v| v.borrow_mut().insert(id, f));
    });
}

// ------------------------------------------------------ the application handler

struct NativeApp {
    inner: BlitzApplication<DioxusNativeWindowRenderer>,
}

impl NativeApp {
    /// Turn every queued `Pending` into a real window hosting its VirtualDom.
    /// This is the part `dioxus_native` only does for its single startup window.
    fn drain(&mut self, event_loop: &dyn ActiveEventLoop) {
        loop {
            let Some(p) = PENDING.with(|q| q.borrow_mut().pop()) else {
                break;
            };
            let doc = DioxusDocument::new(p.dom, DocumentConfig::default());
            let renderer = DioxusNativeWindowRenderer::new();
            let cfg = WindowConfig::with_attributes(Box::new(doc) as _, renderer, p.attrs);
            let mut view = View::init(cfg, event_loop, &self.inner.proxy);

            let win: Arc<dyn Window> = Arc::clone(&view.window);
            let rend = view.renderer.clone();
            let id = view.window_id();
            {
                let doc = view.downcast_doc_mut::<DioxusDocument>();
                let shell = doc.inner.borrow().shell_provider.clone();
                let w2 = Arc::clone(&win);
                // The contexts dioxus-native injects in `can_create_surfaces`.
                // (`Rc<dyn Document>` and the `WindowEventHandlers` registry are
                // crate-private in dioxus-native, so they are omitted: this app
                // uses neither `document::eval` nor `use_window_event`.)
                doc.vdom.in_scope(ScopeId::ROOT, move || {
                    provide_context(shell);
                    provide_context(rend);
                    provide_context(w2);
                });
                doc.initial_build();
            }
            view.resume();
            view.request_redraw();

            WINDOWS.with(|m| m.borrow_mut().insert(p.key, Arc::clone(&win)));
            IDS.with(|m| m.borrow_mut().insert(id, p.key));
            self.inner.windows.insert(id, view);
            println!(
                "[windows] open {} scale_factor={}",
                p.key,
                win.scale_factor()
            );
        }
    }

    fn forget(&mut self, id: WindowId) {
        VETO.with(|v| {
            v.borrow_mut().remove(&id);
        });
        if let Some(key) = IDS.with(|m| m.borrow_mut().remove(&id)) {
            WINDOWS.with(|m| {
                m.borrow_mut().remove(key);
            });
            println!("[windows] closed {key}");
        }
    }
}

impl ApplicationHandler for NativeApp {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.drain(event_loop);
        self.inner.can_create_surfaces(event_loop);
    }

    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }

    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.destroy_surfaces(event_loop);
    }

    fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, cause: StartCause) {
        self.inner.new_events(event_loop, cause);
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.drain(event_loop);
        self.inner.about_to_wait(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // req 7: the real close veto. Ask the guard BEFORE BlitzApplication
        // gets the event, and simply do not forward it if the app says no.
        if matches!(event, WindowEvent::CloseRequested) {
            let guard = VETO.with(|v| v.borrow().get(&window_id).cloned());
            if let Some(guard) = guard {
                if guard() {
                    println!("[windows] CloseRequested vetoed");
                    // The guard wrote signals; make the window re-render.
                    self.inner
                        .proxy
                        .send_event(BlitzShellEvent::Poll { window_id });
                    self.drain(event_loop);
                    return;
                }
            }
            self.forget(window_id);
            drop(self.inner.windows.remove(&window_id));
            if self.inner.windows.is_empty() {
                event_loop.exit();
            }
            return;
        }

        self.inner.window_event(event_loop, window_id, event);
        self.drain(event_loop);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.inner.event_queue.try_recv() {
            if let BlitzShellEvent::CloseWindow { window_id } = &event {
                self.forget(*window_id);
            }
            self.inner.handle_blitz_shell_event(event_loop, event);
        }
        self.drain(event_loop);
    }

    #[cfg(target_os = "macos")]
    fn macos_handler(
        &mut self,
    ) -> Option<&mut dyn dioxus_native::winit::platform::macos::ApplicationHandlerExtMacOS> {
        self.inner.macos_handler()
    }
}
