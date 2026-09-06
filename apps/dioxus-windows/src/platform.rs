//! SPEC-9 "Windows" — the *desktop-only* half of the app.
//!
//! Everything in this file talks to `dioxus::desktop` (tao/wry/muda/rfd).
//! `src/app.rs` never does: it only calls the plain-Rust functions below, so a
//! Dioxus-Native (Blitz) port can replace this one file and keep the UI.
//!
//! The functions a Native port must reimplement are listed at the bottom of
//! FRICTION.md.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use dioxus::desktop::tao::event::{Event, WindowEvent};
use dioxus::desktop::{
    Config, DesktopContext, LogicalPosition, LogicalSize, WeakDesktopContext, WindowBuilder,
    WindowCloseBehaviour, use_window, use_wry_event_handler, window,
};
use dioxus::prelude::*;

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

thread_local! {
    /// key -> weak handle. Every window in the process lives on the main
    /// thread, so one thread-local map is shared by all VirtualDoms.
    static REG: RefCell<HashMap<&'static str, WeakDesktopContext>> = RefCell::new(HashMap::new());
    /// Set when a CloseRequested was vetoed and the window must be un-hidden.
    static RESTORE: Cell<bool> = const { Cell::new(false) };
}

/// Self-test mode forces every window on top: an occluded/unactivated Dioxus
/// window stops servicing tokio timers entirely (see apps/dioxus-fetch/FRICTION.md),
/// which would deadlock a scripted run.
pub fn selftest() -> bool {
    std::env::var_os("WINDOWS_SELFTEST").is_some()
}

/// Verification-only: place every window at a fixed spot so screenshots and
/// synthetic clicks are reproducible on this shared desktop.
/// (WINDOWS_SELFTEST additionally forces always-on-top: an occluded Dioxus
/// window parks its whole tokio/VirtualDom loop — apps/dioxus-fetch/FRICTION.md.
/// always-on-top raises the CGWindow level above 0, which hides the window from
/// `scripts/window-count.swift`, so the window-count evidence uses WINDOWS_PLACE.)
pub fn place() -> bool {
    selftest() || on_top() || std::env::var_os("WINDOWS_PLACE").is_some()
}

/// Verification-only: float above the *other* agents' windows on this shared
/// desktop so a CGEvent click cannot be stolen by them. Raises the CGWindow
/// level above 0, which hides the window from `scripts/window-count.swift` —
/// hence two separate knobs.
pub fn on_top() -> bool {
    selftest() || std::env::var_os("WINDOWS_TOPMOST").is_some()
}

/// `main.rs` calls exactly this.
pub fn launch(root: fn() -> Element) {
    let mut wb = WindowBuilder::new()
        .with_title("Windows (dioxus)")
        .with_inner_size(LogicalSize::new(720.0, 480.0))
        .with_resizable(true);
    if on_top() {
        wb = wb.with_always_on_top(true);
    }
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                // The main window must survive a vetoed close; see `use_close_guard`.
                .with_close_behaviour(WindowCloseBehaviour::WindowHides)
                .with_window(wb),
        )
        .launch(root)
}

fn get(key: &str) -> Option<DesktopContext> {
    REG.with(|r| r.borrow().get(key).and_then(|w| w.upgrade()))
}

/// Register the window this component is mounted in under `key`.
/// Call once, from a `use_hook`.
pub fn register(key: &'static str) {
    let ctx = window();
    REG.with(|r| r.borrow_mut().insert(key, Rc::downgrade(&ctx)));
}

pub fn is_open(key: &str) -> bool {
    get(key).is_some()
}

/// Number of live windows this process owns (registry view; the canonical
/// count in the evidence comes from `scripts/window-count.swift`).
pub fn open_count() -> usize {
    REG.with(|r| r.borrow().values().filter(|w| w.upgrade().is_some()).count())
}

/// Singleton open: focus the existing window if there is one, else create it
/// with `ctx` injected as a root context of the new window's VirtualDom.
pub fn open_or_focus<T: Clone + 'static>(spec: WinSpec, root: fn() -> Element, ctx: T) {
    if let Some(existing) = get(spec.key) {
        existing.window.set_visible(true);
        existing.window.set_focus();
        return;
    }
    let key = spec.key;
    let mut wb = WindowBuilder::new()
        .with_title(spec.title)
        .with_inner_size(LogicalSize::new(spec.size.0, spec.size.1))
        .with_resizable(true);
    if let Some((x, y)) = spec.pos {
        wb = wb.with_position(LogicalPosition::new(x, y));
    }
    if on_top() {
        wb = wb.with_always_on_top(true);
    }
    #[cfg(target_os = "macos")]
    if spec.child_of_main {
        use dioxus::desktop::tao::platform::macos::{WindowBuilderExtMacOS, WindowExtMacOS};
        if let Some(main) = get("main") {
            // tao maps this to -[NSWindow addChildWindow:ordered:NSWindowAbove]:
            // the child floats above its parent and is hidden/minimised with it.
            wb = wb.with_parent_window(main.window.ns_window());
        }
    }

    // *** The cross-window state mechanism. ***
    // Each window is its own VirtualDom with its own runtime; `with_root_context`
    // hands the *same* Signal handles to the new one before it mounts.
    let dom = VirtualDom::new(root).with_root_context(ctx);
    let cfg = Config::new()
        // A child Config would otherwise install the *default* menubar, which is
        // app-global on macOS (apps/dioxus-tray finding).
        .with_menu(None::<dioxus::desktop::muda::Menu>)
        .with_window(wb);
    let pending = window().new_window(dom, cfg);
    spawn(async move {
        let ctx = pending.await;
        let sf = ctx.window.scale_factor();
        REG.with(|r| r.borrow_mut().insert(key, Rc::downgrade(&ctx)));
        println!("[windows] open {key} scale_factor={sf}");
    });
}

/// Close a window by key (no-op if it is not open).
pub fn close_key(key: &str) {
    if let Some(c) = get(key) {
        c.set_close_behavior(WindowCloseBehaviour::WindowCloses);
        c.close();
    }
}

/// Close the window this component lives in (⌘W / Esc from inside a child).
pub fn close_self() {
    window().close();
}

pub fn focus(key: &str) {
    if let Some(c) = get(key) {
        c.window.set_focus();
    }
}

pub fn bounds(key: &str) -> Option<Bounds> {
    let c = get(key)?;
    let sf = c.window.scale_factor();
    let p = c.window.outer_position().ok()?.to_logical::<f64>(sf);
    let s = c.window.inner_size().to_logical::<f64>(sf);
    Some((p.x, p.y, s.width, s.height))
}

pub fn set_bounds(key: &str, b: Bounds) {
    if let Some(c) = get(key) {
        c.window.set_outer_position(LogicalPosition::new(b.0, b.1));
        c.window.set_inner_size(LogicalSize::new(b.2, b.3));
    }
}

/// DPI scale of the window this component lives in.
pub fn scale_factor() -> f64 {
    window().window.scale_factor()
}

/// Native message box (rfd). Async so it never runs a nested modal run-loop
/// inside the tao event callback (that re-enters `WindowEventHandlers` and
/// panics on a double `RefCell::borrow_mut`).
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

/// Exit the process. Closing the main window quits the app even if children
/// live. Declared `-> ()` rather than `-> !` on purpose: a `!`-returning call
/// makes a Dioxus event closure infer the never type, which its `SpawnIfAsync`
/// bound rejects.
pub fn quit() {
    std::process::exit(0)
}

/// Close interception for the window this component lives in.
///
/// dioxus-desktop 0.7.9 has **no veto**: `App::handle_close_requested` either
/// closes or hides, unconditionally, after user wry handlers have run. The
/// closest approximation: the window is configured `WindowHides`, our handler
/// runs first and decides, and if the callback says "keep it", we re-show the
/// window on the very next event-loop iteration (`RedrawEventsCleared`).
///
/// `on_close` returns true to keep the window alive.
pub fn use_close_guard(on_close: Callback<(), bool>) {
    let win = use_window();
    win.set_close_behavior(WindowCloseBehaviour::WindowHides);
    let my_id = win.window.id();
    use_wry_event_handler(move |event, _| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            window_id,
            ..
        } if *window_id == my_id => {
            if on_close.call(()) {
                RESTORE.with(|r| r.set(true));
            }
        }
        Event::RedrawEventsCleared | Event::MainEventsCleared => {
            if RESTORE.with(|r| r.replace(false)) {
                if let Some(c) = get("main") {
                    c.window.set_visible(true);
                    c.window.set_focus();
                }
            }
        }
        _ => {}
    });
}
