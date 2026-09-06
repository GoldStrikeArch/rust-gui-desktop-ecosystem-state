// macOS window-model escape hatches.
//
// Slint 1.17 models a window as a component instance plus a `slint::Window`
// handle; it has no notion of modality, ownership or parenting. Both concepts
// exist on the NSWindow one level below, and `Window::window_handle()`
// (feature `raw-window-handle-06`) is the only supported way down there.

#[cfg(target_os = "macos")]
mod imp {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    /// `slint::Window` -> `NSWindow*` (via the NSView the renderer draws into).
    pub fn ns_window(w: &slint::Window) -> Option<*mut AnyObject> {
        let handle = w.window_handle();
        let raw = handle.window_handle().ok()?;
        match raw.as_raw() {
            RawWindowHandle::AppKit(h) => {
                let view: *mut AnyObject = h.ns_view.as_ptr().cast();
                let win: *mut AnyObject = unsafe { msg_send![view, window] };
                (!win.is_null()).then_some(win)
            }
            _ => None,
        }
    }

    /// Present `sheet` as a real window-modal sheet of `parent`.
    /// AppKit disables mouse/key delivery to `parent` for as long as it runs.
    pub fn begin_sheet(parent: &slint::Window, sheet: &slint::Window) -> bool {
        let (Some(p), Some(s)) = (ns_window(parent), ns_window(sheet)) else {
            return false;
        };
        unsafe {
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![s, orderOut: nil];
            let _: () = msg_send![p, beginSheet: s, completionHandler: nil];
        }
        true
    }

    pub fn end_sheet(parent: &slint::Window, sheet: &slint::Window) {
        if let (Some(p), Some(s)) = (ns_window(parent), ns_window(sheet)) {
            unsafe {
                let _: () = msg_send![p, endSheet: s];
            }
        }
    }

    /// Real macOS parenting: the child stays above the parent, moves with it,
    /// and is ordered out when the parent is minimised/hidden.
    pub fn add_child(parent: &slint::Window, child: &slint::Window) -> bool {
        let (Some(p), Some(c)) = (ns_window(parent), ns_window(child)) else {
            return false;
        };
        unsafe {
            // NSWindowAbove == 1
            let _: () = msg_send![p, addChildWindow: c, ordered: 1isize];
        }
        true
    }

    pub fn remove_child(parent: &slint::Window, child: &slint::Window) {
        if let (Some(p), Some(c)) = (ns_window(parent), ns_window(child)) {
            unsafe {
                let _: () = msg_send![p, removeChildWindow: c];
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn begin_sheet(_: &slint::Window, _: &slint::Window) -> bool {
        false
    }
    pub fn end_sheet(_: &slint::Window, _: &slint::Window) {}
    pub fn add_child(_: &slint::Window, _: &slint::Window) -> bool {
        false
    }
    pub fn remove_child(_: &slint::Window, _: &slint::Window) {}
}

pub use imp::{add_child, begin_sheet, end_sheet, remove_child};
