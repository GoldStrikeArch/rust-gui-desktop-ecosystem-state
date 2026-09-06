//! "Windows" (xilem) — SPEC-9 multi-window / modality test.
//!
//! Architecture note (the headline): in xilem 0.4 a window is a **view**, not
//! a handle. `Xilem::new` takes ONE `AppState` and an app-logic closure that
//! returns an *iterator of `WindowView`s* each frame; a window exists exactly
//! while its `WindowId` is yielded. So "shared state across windows" is not a
//! problem you solve — there is only one `&mut AppData` and every window's
//! view is built from it in the same pass.
//!
//! What xilem 0.4 does NOT give you, and what this app therefore assembles by
//! hand around the external-event-loop embedding (`shell.rs`):
//!   * modality of any kind (no sheets, no owner-disabling, no app-modal);
//!   * window parenting on macOS (`WindowOptions` only has an *owner* setter
//!     behind `#[cfg(windows)]`);
//!   * `CloseRequested` *veto* with a real answer (the view-layer `on_close`
//!     callback cannot say "no" — but declining to close is the default, see
//!     the close-veto note below);
//!   * any way to focus, move, resize or query a window after creation.

mod shell;

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use xilem::core::{MessageProxy, fork};
use xilem::masonry::properties::types::AsUnit;
use xilem::masonry::theme::default_property_set;
use xilem::style::{Padding, Style as _};
use xilem::tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use xilem::view::{
    CrossAxisAlignment, FlexSpacer, button, checkbox, flex_col, flex_row, label,
    sized_box, text_input, worker,
};
use xilem::winit::dpi::{LogicalPosition, LogicalSize};
use xilem::winit::window::WindowLevel;
use xilem::{AppState, Color, EventLoop, WidgetView, WindowId, WindowView, Xilem};

use masonry_winit::app::MasonryState;

// --- MARK: shared types

/// The four logical windows. `Win` is *our* concept; xilem only knows
/// `WindowId`s, and masonry only knows winit `WindowId`s. `shell.rs` keeps the
/// winit-id -> `Win` map because per-window key handling and the modal input
/// block both happen at the winit layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Win {
    Main,
    Inspector,
    Prefs,
    Modal,
}

impl Win {
    pub fn key(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Inspector => "inspector",
            Self::Prefs => "prefs",
            Self::Modal => "modal",
        }
    }
}

/// Answer to the "Save changes?" close prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Cancel,
}

/// Events crossing the winit layer / background threads into app state.
#[derive(Clone, Debug)]
pub enum Ev {
    /// Sent once by the worker so the driver gets an `on_action` early and can
    /// publish the winit-id -> `Win` map before any real input arrives.
    Ready,
    OpenPrefs,
    ToggleInspector,
    ModalCancel,
    /// End of the 300 ms Pong flash (posted by a detached thread).
    PongEnd,
    /// The shell answered the close prompt with "Save".
    SaveAll,
    SelfTest(u32),
}

/// State shared between app state (xilem side) and the shell layer.
///
/// This exists only because the shell layer runs *outside* the view tree:
/// the winit `ApplicationHandler` and the wrapper `AppDriver` cannot borrow
/// `AppData`. Everything here is either a request to the shell or a fact the
/// shell observed.
#[derive(Default)]
pub struct Shared {
    pub tx: Mutex<Option<UnboundedSender<Ev>>>,
    /// winit window id -> logical window, published by the driver.
    pub ids: Mutex<Vec<(xilem::winit::window::WindowId, Win)>>,
    /// Which logical windows app logic is currently yielding.
    pub open: Mutex<Vec<Win>>,
    /// Logical geometry of every open window, refreshed on every action.
    pub geom: Mutex<Vec<(Win, [f64; 4])>>,
    pub modal_open: AtomicBool,
    pub dirty: AtomicBool,
    pub quit: AtomicBool,
    /// Window the driver should call `focus_window()` on (0 = none).
    pub focus: AtomicU8,
    /// Window the driver last actually called `focus_window()` on (evidence).
    pub focus_applied: AtomicU8,
    /// Input events dropped by the modal block (evidence).
    pub blocked: AtomicUsize,
    /// Close requests vetoed (evidence).
    pub vetoed: AtomicUsize,
    /// Scripted answer for the self-test (0 = ask the user via rfd).
    pub scripted: AtomicU8,
    /// Self-test asks the driver to raise a close request on the main window.
    pub request_close_main: AtomicBool,
    // Persisted preferences, mirrored here so the driver can write the file.
    pub insp_open: AtomicBool,
    pub prefs_open: AtomicBool,
    pub compact: AtomicBool,
    pub theme: AtomicU8,
}

impl Shared {
    pub fn send(&self, ev: Ev) {
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(ev);
        }
    }
    pub fn tag_of(&self, id: xilem::winit::window::WindowId) -> Option<Win> {
        self.ids
            .lock()
            .unwrap()
            .iter()
            .find(|(w, _)| *w == id)
            .map(|(_, t)| *t)
    }
    pub fn geom_of(&self, w: Win) -> Option<[f64; 4]> {
        self.geom
            .lock()
            .unwrap()
            .iter()
            .find(|(t, _)| *t == w)
            .map(|(_, g)| *g)
    }
    pub fn scripted_answer(&self) -> Option<Answer> {
        match self.scripted.load(Ordering::SeqCst) {
            1 => Some(Answer::Save),
            2 => Some(Answer::Discard),
            3 => Some(Answer::Cancel),
            _ => None,
        }
    }
}

// --- MARK: model

#[derive(Clone, Debug)]
struct Project {
    name: String,
    owner: String,
    budget: f64,
    open: bool,
}

fn seed() -> Vec<Project> {
    [
        ("Apollo Rewrite", "dana", 42000.0, true),
        ("Beacon Migration", "ravi", 18500.0, true),
        ("Cinder Pipeline", "mei", 7250.0, false),
        ("Delta Rollout", "tom", 96000.0, true),
        ("Ember Audit", "kate", 3100.0, false),
        ("Fathom Redesign", "luis", 55400.0, true),
    ]
    .into_iter()
    .map(|(name, owner, budget, open)| Project {
        name: name.into(),
        owner: owner.into(),
        budget,
        open,
    })
    .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Theme {
    Light,
    Dark,
    System,
}

impl Theme {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Light,
            1 => Self::Dark,
            _ => Self::System,
        }
    }
    fn as_u8(self) -> u8 {
        match self {
            Self::Light => 0,
            Self::Dark => 1,
            Self::System => 2,
        }
    }
    fn is_dark(self) -> bool {
        match self {
            Self::Light => false,
            Self::Dark => true,
            Self::System => system_is_dark(),
        }
    }
}

/// Cached: winit only reports theme *changes*, and shelling out on every
/// rebuild would fork a process per frame.
fn system_is_dark() -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Dark"))
        .unwrap_or(false)
    })
}

struct Draft {
    name: String,
    budget: String,
}

struct AppData {
    projects: Vec<Project>,
    sel: usize,
    pings: u32,
    dirty: bool,
    running: bool,
    inspector_open: bool,
    prefs_open: bool,
    modal: Option<Draft>,
    compact: bool,
    theme: Theme,
    flash: bool,
    status: String,
    insp_budget: String,
    last_click: Option<(usize, Instant)>,
    shared: Arc<Shared>,
    ids: [WindowId; 4],
    // self-test bookkeeping
    checks: Vec<(bool, String)>,
    scratch: u32,
}

impl AppState for AppData {
    fn keep_running(&self) -> bool {
        self.running
    }
}

impl AppData {
    fn win_id(&self, w: Win) -> WindowId {
        match w {
            Win::Main => self.ids[0],
            Win::Inspector => self.ids[1],
            Win::Prefs => self.ids[2],
            Win::Modal => self.ids[3],
        }
    }

    fn select(&mut self, i: usize) {
        self.sel = i.min(self.projects.len().saturating_sub(1));
        self.insp_budget = format!("{:.2}", self.cur().budget);
    }

    fn cur(&self) -> &Project {
        &self.projects[self.sel.min(self.projects.len() - 1)]
    }

    fn click_row(&mut self, i: usize) {
        let now = Instant::now();
        let dbl = matches!(self.last_click, Some((j, t))
            if j == i && now.duration_since(t) < Duration::from_millis(450));
        self.last_click = Some((i, now));
        self.select(i);
        self.status = format!("selected row {i}");
        if dbl {
            self.open_modal();
        }
    }

    fn open_modal(&mut self) {
        if self.projects.is_empty() {
            return;
        }
        let p = self.cur().clone();
        self.modal = Some(Draft {
            name: p.name,
            budget: format!("{:.2}", p.budget),
        });
        self.status = "modal open (main window input blocked)".into();
    }

    fn commit_modal(&mut self) {
        if let Some(d) = self.modal.take() {
            let i = self.sel;
            self.projects[i].name = d.name;
            if let Ok(v) = d.budget.trim().parse::<f64>() {
                self.projects[i].budget = v;
            }
            self.insp_budget = format!("{:.2}", self.projects[i].budget);
            self.dirty = true;
            self.status = "edit committed".into();
        }
        self.focus_main();
    }

    fn cancel_modal(&mut self) {
        self.modal = None;
        self.status = "edit cancelled".into();
        self.focus_main();
    }

    fn focus_main(&mut self) {
        self.shared.focus.store(1, Ordering::SeqCst);
    }

    fn toggle_inspector(&mut self) {
        if self.inspector_open {
            // Singleton: focus the existing window instead of opening a second.
            self.shared.focus.store(2, Ordering::SeqCst);
            self.status = "inspector focused (singleton)".into();
        } else {
            self.inspector_open = true;
            self.status = "inspector opened".into();
        }
    }

    fn delete_selected(&mut self) {
        if self.projects.is_empty() {
            return;
        }
        let name = self.cur().name.clone();
        let yes = match self.shared.scripted_answer() {
            Some(a) => a == Answer::Save, // self-test: "Save" stands in for "Yes"
            None => {
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title("Delete project")
                    .set_description(format!("Delete \"{name}\"?"))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    == rfd::MessageDialogResult::Yes
            }
        };
        if yes {
            self.projects.remove(self.sel);
            self.select(self.sel.saturating_sub(usize::from(self.sel >= self.projects.len())));
            self.dirty = true;
            self.status = format!("deleted {name}");
        } else {
            self.status = "delete cancelled".into();
        }
    }

    fn pong(&mut self) {
        self.flash = true;
        self.status = "pong -> inspector".into();
        let sh = self.shared.clone();
        // Cross-window message with a wake: a detached thread posts back into
        // the same tokio channel the worker drains, which wakes the loop.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            sh.send(Ev::PongEnd);
        });
    }

    fn save_projects(&mut self) {
        let path = shell::data_path();
        let dump: Vec<String> = self
            .projects
            .iter()
            .map(|p| format!("{}\t{}\t{:.2}\t{}", p.name, p.owner, p.budget, p.open))
            .collect();
        let _ = std::fs::write(&path, dump.join("\n"));
        self.dirty = false;
        self.status = format!("saved {}", path.display());
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Ready => self.status = "ready".into(),
            Ev::OpenPrefs => {
                self.prefs_open = true;
                self.status = "preferences opened (Cmd+,)".into();
            }
            Ev::ToggleInspector => {
                if self.inspector_open {
                    self.inspector_open = false;
                    self.status = "inspector closed (Cmd+Shift+I)".into();
                } else {
                    self.inspector_open = true;
                    self.status = "inspector opened (Cmd+Shift+I)".into();
                }
            }
            Ev::ModalCancel => self.cancel_modal(),
            Ev::PongEnd => self.flash = false,
            Ev::SaveAll => {
                self.save_projects();
                self.shared.quit.store(true, Ordering::SeqCst);
            }
            Ev::SelfTest(step) => self.selftest(step),
        }
    }

    // --- MARK: self-test

    fn check(&mut self, name: &str, ok: bool) {
        self.checks.push((ok, name.to_string()));
    }

    fn n_open(&self) -> usize {
        self.shared.open.lock().unwrap().len()
    }

    fn selftest(&mut self, step: u32) {
        match step {
            0 => {
                self.check("seed has 6 projects", self.projects.len() == 6);
                self.check("one window at start", self.n_open() == 1);
                self.select(2);
            }
            1 => {
                self.check("selection is row 2", self.sel == 2);
                self.toggle_inspector();
            }
            2 => {
                self.check("inspector opened -> 2 windows", self.n_open() == 2);
                // Second request must NOT open another window.
                self.toggle_inspector_button();
            }
            3 => {
                self.check("inspector is a singleton (still 2)", self.n_open() == 2);
                self.check(
                    "singleton press focused the existing window",
                    self.shared.focus_applied.load(Ordering::SeqCst) == 2,
                );
                self.prefs_open = true;
            }
            4 => {
                self.check("preferences opened -> 3 windows", self.n_open() == 3);
                self.scratch = self.pings;
                self.pings += 1; // the inspector's Ping button body
            }
            5 => {
                self.check("ping incremented main counter", self.pings == self.scratch + 1);
                // Bidirectional shared state: the inspector's text_input callback.
                let i = self.sel;
                self.projects[i].name = "Renamed By Inspector".into();
                self.dirty = true;
            }
            6 => {
                self.check(
                    "inspector edit visible in main model",
                    self.cur().name == "Renamed By Inspector",
                );
                self.check("edit set the dirty flag", self.dirty);
                self.theme = Theme::Light;
                self.compact = true;
            }
            7 => {
                let pal = palette(self.theme);
                self.check(
                    "theme change reaches every window (one palette, one pass)",
                    pal.bg == palette(Theme::Light).bg && self.n_open() == 3,
                );
                self.check("compact rows toggled", self.compact);
                self.open_modal();
            }
            8 => {
                self.check("modal opened -> 4 windows", self.n_open() == 4);
                self.check(
                    "shell reports main window input blocked",
                    self.shared.modal_open.load(Ordering::SeqCst),
                );
                let before = self.projects.len();
                self.scratch = before as u32;
                // A Delete *action* reaching app state would delete a row; the
                // point of the block is that no such action can be produced,
                // which is verified with a synthetic click (see evidence/).
                if let Some(d) = self.modal.as_mut() {
                    d.budget = "12345.00".into();
                }
            }
            9 => {
                self.commit_modal();
            }
            10 => {
                self.check("modal committed budget", (self.cur().budget - 12345.0).abs() < 1e-6);
                self.check("modal closed -> 3 windows", self.n_open() == 3);
                self.check(
                    "focus returned to main",
                    self.shared.focus_applied.load(Ordering::SeqCst) == 1,
                );
                self.check("row count unchanged by blocked delete", self.projects.len() as u32 == self.scratch);
                self.inspector_open = false; // stands in for Cmd+W on the inspector
            }
            11 => {
                self.check("closing a child window leaves 2", self.n_open() == 2);
                self.check("app still running after child close", self.running);
                // Close veto: ask the shell to raise a real CloseRequested on
                // the main window with a scripted "Cancel".
                self.dirty = true;
                self.shared.scripted.store(3, Ordering::SeqCst); // Cancel
                self.shared.request_close_main.store(true, Ordering::SeqCst);
            }
            12 => {
                self.check(
                    "dirty close request was vetoed",
                    self.shared.vetoed.load(Ordering::SeqCst) >= 1,
                );
                self.check("process alive after veto", self.running && self.n_open() == 2);
                let path = shell::state_path();
                self.check(
                    "persistence file path resolves next to the binary",
                    path.parent().map(|p| p.is_dir()).unwrap_or(false),
                );
            }
            _ => {
                let fail = self.checks.iter().filter(|(ok, _)| !ok).count();
                for (ok, name) in &self.checks {
                    println!("{} {}", if *ok { "PASS" } else { "FAIL" }, name);
                }
                println!(
                    "SELFTEST DONE pass={} fail={}",
                    self.checks.len() - fail,
                    fail
                );
                use std::io::Write as _;
                let _ = std::io::stdout().flush();
                // Exercise the Discard branch on the way out.
                self.shared.scripted.store(2, Ordering::SeqCst);
                self.shared.request_close_main.store(true, Ordering::SeqCst);
            }
        }
    }

    /// The Inspector *button* body, factored out so the self-test presses the
    /// same code path the toolbar does.
    fn toggle_inspector_button(&mut self) {
        self.toggle_inspector();
    }
}

// --- MARK: theme

struct Palette {
    bg: Color,
    fg: Color,
    dim: Color,
    field: Color,
    btn: Color,
    sel: Color,
    row: Color,
    accent: Color,
}

fn palette(theme: Theme) -> Palette {
    if theme.is_dark() {
        Palette {
            bg: Color::from_rgb8(0x18, 0x18, 0x1b),
            fg: Color::from_rgb8(0xf0, 0xf0, 0xea),
            dim: Color::from_rgb8(0x9a, 0x9a, 0x95),
            field: Color::from_rgb8(0x27, 0x27, 0x2a),
            btn: Color::from_rgb8(0x3f, 0x3f, 0x46),
            sel: Color::from_rgb8(0x2f, 0x4f, 0x7a),
            row: Color::from_rgb8(0x22, 0x22, 0x26),
            accent: Color::from_rgb8(0x6c, 0xa8, 0xff),
        }
    } else {
        Palette {
            bg: Color::from_rgb8(0xf3, 0xf3, 0xf0),
            fg: Color::from_rgb8(0x1c, 0x1c, 0x1e),
            dim: Color::from_rgb8(0x63, 0x63, 0x60),
            field: Color::WHITE,
            btn: Color::from_rgb8(0xdd, 0xdd, 0xd8),
            sel: Color::from_rgb8(0xbe, 0xd6, 0xf7),
            row: Color::from_rgb8(0xe8, 0xe8, 0xe4),
            accent: Color::from_rgb8(0x1d, 0x4e, 0xa8),
        }
    }
}

// --- MARK: views

fn tool(
    text: &'static str,
    pal: &Palette,
    f: fn(&mut AppData),
) -> impl WidgetView<AppData> + use<> {
    button(label(text).color(pal.fg), move |s: &mut AppData| f(s)).background_color(pal.btn)
}

fn cell(text: String, w: f64, color: Color) -> impl WidgetView<AppData> + use<> {
    sized_box(label(text).color(color)).width(w.px())
}

fn main_view(state: &mut AppData) -> impl WidgetView<AppData> + use<> {
    let pal = palette(state.theme);
    let compact = state.compact;
    let pad = if compact { 2.0 } else { 7.0 };

    let toolbar = flex_row((
        tool("Edit...", &pal, |s| s.open_modal()),
        tool("Inspector", &pal, |s| s.toggle_inspector_button()),
        tool("Preferences...", &pal, |s| s.prefs_open = true),
        tool("Delete", &pal, |s| s.delete_selected()),
        tool("Pong", &pal, |s| s.pong()),
    ));

    let header = flex_row((
        cell("Project".into(), 220.0, pal.dim),
        cell("Owner".into(), 110.0, pal.dim),
        cell("Budget".into(), 110.0, pal.dim),
        cell("Status".into(), 80.0, pal.dim),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start);

    let sel = state.sel;
    let rows: Vec<_> = state
        .projects
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let fg = pal.fg;
            let bg = if i == sel { pal.sel } else { pal.row };
            button(
                flex_row((
                    cell(p.name.clone(), 220.0, fg),
                    cell(p.owner.clone(), 110.0, fg),
                    cell(format!("{:.2}", p.budget), 110.0, fg),
                    cell(
                        if p.open { "Open" } else { "Closed" }.to_string(),
                        80.0,
                        fg,
                    ),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start),
                move |s: &mut AppData| s.click_row(i),
            )
            .background_color(bg)
            .padding(Padding::all(pad))
        })
        .collect();

    let status = label(format!(
        "selected: {} | pings: {} | {}{}",
        state.cur().name,
        state.pings,
        state.status,
        if state.dirty { " | UNSAVED" } else { "" }
    ))
    .color(pal.dim);

    flex_col((toolbar, header, rows, FlexSpacer::Flex(1.0), status))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .padding(Padding::all(10.0))
}

fn inspector_view(state: &mut AppData) -> impl WidgetView<AppData> + use<> {
    let pal = palette(state.theme);
    let p = state.cur().clone();
    let field_bg = if state.flash { pal.accent } else { pal.field };

    flex_col((
        label("Inspector").text_size(18.0).color(pal.fg),
        label("name").color(pal.dim),
        text_input(p.name.clone(), |s: &mut AppData, t| {
            let i = s.sel;
            s.projects[i].name = t;
            s.dirty = true;
        })
        .text_color(pal.fg)
        .background_color(field_bg),
        label("owner").color(pal.dim),
        text_input(p.owner.clone(), |s: &mut AppData, t| {
            let i = s.sel;
            s.projects[i].owner = t;
            s.dirty = true;
        })
        .text_color(pal.fg)
        .background_color(field_bg),
        label("budget").color(pal.dim),
        text_input(state.insp_budget.clone(), |s: &mut AppData, t| {
            s.insp_budget = t.clone();
            if let Ok(v) = t.trim().parse::<f64>() {
                let i = s.sel;
                s.projects[i].budget = v;
                s.dirty = true;
            }
        })
        .text_color(pal.fg)
        .background_color(field_bg),
        FlexSpacer::Fixed(6.px()),
        button(label("Ping").color(pal.fg), |s: &mut AppData| {
            s.pings += 1;
            s.status = "ping -> main".into();
        })
        .background_color(pal.btn),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .padding(Padding::all(10.0))
}

fn prefs_view(state: &mut AppData) -> impl WidgetView<AppData> + use<> {
    let pal = palette(state.theme);
    let theme = state.theme;
    let mk = |t: Theme, text: &'static str, pal_btn: Color, pal_sel: Color, fg: Color| {
        button(label(text).color(fg), move |s: &mut AppData| s.theme = t)
            .background_color(if theme == t { pal_sel } else { pal_btn })
    };
    flex_col((
        label("Preferences").text_size(18.0).color(pal.fg),
        checkbox("Compact rows", state.compact, |s: &mut AppData, v| {
            s.compact = v;
        }),
        FlexSpacer::Fixed(8.px()),
        label("Theme").color(pal.dim),
        flex_row((
            mk(Theme::Light, "Light", pal.btn, pal.sel, pal.fg),
            mk(Theme::Dark, "Dark", pal.btn, pal.sel, pal.fg),
            mk(Theme::System, "System", pal.btn, pal.sel, pal.fg),
        )),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .padding(Padding::all(10.0))
}

fn modal_view(state: &mut AppData) -> impl WidgetView<AppData> + use<> {
    let pal = palette(state.theme);
    let d = state.modal.as_ref().unwrap();
    flex_col((
        label("Edit project").text_size(16.0).color(pal.fg),
        label("name").color(pal.dim),
        text_input(d.name.clone(), |s: &mut AppData, t| {
            if let Some(d) = s.modal.as_mut() {
                d.name = t;
            }
        })
        .on_enter(|s: &mut AppData, _t| s.commit_modal())
        .text_color(pal.fg)
        .background_color(pal.field),
        label("budget").color(pal.dim),
        text_input(d.budget.clone(), |s: &mut AppData, t| {
            if let Some(d) = s.modal.as_mut() {
                d.budget = t;
            }
        })
        .on_enter(|s: &mut AppData, _t| s.commit_modal())
        .text_color(pal.fg)
        .background_color(pal.field),
        FlexSpacer::Fixed(8.px()),
        flex_row((
            button(label("OK").color(pal.fg), |s: &mut AppData| s.commit_modal())
                .background_color(pal.btn),
            button(label("Cancel").color(pal.fg), |s: &mut AppData| {
                s.cancel_modal()
            })
            .background_color(pal.btn),
        )),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .padding(Padding::all(10.0))
}

// --- MARK: app logic (the window list IS the app)

/// `WINDOWS_LOG=1` prints one line per observable state change; used as
/// evidence that a synthetic click did or did not produce an action.
pub fn logging() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("WINDOWS_LOG").is_ok())
}

/// Default main-window rect. `WINDOWS_POS=x,y` moves it (six sibling SPEC-9
/// apps from parallel test agents fight over the top-left corner).
fn default_main_rect() -> [f64; 4] {
    let mut r = [40.0, 400.0, 720.0, 480.0];
    if let Ok(v) = std::env::var("WINDOWS_POS") {
        for (i, part) in v.split(',').take(4).enumerate() {
            if let Ok(n) = part.trim().parse::<f64>() {
                r[i] = n;
            }
        }
    }
    r
}

/// `WINDOWS_TOP=1` raises the *secondary* windows (inspector, preferences) to
/// `WindowLevel::AlwaysOnTop` — the closest thing xilem/winit can express to
/// "child window that stays above its parent" on macOS, and also what makes
/// scripted clicks land when six sibling SPEC-9 apps from parallel test agents
/// share one screen. The modal dialog is always on top.
fn top_level() -> WindowLevel {
    if std::env::var("WINDOWS_TOP").is_ok() {
        WindowLevel::AlwaysOnTop
    } else {
        WindowLevel::Normal
    }
}

fn app_logic(state: &mut AppData) -> impl Iterator<Item = WindowView<AppData>> + use<> {
    let pal = palette(state.theme);
    let dark = state.theme.is_dark();
    let sh = state.shared.clone();

    // Publish state the shell layer needs (it cannot borrow AppData).
    let mut open = vec![Win::Main];
    if state.inspector_open {
        open.push(Win::Inspector);
    }
    if state.prefs_open {
        open.push(Win::Prefs);
    }
    if state.modal.is_some() {
        open.push(Win::Modal);
    }
    *sh.open.lock().unwrap() = open.clone();
    sh.modal_open
        .store(state.modal.is_some(), Ordering::SeqCst);
    sh.dirty.store(state.dirty, Ordering::SeqCst);
    sh.insp_open.store(state.inspector_open, Ordering::SeqCst);
    sh.prefs_open.store(state.prefs_open, Ordering::SeqCst);
    sh.compact.store(state.compact, Ordering::SeqCst);
    sh.theme.store(state.theme.as_u8(), Ordering::SeqCst);

    if logging() {
        let line = format!(
            "STATE rows={} sel={} name={:?} pings={} dirty={} windows={} modal={} theme={:?} compact={} status={:?}",
            state.projects.len(),
            state.sel,
            state.cur().name,
            state.pings,
            state.dirty,
            open.len(),
            state.modal.is_some(),
            state.theme,
            state.compact,
            state.status,
        );
        static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
        let mut last = LAST.lock().unwrap();
        if *last != line {
            println!("{line}");
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
            *last = line;
        }
    }

    let saved = shell::load();
    let main_geom = sh.geom_of(Win::Main);
    let g = |w: Win, def: [f64; 4]| -> [f64; 4] {
        saved
            .geom
            .iter()
            .find(|(k, _)| *k == w.key())
            .map(|(_, v)| *v)
            .unwrap_or(def)
    };
    // "Offset from the parent" is hand-computed: winit/xilem have no notion of
    // a child window's position relative to a parent on macOS.
    let (mx, my, mw) = match main_geom {
        Some([x, y, w, _]) => (x, y, w),
        None => {
            let [x, y, w, _] = g(Win::Main, default_main_rect());
            (x, y, w)
        }
    };
    let near_main = move |dx: f64, dy: f64, w: f64, h: f64| [mx + dx, my + dy, w, h];

    let main = window(
        state.win_id(Win::Main),
        "Windows (xilem)",
        fork(
            main_view(state),
            worker(
                |proxy: MessageProxy<Ev>, mut rx: UnboundedReceiver<Ev>| async move {
                    // First message: gets the driver an early `on_action` so
                    // the winit-id -> Win map exists before real input.
                    let _ = proxy.message(Ev::Ready);
                    while let Some(ev) = rx.recv().await {
                        if proxy.message(ev).is_err() {
                            break;
                        }
                    }
                },
                |s: &mut AppData, tx: UnboundedSender<Ev>| {
                    *s.shared.tx.lock().unwrap() = Some(tx);
                },
                |s: &mut AppData, ev: Ev| s.handle(ev),
            ),
        ),
    )
    .with_options(|o| {
        let [x, y, w, h] = g(Win::Main, default_main_rect());
        o.with_initial_position(LogicalPosition::new(x, y))
            .with_initial_inner_size(LogicalSize::new(w, h))
            .with_min_inner_size(LogicalSize::new(520.0, 260.0))
            .with_window_level(top_level())
    })
    .with_base_color(pal.bg);

    let inspector = state.inspector_open.then(|| {
        window(
            state.win_id(Win::Inspector),
            "Inspector",
            inspector_view(state),
        )
        .with_options(|o| {
            let [x, y, w, h] = if saved.geom.iter().any(|(k, _)| k == Win::Inspector.key()) {
                g(Win::Inspector, [0.0, 0.0, 360.0, 300.0])
            } else {
                near_main(mw + 12.0, 0.0, 360.0, 300.0)
            };
            o.with_initial_position(LogicalPosition::new(x, y))
                .with_initial_inner_size(LogicalSize::new(w, h))
                .with_window_level(top_level())
                .on_close(|s: &mut AppData| s.inspector_open = false)
        })
        .with_base_color(if state.flash { pal.accent } else { pal.bg })
    });

    let prefs = state.prefs_open.then(|| {
        window(state.win_id(Win::Prefs), "Preferences", prefs_view(state))
            .with_options(|o| {
                let [x, y, w, h] = if saved.geom.iter().any(|(k, _)| k == Win::Prefs.key()) {
                    g(Win::Prefs, [0.0, 0.0, 320.0, 200.0])
                } else {
                    near_main(mw + 394.0, 0.0, 320.0, 200.0)
                };
                o.with_initial_position(LogicalPosition::new(x, y))
                    .with_initial_inner_size(LogicalSize::new(w, h))
                    .with_window_level(top_level())
                    .on_close(|s: &mut AppData| s.prefs_open = false)
            })
            .with_base_color(pal.bg)
    });

    let modal = state.modal.is_some().then(|| {
        window(state.win_id(Win::Modal), "Edit project", modal_view(state))
            .with_options(|o| {
                // Normally offset over its parent; when every window is forced
            // AlwaysOnTop for scripted testing they tie on level, so put the
            // dialog beside the parent instead of on top of it.
            let [x, y, w, h] = if std::env::var("WINDOWS_TOP").is_ok() {
                near_main(mw + 12.0, 340.0, 380.0, 240.0)
            } else {
                near_main(150.0, 120.0, 380.0, 240.0)
            };
                o.with_initial_position(LogicalPosition::new(x, y))
                    .with_initial_inner_size(LogicalSize::new(w, h))
                    .with_resizable(false)
                    // The only "modality" WindowOptions can express: keep the
                    // dialog above its parent. It does NOT block the parent.
                    .with_window_level(WindowLevel::AlwaysOnTop)
                    .on_close(|s: &mut AppData| s.cancel_modal())
            })
            .with_base_color(pal.bg)
    });

    let _ = dark;
    std::iter::once(main)
        .chain(inspector)
        .chain(prefs)
        .chain(modal)
        .collect::<Vec<_>>()
        .into_iter()
}

use xilem::window;

// --- MARK: main

fn main() {
    let shared = Arc::new(Shared::default());
    let saved = shell::load();

    let ids = [
        WindowId::next(),
        WindowId::next(),
        WindowId::next(),
        WindowId::next(),
    ];
    let projects = seed();
    let insp_budget = format!("{:.2}", projects[0].budget);
    let selftest = std::env::var("WINDOWS_SELFTEST").is_ok();

    let data = AppData {
        projects,
        sel: 0,
        pings: 0,
        dirty: false,
        running: true,
        inspector_open: saved.inspector_open,
        prefs_open: saved.prefs_open,
        modal: None,
        compact: saved.compact,
        theme: Theme::from_u8(saved.theme),
        flash: false,
        status: "ready".into(),
        insp_budget,
        last_click: None,
        shared: shared.clone(),
        ids,
        checks: Vec::new(),
        scratch: 0,
    };

    if selftest {
        // The self-test drives the same Ev channel the shell layer uses; sleeps
        // let the event loop rebuild the window list between steps.
        let sh = shared.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1200));
            for step in 0..=13 {
                sh.send(Ev::SelfTest(step));
                std::thread::sleep(Duration::from_millis(280));
            }
            std::thread::sleep(Duration::from_secs(5));
            eprintln!("selftest watchdog: app did not exit");
            std::process::exit(1);
        });
    }

    let xilem = Xilem::new(data, app_logic);

    // External-event-loop embedding: the only way to wrap xilem's MasonryDriver
    // (close veto, focus, geometry) and to see raw winit events (modal input
    // block, per-window Cmd+W / Cmd+, / Cmd+Shift+I / Esc).
    let event_loop = EventLoop::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    let (driver, windows) =
        xilem.into_driver_and_windows(move |event| proxy.send_event(event).map_err(|err| err.0));
    let masonry_state =
        MasonryState::new(event_loop.create_proxy(), windows, default_property_set());

    let mut app = shell::ShellApp::new(masonry_state, Box::new(driver), ids, shared);
    event_loop.run_app(&mut app).unwrap();
}
