//! "Windows" — multi-window & modal test (SPEC-9), iced 0.14.
//!
//! Architecture notes (research-relevant):
//!
//! * `iced::daemon` is the whole multi-window story. There is exactly ONE
//!   `State` and ONE `update`; `view`/`title`/`theme` are closures that take a
//!   `window::Id` and pick what to draw. A window is therefore *not* an entity
//!   with its own state — it is a `window::Id` key plus a `match` in `view`.
//!   Consequence: "shared state across windows" and "cross-window messages"
//!   cost literally nothing (see `Message::Ping`), while "per-window state"
//!   is the thing you have to hand-roll.
//!
//! * iced 0.14 has **no modality API and no parenting API**: `window::Settings`
//!   has no `parent`/`owner`/`transient_for` field (verified in
//!   iced_core-0.14.0/src/window/settings.rs) and nothing anywhere spells
//!   "modal". Three levels were implemented and measured:
//!     1. macOS `beginSheet:` — a real OS window-modal sheet, reachable only
//!        because `iced::window::run` hands a `&dyn Window`
//!        (`HasWindowHandle`) to a closure that iced_winit invokes on the main
//!        thread. ~30 lines of objc2. This is the default.
//!     2. `window::enable_mouse_passthrough` (= winit `set_cursor_hittest`,
//!        = `setIgnoresMouseEvents:`) on the parent + `Level::AlwaysOnTop` on
//!        the dialog — OS-level click blocking without objc2, but the parent
//!        still takes keystrokes. `WINDOWS_NO_SHEET=1` selects this path.
//!     3. A dim scrim drawn over the parent's view. This one is *visual only*
//!        here (a plain `container` in a `stack`) so that the synthetic-click
//!        test measures the OS mechanism and not the scrim; wrapping it in
//!        `opaque(..)` would block mouse input at the iced level (but never
//!        keyboard input).
//!
//! * Widget operations (`operation::focus_next`, ...) are applied to **every**
//!   window's interface in one pass (iced_winit-0.14.0/src/lib.rs:1742-1750),
//!   so `focus_next()` walks the concatenation of all open windows. Tab order
//!   inside the dialog is therefore hand-rolled from explicit
//!   `operation::focus(id)` calls.
//!
//! * `exit_on_close_request: false` on every window + `window::close_requests()`
//!   gives the close veto; `iced::exit()` is the only "quit" primitive.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use iced::event;
use iced::keyboard;
use iced::widget::{
    button, column, container, mouse_area, operation, radio, row, space,
    stack, text, text_input, toggler,
};
use iced::window;
use iced::{
    Background, Border, Center, Color, Element, Event, Fill, Length, Point,
    Size, Subscription, Task, Theme,
};
use serde::{Deserialize, Serialize};

pub fn main() -> iced::Result {
    iced::daemon(Windows::boot, Windows::update, Windows::view)
        .title(Windows::title)
        .theme(Windows::theme)
        .subscription(Windows::subscription)
        .run()
}

// ---------------------------------------------------------------------------
// macOS window surgery (the two things iced cannot express)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod mac {
    use objc2::runtime::AnyObject;

    /// `NSView -> NSWindow` for an iced window, as a `usize` so it can travel
    /// through a `Send` `Task`. Runs inside `iced::window::run`, i.e. on the
    /// winit main thread (iced_winit-0.14.0/src/lib.rs:1632).
    pub fn ns_window(handle: &dyn iced::window::Window) -> usize {
        use iced::window::raw_window_handle::RawWindowHandle;

        let Ok(handle) = handle.window_handle() else {
            return 0;
        };

        match handle.as_raw() {
            RawWindowHandle::AppKit(appkit) => {
                let view: *mut AnyObject = appkit.ns_view.as_ptr().cast();
                let window: *mut AnyObject =
                    unsafe { objc2::msg_send![view, window] };
                window as usize
            }
            _ => 0,
        }
    }

    /// `NSWindow addChildWindow:ordered:` — real OS parenting: the child stays
    /// above the parent, moves with it and is hidden/minimised with it.
    pub fn add_child(parent: usize, child: usize) {
        if parent == 0 || child == 0 {
            return;
        }
        let parent = parent as *mut AnyObject;
        let child = child as *mut AnyObject;
        // NSWindowAbove == 1
        unsafe { objc2::msg_send![parent, addChildWindow: child, ordered: 1isize] }
    }

    pub fn remove_child(parent: usize, child: usize) {
        if parent == 0 || child == 0 {
            return;
        }
        let parent = parent as *mut AnyObject;
        let child = child as *mut AnyObject;
        unsafe { objc2::msg_send![parent, removeChildWindow: child] }
    }

    /// `NSWindow beginSheet:completionHandler:` — real OS *window-modal*
    /// presentation. AppKit itself disables input to the parent.
    pub fn begin_sheet(parent: usize, sheet: usize) -> bool {
        if parent == 0 || sheet == 0 {
            return false;
        }
        let parent = parent as *mut AnyObject;
        let sheet = sheet as *mut AnyObject;
        let handler: *mut AnyObject = std::ptr::null_mut();
        unsafe {
            objc2::msg_send![parent, beginSheet: sheet, completionHandler: handler]
        }
        true
    }

    pub fn end_sheet(parent: usize, sheet: usize) {
        if parent == 0 || sheet == 0 {
            return;
        }
        let parent = parent as *mut AnyObject;
        let sheet = sheet as *mut AnyObject;
        unsafe { objc2::msg_send![parent, endSheet: sheet] }
    }
}

#[cfg(not(target_os = "macos"))]
mod mac {
    pub fn ns_window(_handle: &dyn iced::window::Window) -> usize {
        0
    }
    pub fn add_child(_parent: usize, _child: usize) {}
    pub fn remove_child(_parent: usize, _child: usize) {}
    pub fn begin_sheet(_parent: usize, _sheet: usize) -> bool {
        false
    }
    pub fn end_sheet(_parent: usize, _sheet: usize) {}
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Project {
    name: String,
    owner: String,
    /// Kept as the *edited text* so that there is exactly one source of truth
    /// for a value that two windows can type into. `value()` parses it.
    budget: String,
    open: bool,
}

impl Project {
    fn value(&self) -> Option<f64> {
        self.budget.trim().parse::<f64>().ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Main,
    Inspector,
    Prefs,
    Modal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeChoice {
    Light,
    Dark,
    System,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Geo {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Persisted {
    main: Option<Geo>,
    inspector: Option<Geo>,
    prefs: Option<Geo>,
    #[serde(default)]
    inspector_open: bool,
}

struct Modal {
    id: window::Id,
    name: String,
    budget: String,
    /// 0 = name field, 1 = budget field. Hand-rolled because `focus_next()`
    /// would walk into the main window's widgets (see module docs).
    focus: usize,
}

struct SelfTest {
    pass: u32,
    fail: u32,
}

impl SelfTest {
    fn check(&mut self, name: &str, ok: bool) {
        if ok {
            self.pass += 1;
        } else {
            self.fail += 1;
        }
        println!("selftest {}: {name}", if ok { "PASS" } else { "FAIL" });
    }
}

struct Windows {
    projects: Vec<Project>,
    selected: usize,
    pings: u32,
    dirty: bool,
    compact: bool,
    theme_choice: ThemeChoice,
    /// Pong flash on the inspector.
    pong: bool,

    main: window::Id,
    inspector: Option<window::Id>,
    prefs: Option<window::Id>,
    modal: Option<Modal>,

    /// NSWindow pointers per iced window (0 on non-macOS).
    ns: HashMap<window::Id, usize>,
    /// Live geometry, mirrored into `Persisted` on every Moved/Resized.
    geo: Persisted,
    parented: bool,
    /// Which modality mechanism is actually in force.
    modality: &'static str,
    scale: f32,

    selftest: Option<SelfTest>,
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    OpenModal,
    ModalName(String),
    ModalBudget(String),
    ModalOk,
    ModalCancel,
    ToggleInspector,
    OpenPrefs,
    InspectorName(String),
    InspectorOwner(String),
    InspectorBudget(String),
    InspectorOpen(bool),
    Ping,
    Pong,
    PongDone,
    DeleteRequested,
    DeleteAnswered(bool),
    ToggleCompact(bool),
    SetTheme(ThemeChoice),
    Opened(window::Id, Kind),
    Ptr(window::Id, usize),
    Moved(window::Id, Point),
    Resized(window::Id, Size),
    Rescaled(window::Id, f32),
    CloseRequested(window::Id),
    Closed(window::Id),
    SaveAnswer(u8),
    Key(window::Id, keyboard::Key, keyboard::Modifiers),
    SelfTestStep(usize),
    Noop,
}

const MAIN_SIZE: Size = Size::new(720.0, 480.0);
const INSPECTOR_SIZE: Size = Size::new(360.0, 300.0);
const PREFS_SIZE: Size = Size::new(320.0, 200.0);
const MODAL_SIZE: Size = Size::new(400.0, 190.0);

fn state_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("windows-state.json")))
        .unwrap_or_else(|| PathBuf::from("windows-state.json"))
}

/// iced's default executor is a thread pool with no timer; a blocking sleep in
/// a `Task::future` is the cheapest delay that needs no `smol`/`tokio` feature.
fn delay(ms: u64) -> Task<()> {
    Task::future(async move {
        std::thread::sleep(Duration::from_millis(ms));
    })
}

fn settings(size: Size, geo: Option<Geo>) -> window::Settings {
    let mut settings = window::Settings {
        size,
        // Everything close-related is ours: veto on the main window, and
        // "closing a child must not quit the app" everywhere else.
        exit_on_close_request: false,
        ..window::Settings::default()
    };

    if let Some(geo) = geo {
        settings.size = Size::new(geo.w.max(200.0), geo.h.max(120.0));
        settings.position = window::Position::Specific(Point::new(geo.x, geo.y));
    }

    settings
}

impl Windows {
    fn boot() -> (Self, Task<Message>) {
        let stored: Persisted = std::fs::read_to_string(state_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        let reopen_inspector = stored.inspector_open;
        let (main, open) = window::open(settings(MAIN_SIZE, stored.main));

        let seed = [
            ("Aurora", "kim", "12500.00", true),
            ("Borealis", "lee", "3400.50", true),
            ("Cygnus", "moss", "980.00", false),
            ("Draco", "ito", "21000.00", true),
            ("Eridanus", "vera", "150.75", false),
            ("Fornax", "ash", "7600.00", true),
        ];

        let state = Self {
            projects: seed
                .iter()
                .map(|(name, owner, budget, open)| Project {
                    name: (*name).into(),
                    owner: (*owner).into(),
                    budget: (*budget).into(),
                    open: *open,
                })
                .collect(),
            selected: 0,
            pings: 0,
            // Verification hook: start with the unsaved-changes flag set so the
            // close-veto path can be driven without first typing into a field.
            dirty: std::env::var_os("WINDOWS_DIRTY").is_some(),
            compact: false,
            theme_choice: ThemeChoice::System,
            pong: false,
            main,
            inspector: None,
            prefs: None,
            modal: None,
            ns: HashMap::new(),
            geo: stored,
            parented: false,
            modality: "none",
            scale: 1.0,
            selftest: std::env::var_os("WINDOWS_SELFTEST")
                .map(|_| SelfTest { pass: 0, fail: 0 }),
        };

        let mut tasks =
            vec![open.map(move |id| Message::Opened(id, Kind::Main))];

        if reopen_inspector {
            tasks.push(
                delay(150).then(|()| Task::done(Message::ToggleInspector)),
            );
        }

        if state.selftest.is_some() {
            tasks
                .push(delay(900).then(|()| Task::done(Message::SelfTestStep(0))));
        }

        (state, Task::batch(tasks))
    }

    fn kind(&self, id: window::Id) -> Kind {
        if Some(id) == self.inspector {
            Kind::Inspector
        } else if Some(id) == self.prefs {
            Kind::Prefs
        } else if self.modal.as_ref().is_some_and(|m| m.id == id) {
            Kind::Modal
        } else {
            Kind::Main
        }
    }

    fn open_windows(&self) -> usize {
        1 + usize::from(self.inspector.is_some())
            + usize::from(self.prefs.is_some())
            + usize::from(self.modal.is_some())
    }

    fn save_geometry(&self) {
        if let Ok(raw) = serde_json::to_string_pretty(&self.geo) {
            let _ = std::fs::write(state_path(), raw);
        }
    }

    fn title(&self, id: window::Id) -> String {
        match self.kind(id) {
            Kind::Main => String::from("Windows (iced)"),
            Kind::Inspector => String::from("Inspector"),
            Kind::Prefs => String::from("Preferences"),
            Kind::Modal => String::from("Edit project"),
        }
    }

    fn theme(&self, _id: window::Id) -> Option<Theme> {
        // Returning `None` lets the shell resolve the system light/dark theme
        // per window (iced 0.14 dark-mode support). One closure, every window.
        match self.theme_choice {
            ThemeChoice::Light => Some(Theme::Light),
            ThemeChoice::Dark => Some(Theme::Dark),
            ThemeChoice::System => None,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Noop => Task::none(),
            Message::Select(index) => {
                self.selected = index.min(self.projects.len() - 1);
                Task::none()
            }
            Message::ToggleCompact(compact) => {
                self.compact = compact;
                Task::none()
            }
            Message::SetTheme(choice) => {
                self.theme_choice = choice;
                Task::none()
            }
            Message::Ping => {
                // Cross-window "message": there is only one State, so the
                // inspector's button mutates the field the main window renders.
                self.pings += 1;
                eprintln!("ping: {}", self.pings);
                Task::none()
            }
            Message::Pong => {
                self.pong = true;
                delay(300).then(|()| Task::done(Message::PongDone))
            }
            Message::PongDone => {
                self.pong = false;
                Task::none()
            }

            // --- inspector edits: straight into the shared model -----------
            Message::InspectorName(value) => {
                self.projects[self.selected].name = value;
                self.dirty = true;
                Task::none()
            }
            Message::InspectorOwner(value) => {
                self.projects[self.selected].owner = value;
                self.dirty = true;
                Task::none()
            }
            Message::InspectorBudget(value) => {
                self.projects[self.selected].budget = value;
                self.dirty = true;
                Task::none()
            }
            Message::InspectorOpen(open) => {
                self.projects[self.selected].open = open;
                self.dirty = true;
                Task::none()
            }

            // --- windows ----------------------------------------------------
            Message::ToggleInspector => match self.inspector {
                Some(id) => window::gain_focus(id),
                None => {
                    let offset = self.geo.main.map(|g| Geo {
                        x: g.x + g.w + 16.0,
                        y: g.y,
                        w: INSPECTOR_SIZE.width,
                        h: INSPECTOR_SIZE.height,
                    });
                    let (id, open) = window::open(settings(
                        INSPECTOR_SIZE,
                        self.geo.inspector.or(offset),
                    ));
                    self.inspector = Some(id);
                    self.geo.inspector_open = true;
                    open.map(|id| Message::Opened(id, Kind::Inspector))
                }
            },
            Message::OpenPrefs => match self.prefs {
                Some(id) => window::gain_focus(id),
                None => {
                    let (id, open) =
                        window::open(settings(PREFS_SIZE, self.geo.prefs));
                    self.prefs = Some(id);
                    open.map(|id| Message::Opened(id, Kind::Prefs))
                }
            },
            Message::OpenModal => {
                if self.modal.is_some() {
                    return Task::none();
                }
                let project = &self.projects[self.selected];
                let (id, open) = window::open(window::Settings {
                    resizable: false,
                    ..settings(MODAL_SIZE, None)
                });
                self.modal = Some(Modal {
                    id,
                    name: project.name.clone(),
                    budget: project.budget.clone(),
                    focus: 0,
                });
                open.map(|id| Message::Opened(id, Kind::Modal))
            }
            Message::ModalName(value) => {
                if let Some(modal) = &mut self.modal {
                    modal.name = value;
                }
                Task::none()
            }
            Message::ModalBudget(value) => {
                if let Some(modal) = &mut self.modal {
                    modal.budget = value;
                }
                Task::none()
            }
            Message::ModalOk => {
                let Some(modal) = self.modal.take() else {
                    return Task::none();
                };
                self.projects[self.selected].name = modal.name;
                self.projects[self.selected].budget = modal.budget;
                self.dirty = true;
                self.dismiss_modal(modal.id)
            }
            Message::ModalCancel => {
                let Some(modal) = self.modal.take() else {
                    return Task::none();
                };
                self.dismiss_modal(modal.id)
            }

            Message::DeleteRequested => {
                let name = self.projects[self.selected].name.clone();
                eprintln!("delete-requested: {name}");
                if self.selftest.is_some() {
                    return Task::done(Message::DeleteAnswered(true));
                }
                Task::perform(
                    async move {
                        rfd::AsyncMessageDialog::new()
                            .set_title("Delete project")
                            .set_description(format!("Delete \"{name}\"?"))
                            .set_buttons(rfd::MessageButtons::YesNo)
                            .show()
                            .await
                            == rfd::MessageDialogResult::Yes
                    },
                    Message::DeleteAnswered,
                )
            }
            Message::DeleteAnswered(yes) => {
                if yes && self.projects.len() > 1 {
                    self.projects.remove(self.selected);
                    self.selected =
                        self.selected.min(self.projects.len() - 1);
                    self.dirty = true;
                    eprintln!("delete: rows={}", self.projects.len());
                }
                Task::none()
            }

            Message::Opened(id, kind) => {
                if kind == Kind::Main {
                    self.main = id;
                }

                // Ask iced for the raw window handle so the OS-level bits
                // (parenting, sheet) become reachable.
                Task::batch([
                    window::run(id, mac::ns_window)
                        .map(move |ptr| Message::Ptr(id, ptr)),
                    window::position(id).map(move |p| match p {
                        Some(p) => Message::Moved(id, p),
                        None => Message::Noop,
                    }),
                ])
            }
            Message::Ptr(id, ptr) => {
                let _ = self.ns.insert(id, ptr);
                let main = self.ns.get(&self.main).copied().unwrap_or(0);

                match self.kind(id) {
                    Kind::Inspector if main != 0 && ptr != 0 => {
                        mac::add_child(main, ptr);
                        self.parented = true;
                        eprintln!("parenting: addChildWindow ok");
                    }
                    Kind::Modal => return self.make_modal(id, ptr, main),
                    _ => {}
                }
                Task::none()
            }
            Message::Moved(id, position) => {
                self.record_geo(id, Some(position), None);
                Task::none()
            }
            Message::Resized(id, size) => {
                self.record_geo(id, None, Some(size));
                Task::none()
            }
            Message::Rescaled(id, scale) => {
                if id == self.main {
                    self.scale = scale;
                }
                eprintln!("rescaled: {id:?} scale_factor={scale}");
                Task::none()
            }

            Message::CloseRequested(id) => match self.kind(id) {
                Kind::Main => {
                    if !self.dirty {
                        self.save_geometry();
                        return iced::exit();
                    }
                    eprintln!("close-veto: asking (dirty)");
                    if self.selftest.is_some() {
                        return Task::done(Message::SaveAnswer(2));
                    }
                    Task::perform(
                        async {
                            match rfd::AsyncMessageDialog::new()
                                .set_title("Save changes?")
                                .set_description(
                                    "This project list has unsaved changes.",
                                )
                                .set_buttons(
                                    rfd::MessageButtons::YesNoCancelCustom(
                                        "Save".into(),
                                        "Discard".into(),
                                        "Cancel".into(),
                                    ),
                                )
                                .show()
                                .await
                            {
                                rfd::MessageDialogResult::Custom(label)
                                    if label == "Save" =>
                                {
                                    0
                                }
                                rfd::MessageDialogResult::Custom(label)
                                    if label == "Discard" =>
                                {
                                    1
                                }
                                _ => 2,
                            }
                        },
                        Message::SaveAnswer,
                    )
                }
                Kind::Modal => Task::done(Message::ModalCancel),
                Kind::Inspector => {
                    let id = self.inspector.take().unwrap();
                    self.geo.inspector_open = false;
                    self.save_geometry();
                    window::close(id)
                }
                Kind::Prefs => {
                    let id = self.prefs.take().unwrap();
                    window::close(id)
                }
            },
            Message::SaveAnswer(answer) => match answer {
                0 | 1 => {
                    eprintln!(
                        "close-veto: {} -> exit",
                        if answer == 0 { "save" } else { "discard" }
                    );
                    self.dirty = false;
                    self.save_geometry();
                    iced::exit()
                }
                _ => {
                    eprintln!("close-veto: cancel -> window survives");
                    Task::none()
                }
            },
            Message::Closed(id) => {
                if Some(id) == self.inspector {
                    self.inspector = None;
                    self.geo.inspector_open = false;
                } else if Some(id) == self.prefs {
                    self.prefs = None;
                }
                self.ns.remove(&id);
                Task::none()
            }

            Message::Key(id, key, modifiers) => self.on_key(id, key, modifiers),
            Message::SelfTestStep(step) => self.selftest_step(step),
        }
    }

    fn record_geo(
        &mut self,
        id: window::Id,
        position: Option<Point>,
        size: Option<Size>,
    ) {
        let slot = match self.kind(id) {
            Kind::Main => &mut self.geo.main,
            Kind::Inspector => &mut self.geo.inspector,
            Kind::Prefs => &mut self.geo.prefs,
            Kind::Modal => return,
        };
        let current = slot.unwrap_or(Geo {
            x: 0.0,
            y: 0.0,
            w: MAIN_SIZE.width,
            h: MAIN_SIZE.height,
        });
        let updated = Geo {
            x: position.map_or(current.x, |p| p.x),
            y: position.map_or(current.y, |p| p.y),
            w: size.map_or(current.w, |s| s.width),
            h: size.map_or(current.h, |s| s.height),
        };
        *slot = Some(updated);
        self.save_geometry();
    }

    /// Install the strongest modality this build can reach for the dialog.
    fn make_modal(
        &mut self,
        id: window::Id,
        ptr: usize,
        main: usize,
    ) -> Task<Message> {
        let no_sheet = std::env::var_os("WINDOWS_NO_SHEET").is_some();

        // MEASURED TRAP: `beginSheet:` on a parent that already owns child
        // windows (our inspector, attached with `addChildWindow:`) destroys
        // the child when the sheet is dismissed — iced reports a `Closed`
        // event for a window the app never closed. Detaching the child for the
        // lifetime of the sheet and re-attaching afterwards is the fix.
        let inspector_ns = self
            .inspector
            .and_then(|id| self.ns.get(&id).copied())
            .unwrap_or(0);

        if !no_sheet && inspector_ns != 0 {
            mac::remove_child(main, inspector_ns);
        }

        if !no_sheet && mac::begin_sheet(main, ptr) {
            self.modality = "OS window-modal (NSWindow beginSheet:)";
            eprintln!("modality: {}", self.modality);
            return operation::focus("modal-name");
        }

        // Fallback: block the parent at the OS level with
        // setIgnoresMouseEvents: and float the dialog above everything.
        self.modality =
            "OS click-block (enable_mouse_passthrough + AlwaysOnTop)";
        eprintln!("modality: {}", self.modality);
        Task::batch([
            window::enable_mouse_passthrough(self.main),
            window::set_level(id, window::Level::AlwaysOnTop),
            operation::focus("modal-name"),
        ])
    }

    fn dismiss_modal(&mut self, id: window::Id) -> Task<Message> {
        let main = self.ns.get(&self.main).copied().unwrap_or(0);
        let sheet = self.ns.get(&id).copied().unwrap_or(0);
        mac::end_sheet(main, sheet);
        self.modality = "none";

        // Re-parent the inspector (see the note in `make_modal`).
        if let Some(inspector) = self.inspector
            && let Some(inspector_ns) = self.ns.get(&inspector).copied()
        {
            mac::add_child(main, inspector_ns);
        }

        Task::batch([
            window::disable_mouse_passthrough(self.main),
            window::close(id),
            window::gain_focus(self.main),
        ])
    }

    fn on_key(
        &mut self,
        id: window::Id,
        key: keyboard::Key,
        modifiers: keyboard::Modifiers,
    ) -> Task<Message> {
        let character = match &key {
            keyboard::Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };

        // ⌘W closes the *focused* window only: `event::listen_with` reports the
        // window each event landed in, which is iced's only per-window key
        // routing.
        if modifiers.command() && character == "w" {
            eprintln!("cmd-w on {:?}", self.kind(id));
            return Task::done(Message::CloseRequested(id));
        }
        if modifiers.command() && character == "," {
            return Task::done(Message::OpenPrefs);
        }
        if modifiers.command() && modifiers.shift() && character == "i" {
            return match self.inspector {
                Some(inspector) => Task::done(Message::CloseRequested(inspector)),
                None => Task::done(Message::ToggleInspector),
            };
        }

        if key == keyboard::Key::Named(keyboard::key::Named::Escape) {
            // text_input swallows Escape, so this arrives only because
            // `event::listen_with` ignores capture status (same trap as
            // apps/iced-board).
            if let Some(modal) = &self.modal {
                let modal = modal.id;
                let _ = modal;
                return Task::done(Message::ModalCancel);
            }
            if let Some(prefs) = self.prefs {
                return Task::done(Message::CloseRequested(prefs));
            }
            if let Some(inspector) = self.inspector {
                return Task::done(Message::CloseRequested(inspector));
            }
            return Task::none();
        }

        // Tab inside the dialog. `operation::focus_next()` is useless here:
        // iced applies widget operations to every open window's interface in
        // one pass, so it would walk out of the dialog and into the main list.
        if key == keyboard::Key::Named(keyboard::key::Named::Tab)
            && self.modal.as_ref().is_some_and(|m| m.id == id)
        {
            let modal = self.modal.as_mut().unwrap();
            modal.focus = if modifiers.shift() {
                (modal.focus + 1) % 2
            } else {
                (modal.focus + 1) % 2
            };
            return operation::focus(if modal.focus == 0 {
                "modal-name"
            } else {
                "modal-budget"
            });
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window::close_requests().map(Message::CloseRequested),
            window::close_events().map(Message::Closed),
            event::listen_with(|event, _status, window| match event {
                Event::Window(window::Event::Moved(position)) => {
                    Some(Message::Moved(window, position))
                }
                Event::Window(window::Event::Resized(size)) => {
                    Some(Message::Resized(window, size))
                }
                Event::Window(window::Event::Rescaled(scale)) => {
                    Some(Message::Rescaled(window, scale))
                }
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    ..
                }) => Some(Message::Key(window, key, modifiers)),
                _ => None,
            }),
        ])
    }

    // -----------------------------------------------------------------------
    // Views — one function per window kind, chosen by `window::Id`.
    // -----------------------------------------------------------------------

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        match self.kind(id) {
            Kind::Main => self.view_main(),
            Kind::Inspector => self.view_inspector(),
            Kind::Prefs => self.view_prefs(),
            Kind::Modal => self.view_modal(),
        }
    }

    fn view_main(&self) -> Element<'_, Message> {
        let toolbar = row![
            button("Edit…").on_press(Message::OpenModal),
            button("Inspector").on_press(Message::ToggleInspector),
            button("Preferences…").on_press(Message::OpenPrefs),
            button("Delete").on_press(Message::DeleteRequested),
            space::horizontal(),
            button("Pong").on_press(Message::Pong),
        ]
        .spacing(8);

        let padding = if self.compact { 2 } else { 8 };
        let mut list = column![
            row![
                text("Project").width(Length::FillPortion(3)).size(12),
                text("Owner").width(Length::FillPortion(2)).size(12),
                text("Budget").width(Length::FillPortion(2)).size(12),
                text("Status").width(Length::FillPortion(2)).size(12),
            ]
            .padding(padding),
        ]
        .spacing(if self.compact { 1 } else { 4 });

        for (index, project) in self.projects.iter().enumerate() {
            let selected = index == self.selected;
            let budget = project
                .value()
                .map_or_else(|| format!("{}?", project.budget), |v| format!("{v:.2}"));

            let cells = row![
                text(&project.name).width(Length::FillPortion(3)),
                text(&project.owner).width(Length::FillPortion(2)),
                text(budget).width(Length::FillPortion(2)),
                text(if project.open { "Open" } else { "Closed" })
                    .width(Length::FillPortion(2)),
            ];

            let cell = container(cells).padding(padding).style(
                move |theme: &Theme| {
                    let palette = theme.extended_palette();
                    container::Style {
                        background: selected.then(|| {
                            Background::Color(palette.primary.weak.color)
                        }),
                        border: Border {
                            radius: 4.0.into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    }
                },
            );

            list = list.push(
                mouse_area(cell)
                    .on_press(Message::Select(index))
                    .on_double_click(Message::OpenModal),
            );
        }

        let status = format!(
            "selected: {} · pings: {}{}",
            self.projects[self.selected].name,
            self.pings,
            if self.dirty { " · unsaved" } else { "" }
        );

        let body: Element<'_, Message> = column![
            toolbar,
            list,
            space::vertical(),
            row![
                text(status).size(13),
                space::horizontal(),
                text(format!("modality: {}", self.modality)).size(11),
            ],
        ]
        .spacing(10)
        .padding(12)
        .into();

        if self.modal.is_some() {
            // Visual scrim ONLY (not `opaque(..)`): the synthetic-click test
            // must measure the OS-level block, not an iced hit-test.
            stack![
                body,
                container(space())
                    .width(Fill)
                    .height(Fill)
                    .style(|_: &Theme| container::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.0, 0.0, 0.0, 0.35,
                        ))),
                        ..container::Style::default()
                    }),
            ]
            .into()
        } else {
            body
        }
    }

    fn view_inspector(&self) -> Element<'_, Message> {
        let project = &self.projects[self.selected];
        let pong = self.pong;

        let form = column![
            text("Name").size(11),
            text_input("name", &project.name).on_input(Message::InspectorName),
            text("Owner").size(11),
            text_input("owner", &project.owner)
                .on_input(Message::InspectorOwner),
            text("Budget").size(11),
            text_input("budget", &project.budget)
                .on_input(Message::InspectorBudget),
            toggler(project.open)
                .label("Open")
                .on_toggle(Message::InspectorOpen),
            row![
                button("Ping").on_press(Message::Ping),
                space::horizontal(),
                text(format!("pings: {}", self.pings)).size(12),
            ]
            .align_y(Center),
        ]
        .spacing(6);

        container(form)
            .padding(12)
            .width(Fill)
            .height(Fill)
            .style(move |theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    background: Some(Background::Color(if pong {
                        palette.success.base.color
                    } else {
                        palette.background.base.color
                    })),
                    ..container::Style::default()
                }
            })
            .into()
    }

    fn view_prefs(&self) -> Element<'_, Message> {
        column![
            toggler(self.compact)
                .label("Compact rows")
                .on_toggle(Message::ToggleCompact),
            text("Theme").size(12),
            radio(
                "Light",
                ThemeChoice::Light,
                Some(self.theme_choice),
                Message::SetTheme
            ),
            radio(
                "Dark",
                ThemeChoice::Dark,
                Some(self.theme_choice),
                Message::SetTheme
            ),
            radio(
                "System",
                ThemeChoice::System,
                Some(self.theme_choice),
                Message::SetTheme
            ),
        ]
        .spacing(8)
        .padding(12)
        .into()
    }

    fn view_modal(&self) -> Element<'_, Message> {
        let Some(modal) = &self.modal else {
            return space().into();
        };

        column![
            text("Edit project").size(16),
            text_input("name", &modal.name)
                .id("modal-name")
                .on_input(Message::ModalName)
                .on_submit(Message::ModalOk),
            text_input("budget", &modal.budget)
                .id("modal-budget")
                .on_input(Message::ModalBudget)
                .on_submit(Message::ModalOk),
            row![
                space::horizontal(),
                button("Cancel").on_press(Message::ModalCancel),
                button("OK").on_press(Message::ModalOk),
            ]
            .spacing(8),
        ]
        .spacing(10)
        .padding(14)
        .into()
    }

    // -----------------------------------------------------------------------
    // Scripted self-test (WINDOWS_SELFTEST=1)
    // -----------------------------------------------------------------------

    fn selftest_step(&mut self, step: usize) -> Task<Message> {
        if self.selftest.is_none() {
            return Task::none();
        }
        let next =
            move || delay(350).then(move |()| Task::done(Message::SelfTestStep(step + 1)));

        macro_rules! check {
            ($name:expr, $ok:expr) => {
                let ok = $ok;
                self.selftest.as_mut().unwrap().check($name, ok);
            };
        }

        match step {
            0 => {
                check!("boot: exactly 1 window", self.open_windows() == 1);
                next()
            }
            1 => Task::batch([
                Task::done(Message::ToggleInspector),
                next(),
            ]),
            2 => {
                check!("inspector opened (2 windows)", self.open_windows() == 2);
                check!("inspector is an OS child window", self.parented || cfg!(not(target_os = "macos")));
                Task::batch([
                    Task::done(Message::InspectorName(
                        "Renamed by inspector".into(),
                    )),
                    next(),
                ])
            }
            3 => {
                check!(
                    "inspector edit visible in main list (shared state)",
                    self.projects[self.selected].name == "Renamed by inspector"
                );
                check!("edit marked the app dirty", self.dirty);
                Task::batch([Task::done(Message::OpenPrefs), next()])
            }
            4 => {
                check!("preferences opened (3 windows)", self.open_windows() == 3);
                Task::batch([
                    Task::done(Message::Ping),
                    Task::done(Message::Ping),
                    Task::done(Message::ToggleCompact(true)),
                    next(),
                ])
            }
            5 => {
                check!("ping crossed windows twice", self.pings == 2);
                check!("compact rows applied to main list", self.compact);
                Task::batch([
                    Task::done(Message::SetTheme(ThemeChoice::Dark)),
                    next(),
                ])
            }
            6 => {
                check!(
                    "theme choice is global to every window",
                    self.theme_choice == ThemeChoice::Dark
                );
                Task::batch([Task::done(Message::OpenModal), next()])
            }
            7 => {
                check!("modal window opened (4 windows)", self.open_windows() == 4);
                check!(
                    "a real modality mechanism is in force",
                    self.modality != "none"
                );
                Task::batch([
                    Task::done(Message::ModalName("Modal edit".into())),
                    Task::done(Message::ModalBudget("999.50".into())),
                    next(),
                ])
            }
            8 => Task::batch([Task::done(Message::ModalOk), next()]),
            9 => {
                check!(
                    "modal OK committed name",
                    self.projects[self.selected].name == "Modal edit"
                );
                check!(
                    "modal OK committed budget",
                    self.projects[self.selected].budget == "999.50"
                );
                check!("modal closed (3 windows)", self.open_windows() == 3);
                Task::batch([Task::done(Message::OpenModal), next()])
            }
            10 => Task::batch([
                Task::done(Message::ModalName("DISCARD ME".into())),
                Task::done(Message::ModalCancel),
                next(),
            ]),
            11 => {
                check!(
                    "modal Cancel discarded the edit",
                    self.projects[self.selected].name == "Modal edit"
                );
                let prefs = self.prefs;
                let inspector = self.inspector;
                Task::batch([
                    prefs.map_or(Task::none(), |id| {
                        Task::done(Message::CloseRequested(id))
                    }),
                    inspector.map_or(Task::none(), |id| {
                        Task::done(Message::CloseRequested(id))
                    }),
                    next(),
                ])
            }
            12 => {
                check!(
                    "closing children left only the main window",
                    self.open_windows() == 1
                );
                self.dirty = true;
                Task::batch([
                    Task::done(Message::CloseRequested(self.main)),
                    next(),
                ])
            }
            13 => {
                check!(
                    "close veto: Cancel kept the app alive",
                    self.open_windows() == 1 && self.dirty
                );
                self.save_geometry();
                next()
            }
            14 => {
                let stored: Option<Persisted> =
                    std::fs::read_to_string(state_path())
                        .ok()
                        .and_then(|raw| serde_json::from_str(&raw).ok());
                check!(
                    "geometry persisted to disk",
                    stored.is_some_and(|s| s.main.is_some())
                );
                next()
            }
            _ => {
                let test = self.selftest.as_ref().unwrap();
                println!(
                    "SELFTEST DONE pass={} fail={}",
                    test.pass, test.fail
                );
                iced::exit()
            }
        }
    }
}
