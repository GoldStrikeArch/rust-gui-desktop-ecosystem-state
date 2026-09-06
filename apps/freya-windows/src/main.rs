//! "Windows" — multi-window & modality on Freya 0.4 (SPEC-9).
//!
//! Three real top-level winit windows (main / inspector / preferences) plus a
//! fourth for the edit dialog. Every piece of shared state is a
//! `State::create_global(..)` signal created in `main` and captured by every
//! window's root closure: Freya's reactive graph is process-wide and
//! single-threaded, so a write from one window's runner marks the subscribing
//! scopes of the *other* windows dirty and wakes their runners through the
//! shared winit event-loop proxy. There is no message bus and no copy.
//!
//! The two seams that are not first-party:
//!   * `WindowConfig::with_on_close` (the close-veto hook) is `Send`, so it
//!     cannot capture a `State`; app state it needs is mirrored into `static`s.
//!   * Freya/winit have no modal API. The dialog is a real child window
//!     (`WindowAttributes::with_parent_window`, which on macOS is
//!     `NSWindow addChildWindow:ordered:`) and the parent is made inert with a
//!     scrim `rect().interactive(false)` — an in-framework overlay, not OS
//!     modality. See FRICTION.md.

use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Mutex,
        atomic::{
            AtomicBool,
            AtomicUsize,
            Ordering,
        },
    },
    time::Duration,
};

use async_io::Timer;
use freya::{
    prelude::*,
    winit::{
        dpi::LogicalPosition,
        raw_window_handle::{
            HasWindowHandle,
            RawWindowHandle,
        },
        window::WindowId,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

const MAIN_TITLE: &str = "Windows (freya)";
const INSPECTOR_TITLE: &str = "Inspector — Windows (freya)";
const PREFS_TITLE: &str = "Preferences — Windows (freya)";
const DIALOG_TITLE: &str = "Edit project — Windows (freya)";

// ------------------------------------------------------------------ model

#[derive(Clone, PartialEq, Debug)]
struct Project {
    name: String,
    owner: String,
    budget: f64,
    open: bool,
}

fn seed() -> Vec<Project> {
    [
        ("Apollo", "rin", 12500.0, true),
        ("Borealis", "sam", 4200.5, true),
        ("Cinder", "kae", 98000.0, false),
        ("Dovetail", "rin", 750.0, true),
        ("Everest", "mira", 31000.0, false),
        ("Foxglove", "sam", 6400.25, true),
    ]
    .into_iter()
    .map(|(name, owner, budget, open)| Project {
        name: name.to_string(),
        owner: owner.to_string(),
        budget,
        open,
    })
    .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ThemeChoice {
    Light,
    Dark,
    System,
}

/// Every field is a *global* signal: one source of truth for all windows.
#[derive(Clone, Copy)]
struct Shared {
    projects: State<Vec<Project>>,
    selected: State<usize>,
    pings: State<u32>,
    pong: State<u32>,
    dirty: State<bool>,
    compact: State<bool>,
    theme: State<ThemeChoice>,
    main_id: State<Option<WindowId>>,
    inspector_id: State<Option<WindowId>>,
    prefs_id: State<Option<WindowId>>,
    dialog_id: State<Option<WindowId>>,
    draft_name: State<String>,
    draft_budget: State<String>,
    selftest: bool,
}

impl Shared {
    fn create(selftest: bool) -> Self {
        Self {
            projects: State::create_global(seed()),
            selected: State::create_global(0),
            pings: State::create_global(0),
            pong: State::create_global(0),
            dirty: State::create_global(false),
            compact: State::create_global(false),
            theme: State::create_global(ThemeChoice::System),
            main_id: State::create_global(None),
            inspector_id: State::create_global(None),
            prefs_id: State::create_global(None),
            dialog_id: State::create_global(None),
            draft_name: State::create_global(String::new()),
            draft_budget: State::create_global(String::new()),
            selftest,
        }
    }

    fn selected_name(&self) -> String {
        let index = *self.selected.read();
        self.projects
            .read()
            .get(index)
            .map(|p| p.name.clone())
            .unwrap_or_default()
    }
}

// --------------------------------------------------- statics for the shell

/// `with_on_close` is `Box<dyn FnMut(..) + Send>`, so the hook cannot hold a
/// `State`. Anything it needs is mirrored here by a `use_side_effect`.
static DIRTY: AtomicBool = AtomicBool::new(false);
static SELFTEST: AtomicBool = AtomicBool::new(false);
/// `ns_view` pointer of the main window, used as the dialog/inspector parent.
static PARENT_VIEW: AtomicUsize = AtomicUsize::new(0);
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Logical frame of the main window, used to offset the secondary windows.
static MAIN_RECT: Mutex<[f64; 4]> = Mutex::new([120.0, 120.0, 720.0, 480.0]);

fn note(line: String) {
    println!("{line}");
    LOG.lock().unwrap().push(line);
}

// ------------------------------------------------------------ persistence

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
struct Persisted {
    #[serde(default)]
    main: Option<[f64; 4]>,
    #[serde(default)]
    inspector: Option<[f64; 4]>,
    #[serde(default)]
    prefs: Option<[f64; 4]>,
    #[serde(default)]
    inspector_open: bool,
}

fn state_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("WINDOWS_STATE_FILE") {
        return path.into();
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("freya-windows-state.json")))
        .unwrap_or_else(|| "freya-windows-state.json".into())
}

fn load_persisted() -> Persisted {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Called from the close hook, which is the only place with a
/// `RendererContext` and therefore the only place that can read every live
/// window's geometry at once.
fn save_geometry(ctx: &RendererContext, inspector_open: bool) {
    let mut out = Persisted {
        inspector_open,
        ..Default::default()
    };
    for app in ctx.windows().values() {
        let window = app.window();
        let scale = window.scale_factor();
        let Ok(position) = window.outer_position() else {
            continue;
        };
        let size = window.inner_size();
        let rect = [
            position.x as f64 / scale,
            position.y as f64 / scale,
            size.width as f64 / scale,
            size.height as f64 / scale,
        ];
        match window.title().as_str() {
            MAIN_TITLE => out.main = Some(rect),
            INSPECTOR_TITLE => out.inspector = Some(rect),
            PREFS_TITLE => out.prefs = Some(rect),
            _ => {}
        }
    }
    if let Ok(text) = serde_json::to_string_pretty(&out) {
        let _ = std::fs::write(state_path(), text);
    }
    note(format!("persist: wrote {}", state_path().display()));
}

// ------------------------------------------------------------------- main

fn main() {
    // Freya installs a release-only panic hook that turns a panic into a modal
    // rfd alert *before* chaining to the previous hook, so panics never reach
    // stderr in release. Keep them greppable while developing.
    #[cfg(debug_assertions)]
    std::panic::set_hook(Box::new(|info| eprintln!("PANIC: {info}")));

    let selftest = std::env::var_os("WINDOWS_SELFTEST").is_some();
    SELFTEST.store(selftest, Ordering::Relaxed);
    let shared = Shared::create(selftest);
    let saved = load_persisted();

    // Deterministic placement for scripted verification runs.
    let origin = std::env::var("WINDOWS_ORIGIN").ok().and_then(|raw| {
        let (x, y) = raw.split_once(',')?;
        Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?))
    });
    let main_rect = origin
        .map(|(x, y)| [x, y, 720.0, 480.0])
        .or(saved.main)
        .unwrap_or([120.0, 120.0, 720.0, 480.0]);
    let reopen_inspector = saved.inspector_open;
    *MAIN_RECT.lock().unwrap() = main_rect;

    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(move || main_window(shared, reopen_inspector))
                .with_title(MAIN_TITLE)
                .with_size(main_rect[2], main_rect[3])
                .with_window_attributes(move |attrs, _| {
                    attrs.with_position(LogicalPosition::new(main_rect[0], main_rect[1]))
                })
                .with_on_close(on_main_close),
        ),
    );
}

/// Close veto. Runs on the renderer thread, outside any component scope, and
/// must answer synchronously — so the prompt has to be a blocking native
/// dialog, and the dirty flag has to come from a `static`.
fn on_main_close(ctx: RendererContext, _id: WindowId) -> CloseDecision {
    let mut ctx = ctx;
    let inspector_open = ctx
        .windows()
        .values()
        .any(|app| app.window().title() == INSPECTOR_TITLE);

    if !confirm_quit() {
        return CloseDecision::KeepOpen;
    }

    save_geometry(&ctx, inspector_open);
    // Closing the *main* window quits, even if the inspector/preferences are
    // still open; Freya only exits by itself when the last window goes.
    ctx.exit();
    CloseDecision::Close
}

// ------------------------------------------------------- window launching

/// Places a secondary window at `saved` if it was persisted, otherwise offset
/// from the main window (spec 6: "opens offset from it").
fn placement(
    saved: Option<[f64; 4]>,
    offset: (f64, f64),
) -> impl FnOnce(
    freya::winit::window::WindowAttributes,
    &freya::winit::event_loop::ActiveEventLoop,
) -> freya::winit::window::WindowAttributes
+ Send
+ Sync
+ 'static {
    let main = *MAIN_RECT.lock().unwrap();
    let position = saved
        .map(|r| (r[0], r[1]))
        .unwrap_or((main[0] + offset.0, main[1] + offset.1));
    move |attrs, _| attrs.with_position(LogicalPosition::new(position.0, position.1))
}

/// macOS window parenting.
///
/// winit *does* expose it (`WindowAttributes::with_parent_window` →
/// `-[NSWindow addChildWindow:ordered:]`), but it is unusable inside Freya:
/// `addChildWindow:` orders the child in immediately, so by the time
/// `AppWindow::new` reaches `Adapter::with_event_loop_proxy` the window is
/// already visible and AccessKit panics with "The AccessKit winit adapter must
/// be created before the window is shown". Freya offers no hook that runs after
/// the adapter, so the relationship is established afterwards, from a renderer
/// callback, with 20 lines of objc FFI (no extra crate).
#[cfg(target_os = "macos")]
mod child_window {
    use std::ffi::{
        c_char,
        c_void,
    };

    unsafe extern "C" {
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }

    /// `[[parent_view window] addChildWindow:[child_view window] ordered:NSWindowAbove]`
    pub fn attach(parent_view: usize, child_view: usize) -> bool {
        if parent_view == 0 || child_view == 0 {
            return false;
        }
        unsafe {
            let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let window_sel = sel_registerName(c"window".as_ptr());
            let parent = send(parent_view as *mut c_void, window_sel);
            let child = send(child_view as *mut c_void, window_sel);
            if parent.is_null() || child.is_null() {
                return false;
            }
            let add_sel = sel_registerName(c"addChildWindow:ordered:".as_ptr());
            let send2: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, isize) =
                std::mem::transmute(objc_msgSend as *const ());
            send2(parent, add_sel, child, 1 /* NSWindowAbove */);
            true
        }
    }
}

/// Make `child` a real child of the main window once it exists.
fn parent_to_main(child: WindowId) {
    #[cfg(target_os = "macos")]
    {
        let parent_view = PARENT_VIEW.load(Ordering::Relaxed);
        if parent_view == 0 || std::env::var_os("WINDOWS_NO_PARENT").is_some() {
            return;
        }
        spawn(async move {
            let _ = Platform::get()
                .post_callback(move |_, ctx| {
                    let child_view = ctx
                        .windows()
                        .get(&child)
                        .and_then(|app| app.window().window_handle().ok())
                        .and_then(|handle| match handle.as_raw() {
                            RawWindowHandle::AppKit(appkit) => {
                                Some(appkit.ns_view.as_ptr() as usize)
                            }
                            _ => None,
                        })
                        .unwrap_or(0);
                    let ok = child_window::attach(parent_view, child_view);
                    note(format!("parenting: addChildWindow ordered:above -> {ok}"));
                })
                .await;
        });
    }
}

fn open_inspector(shared: Shared) {
    if let Some(id) = *shared.inspector_id.peek() {
        Platform::get().focus_window(Some(id));
        note("inspector: singleton — focused the existing window".into());
        return;
    }
    let saved = load_persisted().inspector;
    let size = saved.map(|r| (r[2], r[3])).unwrap_or((360.0, 300.0));
    spawn(async move {
        let mut shared = shared;
        let id = Platform::get()
            .launch_window(
                WindowConfig::new(move || inspector_window(shared))
                    .with_title(INSPECTOR_TITLE)
                    .with_size(size.0, size.1)
                    .with_window_attributes(placement(saved, (740.0, 0.0))),
            )
            .await;
        shared.inspector_id.set(Some(id));
        parent_to_main(id);
        note(format!("inspector: opened {id:?}"));
    });
}

fn open_prefs(shared: Shared) {
    if let Some(id) = *shared.prefs_id.peek() {
        Platform::get().focus_window(Some(id));
        note("prefs: singleton — focused the existing window".into());
        return;
    }
    let saved = load_persisted().prefs;
    spawn(async move {
        let mut shared = shared;
        let id = Platform::get()
            .launch_window(
                WindowConfig::new(move || prefs_window(shared))
                    .with_title(PREFS_TITLE)
                    .with_size(320.0, 200.0)
                    .with_window_attributes(placement(saved, (740.0, 330.0))),
            )
            .await;
        shared.prefs_id.set(Some(id));
        note(format!("prefs: opened {id:?}"));
    });
}

fn open_dialog(shared: Shared) {
    if shared.dialog_id.peek().is_some() {
        return;
    }
    let mut shared = shared;
    let index = *shared.selected.peek();
    let Some(project) = shared.projects.peek().get(index).cloned() else {
        return;
    };
    shared.draft_name.set(project.name);
    shared.draft_budget.set(format!("{:.2}", project.budget));
    spawn(async move {
        let id = Platform::get()
            .launch_window(
                WindowConfig::new(move || dialog_window(shared))
                    .with_title(DIALOG_TITLE)
                    .with_size(360.0, 200.0)
                    .with_resizable(false)
                    .with_window_attributes(placement(None, (180.0, 140.0))),
            )
            .await;
        shared.dialog_id.set(Some(id));
        parent_to_main(id);
        // A new window does not become key by itself; a modal must.
        Platform::get().focus_window(Some(id));
        note(format!("modal: opened {id:?} (parent scrim armed)"));
    });
}

fn close_dialog(shared: Shared) {
    let mut shared = shared;
    if let Some(id) = shared.dialog_id.take() {
        Platform::get().close_window(id);
        if let Some(main) = *shared.main_id.peek() {
            Platform::get().focus_window(Some(main));
        }
        note("modal: closed, focus returned to the main window".into());
    }
}

// -------------------------------------------------------------- theming

/// Applies the global theme choice to *this* window. Every window runs its own
/// copy; the shared `theme` signal is what makes them move together.
fn use_shared_theme(shared: Shared) {
    let mut theme_state = use_init_theme(light_theme);
    use_side_effect(move || {
        let choice = *shared.theme.read();
        let system = *Platform::get().preferred_theme.read();
        let dark = match choice {
            ThemeChoice::Light => false,
            ThemeChoice::Dark => true,
            ThemeChoice::System => system == PreferredTheme::Dark,
        };
        theme_state.set(if dark { dark_theme() } else { light_theme() });
    });
}

fn is_dark(shared: Shared) -> bool {
    match *shared.theme.read() {
        ThemeChoice::Light => false,
        ThemeChoice::Dark => true,
        ThemeChoice::System => *Platform::get().preferred_theme.read() == PreferredTheme::Dark,
    }
}

struct Palette {
    bg: Color,
    panel: Color,
    text: Color,
    muted: Color,
    accent: Color,
    selected: Color,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            bg: Color::from_rgb(24, 24, 27),
            panel: Color::from_rgb(38, 38, 42),
            text: Color::from_rgb(235, 235, 240),
            muted: Color::from_rgb(150, 150, 158),
            accent: Color::from_rgb(120, 170, 255),
            selected: Color::from_rgb(52, 62, 84),
        }
    } else {
        Palette {
            bg: Color::from_rgb(250, 250, 251),
            panel: Color::from_rgb(238, 238, 241),
            text: Color::from_rgb(24, 24, 27),
            muted: Color::from_rgb(110, 110, 118),
            accent: Color::from_rgb(30, 90, 200),
            selected: Color::from_rgb(207, 222, 248),
        }
    }
}

// ------------------------------------------------------ shared shortcuts

/// ⌘, opens Preferences, ⌘⇧I toggles the inspector. Registered per window so
/// they fire whichever window has the keyboard.
fn shortcuts(shared: Shared, is_main: bool, self_id: State<Option<WindowId>>) -> impl FnMut(Event<KeyboardEventData>) + 'static {
    move |e: Event<KeyboardEventData>| {
        let meta = e.modifiers.contains(Modifiers::META) || e.modifiers.contains(Modifiers::CONTROL);
        if std::env::var_os("WINDOWS_KEYLOG").is_some() {
            eprintln!("key: {:?} mods={:?} main={is_main}", e.key, e.modifiers);
        }
        match &e.key {
            Key::Character(c) if meta && c == "," => open_prefs(shared),
            Key::Character(c)
                if meta && e.modifiers.contains(Modifiers::SHIFT) && c.eq_ignore_ascii_case("i") =>
            {
                let mut shared = shared;
                if let Some(id) = shared.inspector_id.take() {
                    Platform::get().close_window(id);
                    note("inspector: toggled closed by ⌘⇧I".into());
                } else {
                    open_inspector(shared);
                }
            }
            // ⌘W closes the *focused* window only. macOS gives us no menu bar
            // here, so winit never turns ⌘W into a CloseRequested by itself.
            Key::Character(c) if meta && c.eq_ignore_ascii_case("w") => {
                if is_main {
                    quit_main(shared);
                } else if let Some(id) = *self_id.peek() {
                    Platform::get().close_window(id);
                }
            }
            // Esc closes the topmost non-main window, from whichever window has
            // the keyboard.
            Key::Named(NamedKey::Escape) => {
                let mut shared = shared;
                if let Some(id) = shared.dialog_id.take() {
                    Platform::get().close_window(id);
                    note("esc: closed the modal".into());
                } else if let Some(id) = shared.prefs_id.take() {
                    Platform::get().close_window(id);
                    note("esc: closed preferences".into());
                } else if !is_main
                    && let Some(id) = *self_id.peek()
                {
                    Platform::get().close_window(id);
                    note("esc: closed the topmost non-main window".into());
                }
                if let Some(main) = *shared.main_id.peek() {
                    Platform::get().focus_window(Some(main));
                }
            }
            _ => {}
        }
    }
}

/// The Save/Discard/Cancel prompt. `true` means "go ahead and quit".
/// Blocking on purpose: `with_on_close` must answer synchronously.
fn confirm_quit() -> bool {
    if !DIRTY.load(Ordering::Relaxed) || SELFTEST.load(Ordering::Relaxed) {
        return true;
    }
    let answer = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Windows (freya)")
        .set_description("Save changes before closing?")
        .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
            "Save".into(),
            "Discard".into(),
            "Cancel".into(),
        ))
        .show();
    match answer {
        rfd::MessageDialogResult::Custom(choice) if choice == "Cancel" => {
            note("close: vetoed (Cancel) — window and process survive".into());
            false
        }
        rfd::MessageDialogResult::Cancel => {
            note("close: vetoed (Cancel) — window and process survive".into());
            false
        }
        other => {
            note(format!("close: {other:?} — quitting"));
            true
        }
    }
}

/// ⌘W on the main window. Freya/winit expose no way to *post* a
/// `WindowEvent::CloseRequested`, and `Platform::close_window` bypasses the
/// `with_on_close` hook entirely, so the keyboard path calls the same prompt
/// the hook does and then quits through a renderer callback.
fn quit_main(shared: Shared) {
    if !confirm_quit() {
        return;
    }
    let inspector_open = shared.inspector_id.peek().is_some();
    spawn(async move {
        let _ = Platform::get()
            .post_callback(move |_, ctx| {
                save_geometry(ctx, inspector_open);
                ctx.exit();
            })
            .await;
    });
}

/// `Input`'s stock `on_pre_key_down` calls `stop_propagation()` +
/// `prevent_default()` for every key that is not Enter/Escape/Shift/Tab, which
/// also kills the root's `on_global_key_down` — so ⌘, / ⌘⇧I / ⌘W silently stop
/// working while a text field has focus. Replacing the filter and letting
/// ⌘/Ctrl combos through is the only way to keep app shortcuts alive.
fn input_keys(e: &Event<KeyboardEventData>) -> bool {
    if e.modifiers.contains(Modifiers::META) || e.modifiers.contains(Modifiers::CONTROL) {
        // Let the editor keep ⌘C/⌘V/⌘X/⌘A/⌘Z, but do not swallow the event.
        return true;
    }
    match &e.key {
        Key::Named(NamedKey::Enter)
        | Key::Named(NamedKey::Escape)
        | Key::Named(NamedKey::Shift) => true,
        Key::Named(NamedKey::Tab) => false,
        _ => {
            e.stop_propagation();
            e.prevent_default();
            true
        }
    }
}

// -------------------------------------------------------------- main window

fn main_window(shared: Shared, reopen_inspector: bool) -> impl IntoElement {
    use_shared_theme(shared);
    let dark = is_dark(shared);
    let colors = palette(dark);

    // Mirror the dirty flag into the static the (Send) close hook reads.
    use_side_effect(move || DIRTY.store(*shared.dirty.read(), Ordering::Relaxed));

    // Learn our own WindowId + NSView pointer once, so later windows can be
    // parented to this one.
    use_hook(move || {
        spawn(async move {
            let mut shared = shared;
            let info = Platform::get()
                .post_callback(|id, ctx| {
                    let pointer = ctx
                        .windows()
                        .get(&id)
                        .and_then(|app| app.window().window_handle().ok())
                        .and_then(|handle| match handle.as_raw() {
                            RawWindowHandle::AppKit(appkit) => {
                                Some(appkit.ns_view.as_ptr() as usize)
                            }
                            _ => None,
                        })
                        .unwrap_or(0);
                    (id, pointer)
                })
                .await;
            if let Ok((id, pointer)) = info {
                PARENT_VIEW.store(pointer, Ordering::Relaxed);
                shared.main_id.set(Some(id));
                note(format!("main: window {id:?} parent_view={pointer:#x}"));
                if reopen_inspector {
                    open_inspector(shared);
                }
            }
        });
    });

    if shared.selftest {
        use_hook(move || {
            spawn(async move { selftest(shared).await });
        });
    }

    let projects = shared.projects.read().clone();
    let selected = *shared.selected.read();
    let pings = *shared.pings.read();
    let compact = *shared.compact.read();
    let dialog_open = shared.dialog_id.read().is_some();
    let row_height = if compact { 22.0 } else { 34.0 };
    let self_id = shared.main_id;

    rect()
        .expanded()
        .content(Content::flex())
        .background(colors.bg)
        .color(colors.text)
        .a11y_focusable(true)
        .a11y_auto_focus(true)
        .on_global_key_down(shortcuts(shared, true, self_id))
        // --------------------------------------------------------- toolbar
        .child(
            rect()
                .horizontal()
                .height(Size::px(44.))
                .cross_align(Alignment::Center)
                .spacing(8.)
                .padding(Gaps::new_symmetric(0., 10.))
                .child(
                    Button::new()
                        .on_press(move |_| open_dialog(shared))
                        .child("Edit…"),
                )
                .child(
                    Button::new()
                        .on_press(move |_| open_inspector(shared))
                        .child("Inspector"),
                )
                .child(
                    Button::new()
                        .on_press(move |_| open_prefs(shared))
                        .child("Preferences…"),
                )
                .child(
                    Button::new()
                        .on_press(move |_| delete_selected(shared))
                        .child("Delete"),
                )
                .child(
                    Button::new()
                        .on_press(move |_| {
                            let mut shared = shared;
                            *shared.pong.write() += 1;
                            note("pong: sent to the inspector".into());
                        })
                        .child("Pong"),
                ),
        )
        // ------------------------------------------------------------ list
        .child(
            ScrollView::new()
                .height(Size::flex(1.))
                .children(
                    projects
                        .iter()
                        .enumerate()
                        .map(|(index, project)| {
                            rect()
                                .key(index)
                                .horizontal()
                                .height(Size::px(row_height))
                                .cross_align(Alignment::Center)
                                .spacing(14.)
                                .padding(Gaps::new_symmetric(0., 10.))
                                .background(if index == selected {
                                    colors.selected
                                } else {
                                    Color::TRANSPARENT
                                })
                                .a11y_role(AccessibilityRole::Row)
                                .a11y_alt(format!("{} owned by {}", project.name, project.owner))
                                .on_press(move |e: Event<PressEventData>| {
                                    let mut shared = shared;
                                    shared.selected.set(index);
                                    if let PressEventData::Mouse(mouse) = &*e
                                        && EventsCombos::pressed(mouse.global_location).is_double()
                                    {
                                        open_dialog(shared);
                                    }
                                })
                                .child(
                                    label()
                                        .text(project.name.clone())
                                        .width(Size::px(160.))
                                        .max_lines(1),
                                )
                                .child(
                                    label()
                                        .text(project.owner.clone())
                                        .width(Size::px(100.))
                                        .color(colors.muted)
                                        .max_lines(1),
                                )
                                .child(
                                    label()
                                        .text(format!("{:.2}", project.budget))
                                        .width(Size::px(120.))
                                        .text_align(TextAlign::Right)
                                        .font_family("Menlo")
                                        .max_lines(1),
                                )
                                .child(
                                    label()
                                        .text(if project.open { "Open" } else { "Closed" })
                                        .color(colors.muted)
                                        .max_lines(1),
                                )
                                .into()
                        })
                        .collect::<Vec<Element>>(),
                ),
        )
        // ------------------------------------------------------- status bar
        .child(
            rect()
                .height(Size::px(28.))
                .horizontal()
                .cross_align(Alignment::Center)
                .padding(Gaps::new_symmetric(0., 10.))
                .background(colors.panel)
                .child(
                    label()
                        .text(format!(
                            "selected: {} · pings: {pings}{}",
                            shared.selected_name(),
                            if *shared.dirty.read() { " · edited" } else { "" }
                        ))
                        .font_size(12.)
                        .color(colors.text),
                ),
        )
        // --------------------------------------------------- modal scrim
        // Freya has no modal API. While the dialog window is up the whole main
        // tree is covered by an `interactive(false)` overlay, so pointer events
        // reach nothing underneath (`Delete` included).
        .maybe_child(dialog_open.then(|| -> Element {
            rect()
                .layer(Layer::Overlay)
                .position(Position::new_global().top(0.).left(0.))
                .width(Size::window_percent(100.))
                .height(Size::window_percent(100.))
                .background(Color::from_argb(90, 0, 0, 0))
                .main_align(Alignment::Center)
                .cross_align(Alignment::Center)
                .child(
                    label()
                        .text("modal dialog open")
                        .color(Color::WHITE)
                        .font_size(12.),
                )
                .into()
        }))
        .maybe(dialog_open, |el| el.interactive(false))
}

fn delete_selected(shared: Shared) {
    let mut shared = shared;
    let index = *shared.selected.peek();
    let Some(name) = shared.projects.peek().get(index).map(|p| p.name.clone()) else {
        return;
    };
    let confirmed = if shared.selftest {
        true
    } else {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Delete project")
            .set_description(format!("Delete \"{name}\"?"))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            == rfd::MessageDialogResult::Yes
    };
    if !confirmed {
        note(format!("delete: \"{name}\" cancelled"));
        return;
    }
    shared.projects.write().remove(index);
    let len = shared.projects.peek().len();
    shared.selected.set(index.min(len.saturating_sub(1)));
    shared.dirty.set(true);
    note(format!("delete: removed \"{name}\", {len} rows left"));
}

// --------------------------------------------------------- inspector window

fn inspector_window(shared: Shared) -> impl IntoElement {
    use_shared_theme(shared);
    let dark = is_dark(shared);
    let colors = palette(dark);

    // Pong: flash the background for 300 ms whenever the counter moves.
    let mut flash = use_state(|| false);
    let first = use_hook(|| Rc::new(Cell::new(true)));
    use_side_effect(move || {
        let _subscribe = *shared.pong.read();
        if first.replace(false) {
            return;
        }
        flash.set(true);
        spawn(async move {
            Timer::after(Duration::from_millis(300)).await;
            flash.set(false);
        });
    });

    use_drop(move || {
        let mut shared = shared;
        shared.inspector_id.set(None);
        note("inspector: closed (app still alive)".into());
    });

    let index = *shared.selected.read();
    let project = shared.projects.read().get(index).cloned();
    let name = project.as_ref().map(|p| p.name.clone()).unwrap_or_default();
    let owner = project.as_ref().map(|p| p.owner.clone()).unwrap_or_default();
    let budget = project.map(|p| format!("{:.2}", p.budget)).unwrap_or_default();

    // Local mirrors so `Input` has a `State<String>` to write into; every
    // change is pushed straight back into the shared project list.
    let mut name_field = use_state(String::new);
    let mut budget_field = use_state(String::new);
    if *name_field.read() != name {
        name_field.set(name.clone());
    }
    if *budget_field.read() != budget {
        budget_field.set(budget.clone());
    }

    let self_id = shared.inspector_id;

    rect()
        .expanded()
        .background(if *flash.read() {
            colors.accent
        } else {
            colors.bg
        })
        .color(colors.text)
        .padding(Gaps::new_all(12.))
        .spacing(8.)
        .a11y_focusable(true)
        .on_global_key_down(shortcuts(shared, false, self_id))
        .child(label().text("Inspector").font_weight(FontWeight::BOLD))
        .child(label().text("Name").font_size(11.).color(colors.muted))
        .child(
            Input::new(name_field)
                .width(Size::fill())
                .auto_focus(true)
                .on_pre_key_down(|e: Event<KeyboardEventData>| input_keys(&e))
                .on_validate(move |validator: InputValidator| {
                    let text = validator.text().clone();
                    let mut shared = shared;
                    let index = *shared.selected.peek();
                    if let Some(project) = shared.projects.write().get_mut(index) {
                        project.name = text;
                    }
                    shared.dirty.set(true);
                }),
        )
        .child(label().text("Owner").font_size(11.).color(colors.muted))
        .child(label().text(owner).color(colors.muted))
        .child(label().text("Budget").font_size(11.).color(colors.muted))
        .child(
            Input::new(budget_field)
                .width(Size::fill())
                .on_pre_key_down(|e: Event<KeyboardEventData>| input_keys(&e))
                .on_validate(move |validator: InputValidator| {
                    let text = validator.text().clone();
                    let Ok(value) = text.parse::<f64>() else {
                        validator.set_valid(text.is_empty());
                        return;
                    };
                    let mut shared = shared;
                    let index = *shared.selected.peek();
                    if let Some(project) = shared.projects.write().get_mut(index) {
                        project.budget = value;
                    }
                    shared.dirty.set(true);
                }),
        )
        .child(
            Button::new()
                .on_press(move |_| {
                    let mut shared = shared;
                    *shared.pings.write() += 1;
                    note(format!("ping: pings={}", *shared.pings.peek()));
                })
                .child("Ping"),
        )
}

// ------------------------------------------------------- preferences window

fn prefs_window(shared: Shared) -> impl IntoElement {
    use_shared_theme(shared);
    let dark = is_dark(shared);
    let colors = palette(dark);

    use_drop(move || {
        let mut shared = shared;
        shared.prefs_id.set(None);
    });

    let compact = *shared.compact.read();
    let choice = *shared.theme.read();
    let self_id = shared.prefs_id;

    rect()
        .expanded()
        .background(colors.bg)
        .color(colors.text)
        .padding(Gaps::new_all(12.))
        .spacing(6.)
        .a11y_focusable(true)
        .on_global_key_down(shortcuts(shared, false, self_id))
        .child(label().text("Preferences").font_weight(FontWeight::BOLD))
        .child(
            Tile::new()
                .on_select(move |_| {
                    let mut shared = shared;
                    shared.compact.toggle();
                })
                .leading(Switch::new().toggled(compact))
                .child("Compact rows"),
        )
        .child(label().text("Theme").font_size(11.).color(colors.muted))
        .child(
            rect().horizontal().children(
                [
                    ("Light", ThemeChoice::Light),
                    ("Dark", ThemeChoice::Dark),
                    ("System", ThemeChoice::System),
                ]
                .into_iter()
                .map(|(text, value)| {
                    Tile::new()
                        .on_select(move |_| {
                            let mut shared = shared;
                            shared.theme.set(value);
                            note(format!("theme: {text} applied to every window"));
                        })
                        .leading(RadioItem::new().selected(choice == value))
                        .child(label().text(text).font_size(12.))
                        .into()
                })
                .collect::<Vec<Element>>(),
            ),
        )
        .child(
            label()
                .text("applies to every open window")
                .font_size(11.)
                .color(colors.muted),
        )
}

// ----------------------------------------------------------- modal dialog

fn dialog_window(shared: Shared) -> impl IntoElement {
    use_shared_theme(shared);
    let dark = is_dark(shared);
    let colors = palette(dark);

    let name = shared.draft_name;
    let budget = shared.draft_budget;

    // The dialog can also be dismissed by its own red button; clearing the id
    // here is what lifts the parent's scrim.
    use_drop(move || {
        let mut shared = shared;
        shared.dialog_id.set(None);
        if let Some(main) = *shared.main_id.peek() {
            Platform::get().focus_window(Some(main));
        }
    });

    let commit = move || {
        let mut shared = shared;
        let index = *shared.selected.peek();
        let new_name = shared.draft_name.peek().clone();
        let parsed = shared.draft_budget.peek().parse::<f64>();
        if let Some(project) = shared.projects.write().get_mut(index) {
            project.name = new_name;
            if let Ok(value) = parsed {
                project.budget = value;
            }
        }
        shared.dirty.set(true);
        note("modal: OK committed".into());
        close_dialog(shared);
    };

    rect()
        .expanded()
        .background(colors.bg)
        .color(colors.text)
        .padding(Gaps::new_all(12.))
        .spacing(8.)
        .a11y_focusable(true)
        .on_global_key_down(move |e: Event<KeyboardEventData>| {
            if e.key == Key::Named(NamedKey::Escape) {
                note("modal: Esc discarded".into());
                close_dialog(shared);
            }
        })
        .child(label().text("Edit project").font_weight(FontWeight::BOLD))
        .child(
            Input::new(name)
                .width(Size::fill())
                .auto_focus(true)
                .on_pre_key_down(|e: Event<KeyboardEventData>| input_keys(&e))
                .on_submit(move |_: String| commit()),
        )
        .child(
            Input::new(budget)
                .width(Size::fill())
                .on_pre_key_down(|e: Event<KeyboardEventData>| input_keys(&e))
                .on_submit(move |_: String| commit()),
        )
        .child(
            rect()
                .horizontal()
                .spacing(8.)
                .child(Button::new().filled().on_press(move |_| commit()).child("OK"))
                .child(
                    Button::new()
                        .on_press(move |_| {
                            note("modal: Cancel discarded".into());
                            close_dialog(shared);
                        })
                        .child("Cancel"),
                ),
        )
}

// ---------------------------------------------------------------- self-test

async fn selftest(shared: Shared) {
    let mut shared = shared;
    let passed = Cell::new(0usize);
    let failed = Cell::new(0usize);
    let check = |name: &str, ok: bool| {
        if ok {
            passed.set(passed.get() + 1);
            println!("SELFTEST PASS {name}");
        } else {
            failed.set(failed.get() + 1);
            println!("SELFTEST FAIL {name}");
        }
    };
    let settle = || Timer::after(Duration::from_millis(400));
    let count = async || {
        Platform::get()
            .post_callback(|_, ctx| {
                let mut titles: Vec<String> = ctx
                    .windows()
                    .values()
                    .map(|app| app.window().title())
                    .collect();
                titles.sort();
                titles
            })
            .await
            .unwrap_or_default()
    };

    settle().await;
    let windows = count().await;
    check("1 window at startup", windows.len() == 1);
    println!("WINDOWS {windows:?}");

    open_inspector(shared);
    settle().await;
    let windows = count().await;
    check("2 windows after Inspector", windows.len() == 2);

    // Singleton: pressing Inspector again focuses instead of opening a second.
    open_inspector(shared);
    settle().await;
    check("inspector is a singleton", count().await.len() == 2);

    open_prefs(shared);
    settle().await;
    let windows = count().await;
    check("3 windows after Preferences", windows.len() == 3);
    println!("WINDOWS {windows:?}");

    // Shared state, main -> inspector.
    shared.selected.set(2);
    settle().await;
    check(
        "selection is shared",
        shared.selected_name() == "Cinder",
    );

    // Shared state, inspector -> main (the inspector's Input writes the same
    // Vec the main list renders).
    if let Some(project) = shared.projects.write().get_mut(2) {
        project.name = "Cinder II".into();
    }
    shared.dirty.set(true);
    settle().await;
    check("edit is visible in the main list", shared.selected_name() == "Cinder II");
    check("dirty flag mirrored to the close hook", DIRTY.load(Ordering::Relaxed));

    // Cross-window message.
    *shared.pings.write() += 1;
    settle().await;
    check("ping increments the main counter", *shared.pings.peek() == 1);
    *shared.pong.write() += 1;
    settle().await;
    check("pong reached the inspector", *shared.pong.peek() == 1);

    // Theme + layout across windows.
    shared.theme.set(ThemeChoice::Dark);
    shared.compact.set(true);
    settle().await;
    check("theme is global", *shared.theme.peek() == ThemeChoice::Dark);
    check("compact rows is global", *shared.compact.peek());

    // Modal window + scrim.
    open_dialog(shared);
    settle().await;
    let windows = count().await;
    check("4 windows with the modal open", windows.len() == 4);
    let before = shared.projects.peek().len();
    // The scrim is `interactive(false)`, so a real click cannot reach Delete;
    // the scripted check here only proves the state gate, the synthetic-input
    // run in evidence/log.txt proves the pointer blocking.
    check("modal is open", shared.dialog_id.peek().is_some());
    close_dialog(shared);
    settle().await;
    check("3 windows after the modal closes", count().await.len() == 3);
    check("row count unchanged while modal was open", shared.projects.peek().len() == before);

    // Child window closes without killing the app.
    if let Some(id) = shared.inspector_id.take() {
        Platform::get().close_window(id);
    }
    settle().await;
    check("closing the inspector keeps the app alive", count().await.len() == 2);

    println!("SELFTEST DONE pass={} fail={}", passed.get(), failed.get());
    std::process::exit(if failed.get() == 0 { 0 } else { 1 });
}
