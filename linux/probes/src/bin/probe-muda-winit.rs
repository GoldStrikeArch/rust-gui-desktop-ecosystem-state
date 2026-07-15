//! probe-muda-winit — claim under test (ecosystem map §1.6): on Linux a muda
//! menubar cannot attach to a winit window; `Menu::init_for_gtk_window` is the
//! only Linux attach API and its bounds require a `gtk::Window`.
//!
//! THIS PROBE IS EXPECTED TO FAIL TO COMPILE. The verbatim rustc trait-bound
//! error is the evidence: `winit::window::Window` does not (and cannot)
//! implement `IsA<gtk::Window>`, so there is no way to hand muda a winit
//! window on Linux. (On macOS the equivalent is `init_for_nsapp`, which is
//! process-global and works fine next to winit — that asymmetry is the fault
//! line.)

fn main() {
    let event_loop = winit::event_loop::EventLoop::new().expect("event loop");
    #[allow(deprecated)]
    let window = event_loop
        .create_window(winit::window::Window::default_attributes().with_title("probe-muda-winit"))
        .expect("winit window");

    let menubar = muda::Menu::new();
    let file_menu = muda::Submenu::new("&File", true);
    menubar.append(&file_menu).expect("append submenu");

    // The port attempt: give muda the winit window, as a macOS-written shell
    // app would hope to. There is no other winit-facing attach API on Linux.
    #[cfg(target_os = "linux")]
    menubar
        .init_for_gtk_window(&window, None::<&gtk::Box>)
        .expect("attach menubar");

    // Never reached on Linux (does not compile). Keep `window` alive for the
    // hypothetical runtime path.
    drop(window);
}
