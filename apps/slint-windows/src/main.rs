// Windows (slint) — SPEC-9: multi-window, modality, shared state.
//
// Window model: every top-level window is an independently instantiated
// exported component. Nothing is shared between two instances — not even the
// `Palette` global — so the single source of truth is a pair of Rust-owned
// models handed to all windows as the same `ModelRc`:
//   projects : Rc<VecModel<Project>>   (row data, edited from 2 windows)
//   sh       : Rc<VecModel<Shared>>    (exactly one row: the app's scalars)
// Reads are reactive everywhere (ModelNotify wakes every window on the shared
// event loop); writes go through Rust callbacks.
//
// Env hooks: WINDOWS_SELFTEST=1 scripted checks, WINDOWS_NO_OVERLAY=1 disables
// the click-swallowing overlay (so a CGEvent test measures only the AppKit
// sheet), WINDOWS_NO_CONFIRM=1 skips the rfd Delete confirmation,
// WINDOWS_ORIGIN=x,y pins the main window (parallel test runs).

mod mac;
mod selftest;

use slint::{CloseRequestResponse, ComponentHandle, LogicalPosition, LogicalSize, Model, ModelRc,
            SharedString, Timer, TimerMode, VecModel};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

pub struct App {
    pub projects: Rc<VecModel<Project>>,
    pub sh: Rc<VecModel<Shared>>,
    pub main: MainWindow,
    pub insp: InspectorWindow,
    pub prefs: PrefsWindow,
    pub dialog: EditDialog,
    pub prompt: SavePrompt,
    pub sheet_ok: Cell<bool>,
    pub may_close: Cell<bool>,
    flash_timer: Timer,
}

impl App {
    pub fn shared(&self) -> Shared {
        self.sh.row_data(0).unwrap()
    }
    pub fn set_shared(&self, f: impl FnOnce(&mut Shared)) {
        let mut s = self.shared();
        f(&mut s);
        self.sh.set_row_data(0, s); // one notify -> every window repaints
    }
}

fn money(v: f32) -> SharedString {
    let cents = (v * 100.0).round() as i64;
    format!("{}.{:02}", cents / 100, (cents % 100).abs()).into()
}

fn seed() -> Vec<Project> {
    [
        ("Apollo", "rita", 128_000.0, "Open"),
        ("Borealis", "sam", 42_500.0, "Open"),
        ("Cinder", "amir", 9_900.0, "Closed"),
        ("Delta Nine", "jo", 250_000.0, "Open"),
        ("Everest", "kim", 76_250.5, "Closed"),
        ("Fathom", "lee", 18_000.0, "Open"),
    ]
    .into_iter()
    .map(|(n, o, b, s)| Project {
        name: n.into(),
        owner: o.into(),
        budget: b,
        budget_s: money(b),
        status: s.into(),
    })
    .collect()
}

// ---------------------------------------------------------------------------
// Position/size persistence: a 4-line text file next to the binary. (No serde:
// the whole state is 13 numbers, and the corpus measures dependency counts.)
// ---------------------------------------------------------------------------
pub fn state_path() -> std::path::PathBuf {
    std::env::current_exe()
        .map(|p| p.with_file_name("windows-state.txt"))
        .unwrap_or_else(|_| "windows-state.txt".into())
}

// Geometry is stored in LOGICAL units on purpose: `Window::position()/size()`
// are physical, but the scale factor is only correct once the window has been
// mapped — right after `show()` it still reads 1 on a 2x display, so a
// physical round-trip restores a window at twice its size.
fn save_state(app: &App) {
    let g = |w: &slint::Window| {
        let f = w.scale_factor();
        let (p, s) = (w.position().to_logical(f), w.size().to_logical(f));
        format!("{} {} {} {}", p.x as i32, p.y as i32, s.width as i32, s.height as i32)
    };
    let text = format!(
        "main {}\ninsp {}\nprefs {}\ninsp_open {}\n",
        g(app.main.window()),
        g(app.insp.window()),
        g(app.prefs.window()),
        if app.shared().insp_open { 1 } else { 0 }
    );
    let _ = std::fs::write(state_path(), text);
    eprintln!("[windows] state saved to {}", state_path().display());
}

fn load_state() -> std::collections::HashMap<String, Vec<i32>> {
    std::fs::read_to_string(state_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let k = it.next()?.to_string();
            Some((k, it.filter_map(|n| n.parse().ok()).collect()))
        })
        .collect()
}

fn restore(w: &slint::Window, v: Option<&Vec<i32>>) {
    if let Some(v) = v
        && v.len() == 4
    {
        w.set_size(LogicalSize::new(v[2].max(120) as f32, v[3].max(80) as f32));
        w.set_position(LogicalPosition::new(v[0] as f32, v[1] as f32));
    }
}

// ---------------------------------------------------------------------------

pub fn open_inspector(app: &Rc<App>) {
    if app.shared().insp_open {
        // Singleton: focus the existing window instead of opening a second one.
        let _ = app.insp.show();
        app.insp.window().with_winit_focus();
        return;
    }
    let _ = app.insp.show();
    app.set_shared(|s| s.insp_open = true);
    // Offset from the parent + real macOS parenting.
    let f = app.main.window().scale_factor();
    let p = app.main.window().position().to_logical(f);
    app.insp
        .window()
        .set_position(LogicalPosition::new(p.x + 370.0, p.y + 20.0));
    let parented = mac::add_child(app.main.window(), app.insp.window());
    eprintln!("[windows] inspector opened (addChildWindow: {parented})");
}

fn close_inspector(app: &Rc<App>) {
    mac::remove_child(app.main.window(), app.insp.window());
    let _ = app.insp.hide();
    app.set_shared(|s| s.insp_open = false);
}

pub fn open_dialog(app: &Rc<App>) {
    let s = app.shared();
    if s.selected < 0 || s.blocked {
        return;
    }
    let p = app.projects.row_data(s.selected as usize).unwrap();
    app.dialog.set_name_text(p.name.clone());
    app.dialog.set_budget_text(p.budget_s.clone());
    let _ = app.dialog.show();
    let ok = mac::begin_sheet(app.main.window(), app.dialog.window());
    app.sheet_ok.set(ok);
    app.set_shared(|s| s.blocked = true);
    eprintln!("[windows] modal open (beginSheet: {ok})");
}

pub fn close_dialog(app: &Rc<App>) {
    if app.sheet_ok.get() {
        mac::end_sheet(app.main.window(), app.dialog.window());
    }
    let _ = app.dialog.hide();
    app.set_shared(|s| s.blocked = false);
    // Focus must return to the parent.
    app.main.window().with_winit_focus();
}

/// `winit::Window::focus_window()` through the public snapshot-free path we
/// have: Slint re-shows and raises on `show()`.
trait Raise {
    fn with_winit_focus(&self);
}
impl Raise for slint::Window {
    fn with_winit_focus(&self) {
        let _ = self.show();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let projects = Rc::new(VecModel::from(seed()));
    let sh = Rc::new(VecModel::from(vec![Shared {
        selected: 0,
        pings: 0,
        theme: 2,
        compact: false,
        dirty: std::env::var("WINDOWS_DIRTY").is_ok(), // test hook: start dirty
        flash: false,
        blocked: false,
        insp_open: false,
        prefs_open: false,
    }]));

    let app = Rc::new(App {
        main: MainWindow::new()?,
        insp: InspectorWindow::new()?,
        prefs: PrefsWindow::new()?,
        dialog: EditDialog::new()?,
        prompt: SavePrompt::new()?,
        projects: projects.clone(),
        sh: sh.clone(),
        sheet_ok: Cell::new(false),
        may_close: Cell::new(false),
        flash_timer: Timer::default(),
    });

    // The SAME ModelRc goes into every window: this is the shared state.
    let pm = ModelRc::from(projects.clone());
    let sm = ModelRc::from(sh.clone());
    app.main.set_projects(pm.clone());
    app.main.set_sh(sm.clone());
    app.insp.set_projects(pm);
    app.insp.set_sh(sm.clone());
    app.prefs.set_sh(sm.clone());
    app.dialog.set_sh(sm);
    let selftest = std::env::var("WINDOWS_SELFTEST").as_deref() == Ok("1");
    app.main
        .set_overlay_enabled(std::env::var("WINDOWS_NO_OVERLAY").is_err());

    // ---- main window callbacks -------------------------------------------
    {
        let a = app.clone();
        app.main.on_select(move |i| a.set_shared(|s| s.selected = i));
    }
    {
        let a = app.clone();
        app.main.on_edit(move || open_dialog(&a));
    }
    {
        let a = app.clone();
        app.main.on_toggle_inspector(move || {
            if a.shared().insp_open {
                close_inspector(&a)
            } else {
                open_inspector(&a)
            }
        });
    }
    {
        let a = app.clone();
        app.main.on_show_inspector(move || open_inspector(&a));
    }
    {
        let a = app.clone();
        app.main.on_open_prefs(move || {
            let _ = a.prefs.show();
            if !a.shared().prefs_open {
                let f = a.main.window().scale_factor();
                let p = a.main.window().position().to_logical(f);
                a.prefs
                    .window()
                    .set_position(LogicalPosition::new(p.x + 30.0, p.y + 260.0));
                mac::add_child(a.main.window(), a.prefs.window());
                a.set_shared(|s| s.prefs_open = true);
            }
        });
    }
    {
        let a = app.clone();
        let quiet = selftest || std::env::var("WINDOWS_NO_CONFIRM").is_ok();
        app.main.on_del(move || {
            let s = a.shared();
            if s.selected < 0 {
                return;
            }
            let idx = s.selected as usize;
            let name = a.projects.row_data(idx).map(|p| p.name).unwrap_or_default();
            if quiet {
                delete_row(&a, idx);
                return;
            }
            let a2 = a.clone();
            let _ = slint::spawn_local(async move {
                let answer = rfd::AsyncMessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title("Delete project")
                    .set_description(format!("Delete \"{name}\"?"))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    .await;
                if answer == rfd::MessageDialogResult::Yes {
                    delete_row(&a2, idx);
                }
            });
        });
    }
    {
        let a = app.clone();
        app.main.on_pong(move || {
            a.set_shared(|s| s.flash = true);
            let a2 = a.clone();
            a.flash_timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(300),
                move || a2.set_shared(|s| s.flash = false),
            );
        });
    }

    // ---- inspector callbacks ---------------------------------------------
    {
        let a = app.clone();
        app.insp.on_set_field(move |field, value| {
            let s = a.shared();
            if s.selected < 0 {
                return;
            }
            let i = s.selected as usize;
            let Some(mut p) = a.projects.row_data(i) else { return };
            match field {
                0 => p.name = value,
                1 => p.owner = value,
                _ => {
                    if let Ok(v) = value.parse::<f32>() {
                        p.budget = v;
                        p.budget_s = value;
                    } else {
                        return;
                    }
                }
            }
            a.projects.set_row_data(i, p); // updates the main list live
            a.set_shared(|s| s.dirty = true);
        });
    }
    {
        let a = app.clone();
        app.insp.on_ping(move || a.set_shared(|s| s.pings += 1));
    }
    {
        // The same two shortcut handlers, replicated per window.
        let a = app.clone();
        app.insp.on_open_prefs(move || a.main.invoke_open_prefs());
    }
    {
        let a = app.clone();
        app.insp.on_toggle_inspector(move || a.main.invoke_toggle_inspector());
    }
    {
        let a = app.clone();
        app.prefs.on_toggle_inspector(move || a.main.invoke_toggle_inspector());
    }

    // ---- prefs callbacks --------------------------------------------------
    {
        let a = app.clone();
        app.prefs.on_set_compact(move |v| a.set_shared(|s| s.compact = v));
    }
    {
        let a = app.clone();
        app.prefs.on_set_theme(move |v| a.set_shared(|s| s.theme = v));
    }

    // ---- edit dialog ------------------------------------------------------
    {
        let a = app.clone();
        let commit = move || {
            let s = a.shared();
            if s.selected >= 0
                && let Some(mut p) = a.projects.row_data(s.selected as usize)
            {
                p.name = a.dialog.get_name_text();
                let b = a.dialog.get_budget_text();
                if let Ok(v) = b.parse::<f32>() {
                    p.budget = v;
                    p.budget_s = money(v);
                }
                a.projects.set_row_data(s.selected as usize, p);
                a.set_shared(|s| s.dirty = true);
            }
            close_dialog(&a);
        };
        app.dialog.on_commit(commit.clone());
        app.dialog.on_ok_clicked(commit); // auto-generated by the Dialog element
    }
    {
        let a = app.clone();
        let cancel = move || close_dialog(&a);
        app.dialog.on_dismiss(cancel.clone());
        app.dialog.on_cancel_clicked(cancel);
    }

    // ---- close semantics --------------------------------------------------
    {
        let a = app.clone();
        app.main.window().on_close_requested(move || {
            if a.may_close.get() || !a.shared().dirty {
                save_state(&a);
                let _ = slint::quit_event_loop();
                return CloseRequestResponse::HideWindow;
            }
            let _ = a.prompt.show();
            let f = a.main.window().scale_factor();
            let p = a.main.window().position().to_logical(f);
            a.prompt
                .window()
                .set_position(LogicalPosition::new(p.x + 90.0, p.y + 80.0));
            eprintln!("[windows] close vetoed: unsaved changes");
            CloseRequestResponse::KeepWindowShown
        });
    }
    {
        let a = app.clone();
        let finish = move |save: bool| {
            let _ = a.prompt.hide();
            if save {
                eprintln!("[windows] saved (pretend)");
            }
            a.may_close.set(true);
            save_state(&a);
            let _ = slint::quit_event_loop();
        };
        let f1 = finish.clone();
        app.prompt.on_save(move || f1(true));
        app.prompt.on_discard(move || finish(false));
    }
    {
        let a = app.clone();
        app.prompt.on_cancel(move || {
            let _ = a.prompt.hide();
            eprintln!("[windows] close cancelled");
        });
    }
    {
        // Child windows: closing them must not quit the app.
        let a = app.clone();
        app.insp.window().on_close_requested(move || {
            close_inspector(&a);
            CloseRequestResponse::HideWindow
        });
    }
    {
        let a = app.clone();
        app.prefs.window().on_close_requested(move || {
            a.set_shared(|s| s.prefs_open = false);
            CloseRequestResponse::HideWindow
        });
    }

    // ---- show + restore geometry ------------------------------------------
    app.main.show()?;
    let restore_timer = Timer::default();
    {
        let a = app.clone();
        restore_timer.start(TimerMode::SingleShot, Duration::from_millis(120), move || {
            let st = load_state();
            restore(a.main.window(), st.get("main"));
            if let Ok(o) = std::env::var("WINDOWS_ORIGIN") {
                let n: Vec<f32> = o.split(',').filter_map(|v| v.parse().ok()).collect();
                if n.len() == 2 {
                    a.main.window().set_position(LogicalPosition::new(n[0], n[1]));
                }
            }
            if st.get("insp_open").map(|v| v.first() == Some(&1)) == Some(true) {
                open_inspector(&a);
                restore(a.insp.window(), st.get("insp"));
            }
            eprintln!(
                "[windows] scale_factor main={} (1.0 before the window is mapped)",
                a.main.window().scale_factor()
            );
        });
    }

    if selftest {
        selftest::run(app.clone());
    }
    slint::run_event_loop_until_quit()?;
    Ok(())
}

pub fn delete_row(app: &Rc<App>, idx: usize) {
    if idx < app.projects.row_count() {
        app.projects.remove(idx);
        let n = app.projects.row_count() as i32;
        eprintln!("[windows] DELETED row {idx}; rows now {n}");
        app.set_shared(|s| {
            s.selected = (s.selected).min(n - 1);
            s.dirty = true;
        });
    }
}
