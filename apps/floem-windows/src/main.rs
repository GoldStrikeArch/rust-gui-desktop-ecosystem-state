//! "Windows" — multi-window & modal test (SPEC-9), floem git @ 778bb5f2.
//!
//! Architecture notes (research-relevant):
//! - **Shared state is free.** All app state lives in `RwSignal`s created on a
//!   detached `Scope::new()` in `main()`, i.e. in floem's process-wide reactive
//!   runtime — NOT inside any window. Every window's view closure captures the
//!   same `Copy` `App` struct, so "two windows see the same mutable state" is
//!   literally the single-window code with no plumbing: no message bus, no
//!   channel, no per-window `App`. Ping/Pong are just signal writes.
//! - **Windows are handles, not values.** `new_window(|id| view, config)` is a
//!   fire-and-forget request; you learn the `WindowId` inside the view closure
//!   and must store it yourself to close/focus/measure it later.
//! - **floem has no modality API at all** (`WindowConfig` has `window_level`
//!   but no `parent`/`owner`/`modal`). Real macOS window-modality and window
//!   parenting are reached through `WindowIdExt::with_window_handle` + objc2:
//!   `-[NSWindow beginSheet:completionHandler:]` and `addChildWindow:ordered:`.
//!   Both are non-blocking AppKit calls, so they cooperate with winit's loop.

use std::time::{Duration, Instant};

use floem::action::{exec_after, set_global_theme, set_window_menu};
use floem::ext_event::create_ext_action;
use floem::kurbo::{Point, Rect, Size};
use floem::muda::accelerator::{Accelerator, Code as AccelCode, Modifiers as AccelModifiers};
use floem::prelude::*;
use floem::reactive::{Effect, Scope};
use floem::window::{Theme, WindowConfig, WindowId};
use floem::{
    AppConfig, AppEvent, Application, Menu, WindowIdExt, close_window, new_window, quit_app,
    request_close_window,
};

const ACCENT: Color = Color::from_rgb8(0x3b, 0x6f, 0xe0);
const DANGER: Color = Color::from_rgb8(0xc2, 0x33, 0x2e);
const FLASH: Color = Color::from_rgb8(0xff, 0xd5, 0x4f);

// ---------------------------------------------------------------------------
// Model — every field is a signal, so the model IS the single source of truth
// and the Inspector's TextInput can bind straight to `project.name`.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ThemeChoice {
    Light,
    Dark,
    System,
}

impl std::fmt::Display for ThemeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::System => "System",
        })
    }
}

#[derive(Clone, Copy)]
struct Project {
    id: u32,
    name: RwSignal<String>,
    owner: RwSignal<String>,
    budget: RwSignal<f64>,
    open: RwSignal<bool>,
}

#[derive(Clone, Copy)]
struct App {
    projects: RwSignal<Vec<Project>>,
    selected: RwSignal<u32>,
    pings: RwSignal<u32>,
    dirty: RwSignal<bool>,
    compact: RwSignal<bool>,
    theme: RwSignal<ThemeChoice>,
    flash: RwSignal<bool>,
    main_win: RwSignal<Option<WindowId>>,
    inspector: RwSignal<Option<WindowId>>,
    prefs: RwSignal<Option<WindowId>>,
    modal: RwSignal<Option<WindowId>>,
    focused: RwSignal<Option<WindowId>>,
    /// Draft buffers for the modal Edit dialog (Cancel must discard them).
    draft_name: RwSignal<String>,
    draft_budget: RwSignal<String>,
    /// Mirror buffer for the Inspector's numeric field (TextInput needs a
    /// concrete `RwSignal<String>`); written back to `budget` on every change.
    budget_buf: RwSignal<String>,
    scale: RwSignal<f64>,
}

const SEED: [(&str, &str, f64, bool); 6] = [
    ("Apollo", "rita", 12500.0, true),
    ("Borealis", "sam", 4200.0, true),
    ("Cinder", "wei", 98000.0, false),
    ("Dovetail", "ana", 750.0, true),
    ("Ember", "kofi", 31000.0, false),
    ("Fathom", "lena", 6400.0, true),
];

impl App {
    fn new(scope: Scope) -> Self {
        let projects: Vec<Project> = SEED
            .iter()
            .enumerate()
            .map(|(i, (name, owner, budget, open))| Project {
                id: i as u32,
                name: scope.create_rw_signal(name.to_string()),
                owner: scope.create_rw_signal(owner.to_string()),
                budget: scope.create_rw_signal(*budget),
                open: scope.create_rw_signal(*open),
            })
            .collect();
        Self {
            projects: scope.create_rw_signal(projects),
            selected: scope.create_rw_signal(0),
            pings: scope.create_rw_signal(0),
            dirty: scope.create_rw_signal(false),
            compact: scope.create_rw_signal(false),
            theme: scope.create_rw_signal(ThemeChoice::System),
            flash: scope.create_rw_signal(false),
            main_win: scope.create_rw_signal(None),
            inspector: scope.create_rw_signal(None),
            prefs: scope.create_rw_signal(None),
            modal: scope.create_rw_signal(None),
            focused: scope.create_rw_signal(None),
            draft_name: scope.create_rw_signal(String::new()),
            draft_budget: scope.create_rw_signal(String::new()),
            budget_buf: scope.create_rw_signal(String::new()),
            scale: scope.create_rw_signal(1.0),
        }
    }

    fn sel(&self) -> Option<Project> {
        let id = self.selected.get();
        self.projects.with(|ps| ps.iter().copied().find(|p| p.id == id))
    }

    fn sel_untracked(&self) -> Option<Project> {
        let id = self.selected.get_untracked();
        self.projects
            .with_untracked(|ps| ps.iter().copied().find(|p| p.id == id))
    }

    fn select(&self, id: u32) {
        self.selected.set(id);
        if let Some(p) = self.sel_untracked() {
            self.budget_buf
                .set(format!("{:.2}", p.budget.get_untracked()));
        }
    }

    // ---- windows ---------------------------------------------------------

    fn open_inspector(&self) {
        let app = *self;
        if let Some(id) = self.inspector.get_untracked() {
            focus_window(id);
            trace("inspector: focus existing (singleton)");
            return;
        }
        let pos = layout_of("inspector").unwrap_or_else(|| {
            let main = self
                .main_win
                .get_untracked()
                .and_then(|w| w.bounds_on_screen_including_frame())
                .unwrap_or(Rect::new(80.0, 80.0, 800.0, 560.0));
            Rect::new(main.x1 + 20.0, main.y0, main.x1 + 380.0, main.y0 + 300.0)
        });
        new_window(
            move |id| {
                app.inspector.set(Some(id));
                trace("inspector: opened");
                // Real OS parenting: floem/winit expose no parent/owner field,
                // so attach as an AppKit child window once it exists.
                let parent = app.main_win.get_untracked();
                let restore = layout_of("inspector");
                exec_after(Duration::from_millis(120), move |_| {
                    if let Some(parent) = parent {
                        add_child_window(parent, id);
                    }
                    // Late, and after the main window's own restore: an
                    // NSWindow child MOVES WITH ITS PARENT, so correcting the
                    // parent's position afterwards drags the inspector along.
                    if let Some(r) = restore {
                        exec_after(Duration::from_millis(600), move |_| {
                            id.set_outer_location(Point::new(r.x0, r.y0));
                        });
                    }
                });
                inspector_view(app, id)
            },
            Some(cfg(
                WindowConfig::default()
                    .title("Inspector — Windows (floem)")
                    .size(Size::new(pos.width(), pos.height()))
                    .position(Point::new(pos.x0, pos.y0)),
            )),
        );
    }

    fn toggle_inspector(&self) {
        match self.inspector.get_untracked() {
            Some(id) => {
                close_window(id);
                self.inspector.set(None);
                trace("inspector: closed (toggle)");
            }
            None => self.open_inspector(),
        }
    }

    fn open_prefs(&self) {
        let app = *self;
        if let Some(id) = self.prefs.get_untracked() {
            focus_window(id);
            trace("prefs: focus existing (singleton)");
            return;
        }
        let pos = layout_of("prefs").unwrap_or(Rect::new(140.0, 620.0, 460.0, 820.0));
        new_window(
            move |id| {
                app.prefs.set(Some(id));
                trace("prefs: opened");
                prefs_view(app, id)
            },
            Some(cfg(
                WindowConfig::default()
                    .title("Preferences — Windows (floem)")
                    .size(Size::new(pos.width(), pos.height()))
                    .position(Point::new(pos.x0, pos.y0)),
            )),
        );
    }

    fn open_modal(&self) {
        if self.modal.get_untracked().is_some() {
            return;
        }
        let Some(p) = self.sel_untracked() else { return };
        let app = *self;
        self.draft_name.set(p.name.get_untracked());
        self.draft_budget
            .set(format!("{:.2}", p.budget.get_untracked()));
        let main = self.main_win.get_untracked();
        let anchor = main
            .and_then(|w| w.bounds_on_screen_including_frame())
            .unwrap_or(Rect::new(80.0, 80.0, 800.0, 560.0));
        new_window(
            move |id| {
                app.modal.set(Some(id));
                trace("modal: opened");
                // The ONLY OS modality reachable from floem: present the new
                // top-level window as a macOS sheet of the main window.
                // `beginSheet:` is non-blocking (unlike `runModalForWindow:`),
                // so winit's event loop keeps pumping and floem keeps painting.
                if let Some(parent) = main
                    && std::env::var_os("WINDOWS_NO_SHEET").is_none()
                {
                    exec_after(Duration::from_millis(120), move |_| {
                        begin_sheet(parent, id);
                    });
                }
                modal_view(app, id)
            },
            Some(cfg(
                WindowConfig::default()
                    .title("Edit project")
                    .size(Size::new(360.0, 190.0))
                    .position(Point::new(anchor.x0 + 180.0, anchor.y0 + 120.0))
                    .resizable(false),
            )),
        );
    }

    fn close_modal(&self) {
        if let Some(id) = self.modal.get_untracked() {
            if let Some(parent) = self.main_win.get_untracked() {
                end_sheet(parent, id);
            }
            close_window(id);
            self.modal.set(None);
            // Focus must return to the parent when the dialog closes.
            if let Some(parent) = self.main_win.get_untracked() {
                focus_window(parent);
            }
        }
    }

    fn commit_modal(&self) {
        if let Some(p) = self.sel_untracked() {
            p.name.set(self.draft_name.get_untracked());
            if let Some(v) = parse_num(&self.draft_budget.get_untracked()) {
                p.budget.set(v);
                self.budget_buf.set(format!("{v:.2}"));
            }
            self.dirty.set(true);
            trace(&format!("modal: OK name={}", p.name.get_untracked()));
        }
        self.close_modal();
    }

    // ---- actions ---------------------------------------------------------

    fn ping(&self) {
        self.pings.update(|n| *n += 1);
        trace(&format!("ping: pings={}", self.pings.get_untracked()));
    }

    fn pong(&self) {
        self.flash.set(true);
        trace("pong: inspector flash on");
        let flash = self.flash;
        exec_after(Duration::from_millis(300), move |_| {
            flash.set(false);
            trace("pong: inspector flash off");
        });
    }

    fn delete_selected(&self) {
        trace("toolbar: Delete pressed");
        let Some(p) = self.sel_untracked() else { return };
        let name = p.name.get_untracked();
        let app = *self;
        // Native confirm on a worker thread — the same pattern floem's own
        // `open_file`/`save_as` use (rfd blocking API off the UI thread, the
        // answer marshalled back with `create_ext_action`).
        let send = create_ext_action(Scope::new(), move |yes: bool| {
            if yes {
                app.projects.update(|ps| ps.retain(|x| x.id != p.id));
                app.dirty.set(true);
                trace(&format!("delete: removed {name}"));
                if let Some(first) = app.projects.with_untracked(|ps| ps.first().map(|p| p.id)) {
                    app.select(first);
                }
            } else {
                trace("delete: cancelled");
            }
        });
        let title = format!("Delete \"{}\"?", p.name.get_untracked());
        std::thread::spawn(move || {
            let answer = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title(&title)
                .set_description("This cannot be undone.")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            send(answer == rfd::MessageDialogResult::Yes);
        });
    }

    fn apply_theme(&self) {
        match self.theme.get_untracked() {
            ThemeChoice::Light => set_global_theme(Theme::Light),
            ThemeChoice::Dark => set_global_theme(Theme::Dark),
            // floem has `set_theme(None)` (back to OS) but it is a per-window
            // update message routed through the *current* view; there is no
            // global "follow the OS again". Resolve the OS theme once and push
            // it globally instead.
            ThemeChoice::System => set_global_theme(os_theme()),
        }
        trace(&format!("theme: {}", self.theme.get_untracked()));
    }

    fn row_count(&self) -> usize {
        self.projects.with_untracked(|p| p.len())
    }
}

/// Verification-only: sibling research apps share this desktop and steal the
/// front, so `WINDOWS_TOP=1` re-activates this app twice a second for the
/// duration of a synthetic-input run. `WindowLevel::AlwaysOnTop` was tried
/// first and rejected: it moves the NSWindow off CGWindowLevel 0, which makes
/// `scripts/window-count.swift` (and `synth bounds`) stop seeing the windows.
fn cfg(config: WindowConfig) -> WindowConfig {
    config
}

/// Verification-only hook for a SHARED research desktop: ~10 sibling GUI apps
/// run concurrently, overlap this window and drive their own synthetic input,
/// so a screen-point click frequently lands on somebody else's window. With
/// `WINDOWS_RAISE_FILE=<path>` set, the app polls for that file and, when the
/// driver script touches it, activates itself and raises its own windows —
/// a one-shot raise immediately before a click, instead of continuously
/// stealing the front (which was tried first and made this app swallow the
/// *siblings'* synthetic keystrokes). `WindowLevel::AlwaysOnTop` was tried
/// too and rejected: it moves the NSWindow off CGWindowLevel 0, where
/// `scripts/window-count.swift` and `synth bounds` stop seeing it.
fn raise_watch(app: App, path: std::path::PathBuf) {
    if path.exists() {
        let close = std::fs::read_to_string(&path).unwrap_or_default().trim() == "close";
        let _ = std::fs::remove_file(&path);
        if close {
            // Same code path as the ⌘W menu item (CloseRequested → veto or
            // close), without needing this app to win the front-most race
            // against the siblings for a keystroke.
            if let Some(id) = app.main_win.get_untracked() {
                request_close_window(id);
            }
            exec_after(Duration::from_millis(120), move |_| raise_watch(app, path));
            return;
        }
        #[cfg(target_os = "macos")]
        unsafe {
            if let Some(cls) = objc2::runtime::AnyClass::get(c"NSApplication") {
                let ns_app: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![cls, sharedApplication];
                if !ns_app.is_null() {
                    let _: () = objc2::msg_send![&*ns_app, activateIgnoringOtherApps: true];
                }
            }
            for sig in [app.main_win, app.prefs, app.inspector, app.modal] {
                if let Some(id) = sig.get_untracked()
                    && let Some(w) = ns_window(id)
                {
                    let _: () = objc2::msg_send![&*w, orderFrontRegardless];
                }
            }
        }
    }
    exec_after(Duration::from_millis(120), move |_| raise_watch(app, path));
}

fn keep_front(app: App) {
    if let Some(path) = std::env::var_os("WINDOWS_RAISE_FILE") {
        raise_watch(app, std::path::PathBuf::from(path));
    }
}

fn parse_num(s: &str) -> Option<f64> {
    s.trim().replace([',', ' '], "").parse::<f64>().ok()
}

fn trace(msg: &str) {
    println!("TRACE {msg}");
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

// ---------------------------------------------------------------------------
// AppKit bridge — the three window relations floem/winit cannot express.
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn ns_window(id: WindowId) -> Option<*mut objc2::runtime::AnyObject> {
    use raw_window_handle::RawWindowHandle;
    id.with_window_handle(|handle| match handle.as_raw() {
        RawWindowHandle::AppKit(h) => {
            let view = h.ns_view.as_ptr() as *mut objc2::runtime::AnyObject;
            unsafe {
                let window: *mut objc2::runtime::AnyObject = objc2::msg_send![&*view, window];
                (!window.is_null()).then_some(window)
            }
        }
        _ => None,
    })
    .flatten()
}

#[cfg(not(target_os = "macos"))]
fn ns_window(_id: WindowId) -> Option<*mut ()> {
    None
}

/// `-[NSWindow beginSheet:completionHandler:]` — a real OS **window-modal**
/// sheet. AppKit disables event delivery to the parent while it is up.
fn begin_sheet(parent: WindowId, sheet: WindowId) {
    #[cfg(target_os = "macos")]
    if let (Some(p), Some(s)) = (ns_window(parent), ns_window(sheet)) {
        unsafe {
            let nil: *mut objc2::runtime::AnyObject = std::ptr::null_mut();
            let _: () = objc2::msg_send![&*p, beginSheet: &*s, completionHandler: nil];
        }
        trace("modal: presented as macOS sheet (beginSheet:)");
        return;
    }
    trace("modal: sheet NOT available (no NSWindow)");
}

fn end_sheet(parent: WindowId, sheet: WindowId) {
    #[cfg(target_os = "macos")]
    if let (Some(p), Some(s)) = (ns_window(parent), ns_window(sheet)) {
        unsafe {
            let _: () = objc2::msg_send![&*p, endSheet: &*s];
        }
    }
}

/// `-[NSWindow addChildWindow:ordered:]` — the macOS spelling of
/// parent/owner/transient_for: the child floats above its parent, and is
/// hidden and minimised together with it.
fn add_child_window(parent: WindowId, child: WindowId) {
    #[cfg(target_os = "macos")]
    if let (Some(p), Some(c)) = (ns_window(parent), ns_window(child)) {
        unsafe {
            let _: () = objc2::msg_send![&*p, addChildWindow: &*c, ordered: 1isize];
        }
        trace("inspector: attached as NSWindow child of main");
    }
}

fn focus_window(id: WindowId) {
    #[cfg(target_os = "macos")]
    if let Some(w) = ns_window(id) {
        unsafe {
            let nil: *mut objc2::runtime::AnyObject = std::ptr::null_mut();
            let _: () = objc2::msg_send![&*w, makeKeyAndOrderFront: nil];
        }
        return;
    }
    id.set_visible(true);
}

fn os_theme() -> Theme {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2::runtime::AnyObject;
        let cls = objc2::runtime::AnyClass::get(c"NSApplication");
        if let Some(cls) = cls {
            let app: *mut AnyObject = objc2::msg_send![cls, sharedApplication];
            if !app.is_null() {
                let appearance: *mut AnyObject = objc2::msg_send![&*app, effectiveAppearance];
                if !appearance.is_null() {
                    let name: *mut AnyObject = objc2::msg_send![&*appearance, name];
                    let utf8: *const std::ffi::c_char = objc2::msg_send![&*name, UTF8String];
                    if !utf8.is_null() {
                        let s = std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned();
                        if s.contains("Dark") {
                            return Theme::Dark;
                        }
                    }
                }
            }
        }
    }
    Theme::Light
}

// ---------------------------------------------------------------------------
// Geometry persistence — a 5-token-per-line text file next to the binary.
// ---------------------------------------------------------------------------

fn layout_path() -> std::path::PathBuf {
    std::env::current_exe()
        .map(|p| p.with_file_name("windows-layout.txt"))
        .unwrap_or_else(|_| "windows-layout.txt".into())
}

fn layout_of(key: &str) -> Option<Rect> {
    let text = std::fs::read_to_string(layout_path()).ok()?;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        if it.next() == Some(key) {
            let v: Vec<f64> = it.filter_map(|t| t.parse().ok()).collect();
            if v.len() == 4 {
                return Some(Rect::new(v[0], v[1], v[0] + v[2], v[1] + v[3]));
            }
        }
    }
    None
}

fn layout_flag(key: &str) -> bool {
    std::fs::read_to_string(layout_path())
        .map(|t| t.lines().any(|l| l == key))
        .unwrap_or(false)
}

fn save_layout(app: App) {
    let mut out = String::new();
    for (key, sig) in [
        ("main", app.main_win),
        ("inspector", app.inspector),
        ("prefs", app.prefs),
    ] {
        // Outer ORIGIN + content SIZE. Two floem quirks force this shape:
        //  * `WindowIdExt::bounds_of_content_on_screen()` is broken on macOS at
        //    this rev — it reports `winit`'s `surface_position()`, which is
        //    window-relative (measured: `0,32`), not screen-relative; only its
        //    *size* is usable.
        //  * `WindowConfig::position` lands the CONTENT at that point, while
        //    `bounds_on_screen_including_frame()` reports the FRAME, so a naive
        //    save/restore creeps up by the 32 px title bar every run. The
        //    restore therefore corrects with `set_outer_location` once the
        //    native window exists.
        if let Some(id) = sig.get_untracked()
            && let (Some(frame), Some(content)) = (
                id.bounds_on_screen_including_frame(),
                id.bounds_of_content_on_screen(),
            )
        {
            out.push_str(&format!(
                "{key} {:.0} {:.0} {:.0} {:.0}\n",
                frame.x0,
                frame.y0,
                content.width(),
                content.height()
            ));
        }
    }
    if app.inspector.get_untracked().is_some() {
        out.push_str("inspector_open\n");
    }
    let _ = std::fs::write(layout_path(), &out);
    trace(&format!("persist: wrote {} bytes", out.len()));
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    // The state lives on a DETACHED scope in floem's process-wide reactive
    // runtime, so it outlives every window and every window sees the same
    // signals. This is the whole "shared state across windows" story.
    let app = App::new(Scope::new());
    app.budget_buf.set(format!("{:.2}", SEED[0].2));

    let main_rect = std::env::var("WINDOWS_POS")
        .ok()
        .and_then(|s| {
            let v: Vec<f64> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
            (v.len() == 2).then(|| Rect::new(v[0], v[1], v[0] + 720.0, v[1] + 480.0))
        })
        .or_else(|| layout_of("main"))
        .unwrap_or(Rect::new(80.0, 80.0, 800.0, 560.0));

    Application::new_with_config(AppConfig::default().exit_on_close(false))
        .on_event(move |event| match event {
            AppEvent::WillTerminate => save_layout(app),
            AppEvent::Reopen { .. } => {
                if let Some(id) = app.main_win.get_untracked() {
                    id.set_visible(true);
                }
            }
        })
        .window(
            move |id| main_view(app, id),
            Some(cfg(
                WindowConfig::default()
                    .title("Windows (floem)")
                    .size(Size::new(main_rect.width(), main_rect.height()))
                    .position(Point::new(main_rect.x0, main_rect.y0)),
            )),
        )
        .run();
}

// ---------------------------------------------------------------------------
// Main window
// ---------------------------------------------------------------------------

fn main_view(app: App, window_id: WindowId) -> impl IntoView {
    app.main_win.set(Some(window_id));
    app.focused.set(Some(window_id));
    // The native window does not exist yet while the view closure runs, so
    // `WindowIdExt::scale()` still reports 1.0 here; sample it once realised.
    // The same callback pins the persisted OUTER origin (see save_layout).
    let restore = layout_of("main");
    exec_after(Duration::from_millis(400), move |_| {
        app.scale.set(window_id.scale());
        if let Some(r) = restore {
            window_id.set_outer_location(Point::new(r.x0, r.y0));
        }
    });
    install_menu(app);
    keep_front(app);

    // Re-open the inspector if it was open at quit (requirement 9).
    if layout_flag("inspector_open") {
        exec_after(Duration::from_millis(250), move |_| app.open_inspector());
    }
    if std::env::var_os("WINDOWS_SELFTEST").is_some() {
        selftest(app);
    }

    let toolbar = Stack::horizontal((
        Button::new("Edit…").action(move || app.open_modal()),
        Button::new("Inspector").action(move || app.open_inspector()),
        Button::new("Preferences…").action(move || app.open_prefs()),
        Button::new("Pong").action(move || app.pong()),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Button::new("Delete")
            .action(move || app.delete_selected())
            .style(|s| s.color(DANGER)),
    ))
    .style(|s| s.gap(8.0).items_center().width_full());

    let header = Stack::horizontal((
        Label::new("name").style(|s| s.width(180.0).font_size(12.0)),
        Label::new("owner").style(|s| s.width(90.0).font_size(12.0)),
        Label::new("budget")
            .style(|s| s.font_size(12.0))
            .container()
            .style(|s| s.width(120.0).justify_end()),
        Label::new("status").style(|s| s.width(80.0).font_size(12.0)),
    ))
    .style(|s| s.gap(8.0).padding_horiz(8.0).padding_vert(4.0));

    let list = dyn_stack(
        move || app.projects.get(),
        |p| p.id,
        move |p| row_view(app, p),
    )
    .style(|s| s.flex_col().width_full())
    .scroll()
    .style(|s| s.width_full().flex_grow(1.0).min_height(0.0));

    let status = Stack::horizontal((
        Label::derived(move || {
            let name = app
                .sel()
                .map(|p| p.name.get())
                .unwrap_or_else(|| "—".into());
            format!("selected: {name} · pings: {}", app.pings.get())
        })
        .style(|s| s.font_size(13.0)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Label::derived(move || {
            format!(
                "{}rows: {} · scale: {:.1}x",
                if app.dirty.get() { "· unsaved " } else { "" },
                app.projects.with(|p| p.len()),
                app.scale.get()
            )
        })
        .style(|s| s.font_size(12.0)),
    ))
    .style(|s| s.gap(8.0).items_center().width_full());

    Stack::vertical((toolbar, header, list, status))
        .style(|s| s.flex_col().gap(8.0).padding(12.0).size_full())
        .on_event_cont(listener::WindowGainedFocus, move |_, _| {
            app.focused.set(Some(window_id))
        })
        .on_event_cont(listener::WindowScaleChanged, move |_, scale| {
            app.scale.set(*scale);
            trace(&format!("main: scale changed to {scale}"));
        })
        .on_event_cont(listener::WindowMoved, move |_, p: &Point| {
            trace(&format!("main: moved to {:.0},{:.0}", p.x, p.y))
        })
        .on_event_cont(listener::WindowClosed, move |_, _| {
            trace("main: closed → quitting app");
            quit_app();
        })
        // Close veto: prevent_default here cancels the OS close.
        .on_event_cont(listener::WindowCloseRequested, move |cx, _| {
            if !app.dirty.get_untracked() {
                save_layout(app);
                return;
            }
            cx.prevent_default();
            trace("close: vetoed, asking Save/Discard/Cancel");
            let send = create_ext_action(Scope::new(), move |answer: String| {
                trace(&format!("close: answer={answer}"));
                match answer.as_str() {
                    "Save" | "Discard" => {
                        app.dirty.set(false);
                        save_layout(app);
                        quit_app();
                    }
                    _ => trace("close: cancelled, window survives"),
                }
            });
            std::thread::spawn(move || {
                let result = rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title("Save changes?")
                    .set_description("This project list has unsaved changes.")
                    .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                        "Save".into(),
                        "Discard".into(),
                        "Cancel".into(),
                    ))
                    .show();
                send(match result {
                    rfd::MessageDialogResult::Custom(s) => s,
                    other => other.to_string(),
                });
            });
        })
}

fn row_view(app: App, p: Project) -> impl IntoView {
    Stack::horizontal((
        Label::derived(move || p.name.get()).style(|s| s.width(180.0)),
        Label::derived(move || p.owner.get()).style(|s| s.width(90.0)),
        // Right-alignment needs a CONTAINER with justify_end; a Label's own
        // `text_align`/`justify_end` do not align text inside a fixed width.
        Label::derived(move || format!("{:.2}", p.budget.get()))
            .container()
            .style(|s| s.width(120.0).justify_end()),
        Label::derived(move || if p.open.get() { "Open" } else { "Closed" }.to_string())
            .style(|s| s.width(80.0)),
    ))
    .style(move |s| {
        let selected = app.selected.get() == p.id;
        s.gap(8.0)
            .padding_horiz(8.0)
            .padding_vert(if app.compact.get() { 2.0 } else { 8.0 })
            .width_full()
            .items_center()
            .border_radius(4.0)
            .apply_if(selected, |s| s.background(ACCENT.with_alpha(0.25)))
    })
    .on_event_stop(listener::Click, move |_, _| {
        app.select(p.id);
        trace(&format!("row: selected {}", p.name.get_untracked()));
    })
    .on_event_stop(listener::DoubleClick, move |_, _| {
        app.select(p.id);
        app.open_modal();
    })
}

// ---------------------------------------------------------------------------
// Modal edit dialog (presented as a macOS sheet of the main window)
// ---------------------------------------------------------------------------

fn modal_view(app: App, window_id: WindowId) -> impl IntoView {
    let commit = move || app.commit_modal();
    let cancel = move || {
        trace("modal: Cancel");
        app.close_modal();
    };
    Stack::vertical((
        Label::new("Edit project").style(|s| s.font_size(16.0)),
        Label::new("Name").style(|s| s.font_size(12.0)),
        TextInput::new(app.draft_name)
            .style(|s| s.width_full().padding(6.0))
            .on_event_stop(TextInputEnter::listener(), move |_, _| commit()),
        Label::new("Budget").style(|s| s.font_size(12.0)),
        TextInput::new(app.draft_budget)
            .style(|s| s.width_full().padding(6.0))
            .on_event_stop(TextInputEnter::listener(), move |_, _| commit()),
        Stack::horizontal((
            Empty::new().style(|s| s.flex_grow(1.0)),
            Button::new("Cancel").action(cancel),
            Button::new("OK").action(commit),
        ))
        .style(|s| s.gap(8.0).width_full()),
    ))
    .style(|s| s.flex_col().gap(6.0).padding(14.0).size_full())
    .on_event_cont(listener::WindowGainedFocus, move |_, _| {
        app.focused.set(Some(window_id))
    })
    .on_event_stop(listener::KeyDown, move |_, event| match &event.key {
        Key::Named(NamedKey::Escape) => cancel(),
        Key::Named(NamedKey::Enter) => commit(),
        _ => {}
    })
    .on_event_cont(listener::WindowClosed, move |_, _| {
        app.modal.set(None);
        if let Some(parent) = app.main_win.get_untracked() {
            focus_window(parent);
        }
    })
}

// ---------------------------------------------------------------------------
// Inspector (second top-level window, singleton, child of main)
// ---------------------------------------------------------------------------

fn inspector_view(app: App, window_id: WindowId) -> impl IntoView {
    // Write the numeric mirror back into the model on every keystroke.
    Effect::new(move |prev: Option<()>| {
        let text = app.budget_buf.get();
        if prev.is_some()
            && let (Some(p), Some(v)) = (app.sel_untracked(), parse_num(&text))
            && p.budget.get_untracked() != v
        {
            p.budget.set(v);
            app.dirty.set(true);
        }
    });

    let field = move |label: &'static str, sig: RwSignal<String>| {
        Stack::vertical((
            Label::new(label).style(|s| s.font_size(11.0)),
            TextInput::new(sig)
                .style(|s| s.width_full().padding(5.0))
                .on_event_stop(listener::KeyDown, move |_, _| app.dirty.set(true)),
        ))
        .style(|s| s.flex_col().gap(2.0).width_full())
    };

    let body = dyn_container(
        move || app.sel().map(|p| p.id),
        move |id| match id.and_then(|id| {
            app.projects
                .with_untracked(|ps| ps.iter().copied().find(|p| p.id == id))
        }) {
            // The inputs bind DIRECTLY to the model's own signals: no copy,
            // no sync code, edits show up in the main list as you type.
            Some(p) => Stack::vertical((field("Name", p.name), field("Owner", p.owner)))
                .style(|s| s.flex_col().gap(6.0).width_full())
                .into_any(),
            None => Label::new("no selection").into_any(),
        },
    )
    .style(|s| s.width_full());

    Stack::vertical((
        Label::derived(move || {
            format!(
                "Inspector — #{}",
                app.sel().map(|p| p.id).unwrap_or_default()
            )
        })
        .style(|s| s.font_size(15.0)),
        body,
        Stack::vertical((
            Label::new("Budget").style(|s| s.font_size(11.0)),
            TextInput::new(app.budget_buf).style(|s| s.width_full().padding(5.0)),
        ))
        .style(|s| s.flex_col().gap(2.0).width_full()),
        Stack::horizontal((
            Button::new("Ping").action(move || app.ping()),
            Empty::new().style(|s| s.flex_grow(1.0)),
            Button::new("Close").action(move || close_window(window_id)),
        ))
        .style(|s| s.gap(8.0).width_full()),
        Label::derived(move || format!("pings seen: {}", app.pings.get())).style(|s| s.font_size(12.0)),
    ))
    .style(move |s| {
        s.flex_col()
            .gap(8.0)
            .padding(12.0)
            .size_full()
            // Pong: the main window writes `flash`, this style closure reads it.
            .apply_if(app.flash.get(), |s| s.background(FLASH))
    })
    .on_event_cont(listener::WindowGainedFocus, move |_, _| {
        app.focused.set(Some(window_id))
    })
    .on_event_cont(listener::WindowScaleChanged, move |_, scale| {
        trace(&format!("inspector: scale changed to {scale}"))
    })
    .on_event_stop(listener::KeyDown, move |_, event| {
        if event.key == Key::Named(NamedKey::Escape) {
            close_window(window_id);
        }
    })
    .on_event_cont(listener::WindowClosed, move |_, _| {
        app.inspector.set(None);
        trace("inspector: closed (app still alive)");
    })
}

// ---------------------------------------------------------------------------
// Preferences (singleton, non-modal)
// ---------------------------------------------------------------------------

fn prefs_view(app: App, window_id: WindowId) -> impl IntoView {
    Stack::vertical((
        Label::new("Preferences").style(|s| s.font_size(16.0)),
        Checkbox::labeled_rw(app.compact, || "Compact rows"),
        Label::new("Theme").style(|s| s.font_size(12.0)),
        Stack::horizontal((
            RadioButton::new_labeled_rw(ThemeChoice::Light, app.theme, || "Light"),
            RadioButton::new_labeled_rw(ThemeChoice::Dark, app.theme, || "Dark"),
            RadioButton::new_labeled_rw(ThemeChoice::System, app.theme, || "System"),
        ))
        .style(|s| s.gap(12.0)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Button::new("Close").action(move || close_window(window_id)),
    ))
    .style(|s| s.flex_col().gap(10.0).padding(14.0).size_full())
    .on_event_cont(listener::WindowGainedFocus, move |_, _| {
        app.focused.set(Some(window_id))
    })
    .on_event_stop(listener::KeyDown, move |_, event| {
        if event.key == Key::Named(NamedKey::Escape) {
            close_window(window_id);
        }
    })
    .on_event_cont(listener::WindowClosed, move |_, _| {
        app.prefs.set(None);
        trace("prefs: closed");
    })
}

// ---------------------------------------------------------------------------
// Menubar — floem builds a real muda/NSApp menu with action closures.
// ---------------------------------------------------------------------------

fn install_menu(app: App) {
    Effect::new(move |_| {
        // Re-applied whenever the theme choice changes.
        app.theme.track();
        app.apply_theme();
    });

    let cmd = Some(AccelModifiers::META);
    let cmd_shift = Some(AccelModifiers::META | AccelModifiers::SHIFT);
    let menu = Menu::new()
        .submenu("Windows", |m| {
            m.item("Preferences…", |i| {
                i.accelerator(Accelerator::new(cmd, AccelCode::Comma))
                    .action(move || app.open_prefs())
            })
            .separator()
            .predefined(&floem::muda::PredefinedMenuItem::quit(None))
        })
        .submenu("Window", |m| {
            m.item("Toggle Inspector", |i| {
                i.accelerator(Accelerator::new(cmd_shift, AccelCode::KeyI))
                    .action(move || app.toggle_inspector())
            })
            .separator()
            // ⌘W must hit the FOCUSED window only; floem tracks no "key
            // window" for us, so a WindowGainedFocus listener in every window
            // maintains `focused` and this item routes to it.
            .item("Close Window", |i| {
                i.accelerator(Accelerator::new(cmd, AccelCode::KeyW))
                    .action(move || {
                        if let Some(id) = app.focused.get_untracked() {
                            trace("cmd-W: closing focused window");
                            request_close_window(id);
                        }
                    })
            })
        });
    set_window_menu(menu);
}

// ---------------------------------------------------------------------------
// Scripted self-test (WINDOWS_SELFTEST=1)
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicUsize, Ordering};
static PASS: AtomicUsize = AtomicUsize::new(0);
static FAIL: AtomicUsize = AtomicUsize::new(0);

fn check(name: &str, ok: bool) {
    if ok {
        PASS.fetch_add(1, Ordering::Relaxed);
    } else {
        FAIL.fetch_add(1, Ordering::Relaxed);
    }
    println!("CHECK {} {name}", if ok { "pass" } else { "FAIL" });
}

fn after(ms: u64, f: impl FnOnce() + 'static) {
    exec_after(Duration::from_millis(ms), move |_| f());
}

fn selftest(app: App) {
    let started = Instant::now();
    let hold = std::env::var_os("WINDOWS_HOLD").is_some();
    check("seed 6 projects", app.row_count() == 6);

    after(900, move || {
        println!("PHASE main t={}ms", started.elapsed().as_millis());
        app.select(1);
        check("select writes selection", app.selected.get_untracked() == 1);

        app.open_inspector();
        after(700, move || {
            println!("PHASE inspector t={}ms", started.elapsed().as_millis());
            check("inspector window opened", app.inspector.get_untracked().is_some());
            let first = app.inspector.get_untracked();
            app.open_inspector();
            check(
                "inspector is a singleton",
                app.inspector.get_untracked() == first,
            );

            // Shared state: write through the model signal the inspector's
            // TextInput is bound to; the main window's list reads the same one.
            let p = app.sel_untracked().unwrap();
            p.name.set("Borealis-EDITED".into());
            check(
                "inspector edit visible in main list model",
                app.projects
                    .with_untracked(|ps| ps[1].name.get_untracked()) == "Borealis-EDITED",
            );

            let before = app.pings.get_untracked();
            app.ping();
            check("ping increments main counter", app.pings.get_untracked() == before + 1);

            app.pong();
            check("pong sets inspector flash", app.flash.get_untracked());

            app.open_prefs();
            after(700, move || {
                println!("PHASE prefs t={}ms", started.elapsed().as_millis());
                check("prefs window opened", app.prefs.get_untracked().is_some());
                let first = app.prefs.get_untracked();
                app.open_prefs();
                check("prefs is a singleton", app.prefs.get_untracked() == first);
                check("pong flash cleared after 300ms", !app.flash.get_untracked());

                app.compact.set(true);
                check("compact rows toggled", app.compact.get_untracked());
                app.theme.set(ThemeChoice::Dark);

                app.open_modal();
                after(600, move || {
                    println!("PHASE modal t={}ms", started.elapsed().as_millis());
                    check("modal window opened", app.modal.get_untracked().is_some());
                    app.draft_name.set("Borealis-MODAL".into());
                    app.draft_budget.set("4321.50".into());
                    app.commit_modal();
                    let p = app.sel_untracked().unwrap();
                    check("modal OK commits name", p.name.get_untracked() == "Borealis-MODAL");
                    check("modal OK commits budget", (p.budget.get_untracked() - 4321.5).abs() < 1e-9);
                    check("edit sets the dirty flag", app.dirty.get_untracked());

                    after(400, move || {
                        check("modal closed after OK", app.modal.get_untracked().is_none());
                        // Cancel path discards the draft.
                        app.open_modal();
                        after(500, move || {
                            app.draft_name.set("SHOULD-NOT-STICK".into());
                            app.close_modal();
                            let p = app.sel_untracked().unwrap();
                            check(
                                "modal Cancel discards draft",
                                p.name.get_untracked() == "Borealis-MODAL",
                            );

                            let rows = app.row_count();
                            app.projects.update(|ps| ps.retain(|x| x.id != 0));
                            check("delete removes a row", app.row_count() == rows - 1);

                            save_layout(app);
                            check(
                                "layout file written",
                                layout_of("main").is_some_and(|r| r.width() > 100.0),
                            );
                            check("inspector_open recorded", layout_flag("inspector_open"));

                            if let Some(id) = app.inspector.get_untracked() {
                                close_window(id);
                            }
                            after(500, move || {
                                check(
                                    "closing inspector keeps app alive",
                                    app.main_win.get_untracked().is_some_and(|w| w.is_visible()),
                                );
                                println!(
                                    "SELFTEST DONE pass={} fail={}",
                                    PASS.load(Ordering::Relaxed),
                                    FAIL.load(Ordering::Relaxed)
                                );
                                use std::io::Write;
                                let _ = std::io::stdout().flush();
                                if !hold {
                                    std::process::exit(
                                        if FAIL.load(Ordering::Relaxed) == 0 { 0 } else { 1 },
                                    );
                                }
                            });
                        });
                    });
                });
            });
        });
    });
}
