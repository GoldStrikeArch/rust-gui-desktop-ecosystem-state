// Windows (Tauri) — SPEC-9: multi-window, modality, shared state, close veto.
//
// Boundary decision (see FRICTION.md): there is exactly ONE copy of the model
// and it lives in Rust (`Mutex<Model>`). No window owns state. Every window is
// a dumb renderer that (a) pulls a snapshot with `get_model` on load and
// (b) subscribes to the `model` event, which Rust broadcasts to *all* windows
// after every mutation. That is what makes main <-> inspector bidirectional
// with no copy: both edit through commands, both re-render from the broadcast.
//
// WINDOWS_SELFTEST=1 drives the whole thing from a Rust thread and prints
// `SELFTEST DONE pass=N fail=M`, then exits 0.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, Theme, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

/// Set once the close veto has been answered "Save"/"Discard" so the second
/// CloseRequested (or the exit path) does not re-ask.
static QUITTING: AtomicBool = AtomicBool::new(false);

// ------------------------------------------------------------------- model

#[derive(Clone, Serialize)]
struct Project {
    id: u32,
    name: String,
    owner: String,
    budget: f64,
    status: String,
}

struct Model {
    projects: Vec<Project>,
    selected: u32,
    pings: u32,
    dirty: bool,
    compact: bool,
    theme: String, // "light" | "dark" | "system"
    modal: bool,
}

struct AppState(Mutex<Model>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snap {
    projects: Vec<Project>,
    selected: u32,
    selected_name: String,
    pings: u32,
    dirty: bool,
    compact: bool,
    theme: String,
    modal: bool,
    inspector_open: bool,
    selftest: bool,
}

fn seed() -> Vec<Project> {
    [
        ("Aurora", "kim", 42_000.0, "Open"),
        ("Basalt", "lee", 12_500.0, "Open"),
        ("Cinder", "ravi", 78_250.0, "Closed"),
        ("Dune", "sam", 5_000.0, "Open"),
        ("Ember", "tess", 31_750.0, "Closed"),
        ("Fjord", "uli", 96_400.0, "Open"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (n, o, b, s))| Project {
        id: i as u32 + 1,
        name: n.to_string(),
        owner: o.to_string(),
        budget: *b,
        status: s.to_string(),
    })
    .collect()
}

fn snap(app: &AppHandle, m: &Model) -> Snap {
    let selected_name = m
        .projects
        .iter()
        .find(|p| p.id == m.selected)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "—".into());
    Snap {
        projects: m.projects.clone(),
        selected: m.selected,
        selected_name,
        pings: m.pings,
        dirty: m.dirty,
        compact: m.compact,
        theme: m.theme.clone(),
        modal: m.modal,
        inspector_open: app.get_webview_window("inspector").is_some(),
        selftest: std::env::var("WINDOWS_SELFTEST").is_ok(),
    }
}

/// One source of truth -> every window. This single line is the whole
/// "shared state across windows" story in Tauri.
fn broadcast(app: &AppHandle) {
    let s = {
        let st = app.state::<AppState>();
        let m = st.0.lock().unwrap();
        snap(app, &m)
    };
    let _ = app.emit("model", s);
}

// ------------------------------------------------------------ window logic

/// SPEC-9 §6: a child window should "open offset from" its parent.
fn offset_from_main(app: &AppHandle, w: &WebviewWindow, dx: i32, dy: i32) {
    if let Some(main) = app.get_webview_window("main") {
        if let Ok(p) = main.outer_position() {
            let _ = w.set_position(PhysicalPosition::new(p.x + dx, p.y + dy));
        }
    }
}

/// True when window-state has never seen this label (fresh profile), so the
/// plugin's on_window_ready restore did not place the window and we may.
fn has_saved_state(app: &AppHandle, label: &str) -> bool {
    let Ok(dir) = app.path().app_config_dir() else {
        return false;
    };
    let path = dir.join(app.filename());
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| v.get(label).is_some())
        .unwrap_or(false)
}

fn open_inspector(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("inspector") {
        // Singleton: focus the existing window, never build a second one.
        let _ = w.show();
        let _ = w.set_focus();
        println!("[win] inspector already open -> focused");
        return;
    }
    let main = app.get_webview_window("main").expect("main window");
    let built = WebviewWindowBuilder::new(app, "inspector", WebviewUrl::App("inspector.html".into()))
        .title("Inspector")
        .inner_size(360.0, 300.0)
        // macOS: tao maps this to [parent addChildWindow:ordered:Above] — a
        // real child window (stays above the parent, hides/minimises with it).
        .parent(&main)
        .expect("parent handle")
        .build();
    if let Ok(w) = &built {
        if !has_saved_state(app, "inspector") {
            offset_from_main(app, w, 740, 0);
        }
    }
    println!("[win] inspector opened ok={}", built.is_ok());
    broadcast(app);
}

fn open_prefs(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("prefs") {
        let _ = w.set_focus();
        println!("[win] prefs already open -> focused");
        return;
    }
    let built = WebviewWindowBuilder::new(app, "prefs", WebviewUrl::App("prefs.html".into()))
        .title("Preferences")
        .inner_size(320.0, 200.0)
        .resizable(false)
        .build();
    if let Ok(w) = &built {
        if !has_saved_state(app, "prefs") {
            offset_from_main(app, w, 60, 520);
        }
    }
    println!("[win] prefs opened ok={}", built.is_ok());
}

/// The "modal" edit dialog. Tauri has no `.modal()` builder flag, but
/// `set_enabled(false)` on the parent IS a real OS-level block on all three
/// desktop platforms (macOS: an invisible alpha-0.5 NSWindow attached with
/// `beginSheet:`; Windows: `EnableWindow`; Linux: `gtk_widget_set_sensitive`).
/// Two traps that cost most of the time on this spec (see FRICTION.md):
///  1. the macOS disabling sheet is the size of the PARENT and is ordered above
///     it, so it also covers a dialog placed over the parent — the dialog
///     becomes unclickable. `always_on_top` lifts the dialog back above it.
///  2. `always_on_top` and `parent` are mutually exclusive on macOS:
///     `addChildWindow:` resets the child to the parent's window level, so a
///     parented dialog sinks under the sheet again. The Inspector keeps
///     `parent` (SPEC-9 §6); the modal trades it for reachability.
/// The JS veil is the belt-and-braces fallback and the visible cue on the
/// platforms where disabling does not dim.
fn open_edit(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("edit") {
        let _ = w.set_focus();
        return;
    }
    let main = app.get_webview_window("main").expect("main window");
    let built = WebviewWindowBuilder::new(app, "edit", WebviewUrl::App("edit.html".into()))
        .title("Edit project")
        .inner_size(360.0, 200.0)
        .resizable(false)
        .minimizable(false)
        // NOT `.parent(&main)`: `addChildWindow:` forces the child back to the
        // parent's window level, which puts it BEHIND the disabling sheet that
        // `set_enabled(false)` attaches — the dialog becomes unclickable. On
        // macOS you get a real child window OR an OS-blocked parent, not both.
        .always_on_top(true)
        .build();
    if let (Ok(w), Ok(p), Ok(sz)) = (&built, main.outer_position(), main.inner_size()) {
        let _ = w.set_position(PhysicalPosition::new(
            p.x + (sz.width as i32 - 360) / 2,
            p.y + 120,
        ));
    }
    let _ = main.set_enabled(false);
    {
        let st = app.state::<AppState>();
        st.0.lock().unwrap().modal = true;
    }
    broadcast(app);
    println!("[win] modal edit opened ok={}", built.is_ok());
}

fn end_modal(app: &AppHandle) {
    {
        let st = app.state::<AppState>();
        let mut m = st.0.lock().unwrap();
        if !m.modal {
            return;
        }
        m.modal = false;
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_enabled(true);
        let _ = main.set_focus(); // focus returns to the parent
    }
    broadcast(app);
    println!("[win] modal closed -> main re-enabled and focused");
}

fn apply_theme(app: &AppHandle, theme: &str) {
    let t = match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None, // follow the system
    };
    // Native window chrome per window; the CSS side rides the broadcast.
    for (_, w) in app.webview_windows() {
        let _ = w.set_theme(t);
    }
}

// ---------------------------------------------------------------- commands

#[tauri::command]
fn get_model(app: AppHandle, state: State<'_, AppState>) -> Snap {
    snap(&app, &state.0.lock().unwrap())
}

#[tauri::command]
fn select(app: AppHandle, id: u32) {
    app.state::<AppState>().0.lock().unwrap().selected = id;
    broadcast(&app);
}

/// The single write path used by BOTH windows (main's modal and the
/// inspector's live inputs). Any write also raises the unsaved-changes flag.
#[tauri::command]
fn set_field(app: AppHandle, id: u32, field: String, value: String) {
    {
        let st = app.state::<AppState>();
        let mut m = st.0.lock().unwrap();
        if let Some(p) = m.projects.iter_mut().find(|p| p.id == id) {
            match field.as_str() {
                "name" => p.name = value,
                "owner" => p.owner = value,
                "budget" => p.budget = value.replace(',', "").trim().parse().unwrap_or(p.budget),
                "status" => p.status = value,
                _ => return,
            }
            m.dirty = true;
        }
    }
    broadcast(&app);
}

#[tauri::command]
fn ping(app: AppHandle) -> u32 {
    let n = {
        let st = app.state::<AppState>();
        let mut m = st.0.lock().unwrap();
        m.pings += 1;
        m.pings
    };
    broadcast(&app);
    println!("[win] ping -> pings={n}");
    n
}

#[tauri::command]
fn pong(app: AppHandle) -> bool {
    let ok = app.emit_to("inspector", "flash", 300u32).is_ok()
        && app.get_webview_window("inspector").is_some();
    println!("[win] pong emitted to inspector ok={ok}");
    ok
}

#[tauri::command]
fn toggle_inspector(app: AppHandle) {
    if let Some(w) = app.get_webview_window("inspector") {
        let _ = w.close();
    } else {
        open_inspector(&app);
    }
}

#[tauri::command]
fn open_window(app: AppHandle, which: String) {
    match which.as_str() {
        "inspector" => open_inspector(&app),
        "prefs" => open_prefs(&app),
        "edit" => open_edit(&app),
        _ => {}
    }
}

#[tauri::command]
fn close_edit(app: AppHandle, commit: bool, name: String, budget: String) {
    if commit {
        let id = app.state::<AppState>().0.lock().unwrap().selected;
        set_field(app.clone(), id, "name".into(), name);
        set_field(app.clone(), id, "budget".into(), budget);
    }
    if let Some(w) = app.get_webview_window("edit") {
        let _ = w.destroy();
    }
    end_modal(&app);
}

/// Native confirm: rfd NSAlert. With `.parent()` it is a real window-modal
/// SHEET on macOS (rfd -> beginSheetModalForWindow_completionHandler).
#[tauri::command]
fn delete_selected(app: AppHandle) {
    let (id, name) = {
        let st = app.state::<AppState>();
        let m = st.0.lock().unwrap();
        match m.projects.iter().find(|p| p.id == m.selected) {
            Some(p) => (p.id, p.name.clone()),
            None => return,
        }
    };
    let main = app.get_webview_window("main").expect("main window");
    let app2 = app.clone();
    app.dialog()
        .message(format!("Delete \"{name}\"?"))
        .title("Delete project")
        .buttons(MessageDialogButtons::YesNo)
        .parent(&main)
        .show(move |yes| {
            println!("[win] delete confirm answered yes={yes}");
            if yes {
                {
                    let st = app2.state::<AppState>();
                    let mut m = st.0.lock().unwrap();
                    m.projects.retain(|p| p.id != id);
                    m.dirty = true;
                    m.selected = m.projects.first().map(|p| p.id).unwrap_or(0);
                }
                broadcast(&app2);
            }
        });
}

#[tauri::command]
fn set_prefs(app: AppHandle, compact: bool, theme: String) {
    {
        let st = app.state::<AppState>();
        let mut m = st.0.lock().unwrap();
        m.compact = compact;
        m.theme = theme.clone();
    }
    apply_theme(&app, &theme);
    broadcast(&app);
}

#[tauri::command]
fn close_self(app: AppHandle, label: String) {
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.close();
    }
}

#[tauri::command]
fn report(line: String) {
    println!("{line}");
}

/// Window census straight from the framework (labels + physical bounds), used
/// alongside scripts/window-count.swift so the two can be cross-checked.
#[tauri::command]
fn census(app: AppHandle) -> Vec<String> {
    let mut v: Vec<String> = app
        .webview_windows()
        .iter()
        .map(|(l, w)| {
            let p = w.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
            let s = w.inner_size().map(|s| (s.width, s.height)).unwrap_or((0, 0));
            format!(
                "{l}@{},{} {}x{} scale={:.1} visible={}",
                p.x,
                p.y,
                s.0,
                s.1,
                w.scale_factor().unwrap_or(1.0),
                w.is_visible().unwrap_or(false)
            )
        })
        .collect();
    v.sort();
    v
}

// ------------------------------------------------------------------- menus

fn build_menubar(app: &tauri::App) -> tauri::Result<()> {
    let prefs = MenuItem::with_id(app, "prefs", "Preferences…", true, Some("CmdOrCtrl+,"))?;
    let app_menu = Submenu::with_items(
        app,
        "Windows",
        true,
        &[
            &prefs,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let inspector = MenuItem::with_id(
        app,
        "inspector",
        "Toggle Inspector",
        true,
        Some("CmdOrCtrl+Shift+I"),
    )?;
    // close_window is the native "Close Window" role: it targets the FOCUSED
    // window and carries ⌘W itself.
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::close_window(app, None)?,
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &inspector,
        ],
    )?;
    app.set_menu(Menu::with_items(app, &[&app_menu, &edit_menu, &window_menu])?)?;
    Ok(())
}

// ---------------------------------------------------------------- selftest

fn selftest(app: AppHandle) {
    let mut pass = 0u32;
    let mut fail = 0u32;
    let mut check = |cond: bool, what: &str| {
        if cond {
            pass += 1
        } else {
            fail += 1
        }
        println!("SELFTEST {} {what}", if cond { "PASS" } else { "FAIL" });
    };
    let nap = |ms| std::thread::sleep(Duration::from_millis(ms));
    let count = |a: &AppHandle| a.webview_windows().len();
    let model = |a: &AppHandle| {
        let st = a.state::<AppState>();
        let m = st.0.lock().unwrap();
        (
            m.projects.len(),
            m.pings,
            m.projects.iter().find(|p| p.id == m.selected).map(|p| p.name.clone()),
            m.dirty,
        )
    };
    nap(2500);

    println!("SELFTEST CENSUS {:?}", census(app.clone()));
    check(count(&app) == 1, "window count == 1 at launch");

    // 2 windows, then 3; re-opening must NOT make a 4th (singleton).
    let a = app.clone();
    let _ = app.run_on_main_thread(move || open_inspector(&a));
    nap(1200);
    check(count(&app) == 2, "window count == 2 after Inspector");
    let a = app.clone();
    let _ = app.run_on_main_thread(move || open_prefs(&a));
    nap(1200);
    check(count(&app) == 3, "window count == 3 after Preferences");
    println!("SELFTEST CENSUS {:?}", census(app.clone()));
    let a = app.clone();
    let _ = app.run_on_main_thread(move || open_inspector(&a));
    nap(800);
    check(count(&app) == 3, "Inspector is a singleton (still 3 windows)");

    // Shared state: type into the inspector's real input, read the model.
    if let Some(w) = app.get_webview_window("inspector") {
        let _ = w.eval(
            "(()=>{const i=document.getElementById('f-name');i.value='Aurora-X';\
             i.dispatchEvent(new Event('input',{bubbles:true}));})()",
        );
    }
    nap(900);
    check(
        model(&app).2.as_deref() == Some("Aurora-X"),
        "inspector edit reached the shared model",
    );
    // ...and the main window rendered it (read the DOM back through eval).
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.eval(
            "window.__TAURI__.core.invoke('report',{line:'SELFTEST MAINROW '+\
             document.querySelector('#rows .row.sel .c-name').textContent})",
        );
    }
    nap(700);

    // Cross-window messages.
    let before = model(&app).1;
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        ping(a);
    });
    nap(500);
    check(model(&app).1 == before + 1, "Ping incremented main's counter");
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        pong(a);
    });
    nap(500);

    // Theme + compact reach every window at once.
    let a = app.clone();
    let _ = app.run_on_main_thread(move || set_prefs(a, true, "dark".into()));
    nap(900);
    let themed = app
        .webview_windows()
        .values()
        .all(|w| w.theme().map(|t| t == Theme::Dark).unwrap_or(false));
    check(themed, "Dark theme applied to every open window");

    // Modality: open the modal, then click main's real Delete button from JS.
    let rows_before = model(&app).0;
    let a = app.clone();
    let _ = app.run_on_main_thread(move || open_edit(&a));
    nap(1200);
    check(count(&app) == 4, "modal edit window opened (4 windows)");
    // Hit-test, not a programmatic .click(): ask the main window what element
    // actually sits at the Delete button's centre, then click THAT. A
    // programmatic click on the button would bypass hit-testing entirely and
    // prove nothing. (The CGEvent version of this check is in evidence/log.txt.)
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.eval(
            "(()=>{const b=document.getElementById('btn-delete');\
             const r=b.getBoundingClientRect();\
             const el=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);\
             window.__TAURI__.core.invoke('report',{line:'SELFTEST HITTEST topmost=#'+\
             (el.id||el.tagName)});el.click();})()",
        );
    }
    nap(1500);
    check(
        model(&app).0 == rows_before,
        "hit-tested click at main's Delete while modal is open changed nothing",
    );
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        close_edit(a, true, "Aurora-Z".into(), "44000".into());
    });
    nap(1200);
    check(count(&app) == 3, "modal closed (back to 3 windows)");
    check(model(&app).2.as_deref() == Some("Aurora-Z"), "modal OK committed");
    let mut focused = false;
    for _ in 0..8 {
        focused = app
            .get_webview_window("main")
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false);
        if focused {
            break;
        }
        nap(300);
    }
    println!(
        "SELFTEST FOCUS {:?}",
        app.webview_windows()
            .iter()
            .map(|(l, w)| format!("{l}={}", w.is_focused().unwrap_or(false)))
            .collect::<Vec<_>>()
    );
    check(focused, "focus returned to the main window");

    // Child window closes -> app lives.
    if let Some(w) = app.get_webview_window("inspector") {
        let _ = w.close();
    }
    nap(900);
    check(
        count(&app) == 2 && app.get_webview_window("main").is_some(),
        "closing the Inspector did not quit the app",
    );

    // Close veto: dirty + close request on main must be intercepted.
    check(model(&app).3, "model is dirty after the edits");
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.close();
    }
    nap(1500);
    check(
        app.get_webview_window("main").is_some(),
        "dirty close was vetoed (main window still alive)",
    );

    println!("SELFTEST DONE pass={pass} fail={fail}");
    QUITTING.store(true, Ordering::SeqCst);
    let a = app.clone();
    let _ = app.run_on_main_thread(move || a.exit(0));
}

// -------------------------------------------------------------------- main

/// Sibling-of-the-binary JSON that remembers whether the inspector was open
/// (window-state handles geometry; it does not remember which windows existed).
fn session_path() -> std::path::PathBuf {
    std::env::current_exe()
        .map(|p| p.with_file_name("windows-session.json"))
        .unwrap_or_else(|_| "windows-session.json".into())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::POSITION | StateFlags::SIZE)
                .build(),
        )
        .manage(AppState(Mutex::new(Model {
            projects: seed(),
            selected: 1,
            pings: 0,
            dirty: false,
            compact: false,
            theme: "system".into(),
            modal: false,
        })))
        .invoke_handler(tauri::generate_handler![
            get_model,
            select,
            set_field,
            ping,
            pong,
            toggle_inspector,
            open_window,
            close_edit,
            delete_selected,
            set_prefs,
            close_self,
            report,
            census
        ])
        .setup(|app| {
            build_menubar(app)?;
            app.on_menu_event(|app, e| match e.id().as_ref() {
                "prefs" => open_prefs(app),
                "inspector" => toggle_inspector(app.clone()),
                _ => {}
            });
            // Persistence part 2: re-open the inspector if it was open last run.
            let reopen = std::fs::read_to_string(session_path())
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| v.get("inspectorOpen").and_then(|b| b.as_bool()))
                .unwrap_or(false);
            println!("[win] session: inspectorOpen={reopen}");
            if reopen {
                open_inspector(app.handle());
            }
            if std::env::var("WINDOWS_SELFTEST").is_ok() {
                let h = app.handle().clone();
                std::thread::spawn(move || selftest(h));
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.app_handle().clone();
            match event {
                WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                    if QUITTING.load(Ordering::SeqCst) {
                        return;
                    }
                    // Main always vetoes first: either to ask, or to guarantee
                    // "main closes => the whole app exits" even with children up.
                    api.prevent_close();
                    let dirty = app.state::<AppState>().0.lock().unwrap().dirty;
                    if !dirty {
                        QUITTING.store(true, Ordering::SeqCst);
                        println!("[win] main closed clean -> exiting");
                        app.exit(0);
                        return;
                    }
                    println!("[win] CloseRequested on dirty main -> asking");
                    let w = window.clone();
                    let app2 = app.clone();
                    app.dialog()
                        .message("You have unsaved changes.")
                        .title("Save changes?")
                        .buttons(MessageDialogButtons::YesNoCancelCustom(
                            "Save".into(),
                            "Discard".into(),
                            "Cancel".into(),
                        ))
                        .parent(&w)
                        .show_with_result(move |res| {
                            let answer = match res {
                                MessageDialogResult::Custom(s) => s,
                                other => format!("{other:?}"),
                            };
                            println!("[win] close veto answer: {answer}");
                            if answer == "Save" || answer == "Discard" {
                                QUITTING.store(true, Ordering::SeqCst);
                                let a = app2.clone();
                                let _ = app2.run_on_main_thread(move || a.exit(0));
                            } else {
                                println!("[win] close CANCELLED — window survives");
                            }
                        });
                }
                WindowEvent::CloseRequested { .. } if window.label() == "edit" => {
                    end_modal(&app);
                }
                WindowEvent::Destroyed if window.label() == "inspector" => {
                    println!("[win] inspector destroyed; app alive");
                    broadcast(&app);
                }
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    // SPEC-9 §10: fires when a window is dragged onto a display
                    // with a different backing scale; wry re-rasterises the
                    // webview at the new scale with no app-side work.
                    println!(
                        "[win] ScaleFactorChanged {} -> {scale_factor}",
                        window.label()
                    );
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let open = app.get_webview_window("inspector").is_some();
                let _ = std::fs::write(
                    session_path(),
                    format!("{{\"inspectorOpen\":{open}}}\n"),
                );
                let _ = app.save_window_state(StateFlags::POSITION | StateFlags::SIZE);
                println!("[win] exit: saved window state (inspectorOpen={open})");
            }
        });
}
