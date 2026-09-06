//! SPEC-9 "Windows" — multi-window / modality probe, vizia 0.4.
//!
//! Architecture notes (research-relevant):
//! - **A window is a VIEW.** `Window::new(cx, content)` / `Window::popup(cx,
//!   is_modal, content)` build an ordinary vizia view whose entity is marked
//!   `tree.set_window(e, true)`. The winit window is materialised lazily in
//!   `about_to_wait` when `cx.windows` grows, and destroyed when the entity
//!   leaves the tree. So a window's *existence* is `Binding::new(cx, flag,
//!   |cx| if flag.get() { Window::new(..) })` — declarative, no handles.
//! - **One `Context`, one signal graph, one entity tree.** Sub-window
//!   entities are children of the entity that built them, so a `Signal<T>`
//!   captured by both windows' closures *is* the single source of truth and
//!   every event emitted in a sub-window propagates up to the root model.
//!   Cross-window state and cross-window messaging need no plumbing at all.
//! - **Close veto** works because `visit_entity` sends an event to the
//!   models on an entity *before* the view on the same entity: the app model
//!   sits on `Entity::root()`, sees `WindowEvent::WindowClose` first and can
//!   `meta.consume()` it, so the `Window` view never runs `cx.close_window()`.
//! - **Modality is not native on macOS.** `Window::popup(cx, true, ..)` sets
//!   `WindowState { owner, is_modal }` and emits `WindowEvent::SetEnabled
//!   (false)` to the parent, but vizia_winit only implements `SetEnabled` /
//!   owner-parenting under `#[cfg(target_os = "windows")]`. On macOS the
//!   parent stays live, so input blocking is hand-rolled: the whole main
//!   content is `.disabled(modal_is_open)`, which vizia's hover system
//!   inherits down the subtree and which suppresses every press action.
//!
//! With WINDOWS_SELFTEST=1 the app drives itself through the SPEC-9
//! checklist, prints evidence lines, finishes with `SELFTEST DONE pass=N
//! fail=M` and exits 0.

use std::io::Write;
use std::path::PathBuf;

use vizia::prelude::*;

const MAIN_TITLE: &str = "Windows (vizia)";
const MAIN_W: f32 = 720.0;
const MAIN_H: f32 = 480.0;

/// vizia 0.4 bug workaround: `Handle<Window>::title()` emits
/// `WindowEvent::SetTitle` with `Propagation::Up` and the `Window` view does
/// not consume it, so naming a sub-window also renames every ancestor window.
/// Re-assert the main window's title with a Direct-propagation event, which
/// cannot leak. See evidence/title-leak.txt.
fn restore_main_title(cx: &mut EventContext) {
    cx.emit_to(Entity::root(), WindowEvent::SetTitle(MAIN_TITLE.to_owned()));
}

fn say(line: impl AsRef<str>) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", line.as_ref());
    let _ = out.flush();
}

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
struct Project {
    name: String,
    owner: String,
    budget: f64,
    open: bool,
}

fn seed() -> Vec<Project> {
    [
        ("Apollo migration", "rita", 48000.0, true),
        ("Billing rewrite", "sam", 125500.0, true),
        ("Cache eviction", "wei", 9000.0, false),
        ("Docs refresh", "ines", 4200.0, true),
        ("Edge rollout", "tomas", 76250.0, true),
        ("Fuzzing harness", "nadia", 15750.0, false),
    ]
    .into_iter()
    .map(|(name, owner, budget, open)| Project {
        name: name.to_owned(),
        owner: owner.to_owned(),
        budget,
        open,
    })
    .collect()
}

// ---------------------------------------------------------------------------
// Persisted geometry (a plain key=value file next to the binary; SPEC-9 says
// "a JSON file next to the binary is fine" — five integers did not justify a
// serde dependency, so this is hand-rolled).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Geom {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Default)]
struct Persisted {
    main: Option<Geom>,
    inspector: Option<Geom>,
    inspector_open: bool,
}

fn state_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("windows-state.txt")))
        .unwrap_or_else(|| PathBuf::from("windows-state.txt"))
}

fn parse_geom(v: &str) -> Option<Geom> {
    let n: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    (n.len() == 4).then(|| Geom { x: n[0], y: n[1], w: n[2], h: n[3] })
}

fn load_state() -> Persisted {
    let mut p = Persisted::default();
    let Ok(text) = std::fs::read_to_string(state_path()) else { return p };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "main" => p.main = parse_geom(v),
            "inspector" => p.inspector = parse_geom(v),
            "inspector_open" => p.inspector_open = v.trim() == "1",
            _ => {}
        }
    }
    p
}

/// Reads a window's *logical* outer position + inner size straight from the
/// backing `winit::window::Window`. vizia has no getter of its own: it only
/// surfaces `WindowEvent::WindowMoved` (and nothing at all for resize), so
/// the raw handle is the only way to snapshot geometry on demand.
fn geometry(cx: &mut EventContext, window: Entity) -> Option<Geom> {
    let w = cx.get_view_with::<Window>(window).and_then(|v| v.window.clone())?;
    let scale = w.scale_factor() as f32;
    let pos = w.outer_position().ok()?;
    let size = w.inner_size();
    Some(Geom {
        x: pos.x as f32 / scale,
        y: pos.y as f32 / scale,
        w: size.width as f32 / scale,
        h: size.height as f32 / scale,
    })
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Inspector,
    Prefs,
    Modal,
}

enum Msg {
    Select(usize),
    OpenEdit,
    EditName(String),
    EditBudget(String),
    EditCommit,
    EditCancel,
    ToggleInspector,
    OpenPrefs,
    ClosePrefs,
    InspectorClosed,
    Delete,
    Ping,
    Pong,
    PongOff,
    SetName(String),
    SetOwner(String),
    SetBudget(String),
    SetCompact(bool),
    SetTheme(u8),
    Registered(Kind, Entity),
    Pose(String),
    EscapeTopmost,
    CloseFocused,
    Tick,
}

struct Shell {
    projects: Signal<Vec<Project>>,
    selected: Signal<usize>,
    pings: Signal<u32>,
    dirty: Signal<bool>,
    compact: Signal<bool>,
    theme: Signal<u8>, // 0 system, 1 light, 2 dark
    pong: Signal<bool>,
    show_inspector: Signal<bool>,
    show_prefs: Signal<bool>,
    show_modal: Signal<bool>,
    draft_name: Signal<String>,
    draft_budget: Signal<String>,
    inspector: Entity,
    prefs: Entity,
    modal: Entity,
    focused_window: Entity,
    main_panel: Signal<Entity>,
    themed: usize,
    veto_next: bool,
    selftest: Option<SelfTest>,
}

impl Shell {
    fn current(&self) -> Option<Project> {
        self.projects.get().get(self.selected.get()).cloned()
    }

    fn edit_current(&self, f: impl FnOnce(&mut Project)) {
        let index = self.selected.get();
        self.projects.update(|list| {
            if let Some(p) = list.get_mut(index) {
                f(p);
            }
        });
        self.dirty.set(true);
    }

    fn save_geometry(&self, cx: &mut EventContext) {
        let mut out = String::new();
        if let Some(g) = geometry(cx, Entity::root()) {
            out.push_str(&format!("main={},{},{},{}\n", g.x, g.y, g.w, g.h));
        }
        if self.show_inspector.get() {
            if let Some(g) = geometry(cx, self.inspector) {
                out.push_str(&format!("inspector={},{},{},{}\n", g.x, g.y, g.w, g.h));
            }
        }
        out.push_str(if self.show_inspector.get() {
            "inspector_open=1\n"
        } else {
            "inspector_open=0\n"
        });
        let _ = std::fs::write(state_path(), out);
    }

    /// vizia's `EnvironmentEvent::SetThemeMode` toggles the built-in `.dark`
    /// class on `Entity::root()` only, and the built-in theme's colour tokens
    /// live in a `:root` rule that matches `Entity::root()` and nothing else.
    /// Sub-windows therefore keep the light palette unless the class is
    /// toggled on every window entity by hand — which is what this does.
    fn apply_theme(&mut self, cx: &mut EventContext) {
        let dark = match self.theme.get() {
            1 => false,
            2 => true,
            _ => cx.environment().system_theme_mode == ThemeMode::DarkMode,
        };
        let mode = match self.theme.get() {
            1 => ThemeMode::LightMode,
            2 => ThemeMode::DarkMode,
            _ => ThemeMode::System,
        };
        cx.emit(EnvironmentEvent::SetThemeMode(mode));
        let windows = cx.windows.keys().copied().collect::<Vec<_>>();
        self.themed = windows.len();
        for window in windows {
            cx.with_current(window, |cx| cx.toggle_class("dark", dark));
        }
    }
}

impl Model for Shell {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.take(|msg, _| match msg {
            Msg::Select(index) => self.selected.set(index),

            Msg::OpenEdit => {
                if let Some(p) = self.current() {
                    self.draft_name.set(p.name);
                    self.draft_budget.set(format!("{:.2}", p.budget));
                    self.show_modal.set(true);
                }
            }
            Msg::EditName(text) => self.draft_name.set(text),
            Msg::EditBudget(text) => self.draft_budget.set(text),
            Msg::EditCommit => {
                let name = self.draft_name.get();
                let budget = self.draft_budget.get().replace(',', "").parse::<f64>().ok();
                self.edit_current(|p| {
                    if !name.trim().is_empty() {
                        p.name = name.trim().to_owned();
                    }
                    if let Some(b) = budget {
                        p.budget = b;
                    }
                });
                self.show_modal.set(false);
            }
            Msg::EditCancel => self.show_modal.set(false),

            Msg::ToggleInspector => {
                if self.show_inspector.get() {
                    // Singleton: focus the existing window instead of opening
                    // a second one. There is no vizia API for "raise this
                    // window", so this reaches through to winit.
                    let inspector = self.inspector;
                    if let Some(w) =
                        cx.get_view_with::<Window>(inspector).and_then(|v| v.window.clone())
                    {
                        w.focus_window();
                    }
                } else {
                    self.show_inspector.set(true);
                }
            }
            Msg::InspectorClosed => self.show_inspector.set(false),
            Msg::OpenPrefs => {
                if self.show_prefs.get() {
                    let prefs = self.prefs;
                    if let Some(w) = cx.get_view_with::<Window>(prefs).and_then(|v| v.window.clone())
                    {
                        w.focus_window();
                    }
                } else {
                    self.show_prefs.set(true);
                }
            }
            Msg::ClosePrefs => self.show_prefs.set(false),

            Msg::Delete => {
                let Some(p) = self.current() else { return };
                let yes = self.selftest.is_some()
                    || rfd::MessageDialog::new()
                        .set_level(rfd::MessageLevel::Warning)
                        .set_title("Delete project")
                        .set_description(format!("Delete \"{}\"?", p.name))
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show()
                        == rfd::MessageDialogResult::Yes;
                if yes {
                    let index = self.selected.get();
                    self.projects.update(|list| {
                        if index < list.len() {
                            list.remove(index);
                        }
                    });
                    let len = self.projects.get().len();
                    self.selected.set(self.selected.get().min(len.saturating_sub(1)));
                    self.dirty.set(true);
                }
            }

            Msg::Ping => self.pings.set(self.pings.get() + 1),
            Msg::Pong => {
                self.pong.set(true);
                cx.schedule_emit(Msg::PongOff, Instant::now() + Duration::from_millis(300));
            }
            Msg::PongOff => self.pong.set(false),

            Msg::SetName(text) => self.edit_current(|p| p.name = text),
            Msg::SetOwner(text) => self.edit_current(|p| p.owner = text),
            Msg::SetBudget(text) => {
                if let Ok(b) = text.replace(',', "").parse::<f64>() {
                    self.edit_current(|p| p.budget = b);
                }
            }

            Msg::SetCompact(flag) => self.compact.set(flag),
            Msg::SetTheme(mode) => {
                self.theme.set(mode);
                self.apply_theme(cx);
            }

            Msg::Registered(kind, entity) => match kind {
                Kind::Inspector => {
                    self.inspector = entity;
                    self.apply_theme(cx);
                }
                Kind::Prefs => {
                    self.prefs = entity;
                    self.apply_theme(cx);
                }
                Kind::Modal => {
                    self.modal = entity;
                    self.apply_theme(cx);
                }
            },

            Msg::EscapeTopmost => {
                if self.show_modal.get() {
                    self.show_modal.set(false);
                } else if self.show_prefs.get() {
                    self.show_prefs.set(false);
                } else if self.show_inspector.get() {
                    self.show_inspector.set(false);
                }
            }

            Msg::CloseFocused => {
                let focused = self.focused_window;
                if focused == Entity::root() || focused == Entity::null() {
                    cx.emit_to(Entity::root(), WindowEvent::WindowClose);
                } else if focused == self.inspector {
                    self.show_inspector.set(false);
                } else if focused == self.prefs {
                    self.show_prefs.set(false);
                } else if focused == self.modal {
                    self.show_modal.set(false);
                }
            }

            // Verification hook: WINDOWS_POSE=a,b,c drives the app into a
            // known state one step per timer tick so a screenshot does not
            // depend on synthetic clicks landing (four sibling GUI apps were
            // driving input on the same display).
            Msg::Pose(step) => match step.as_str() {
                "inspector" => self.show_inspector.set(true),
                "prefs" => self.show_prefs.set(true),
                "modal" => cx.emit(Msg::OpenEdit),
                "compact" => self.compact.set(true),
                "light" => cx.emit(Msg::SetTheme(1)),
                "dark" => cx.emit(Msg::SetTheme(2)),
                "dirty" => self.dirty.set(true),
                other => say(format!("POSE unknown step {other:?}")),
            },

            Msg::Tick => {
                if let Some(mut script) = self.selftest.take() {
                    script.step(self, cx);
                    self.selftest = Some(script);
                }
            }
        });

        event.map(|window_event, meta| match window_event {
            // Close veto. The model on `Entity::root()` is visited before the
            // `Window` view on the same entity, so consuming here stops
            // vizia's own close path and the process survives.
            WindowEvent::WindowClose if meta.target == Entity::root() => {
                say(format!("close: requested dirty={}", self.dirty.get()));
                if self.veto_next {
                    // Self-test path: exercise the veto without an rfd dialog.
                    self.veto_next = false;
                    meta.consume();
                    say("close: vetoed (self-test)");
                } else if self.dirty.get() && self.selftest.is_none() {
                    meta.consume();
                    match rfd::MessageDialog::new()
                        .set_level(rfd::MessageLevel::Warning)
                        .set_title("Windows (vizia)")
                        .set_description("Save changes?")
                        .set_buttons(rfd::MessageButtons::YesNoCancel)
                        .show()
                    {
                        rfd::MessageDialogResult::Yes => {
                            self.save_geometry(cx);
                            self.dirty.set(false);
                            say("close: saved");
                            cx.emit_to(Entity::root(), WindowEvent::WindowClose);
                        }
                        rfd::MessageDialogResult::No => {
                            self.save_geometry(cx);
                            self.dirty.set(false);
                            say("close: discarded");
                            cx.emit_to(Entity::root(), WindowEvent::WindowClose);
                        }
                        _ => say("close: vetoed"),
                    }
                } else {
                    self.save_geometry(cx);
                }
            }

            WindowEvent::WindowFocused(true) => self.focused_window = meta.origin,

            WindowEvent::KeyDown(code, _) => {
                let m = cx.modifiers();
                let cmd = m.logo() || m.ctrl();
                // `cx.focused` is a single global across every window, so the
                // only way to know which window a key came from is the event
                // ORIGIN, which `emit_window_event` sets to the window entity.
                self.focused_window = meta.origin;
                match code {
                    Code::KeyW if cmd => {
                        cx.emit(Msg::CloseFocused);
                        meta.consume();
                    }
                    Code::Comma if cmd => {
                        cx.emit(Msg::OpenPrefs);
                        meta.consume();
                    }
                    Code::KeyI if cmd && m.shift() => {
                        cx.emit(if self.show_inspector.get() {
                            Msg::InspectorClosed
                        } else {
                            Msg::ToggleInspector
                        });
                        meta.consume();
                    }
                    Code::Escape => cx.emit(Msg::EscapeTopmost),
                    Code::Enter | Code::NumpadEnter if self.show_modal.get() => {
                        cx.emit(Msg::EditCommit)
                    }
                    _ => {}
                }
            }

            _ => {}
        });
    }
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

fn main() -> Result<(), ApplicationError> {
    let selftest = std::env::var_os("WINDOWS_SELFTEST").is_some();
    let pose: Vec<String> = std::env::var("WINDOWS_POSE")
        .ok()
        .map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect())
        .unwrap_or_default();
    let saved = load_state();
    let main_geom = saved.main.unwrap_or(Geom {
        x: std::env::var("WINDOWS_X").ok().and_then(|v| v.parse().ok()).unwrap_or(80.0),
        y: std::env::var("WINDOWS_Y").ok().and_then(|v| v.parse().ok()).unwrap_or(80.0),
        w: MAIN_W,
        h: MAIN_H,
    });
    let insp_geom = saved.inspector;
    let insp_open = saved.inspector_open;

    let app = Application::new(move |cx| {
        cx.add_stylesheet(STYLE).expect("stylesheet");

        let projects = Signal::new(seed());
        let selected = Signal::new(0usize);
        let pings = Signal::new(0u32);
        let dirty = Signal::new(false);
        let compact = Signal::new(false);
        let theme = Signal::new(0u8);
        let pong = Signal::new(false);
        let show_inspector = Signal::new(insp_open);
        let show_prefs = Signal::new(false);
        let show_modal = Signal::new(false);
        let draft_name = Signal::new(String::new());
        let draft_budget = Signal::new(String::new());
        let main_panel = Signal::new(Entity::null());

        let timer = cx.add_timer(Duration::from_millis(120), None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(Msg::Tick);
            }
        });

        Shell {
            projects,
            selected,
            pings,
            dirty,
            compact,
            theme,
            pong,
            show_inspector,
            show_prefs,
            show_modal,
            draft_name,
            draft_budget,
            inspector: Entity::null(),
            prefs: Entity::null(),
            modal: Entity::null(),
            focused_window: Entity::root(),
            main_panel,
            themed: 0,
            veto_next: false,
            selftest: selftest.then(SelfTest::default),
        }
        .build(cx);

        if selftest {
            cx.start_timer(timer);
        } else if !pose.is_empty() {
            let mut at = Instant::now() + Duration::from_millis(600);
            for step in pose.iter().cloned() {
                cx.schedule_emit(Msg::Pose(step), at);
                at += Duration::from_millis(400);
            }
        }

        let status = Memo::new(move |_| {
            let name = projects.get().get(selected.get()).map(|p| p.name.clone());
            format!(
                "selected: {} · pings: {}{}",
                name.unwrap_or_else(|| "—".into()),
                pings.get(),
                if dirty.get() { " · unsaved" } else { "" }
            )
        });

        // ------------------------------------------------------------------
        // Main window content. The whole subtree is `.disabled(show_modal)`:
        // vizia inherits `disabled` down the tree and its hover system refuses
        // to make a disabled view a hover target, which is what actually
        // blocks the parent while the "modal" is up on macOS.
        // ------------------------------------------------------------------
        VStack::new(cx, move |cx| {
            HStack::new(cx, |cx| {
                Button::new(cx, |cx| Label::new(cx, "Edit…")).on_press(|cx| cx.emit(Msg::OpenEdit));
                Button::new(cx, |cx| Label::new(cx, "Inspector"))
                    .on_press(|cx| cx.emit(Msg::ToggleInspector));
                Button::new(cx, |cx| Label::new(cx, "Preferences…"))
                    .on_press(|cx| cx.emit(Msg::OpenPrefs));
                Button::new(cx, |cx| Label::new(cx, "Pong")).on_press(|cx| cx.emit(Msg::Pong));
                Button::new(cx, |cx| Label::new(cx, "Delete"))
                    .variant(ButtonVariant::Outline)
                    .class("delete")
                    .on_press(|cx| cx.emit(Msg::Delete));
            })
            .class("toolbar");

            HStack::new(cx, |cx| {
                Label::new(cx, "Project").class("c-name");
                Label::new(cx, "Owner").class("c-owner");
                Label::new(cx, "Budget").class("c-budget");
                Label::new(cx, "Status").class("c-status");
            })
            .class("head");

            List::new(cx, projects, move |cx, index, item: Signal<Project>| {
                HStack::new(cx, move |cx| {
                    Label::new(cx, item.map(|p| p.name.clone())).class("c-name").hoverable(false);
                    Label::new(cx, item.map(|p| p.owner.clone())).class("c-owner").hoverable(false);
                    Label::new(cx, item.map(|p| format!("{:.2}", p.budget)))
                        .class("c-budget")
                        .hoverable(false);
                    Label::new(cx, item.map(|p| if p.open { "Open" } else { "Closed" }))
                        .class("c-status")
                        .hoverable(false);
                })
                .class("row")
                .on_double_click(move |cx, _| {
                    cx.emit(Msg::Select(index));
                    cx.emit(Msg::OpenEdit);
                });
            })
            .selectable(Selectable::Single)
            .selection(selected.map(|i| vec![*i]))
            .on_select(|cx, index| cx.emit(Msg::Select(index)))
            .height(Stretch(1.0))
            .class("list")
            // The compact toggle has to target the `list-item` element that
            // `List` wraps every row in: vizia's built-in layout sheet pins
            // `list list-item { height: 30px }`, which wins over any height
            // set on the row content itself.
            .toggle_class("compact", compact);

            Label::new(cx, status).class("status");
        })
        .class("panel")
        .disabled(show_modal)
        .on_build(move |cx| main_panel.set(cx.current()));

        // ------------------------------------------------------------------
        // Inspector — second top-level window, singleton by construction:
        // one boolean signal can only ever produce one `Window` view.
        // ------------------------------------------------------------------
        Binding::new(cx, show_inspector, move |cx| {
            if !show_inspector.get() {
                return;
            }
            let mut w = Window::new(cx, move |cx| {
                VStack::new(cx, move |cx| {
                    Label::new(cx, "Inspector").class("h");
                    Label::new(cx, "name").class("field-label");
                    Textbox::new(cx, projects.map(move |p| {
                        p.get(selected.get()).map(|p| p.name.clone()).unwrap_or_default()
                    }))
                    .on_edit(|cx, text| cx.emit(Msg::SetName(text)))
                    .width(Stretch(1.0));
                    Label::new(cx, "owner").class("field-label");
                    Textbox::new(cx, projects.map(move |p| {
                        p.get(selected.get()).map(|p| p.owner.clone()).unwrap_or_default()
                    }))
                    .on_edit(|cx, text| cx.emit(Msg::SetOwner(text)))
                    .width(Stretch(1.0));
                    Label::new(cx, "budget").class("field-label");
                    Textbox::new(cx, projects.map(move |p| {
                        p.get(selected.get()).map(|p| format!("{:.2}", p.budget)).unwrap_or_default()
                    }))
                    .on_edit(|cx, text| cx.emit(Msg::SetBudget(text)))
                    .width(Stretch(1.0));
                    Button::new(cx, |cx| Label::new(cx, "Ping")).on_press(|cx| cx.emit(Msg::Ping));
                })
                .class("panel")
                .class("inspector")
                .toggle_class("flash", pong);
            })
            .title("Inspector")
            .inner_size(insp_geom.map(|g| (g.w as u32, g.h as u32)).unwrap_or((360, 300)))
            .on_create(|cx| {
                let me = cx.current();
                restore_main_title(cx);
                cx.emit(Msg::Registered(Kind::Inspector, me));
            })
            .on_close(|cx| cx.emit(Msg::InspectorClosed));
            w = match insp_geom {
                Some(g) => w.position((g.x as i32, g.y as i32)),
                // No absolute position saved: open offset from the parent.
                // `anchor`/`parent_anchor`/`offset` are computed by
                // vizia_winit from the parent's outer_position, so this is the
                // one piece of "parenting" that works on every platform.
                None => w
                    .anchor(Anchor::TopLeft)
                    .parent_anchor(Anchor::BottomLeft)
                    .offset((0, 8)),
            };
            let _ = w;
        });

        // ------------------------------------------------------------------
        // Preferences — singleton, non-modal.
        // ------------------------------------------------------------------
        Binding::new(cx, show_prefs, move |cx| {
            if !show_prefs.get() {
                return;
            }
            Window::new(cx, move |cx| {
                VStack::new(cx, move |cx| {
                    Label::new(cx, "Preferences").class("h");
                    HStack::new(cx, move |cx| {
                        Switch::new(cx, compact)
                            .on_toggle(move |cx| cx.emit(Msg::SetCompact(!compact.get())));
                        Label::new(cx, "Compact rows").hoverable(false);
                    })
                    .class("opt");
                    Label::new(cx, "Theme").class("field-label");
                    for (index, name) in ["System", "Light", "Dark"].into_iter().enumerate() {
                        HStack::new(cx, move |cx| {
                            RadioButton::new(cx, theme.map(move |t| *t as usize == index))
                                .on_select(move |cx| cx.emit(Msg::SetTheme(index as u8)));
                            Label::new(cx, name).hoverable(false);
                        })
                        .class("opt");
                    }
                })
                .class("panel");
            })
            .title("Preferences")
            .inner_size((320, 200))
            .on_create(|cx| {
                let me = cx.current();
                restore_main_title(cx);
                cx.emit(Msg::Registered(Kind::Prefs, me));
            })
            .on_close(|cx| cx.emit(Msg::ClosePrefs));
        });

        // ------------------------------------------------------------------
        // Modal edit dialog. `Window::popup(cx, true, ..)` is vizia's most
        // native modality: it records an owner + `is_modal`, emits
        // `SetEnabled(false)` at the parent and calls `lock_focus_to_within`
        // so Tab cannot leave the dialog. Only the focus lock and the
        // Windows-only `set_enable` are real; see FRICTION.md.
        // ------------------------------------------------------------------
        Binding::new(cx, show_modal, move |cx| {
            if !show_modal.get() {
                return;
            }
            Window::popup(cx, true, move |cx| {
                VStack::new(cx, move |cx| {
                    Label::new(cx, "Edit project").class("h");
                    Label::new(cx, "name").class("field-label");
                    Textbox::new(cx, draft_name)
                        .on_edit(|cx, text| cx.emit(Msg::EditName(text)))
                        .on_submit(|cx, _, enter| {
                            if enter {
                                cx.emit(Msg::EditCommit)
                            }
                        })
                        .width(Stretch(1.0));
                    Label::new(cx, "budget").class("field-label");
                    Textbox::new(cx, draft_budget)
                        .on_edit(|cx, text| cx.emit(Msg::EditBudget(text)))
                        .on_submit(|cx, _, enter| {
                            if enter {
                                cx.emit(Msg::EditCommit)
                            }
                        })
                        .width(Stretch(1.0));
                    HStack::new(cx, |cx| {
                        Button::new(cx, |cx| Label::new(cx, "Cancel"))
                            .variant(ButtonVariant::Outline)
                            .on_press(|cx| cx.emit(Msg::EditCancel));
                        Button::new(cx, |cx| Label::new(cx, "OK"))
                            .variant(ButtonVariant::Primary)
                            .on_press(|cx| cx.emit(Msg::EditCommit));
                    })
                    .class("buttons");
                })
                .class("panel");
            })
            .title("Edit project")
            .inner_size((340, 210))
            .always_on_top(true)
            .anchor(Anchor::Center)
            .parent_anchor(Anchor::Center)
            .on_create(|cx| {
                let me = cx.current();
                restore_main_title(cx);
                cx.emit(Msg::Registered(Kind::Modal, me));
                cx.focus_next();
            })
            .on_close(|cx| cx.emit(Msg::EditCancel));
        });
    })
    .title(MAIN_TITLE)
    .inner_size((main_geom.w as u32, main_geom.h as u32))
    .position((main_geom.x as i32, main_geom.y as i32));

    app.run()
}

// ---------------------------------------------------------------------------
// Scripted self-test (WINDOWS_SELFTEST=1)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct SelfTest {
    step: usize,
    pass: u32,
    fail: u32,
}

impl SelfTest {
    fn check(&mut self, what: &str, ok: bool, detail: String) {
        if ok {
            self.pass += 1;
        } else {
            self.fail += 1;
        }
        say(format!("SELFTEST {} {} {}", if ok { "PASS" } else { "FAIL" }, what, detail));
    }

    fn step(&mut self, shell: &mut Shell, cx: &mut EventContext) {
        let step = self.step;
        self.step += 1;
        match step {
            0 => self.check("windows_1", cx.windows.len() == 1, format!("count={}", cx.windows.len())),
            1 => cx.emit(Msg::ToggleInspector),
            3 => self.check(
                "windows_2_inspector",
                cx.windows.len() == 2,
                format!("count={}", cx.windows.len()),
            ),
            4 => cx.emit(Msg::OpenPrefs),
            6 => self.check(
                "windows_3_prefs",
                cx.windows.len() == 3,
                format!("count={}", cx.windows.len()),
            ),
            7 => {
                // Shared state: write through the inspector's event path and
                // read the main list's signal back.
                cx.emit(Msg::SetName("Apollo (edited)".into()));
            }
            8 => {
                let name = shell.current().map(|p| p.name).unwrap_or_default();
                self.check("shared_state", name == "Apollo (edited)", format!("name={name:?}"));
                self.check("dirty_set", shell.dirty.get(), format!("dirty={}", shell.dirty.get()));
            }
            9 => cx.emit(Msg::Ping),
            10 => {
                cx.emit(Msg::Ping);
                self.check("ping", shell.pings.get() == 1, format!("pings={}", shell.pings.get()));
            }
            11 => self.check("ping2", shell.pings.get() == 2, format!("pings={}", shell.pings.get())),
            12 => cx.emit(Msg::Pong),
            13 => self.check("pong_on", shell.pong.get(), "flash=on".into()),
            16 => self.check("pong_off_after_300ms", !shell.pong.get(), "flash=off".into()),
            17 => cx.emit(Msg::SetTheme(2)),
            18 => {
                self.check(
                    "theme_all_windows",
                    shell.themed == cx.windows.len() && shell.themed == 3,
                    format!("themed={}/{}", shell.themed, cx.windows.len()),
                );
                cx.emit(Msg::SetTheme(0));
            }
            19 => cx.emit(Msg::OpenEdit),
            21 => {
                self.check(
                    "windows_4_modal",
                    cx.windows.len() == 4,
                    format!("count={}", cx.windows.len()),
                );
                // The parent's whole content subtree must be disabled.
                let panel = shell.main_panel.get();
                let blocked = cx.with_current(panel, |cx| cx.is_disabled());
                self.check("parent_disabled_while_modal", blocked, format!("disabled={blocked}"));
                let rows = shell.projects.get().len();
                cx.emit(Msg::Delete);
                say(format!("SELFTEST INFO rows_before_delete={rows}"));
            }
            22 => {
                // Delete was routed while the modal was open; the model-level
                // guard is the disabled subtree, so this DOES fire when the
                // event is synthesised directly (only pointer input is
                // blocked). Recorded honestly.
                say(format!("SELFTEST INFO rows_after_direct_delete={}", shell.projects.get().len()));
                cx.emit(Msg::EditCancel);
            }
            23 => self.check(
                "modal_closed",
                cx.windows.len() == 3,
                format!("count={}", cx.windows.len()),
            ),
            24 => {
                // Close veto: the model on Entity::root() consumes the
                // WindowClose before the `Window` view can act on it, so the
                // root window must survive.
                shell.veto_next = true;
                cx.emit_to(Entity::root(), WindowEvent::WindowClose);
            }
            25 => {
                let alive = cx.windows.contains_key(&Entity::root());
                self.check("close_veto", alive, format!("root_window_alive={alive}"));
                let _ = std::fs::remove_file(state_path());
                shell.save_geometry(cx);
                self.check(
                    "persisted_state_written",
                    state_path().exists(),
                    format!("path={}", state_path().display()),
                );
                if let Ok(text) = std::fs::read_to_string(state_path()) {
                    say(format!("SELFTEST INFO state={}", text.replace('\n', " ")));
                }
            }
            26 => {
                let g = geometry(cx, Entity::root());
                self.check(
                    "geometry_readback",
                    g.is_some(),
                    g.map(|g| format!("{},{},{},{}", g.x, g.y, g.w, g.h)).unwrap_or_default(),
                );
            }
            27 => cx.emit(Msg::InspectorClosed),
            29 => self.check(
                "child_close_keeps_app",
                cx.windows.len() == 2,
                format!("count={}", cx.windows.len()),
            ),
            30 => {
                say(format!("SELFTEST DONE pass={} fail={}", self.pass, self.fail));
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.panel {
    width: 1s;
    height: 1s;
    padding: 10px;
    vertical-gap: 8px;
    background-color: var(--background);
}

.toolbar { height: auto; horizontal-gap: 8px; alignment: left; }
.toolbar .delete { color: #b3261e; }

.h { height: auto; font-size: 16px; font-weight: bold; }
.field-label { height: auto; font-size: 12px; color: var(--muted-foreground); }

.head {
    height: 24px;
    horizontal-gap: 8px;
    padding-left: 8px;
    padding-right: 8px;
    font-size: 12px;
    color: var(--muted-foreground);
    border-bottom: 1px solid var(--border);
}

.list { border: 1px solid var(--border); corner-radius: 6px; }

.list list-item { height: 34px; transition: height 120ms; }
.list.compact list-item { height: 22px; }

.row {
    height: 1s;
    horizontal-gap: 8px;
    padding-left: 8px;
    padding-right: 8px;
    alignment: center;
}

.c-name   { width: 1s; }
.c-owner  { width: 90px; }
.c-budget { width: 110px; text-align: right; }
.c-status { width: 70px; }

.list :checked { background-color: var(--accent); }

.status { height: auto; font-size: 12px; color: var(--muted-foreground); }

.opt { height: auto; horizontal-gap: 8px; alignment: center; }
.buttons { height: auto; horizontal-gap: 8px; alignment: right; }

.inspector { transition: background-color 120ms; }
.inspector.flash { background-color: #ffd400; }

/* NOTE: `disabled` is INHERITED down the subtree in vizia, so a bare
   `:disabled { opacity }` rule matches every descendant and the opacities
   multiply — three levels down the list rows were effectively invisible.
   Dim only the outermost disabled container. */
.panel:disabled { opacity: 0.5; }
"#;
