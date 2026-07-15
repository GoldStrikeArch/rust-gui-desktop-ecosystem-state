// Tray Notes (slint) — SPEC-4 OS shell integration test.

use std::rc::Rc;
use std::time::Duration;

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{
    CloseRequestResponse, ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, SharedString,
};

slint::include_modules!();

fn toggle_window(win: &MainWindow) {
    if win.window().is_visible() {
        eprintln!("[tray-notes] toggle: hiding window");
        let _ = win.hide();
    } else {
        eprintln!("[tray-notes] toggle: showing window");
        let _ = win.show();
    }
}

/// notify-rust's show() re-enters the macOS run loop (mac-notification-sys),
/// which panics Slint with "Recursion in timer code" when called from inside
/// a slint::Timer callback (verified: hard abort). Fire it from its own
/// thread instead and report failures via stderr.
fn notify(summary: &str, body: &str) {
    let (summary, body) = (summary.to_owned(), body.to_owned());
    std::thread::spawn(move || {
        match notify_rust::Notification::new().summary(&summary).body(&body).show() {
            Ok(_) => eprintln!("[tray-notes] notification shown: {summary}"),
            Err(e) => eprintln!("[tray-notes] notification FAILED: {e}"),
        }
    });
}

/// Shared by the Save… dialog callback and the TRAY_NOTES_TEST_SAVE hook
/// (the hook exists because scripted NSSavePanel interaction is unreliable
/// on a machine where parallel test apps fight for focus).
fn save_note_to(main: &MainWindow, path: &std::path::Path) {
    match std::fs::write(path, main.get_note_text().as_str()) {
        Ok(()) => {
            eprintln!("[tray-notes] saved: {}", path.display());
            main.set_status(format!("Saved {}", path.display()).into());
            notify("Note saved", &path.display().to_string());
        }
        Err(e) => main.set_status(format!("Save error: {e}").into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let main = MainWindow::new()?;
    let tray = TrayIcon::new()?;
    let about = AboutWindow::new()?;
    // Tray icon image: embedded via @image-url in the DSL. Feeding it from
    // Rust after new() does NOT work — see the note in ui/main.slint.

    // ---- File drop (Finder -> window) -------------------------------------
    // Slint 1.17.1's DropArea only receives in-app DragArea payloads: the
    // winit event loop never translates WindowEvent::DroppedFile (verified in
    // i-slint-backend-winit/event_loop.rs). Escape hatch: raw winit event
    // filter via the `unstable-winit-030` feature.
    {
        let weak = main.as_weak();
        main.window().on_winit_window_event(move |_, ev| {
            if let winit::event::WindowEvent::DroppedFile(path) = ev {
                if let Some(main) = weak.upgrade() {
                    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) {
                        match std::fs::read_to_string(path) {
                            Ok(text) => {
                                main.set_note_text(text.into());
                                main.set_status(
                                    format!("Loaded (drop): {}", path.display()).into(),
                                );
                            }
                            Err(e) => main.set_status(format!("Drop read error: {e}").into()),
                        }
                    } else {
                        main.set_status("Only .txt drops are loaded.".into());
                    }
                }
            }
            EventResult::Propagate
        });
    }

    // ---- Close-to-tray -----------------------------------------------------
    // HideWindow is the default response; installing the handler documents it
    // and lets us hint at the tray. The event loop stays alive because we run
    // run_event_loop_until_quit() (a visible SystemTrayIcon would also keep a
    // plain run_event_loop() alive per the SystemTrayIcon docs).
    {
        let weak = main.as_weak();
        main.window().on_close_requested(move || {
            eprintln!("[tray-notes] close requested -> hide to tray");
            if let Some(main) = weak.upgrade() {
                main.set_status("Hidden to tray — Cmd+Shift+9 or tray menu to restore.".into());
            }
            CloseRequestResponse::HideWindow
        });
    }

    // ---- Global hotkey Cmd+Shift+9 ----------------------------------------
    // Carbon RegisterEventHotKey under the hood; events arrive on a crossbeam
    // channel that has no waker integration with Slint's loop, so poll it
    // with a slint::Timer.
    let _hotkey_manager = match GlobalHotKeyManager::new() {
        Ok(m) => {
            let hk = HotKey::new(Some(Modifiers::META | Modifiers::SHIFT), Code::Digit9);
            match m.register(hk) {
                Ok(()) => Some(m),
                Err(e) => {
                    eprintln!("global hotkey register failed: {e}");
                    None
                }
            }
        }
        Err(e) => {
            eprintln!("GlobalHotKeyManager::new failed: {e}");
            None
        }
    };
    let hotkey_timer = slint::Timer::default();
    {
        let weak = main.as_weak();
        hotkey_timer.start(slint::TimerMode::Repeated, Duration::from_millis(50), move || {
            while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                eprintln!("[tray-notes] hotkey event: {ev:?}");
                if ev.state() == HotKeyState::Pressed {
                    if let Some(main) = weak.upgrade() {
                        toggle_window(&main);
                    }
                }
            }
        });
    }

    // ---- Menubar / tray actions --------------------------------------------
    let new_note = {
        let weak = main.as_weak();
        move || {
            if let Some(main) = weak.upgrade() {
                main.set_note_text(SharedString::default());
                main.set_has_image(false);
                main.set_status("New note.".into());
                let _ = main.show();
            }
        }
    };
    main.on_new_note(new_note.clone());
    tray.on_new_note(new_note);

    {
        let weak = main.as_weak();
        tray.on_show_hide(move || {
            if let Some(main) = weak.upgrade() {
                toggle_window(&main);
            }
        });
    }

    main.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
    tray.on_quit(|| {
        let _ = slint::quit_event_loop();
    });

    // ---- Native file dialogs (rfd, async to avoid blocking the loop) -------
    {
        let weak = main.as_weak();
        main.on_open_file(move || {
            let weak = weak.clone();
            slint::spawn_local(async move {
                let picked = rfd::AsyncFileDialog::new()
                    .add_filter("Text", &["txt"])
                    .set_title("Open note")
                    .pick_file()
                    .await;
                let Some(main) = weak.upgrade() else { return };
                if let Some(file) = picked {
                    let path = file.path().to_path_buf();
                    match std::fs::read_to_string(&path) {
                        Ok(text) => {
                            main.set_note_text(text.into());
                            main.set_status(format!("Opened {}", path.display()).into());
                        }
                        Err(e) => main.set_status(format!("Open error: {e}").into()),
                    }
                }
            })
            .expect("spawn_local (must be called from the event loop thread)");
        });
    }
    {
        let weak = main.as_weak();
        main.on_save_file(move || {
            let weak = weak.clone();
            slint::spawn_local(async move {
                let picked = rfd::AsyncFileDialog::new()
                    .add_filter("Text", &["txt"])
                    .set_file_name("note.txt")
                    .set_title("Save note")
                    .save_file()
                    .await;
                let Some(main) = weak.upgrade() else { return };
                if let Some(file) = picked {
                    save_note_to(&main, file.path());
                }
            })
            .expect("spawn_local (must be called from the event loop thread)");
        });
    }

    // Test hook: TRAY_NOTES_TEST_SAVE=<path> saves once shortly after startup
    // (same code path as Save… minus the dialog) so save+notification can be
    // verified without scripting an out-of-process NSSavePanel.
    let test_save_timer = slint::Timer::default();
    if let Ok(path) = std::env::var("TRAY_NOTES_TEST_SAVE") {
        let weak = main.as_weak();
        test_save_timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(1500),
            move || {
                if let Some(main) = weak.upgrade() {
                    main.set_note_text("test note from TRAY_NOTES_TEST_SAVE".into());
                    save_note_to(&main, std::path::Path::new(&path));
                }
            },
        );
    }

    // ---- Clipboard image (arboard) -----------------------------------------
    {
        let weak = main.as_weak();
        main.on_paste_image(move || {
            let Some(main) = weak.upgrade() else { return };
            let img = arboard::Clipboard::new().and_then(|mut c| c.get_image());
            match img {
                Ok(img) => {
                    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                        &img.bytes,
                        img.width as u32,
                        img.height as u32,
                    );
                    main.set_pasted_image(Image::from_rgba8(buf));
                    main.set_has_image(true);
                    main.set_status(
                        format!("Pasted image {}x{}", img.width, img.height).into(),
                    );
                }
                Err(e) => main.set_status(format!("No clipboard image: {e}").into()),
            }
        });
    }

    // ---- Second window ------------------------------------------------------
    {
        let about = about.as_weak();
        main.on_show_about(move || {
            if let Some(about) = about.upgrade() {
                let _ = about.show();
            }
        });
    }

    // Dark mode: nothing to do — the fluent style repaints on the winit
    // ThemeChanged event (live, verified in iteration 1).

    let _keep_about_alive = Rc::new(about);
    main.show()?;
    tray.show()?;
    slint::run_event_loop_until_quit()?;
    Ok(())
}
