//! Tray Notes (xilem) — SPEC-4 OS shell integration test.
//!
//! Architecture: xilem 0.4 cannot express tray icons, menubars, global
//! hotkeys, file drops or theme changes. We therefore embed xilem in an
//! *external* winit event loop (upstream `external_event_loop.rs` pattern):
//! we own the `ApplicationHandler`, forward everything to `MasonryState`,
//! and splice the shell integrations in at the winit layer. External events
//! reach xilem app state through a tokio channel drained by a stock
//! `worker` view; window visibility (hide-to-tray) is applied by a wrapper
//! `AppDriver` that reaches the winit window handle through `DriverCtx`.

mod shell;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use xilem::tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use xilem::core::{fork, MessageProxy};
use xilem::masonry::peniko::{ImageAlphaType, ImageData};
use xilem::masonry::properties::types::AsUnit;
use xilem::masonry::theme::default_property_set;
use xilem::style::{Padding, Style as _};
use xilem::view::{
    button, flex_col, flex_row, image, label, portal, sized_box, text_input, worker,
    CrossAxisAlignment, FlexExt as _, FlexSpacer, MainAxisAlignment, ObjectFit,
};
use xilem::winit::dpi::{LogicalPosition, LogicalSize};
use xilem::{
    window, AppState, Blob, Color, EventLoop, ImageBrush, ImageFormat, InsertNewline, WidgetView,
    WindowId, WindowView, Xilem,
};

use masonry_winit::app::MasonryState;

/// External events flowing from the OS shell layer into xilem app state.
#[derive(Clone, Debug)]
pub enum Ev {
    NewNote,
    OpenRequested,
    SaveRequested,
    ToggleWindow,
    Quit,
    About,
    FileDropped(PathBuf),
    Theme(bool),
    /// Edit-menu clipboard roles; queued in `Shared::pending_edit` and
    /// injected as synthetic `TextEvent`s by the wrapper driver (the only
    /// place with `RenderRoot` access).
    EditCut,
    EditCopy,
    EditPaste,
    EditSelectAll,
}

/// State shared between the shell layer (tray/hotkey/menu handlers, the
/// external winit handler, the wrapper driver) and xilem app state.
#[derive(Default)]
pub struct Shared {
    /// Sender into the xilem `worker` view (filled at first view build).
    pub tx: Mutex<Option<UnboundedSender<Ev>>>,
    /// Desired main-window visibility; applied by the wrapper driver.
    pub want_visible: AtomicBool,
    /// Set to request app exit; applied by the wrapper driver.
    pub quit: AtomicBool,
    /// Edit-menu commands awaiting injection into the main window's
    /// `RenderRoot` (drained by the wrapper driver).
    pub pending_edit: Mutex<Vec<Ev>>,
}

impl Shared {
    pub fn send(&self, ev: Ev) {
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(ev);
        }
    }
}

struct AppData {
    text: String,
    status: String,
    dark: bool,
    thumb: Option<ImageBrush>,
    about_open: bool,
    shared: Arc<Shared>,
    main_window: WindowId,
    about_window: WindowId,
}

impl AppState for AppData {
    fn keep_running(&self) -> bool {
        // Quit is driven by the wrapper driver (`Shared::quit`), and main
        // window close never reaches the inner driver (close-to-tray).
        true
    }
}

impl AppData {
    fn handle_ev(&mut self, ev: Ev) {
        match ev {
            Ev::NewNote => {
                self.text.clear();
                self.status = "new note".into();
                self.shared.want_visible.store(true, Ordering::SeqCst);
            }
            Ev::OpenRequested => self.open_dialog(),
            Ev::SaveRequested => self.save_dialog(),
            Ev::ToggleWindow => {
                let v = self.shared.want_visible.load(Ordering::SeqCst);
                self.shared.want_visible.store(!v, Ordering::SeqCst);
            }
            Ev::Quit => self.shared.quit.store(true, Ordering::SeqCst),
            Ev::About => self.about_open = true,
            Ev::FileDropped(path) => {
                self.load_file(&path);
                self.status = format!("dropped: {}", path.display());
            }
            Ev::Theme(dark) => {
                self.dark = dark;
                self.status = format!("os theme: {}", if dark { "dark" } else { "light" });
            }
            Ev::EditCut | Ev::EditCopy | Ev::EditPaste | Ev::EditSelectAll => {
                self.shared.pending_edit.lock().unwrap().push(ev);
            }
        }
    }

    fn load_file(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                self.text = text;
                self.status = format!("opened {}", path.display());
            }
            Err(e) => self.status = format!("open failed: {e}"),
        }
    }

    /// Blocking NSOpenPanel. We are on the main thread here (worker messages
    /// are dispatched inside winit's event handling), which AppKit requires.
    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt"])
            .pick_file()
        {
            self.load_file(&path);
        } else {
            self.status = "open cancelled".into();
        }
    }

    fn save_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt"])
            .set_file_name("note.txt")
            .save_file()
        {
            match std::fs::write(&path, &self.text) {
                Ok(()) => {
                    self.status = format!("saved {}", path.display());
                    notify_saved(&path);
                }
                Err(e) => self.status = format!("save failed: {e}"),
            }
        } else {
            self.status = "save cancelled".into();
        }
    }

    fn paste_image(&mut self) {
        let img = arboard::Clipboard::new().and_then(|mut c| c.get_image());
        match img {
            Ok(img) => {
                let (w, h) = (img.width as u32, img.height as u32);
                let data = ImageData {
                    data: Blob::new(Arc::new(img.bytes.into_owned())),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width: w,
                    height: h,
                };
                self.thumb = Some(ImageBrush::new(data));
                self.status = format!("pasted image {w}x{h}");
            }
            Err(e) => self.status = format!("no image on clipboard ({e})"),
        }
    }
}

/// Post the "Note saved" notification.
///
/// Three notify-rust attempts were needed on macOS (see FRICTION.md):
/// 1. naive `.show()` — mac-notification-sys resolves the magic app name
///    "use_default" via LaunchServices, which on macOS 26 pops a *blocking*
///    "Where is use_default?" chooser dialog;
/// 2. `set_application("com.apple.Terminal")` first — fixes the chooser, but
///    the synchronous send pumps the main runloop from inside winit's event
///    callback and winit 0.30 panic-aborts ("tried to handle event while
///    another event is currently being handled");
/// 3. fire it from a detached thread (below) — off the winit callstack, so
///    the runloop pumping is harmless. osascript fallback kept for safety.
fn notify_saved(path: &Path) {
    let body = format!("Saved to {}", path.display());
    std::thread::spawn(move || {
        // macOS: notify-rust (attempts 1-3 above) never produced a visible
        // banner for this unbundled binary even when `.show()` returned Ok
        // (macOS 26 silently drops NSUserNotifications for the borrowed
        // bundle id). Out-of-process osascript is the path that verifiably
        // displays a banner, so it is the macOS primary.
        #[cfg(target_os = "macos")]
        {
            let script = format!(
                "display notification \"{}\" with title \"Note saved\"",
                body.replace('"', "'")
            );
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .status();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = notify_rust::Notification::new()
                .summary("Note saved")
                .body(&body)
                .show();
        }
    });
}

// --- Hand-rolled light/dark palette (masonry 0.4 ships a dark-only theme;
// --- we override the few reachable properties per widget).
struct Palette {
    bg: Color,
    fg: Color,
    dim: Color,
    editor_bg: Color,
    button_bg: Color,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            bg: Color::from_rgb8(0x18, 0x18, 0x1b),
            fg: Color::from_rgb8(0xf0, 0xf0, 0xea),
            dim: Color::from_rgb8(0xa0, 0xa0, 0x9a),
            editor_bg: Color::from_rgb8(0x27, 0x27, 0x2a),
            button_bg: Color::from_rgb8(0x3f, 0x3f, 0x46),
        }
    } else {
        Palette {
            bg: Color::from_rgb8(0xf2, 0xf2, 0xef),
            fg: Color::from_rgb8(0x1c, 0x1c, 0x1e),
            dim: Color::from_rgb8(0x6a, 0x6a, 0x64),
            editor_bg: Color::WHITE,
            button_bg: Color::from_rgb8(0xdd, 0xdd, 0xd8),
        }
    }
}

fn main_view(state: &mut AppData) -> impl WidgetView<AppData> + use<> {
    let pal = palette(state.dark);

    let editor = sized_box(portal(
        text_input(state.text.clone(), |s: &mut AppData, t| s.text = t)
            .insert_newline(InsertNewline::OnEnter)
            .placeholder("Type a note. Drop a .txt here, or use File > Open.")
            .text_color(pal.fg)
            .caret_color(pal.fg)
            .background_color(pal.editor_bg),
    ))
    .expand()
    .background_color(pal.editor_bg)
    .flex(1.0);

    let buttons = flex_row((
        button(label("Paste image").color(pal.fg), |s: &mut AppData| {
            s.paste_image()
        })
        .background_color(pal.button_bg),
        button(label("About").color(pal.fg), |s: &mut AppData| {
            s.about_open = true
        })
        .background_color(pal.button_bg),
        FlexSpacer::Flex(1.0),
        label(state.status.clone()).color(pal.dim),
    ));

    let thumb = state.thumb.clone().map(|b| {
        sized_box(image(b).fit(ObjectFit::Contain))
            .height(96.px())
            .expand_width()
    });

    flex_col((editor, buttons, thumb)).padding(Padding::all(10.0))
}

fn about_view(state: &mut AppData) -> impl WidgetView<AppData> + use<> {
    let pal = palette(state.dark);
    flex_col((
        label("Tray Notes").text_size(24.0).color(pal.fg),
        label("xilem 0.4.0 — SPEC-4 shell integration research app").color(pal.fg),
        label("tray-icon + global-hotkey + muda + rfd + arboard + notify-rust").color(pal.dim),
        FlexSpacer::Fixed(8.px()),
        button(label("Close").color(pal.fg), |s: &mut AppData| {
            s.about_open = false
        })
        .background_color(pal.button_bg),
    ))
    .main_axis_alignment(MainAxisAlignment::Center)
    .cross_axis_alignment(CrossAxisAlignment::Center)
}

fn app_logic(state: &mut AppData) -> impl Iterator<Item = WindowView<AppData>> + use<> {
    let pal = palette(state.dark);

    let main = window(
        state.main_window,
        "Tray Notes (xilem)",
        fork(
            main_view(state),
            // Drains the shell-event channel; `store_sender` publishes the
            // sender into `Shared` so tray/menu/hotkey handlers can reach us.
            worker(
                |proxy: MessageProxy<Ev>, mut rx: UnboundedReceiver<Ev>| async move {
                    while let Some(ev) = rx.recv().await {
                        if proxy.message(ev).is_err() {
                            break;
                        }
                    }
                },
                |s: &mut AppData, tx: UnboundedSender<Ev>| {
                    *s.shared.tx.lock().unwrap() = Some(tx);
                },
                |s: &mut AppData, ev: Ev| s.handle_ev(ev),
            ),
        ),
    )
    .with_options(|o| {
        let o = o.with_initial_inner_size(LogicalSize::new(500.0, 420.0));
        // Optional fixed position for scripted interaction testing.
        match std::env::var("TRAY_POS").ok().and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?))
        }) {
            Some((x, y)) => o.with_initial_position(LogicalPosition::new(x, y)),
            None => o,
        }
    })
    .with_base_color(pal.bg);

    let about = state.about_open.then(|| {
        window(state.about_window, "About Tray Notes", about_view(state))
            .with_options(|o| {
                o.with_initial_inner_size(LogicalSize::new(420.0, 200.0))
                    .on_close(|s: &mut AppData| s.about_open = false)
            })
            .with_base_color(pal.bg)
    });

    std::iter::once(main)
        .chain(about)
        .collect::<Vec<_>>()
        .into_iter()
}

/// Initial OS theme; winit only reports *changes* (`WindowEvent::ThemeChanged`).
fn detect_dark() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Dark"))
        .unwrap_or(false)
}

fn main() {
    let shared = Arc::new(Shared::default());
    shared.want_visible.store(true, Ordering::SeqCst);

    let main_window = WindowId::next();
    let about_window = WindowId::next();
    let data = AppData {
        text: String::new(),
        status: "ready".into(),
        dark: detect_dark(),
        thumb: None,
        about_open: false,
        shared: shared.clone(),
        main_window,
        about_window,
    };

    let xilem = Xilem::new(data, app_logic);

    // External event loop embedding (upstream `external_event_loop` pattern).
    let event_loop = EventLoop::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    let (driver, windows) =
        xilem.into_driver_and_windows(move |event| proxy.send_event(event).map_err(|err| err.0));
    let masonry_state =
        MasonryState::new(event_loop.create_proxy(), windows, default_property_set());

    let mut app = shell::ShellApp::new(masonry_state, Box::new(driver), main_window, shared);
    event_loop.run_app(&mut app).unwrap();
}
