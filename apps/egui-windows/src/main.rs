//! "Windows" per apps/SPEC-9.md — multi-window & modal test for
//! egui 0.35 / eframe 0.35 on macOS.
//!
//! Window model in one sentence: an egui window is a **viewport** — a
//! `ViewportId` key plus a `ViewportBuilder` value plus a UI closure, all
//! re-declared every frame by the *parent's* pass. There is no window
//! object to hold, so "two windows see the same mutable state" is solved
//! outside egui: `show_viewport_deferred` demands
//! `Fn(&mut Ui, ViewportClass) + Send + Sync + 'static`, which forces the
//! shared model into an `Arc<Mutex<Shared>>` (see FRICTION.md).
//!
//! Modality: `egui::Modal` is an in-framework overlay. Its blocking is
//! implemented by `Memory::set_modal_layer` + `allows_interaction`, and the
//! modal-layer field lives in `Memory::focus: ViewportIdMap<Focus>` — i.e.
//! it is **per viewport**. Blocking the other windows is therefore
//! hand-rolled here (`ui.disable()` in the child passes). The only OS-level
//! modality reachable from eframe is an `rfd` message box parented to the
//! root window handle (`NSAlert::beginSheetModalForWindow:`), used for the
//! Delete confirm and the close-veto dialog.

mod selftest;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;

pub fn inspector_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("inspector")
}
pub fn prefs_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("prefs")
}

// ---------------------------------------------------------------- model ---

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Project {
    pub name: String,
    pub owner: String,
    pub budget: f64,
    pub closed: bool,
}

fn seed() -> Vec<Project> {
    [
        ("Atlas", "rin", 12_500.0, false),
        ("Borealis", "kai", 4_200.0, false),
        ("Cinder", "mo", 78_000.0, true),
        ("Dovetail", "ana", 950.0, false),
        ("Everest", "lev", 33_100.0, false),
        ("Fathom", "juno", 6_750.0, true),
    ]
    .into_iter()
    .map(|(name, owner, budget, closed)| Project {
        name: name.to_owned(),
        owner: owner.to_owned(),
        budget,
        closed,
    })
    .collect()
}

#[derive(Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Geom {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Geom {
    /// Position comes from `outer_rect` (what the OS calls the window
    /// position) but size from `inner_rect`, because `ViewportBuilder` only
    /// takes an *inner* size — mixing the two grows every window by the
    /// title-bar height on each relaunch (observed: 512 -> 544 -> 576).
    fn read(ctx: &egui::Context) -> Option<Self> {
        ctx.input(|i| {
            let o = i.viewport().outer_rect?;
            let inner = i.viewport().inner_rect.unwrap_or(o);
            Some(Self { x: o.min.x, y: o.min.y, w: inner.width(), h: inner.height() })
        })
    }
}

/// Hand-rolled persistence. eframe's own `persist_window` (a) needs the
/// non-default `persistence` feature and (b) stores exactly ONE window
/// (`STORAGE_WINDOW_KEY` in eframe/src/native/epi_integration.rs), so it
/// cannot express three viewports.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Persisted {
    pub main: Option<Geom>,
    pub inspector: Option<Geom>,
    pub prefs: Option<Geom>,
    pub inspector_open: bool,
    pub theme: u8,
    pub compact: bool,
}

/// 300 ms per SPEC-9; overridable only so a `screencapture` (which needs
/// ~0.5 s to start) can catch the flash for the evidence screenshot.
fn flash_duration() -> std::time::Duration {
    let ms = std::env::var("WINDOWS_FLASH_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300);
    std::time::Duration::from_millis(ms)
}

/// `WINDOWS_TRACE=1` prints one line per user action. Used only to make the
/// scripted macOS verification deterministic on a shared desktop where other
/// agents' windows fight for the pointer.
fn trace(msg: &str) {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    if *ON.get_or_init(|| std::env::var("WINDOWS_TRACE").is_ok_and(|v| v == "1")) {
        println!("TRACE {msg}");
    }
}

fn state_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("egui-windows-state.json")))
        .unwrap_or_else(|| "egui-windows-state.json".into())
}

fn load_state() -> Persisted {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// One source of truth, shared by every viewport. `Arc<Mutex<_>>` is not a
/// style choice: `Context::show_viewport_deferred` requires the UI closure
/// to be `Send + Sync + 'static`.
pub struct Shared {
    pub projects: Vec<Project>,
    pub selected: usize,
    pub pings: u32,
    pub dirty: bool,
    pub compact: bool,
    pub theme: u8, // 0=Light 1=Dark 2=System
    pub inspector_open: bool,
    pub prefs_open: bool,
    pub modal_open: bool,
    pub pong_until: Option<Instant>,
    pub geom: Persisted,
    /// Recorded from *inside* the inspector pass: proves the theme change
    /// reached the other window, and its own DPI.
    pub inspector_dark: Option<bool>,
    pub inspector_ppp: Option<f32>,
    pub inspector_seen_name: Option<String>,
    pub prefs_ran: bool,
}

pub type SharedState = Arc<Mutex<Shared>>;

// ------------------------------------------------------------------ app ---

struct Draft {
    name: String,
    budget: String,
    focused: bool,
}

pub struct WindowsApp {
    pub shared: SharedState,
    edit: Option<Draft>,
    quit_ok: bool,
    applied_theme: u8,
    insp_pos: Option<[f32; 2]>,
    insp_size: [f32; 2],
    prefs_pos: Option<[f32; 2]>,
    prefs_size: [f32; 2],
    pub selftest: Option<selftest::SelfTest>,
}

fn main() -> eframe::Result {
    let saved = load_state();
    let mut vp = egui::ViewportBuilder::default()
        .with_title("Windows (egui)")
        .with_inner_size([720.0, 480.0])
        .with_resizable(true);
    if let Some(g) = saved.main {
        vp = vp.with_position([g.x, g.y]).with_inner_size([g.w, g.h]);
    } else {
        vp = vp.with_position([760.0, 430.0]);
    }
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(
        "Windows (egui)",
        options,
        Box::new(move |_cc| Ok(Box::new(WindowsApp::new(saved)))),
    )
}

impl WindowsApp {
    fn new(saved: Persisted) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            projects: seed(),
            selected: 0,
            pings: 0,
            dirty: false,
            compact: saved.compact,
            theme: saved.theme,
            inspector_open: saved.inspector_open,
            prefs_open: false,
            modal_open: false,
            pong_until: None,
            geom: saved.clone(),
            inspector_dark: None,
            inspector_ppp: None,
            inspector_seen_name: None,
            prefs_ran: false,
        }));
        Self {
            shared,
            edit: None,
            quit_ok: false,
            applied_theme: 255,
            insp_pos: saved.inspector.map(|g| [g.x, g.y]),
            insp_size: saved.inspector.map_or([360.0, 300.0], |g| [g.w, g.h]),
            prefs_pos: saved.prefs.map(|g| [g.x, g.y]),
            prefs_size: saved.prefs.map_or([320.0, 200.0], |g| [g.w, g.h]),
            selftest: selftest::SelfTest::from_env(),
        }
    }

    pub fn save_state(&self) {
        let s = self.shared.lock().unwrap();
        let mut p = s.geom.clone();
        p.inspector_open = s.inspector_open;
        p.theme = s.theme;
        p.compact = s.compact;
        if let Ok(json) = serde_json::to_string_pretty(&p) {
            let _ = std::fs::write(state_path(), json);
        }
    }

    /// Theme is a `Context`-level option, so one call restyles every
    /// viewport that shares this `Context` — i.e. all of them.
    fn apply_theme(&mut self, ctx: &egui::Context) {
        let want = self.shared.lock().unwrap().theme;
        if want != self.applied_theme {
            self.applied_theme = want;
            ctx.set_theme(match want {
                0 => egui::ThemePreference::Light,
                1 => egui::ThemePreference::Dark,
                _ => egui::ThemePreference::System,
            });
            ctx.request_repaint_of(inspector_id());
            ctx.request_repaint_of(prefs_id());
        }
    }

    pub fn open_inspector(&mut self, ctx: &egui::Context) {
        let mut s = self.shared.lock().unwrap();
        if s.inspector_open {
            // Singleton: focus the existing window instead of a second one.
            ctx.send_viewport_cmd_to(inspector_id(), egui::ViewportCommand::Focus);
            return;
        }
        s.inspector_open = true;
        // "Child opens offset from its parent" — the only part of parenting
        // egui can express (see FRICTION.md).
        if self.insp_pos.is_none()
            && let Some(m) = s.geom.main
        {
            self.insp_pos = Some([m.x + m.w + 12.0, m.y]);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = egui::Modifiers::COMMAND;
        let hit = |m: egui::Modifiers, k: egui::Key| {
            ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(m, k)))
        };
        if hit(cmd, egui::Key::Comma) {
            trace("shortcut Cmd+, -> Preferences");
            self.shared.lock().unwrap().prefs_open = true;
        }
        if hit(cmd | egui::Modifiers::SHIFT, egui::Key::I) {
            trace("shortcut Cmd+Shift+I -> toggle inspector");
            let open = self.shared.lock().unwrap().inspector_open;
            if open {
                self.shared.lock().unwrap().inspector_open = false;
            } else {
                self.open_inspector(ctx);
            }
        }
        // Cmd+W on the *focused* window: this handler only ever runs in the
        // root pass, and each child pass has its own copy (see child_ui).
        if hit(cmd, egui::Key::W) {
            trace("shortcut Cmd+W on root");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.edit.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            let mut s = self.shared.lock().unwrap();
            if s.prefs_open {
                s.prefs_open = false;
            } else if s.inspector_open {
                s.inspector_open = false;
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            if ui.button("Edit…").clicked() {
                trace("toolbar Edit");
                self.begin_edit();
            }
            if ui.button("Inspector").clicked() {
                trace("toolbar Inspector");
                self.open_inspector(&ctx);
            }
            if ui.button("Preferences…").clicked() {
                trace("toolbar Preferences");
                self.shared.lock().unwrap().prefs_open = true;
            }
            if ui.button("Delete").clicked() {
                trace("toolbar Delete");
                self.delete_selected(frame);
            }
            ui.separator();
            if ui.button("Pong").clicked() {
                trace("toolbar Pong");
                let mut s = self.shared.lock().unwrap();
                s.pong_until = Some(Instant::now() + flash_duration());
                ctx.request_repaint_of(inspector_id());
            }
        });
    }

    fn begin_edit(&mut self) {
        let s = self.shared.lock().unwrap();
        let p = &s.projects[s.selected];
        self.edit = Some(Draft {
            name: p.name.clone(),
            budget: format!("{:.2}", p.budget),
            focused: false,
        });
    }

    /// Native confirm: `rfd` with `set_parent(frame)` — on macOS this is
    /// `NSAlert::beginSheetModalForWindow:`, a genuine window-modal sheet.
    pub fn delete_selected(&mut self, frame: &mut eframe::Frame) {
        let (name, n) = {
            let s = self.shared.lock().unwrap();
            (s.projects[s.selected].name.clone(), s.projects.len())
        };
        if n <= 1 {
            return;
        }
        let yes = if self.selftest.is_some() {
            true // scripted runs must not block on a sheet
        } else {
            rfd::MessageDialog::new()
                .set_parent(frame)
                .set_level(rfd::MessageLevel::Warning)
                .set_title(&format!("Delete \"{name}\"?"))
                .set_description("This cannot be undone.")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                == rfd::MessageDialogResult::Yes
        };
        trace(if yes { "delete confirmed" } else { "delete cancelled" });
        if yes {
            let mut s = self.shared.lock().unwrap();
            let i = s.selected;
            s.projects.remove(i);
            s.selected = i.min(s.projects.len() - 1);
            s.dirty = true;
        }
    }

    fn list(&mut self, ui: &mut egui::Ui) {
        let compact = self.shared.lock().unwrap().compact;
        let row_h = if compact { 17.0 } else { 26.0 };
        let mut click = None;
        let mut dbl = false;
        {
            let s = self.shared.lock().unwrap();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, p) in s.projects.iter().enumerate() {
                    let text = format!(
                        "{:<10} {:<6} {:>12.2}  {:<6}",
                        p.name,
                        p.owner,
                        p.budget,
                        if p.closed { "Closed" } else { "Open" }
                    );
                    let w = ui.available_width();
                    let r = ui.add_sized(
                        [w, row_h],
                        egui::Button::selectable(
                            s.selected == i,
                            egui::RichText::new(text).monospace(),
                        ),
                    );
                    if r.clicked() {
                        click = Some(i);
                    }
                    if r.double_clicked() {
                        click = Some(i);
                        dbl = true;
                    }
                }
            });
        }
        if let Some(i) = click {
            let mut s = self.shared.lock().unwrap();
            s.selected = i;
            drop(s);
            if dbl {
                self.begin_edit();
            }
        }
    }

    /// In-framework overlay modal. Blocking is real *within this viewport*:
    /// `Modal::show` calls `Memory::set_modal_layer`, and every widget below
    /// it fails `Memory::allows_interaction`.
    fn modal(&mut self, ctx: &egui::Context) {
        self.shared.lock().unwrap().modal_open = self.edit.is_some();
        let Some(draft) = &mut self.edit else { return };
        let mut done: Option<bool> = None;
        egui::Modal::new(egui::Id::new("edit_modal")).show(ctx, |ui| {
            ui.set_width(300.0);
            ui.heading("Edit project");
            ui.label("Name");
            let n = ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .id(egui::Id::new("edit_name"))
                    .desired_width(f32::INFINITY),
            );
            if !draft.focused {
                draft.focused = true;
                n.request_focus();
            }
            ui.label("Budget");
            ui.add(
                egui::TextEdit::singleline(&mut draft.budget)
                    .id(egui::Id::new("edit_budget"))
                    .desired_width(f32::INFINITY),
            );
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    done = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    done = Some(false);
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            done = Some(true);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            done = Some(false);
        }
        // Deliberately NOT closing on `r.backdrop_response.clicked()`: the
        // backdrop covers the whole content rect, so a click anywhere on the
        // parent (e.g. the Delete button) must be a no-op, not a dismiss.
        if let Some(commit) = done {
            trace(if commit { "modal OK" } else { "modal Cancel" });
            let d = self.edit.take().unwrap();
            if commit {
                let mut s = self.shared.lock().unwrap();
                let i = s.selected;
                s.projects[i].name = d.name;
                if let Some(v) = d.budget.trim().parse::<f64>().ok() {
                    s.projects[i].budget = v;
                }
                s.dirty = true;
            }
            self.shared.lock().unwrap().modal_open = false;
            // Focus returns to the parent automatically: the modal was never
            // an OS window, so the root window never lost OS focus.
        }
    }

    /// Close veto. `close_requested()` is set by winit's CloseRequested (red
    /// button, Cmd+W, Quit); `CancelClose` un-sets it.
    fn close_veto(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        trace("root close_requested");
        let dirty = self.shared.lock().unwrap().dirty;
        if !dirty || self.quit_ok {
            self.save_state();
            return;
        }
        if let Some(st) = &mut self.selftest {
            // Scripted: veto the first request, allow the second.
            if st.veto_next() {
                println!("SELFTEST close_requested -> CancelClose (veto)");
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else {
                self.quit_ok = true;
                self.save_state();
            }
            return;
        }
        let res = rfd::MessageDialog::new()
            .set_parent(frame)
            .set_title("Save changes?")
            .set_description("The project list has unsaved changes.")
            .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                "Save".into(),
                "Discard".into(),
                "Cancel".into(),
            ))
            .show();
        match res {
            rfd::MessageDialogResult::Custom(ref s) if s == "Cancel" => {
                trace("close vetoed (Cancel)");
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            rfd::MessageDialogResult::Cancel => {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            _ => {
                trace("close allowed (Save/Discard)");
                self.quit_ok = true;
                self.save_state();
            }
        }
    }

    /// Both children are declared here, every root frame. Deferred (not
    /// immediate) viewports: they render on their own schedule even when the
    /// root's `ui` is not running.
    fn children(&mut self, ctx: &egui::Context) {
        let (insp, prefs, modal_open) = {
            let s = self.shared.lock().unwrap();
            (s.inspector_open, s.prefs_open, s.modal_open)
        };
        if insp {
            let shared = self.shared.clone();
            let mut b = egui::ViewportBuilder::default()
                .with_title("Inspector")
                .with_inner_size(self.insp_size)
                .with_resizable(true);
            if let Some(p) = self.insp_pos {
                b = b.with_position(p);
            }
            ctx.show_viewport_deferred(inspector_id(), b, move |ui, _class| {
                inspector_ui(ui, &shared, modal_open);
            });
        }
        if prefs {
            let shared = self.shared.clone();
            let mut b = egui::ViewportBuilder::default()
                .with_title("Preferences")
                .with_inner_size(self.prefs_size)
                .with_resizable(true);
            if let Some(p) = self.prefs_pos {
                b = b.with_position(p);
            }
            ctx.show_viewport_deferred(prefs_id(), b, move |ui, _class| {
                prefs_ui(ui, &shared, modal_open);
            });
        }
    }
}

/// Shared child-window chrome: geometry recording, Esc/Cmd+W self-close,
/// close-request handling, and the hand-rolled "parent is modal" block.
fn child_chrome(
    ui: &mut egui::Ui,
    shared: &SharedState,
    modal_open: bool,
    which: bool, // true = inspector
) -> bool {
    let ctx = ui.ctx().clone();
    if let Some(g) = Geom::read(&ctx) {
        let mut s = shared.lock().unwrap();
        if which {
            s.geom.inspector = Some(g);
        } else {
            s.geom.prefs = Some(g);
        }
    }
    let close = ctx.input(|i| i.viewport().close_requested())
        || ctx.input(|i| i.key_pressed(egui::Key::Escape))
        || ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::W,
            ))
        });
    if close {
        trace(if which { "inspector closed" } else { "preferences closed" });
        let mut s = shared.lock().unwrap();
        if which {
            s.inspector_open = false;
        } else {
            s.prefs_open = false;
        }
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
    // egui's modal layer is per-viewport (Memory::focus is a
    // ViewportIdMap<Focus>), so a Modal in the root does NOT block this
    // window. Blocking it is ours to do:
    if modal_open {
        ui.disable();
    }
    !close
}

fn inspector_ui(ui: &mut egui::Ui, shared: &SharedState, modal_open: bool) {
    let ctx = ui.ctx().clone();
    if !child_chrome(ui, shared, modal_open, true) {
        return;
    }
    let mut s = shared.lock().unwrap();
    s.inspector_dark = Some(ctx.theme() == egui::Theme::Dark);
    s.inspector_ppp = ctx.input(|i| i.viewport().native_pixels_per_point);
    let flash = s.pong_until.is_some_and(|t| Instant::now() < t);
    if flash {
        ctx.request_repaint();
    }
    let style = ui.style().clone();
    let mut fr = egui::Frame::central_panel(&style);
    if flash {
        fr = fr.fill(egui::Color32::from_rgb(0xff, 0xd5, 0x4f));
    }
    egui::CentralPanel::default().frame(fr).show(ui, |ui| {
        let sel = s.selected;
        s.inspector_seen_name = Some(s.projects[sel].name.clone());
        ui.heading("Inspector");
        ui.label(format!("project {} of {}", sel + 1, s.projects.len()));
        ui.separator();
        let mut changed = false;
        let p = &mut s.projects[sel];
        ui.label("Name");
        changed |= ui.text_edit_singleline(&mut p.name).changed();
        ui.label("Owner");
        changed |= ui.text_edit_singleline(&mut p.owner).changed();
        ui.label("Budget");
        changed |= ui
            .add(egui::DragValue::new(&mut p.budget).speed(50.0).max_decimals(2))
            .changed();
        changed |= ui.checkbox(&mut p.closed, "Closed").changed();
        if changed {
            s.dirty = true;
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        ui.separator();
        if ui.button("Ping >>").clicked() {
            trace("inspector Ping");
            s.pings += 1;
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        ui.small(format!(
            "scale {:.2}",
            ctx.input(|i| i.viewport().native_pixels_per_point).unwrap_or(1.0)
        ));
    });
}

fn prefs_ui(ui: &mut egui::Ui, shared: &SharedState, modal_open: bool) {
    let ctx = ui.ctx().clone();
    if !child_chrome(ui, shared, modal_open, false) {
        return;
    }
    let mut s = shared.lock().unwrap();
    s.prefs_ran = true;
    egui::CentralPanel::default().show(ui, |ui| {
        ui.heading("Preferences");
        if ui.checkbox(&mut s.compact, "Compact rows").changed() {
            trace("prefs Compact rows toggled");
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        ui.separator();
        ui.label("Theme");
        let mut t = s.theme;
        ui.radio_value(&mut t, 0, "Light");
        ui.radio_value(&mut t, 1, "Dark");
        ui.radio_value(&mut t, 2, "System");
        if t != s.theme {
            s.theme = t;
            // The root pass owns `ctx.set_theme`; wake it.
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

impl eframe::App for WindowsApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        if let Some(g) = Geom::read(&ctx) {
            self.shared.lock().unwrap().geom.main = Some(g);
        }
        if self.edit.is_none() {
            self.shortcuts(&ctx);
        }

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui, frame));
        egui::Panel::bottom("status").show(ui, |ui| {
            let s = self.shared.lock().unwrap();
            let name = s.projects[s.selected].name.clone();
            ui.horizontal(|ui| {
                ui.label(format!("selected: {name} · pings: {}", s.pings));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.small(format!(
                        "{}scale {:.2}",
                        if s.dirty { "• unsaved  " } else { "" },
                        ctx.pixels_per_point()
                    ));
                });
            });
        });
        egui::CentralPanel::default().show(ui, |ui| self.list(ui));

        self.modal(&ctx);
        self.close_veto(&ctx, frame);
        self.children(&ctx);
        selftest::drive(&ctx, self);
    }

    fn on_exit(&mut self) {
        self.save_state();
    }
}
