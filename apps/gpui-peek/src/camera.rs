//! Camera capture via AVFoundation directly, using the exact Apple-glue stack
//! gpui's own macOS backend ships: `objc` (ClassDecl delegate + msg_send!),
//! `gpui_media` (CMSampleBuffer), `core-video` (CVPixelBuffer). The crib is
//! gpui 0.2.2's `src/platform/mac/screen_capture.rs`, which registers an ObjC
//! delegate class the same way and pulls CVImageBuffers out of sample buffers.
//!
//! Why not `nokhwa`: gpui's `surface()` element takes the IOSurface-backed
//! CVPixelBuffer itself and the Metal renderer binds its two NV12 planes as
//! textures through CVMetalTextureCache — zero CPU copies. nokhwa's
//! AVFoundation backend instead copies every frame into a `Vec<u8>` and
//! CPU-converts to RGB, which forecloses the zero-copy path. The renderer
//! *asserts* the buffer is kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
//! ("420f"), which is a native AVFoundation camera output format, so we
//! request exactly that from AVCaptureVideoDataOutput.

use std::{
    ffi::{c_void, CStr},
    ptr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, Once,
    },
};

use block::ConcreteBlock;
use core_foundation::base::TCFType;
use core_video::pixel_buffer::{
    kCVPixelBufferLock_ReadOnly, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, CVPixelBuffer, CVPixelBufferRef,
};
use futures::channel::mpsc;
use media::core_media::{CMSampleBuffer, CMSampleBufferRef};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel, BOOL, YES},
    sel, sel_impl,
};

pub type Id = *mut Object;
const NIL: Id = ptr::null_mut();

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    static AVMediaTypeVideo: Id;
    static AVMediaTypeAudio: Id;
    static AVCaptureSessionPreset1280x720: Id;
}

// ---------------------------------------------------------------------------
// TCC authorization
// ---------------------------------------------------------------------------

/// Mirror of AVAuthorizationStatus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthStatus {
    NotDetermined,
    Restricted,
    Denied,
    Authorized,
}

impl AuthStatus {
    fn from_raw(raw: i64) -> Self {
        match raw {
            0 => AuthStatus::NotDetermined,
            1 => AuthStatus::Restricted,
            2 => AuthStatus::Denied,
            3 => AuthStatus::Authorized,
            _ => AuthStatus::Restricted,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AuthStatus::NotDetermined => "not-determined",
            AuthStatus::Restricted => "restricted",
            AuthStatus::Denied => "denied",
            AuthStatus::Authorized => "authorized",
        }
    }
}

#[derive(Clone, Copy)]
pub enum MediaKind {
    Video,
    Audio,
}

fn media_type(kind: MediaKind) -> Id {
    unsafe {
        match kind {
            MediaKind::Video => AVMediaTypeVideo,
            MediaKind::Audio => AVMediaTypeAudio,
        }
    }
}

/// Query TCC state without triggering a prompt.
pub fn auth_status(kind: MediaKind) -> AuthStatus {
    unsafe {
        let raw: i64 =
            msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: media_type(kind)];
        AuthStatus::from_raw(raw)
    }
}

/// Ask TCC for camera access. Fires the system prompt when the status is
/// NotDetermined; the completion handler runs on an arbitrary thread.
pub fn request_video_access(on_result: impl Fn(bool) + Send + 'static) {
    unsafe {
        let block = ConcreteBlock::new(move |granted: BOOL| {
            on_result(granted == YES);
        });
        let block = block.copy();
        let _: () = msg_send![class!(AVCaptureDevice),
            requestAccessForMediaType: media_type(MediaKind::Video)
            completionHandler: &*block];
    }
}

// ---------------------------------------------------------------------------
// Frame hand-off (capture queue -> main thread)
// ---------------------------------------------------------------------------

/// CVPixelBuffer is a thread-safe, refcounted CF object; the Rust wrapper just
/// isn't marked Send. This newtype carries it from the AVFoundation delegate
/// queue to the main thread.
pub struct SendPixelBuffer(pub CVPixelBuffer);
unsafe impl Send for SendPixelBuffer {}

pub struct SharedFrame {
    latest: Mutex<Option<SendPixelBuffer>>,
    pub captured: AtomicU64,
    /// Coalescing wake for the UI pump: capacity-2 channel, drops when full.
    wake: Mutex<mpsc::Sender<()>>,
}

impl SharedFrame {
    pub fn take_latest(&self) -> Option<CVPixelBuffer> {
        self.latest.lock().unwrap().take().map(|f| f.0)
    }
}

// ---------------------------------------------------------------------------
// Delegate class (cribbed from gpui's screen_capture.rs)
// ---------------------------------------------------------------------------

static REGISTER_DELEGATE: Once = Once::new();
static mut DELEGATE_CLASS: *const Class = ptr::null();
const SHARED_IVAR: &str = "peek_shared_frame";

fn delegate_class() -> &'static Class {
    unsafe {
        REGISTER_DELEGATE.call_once(|| {
            let mut decl = ClassDecl::new("PeekCaptureDelegate", class!(NSObject)).unwrap();
            decl.add_ivar::<*mut c_void>(SHARED_IVAR);
            decl.add_method(
                sel!(captureOutput:didOutputSampleBuffer:fromConnection:),
                did_output_sample_buffer as extern "C" fn(&Object, Sel, Id, Id, Id),
            );
            DELEGATE_CLASS = decl.register();
        });
        #[allow(static_mut_refs)]
        &*DELEGATE_CLASS
    }
}

extern "C" fn did_output_sample_buffer(
    this: &Object,
    _sel: Sel,
    _output: Id,
    sample_buffer: Id,
    _connection: Id,
) {
    unsafe {
        let shared = *this.get_ivar::<*mut c_void>(SHARED_IVAR) as *const SharedFrame;
        if shared.is_null() {
            return;
        }
        let shared = &*shared;
        let sample_buffer = CMSampleBuffer::wrap_under_get_rule(sample_buffer as CMSampleBufferRef);
        if let Some(image_buffer) = sample_buffer.image_buffer() {
            // CVImageBuffer and CVPixelBuffer are the same CF object for
            // camera frames; re-wrap under the pixel-buffer type (retains).
            let pixel_buffer = CVPixelBuffer::wrap_under_get_rule(
                image_buffer.as_concrete_TypeRef() as CVPixelBufferRef,
            );
            *shared.latest.lock().unwrap() = Some(SendPixelBuffer(pixel_buffer));
            shared.captured.fetch_add(1, Ordering::Relaxed);
            // Wake the UI pump; if the (capacity 2) channel is full a wake is
            // already pending and this frame will still be picked up.
            let _ = shared.wake.lock().unwrap().try_send(());
        }
    }
}

// ---------------------------------------------------------------------------
// Capture session
// ---------------------------------------------------------------------------

/// One AVCaptureSession, created after TCC authorization and kept for the
/// life of the app (Start/Stop toggles startRunning/stopRunning, so there is
/// no delegate teardown race to manage).
pub struct Camera {
    session: Id,
    _delegate: Id,
    pub shared: Arc<SharedFrame>,
    running: bool,
}

impl Camera {
    pub fn new() -> Result<(Self, mpsc::Receiver<()>), String> {
        let (wake_tx, wake_rx) = mpsc::channel::<()>(2);
        let shared = Arc::new(SharedFrame {
            latest: Mutex::new(None),
            captured: AtomicU64::new(0),
            wake: Mutex::new(wake_tx),
        });

        unsafe {
            let device: Id =
                msg_send![class!(AVCaptureDevice), defaultDeviceWithMediaType: media_type(MediaKind::Video)];
            if device.is_null() {
                return Err("no default video device".into());
            }

            let mut error: Id = NIL;
            let input: Id = msg_send![class!(AVCaptureDeviceInput),
                deviceInputWithDevice: device
                error: &mut error];
            if input.is_null() {
                return Err(format!("AVCaptureDeviceInput failed: {}", ns_error(error)));
            }

            let session: Id = msg_send![class!(AVCaptureSession), new];
            let _: () = msg_send![session, beginConfiguration];

            let can_preset: BOOL =
                msg_send![session, canSetSessionPreset: AVCaptureSessionPreset1280x720];
            if can_preset == YES {
                let _: () = msg_send![session, setSessionPreset: AVCaptureSessionPreset1280x720];
            }

            let can_add_input: BOOL = msg_send![session, canAddInput: input];
            if can_add_input != YES {
                let _: () = msg_send![session, release];
                return Err("cannot add camera input (TCC denied?)".into());
            }
            let _: () = msg_send![session, addInput: input];

            let output: Id = msg_send![class!(AVCaptureVideoDataOutput), new];
            // Request exactly the format gpui's surface renderer asserts:
            // NV12 full-range ("420f").
            let key = kCVPixelBufferPixelFormatTypeKey as Id;
            let value: Id = msg_send![class!(NSNumber),
                numberWithUnsignedInt: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange];
            let settings: Id =
                msg_send![class!(NSDictionary), dictionaryWithObject: value forKey: key];
            let _: () = msg_send![output, setVideoSettings: settings];
            let _: () = msg_send![output, setAlwaysDiscardsLateVideoFrames: YES];

            let delegate: Id = msg_send![delegate_class(), new];
            // Intentionally leaked (one strong Arc for the delegate's ivar):
            // the session+delegate live for the rest of the process, and never
            // freeing this sidesteps any in-flight-callback teardown race.
            let shared_ptr = Arc::into_raw(shared.clone()) as *mut c_void;
            (*delegate).set_ivar::<*mut c_void>(SHARED_IVAR, shared_ptr);

            let queue = dispatch::ffi::dispatch_queue_create(
                c"org.rcn.gpui-peek.camera".as_ptr(),
                dispatch::ffi::DISPATCH_QUEUE_SERIAL,
            );
            let _: () = msg_send![output, setSampleBufferDelegate: delegate queue: queue];

            let can_add_output: BOOL = msg_send![session, canAddOutput: output];
            if can_add_output != YES {
                let _: () = msg_send![session, release];
                return Err("cannot add video data output".into());
            }
            let _: () = msg_send![session, addOutput: output];
            let _: () = msg_send![session, commitConfiguration];

            Ok((
                Camera {
                    session,
                    _delegate: delegate,
                    shared,
                    running: false,
                },
                wake_rx,
            ))
        }
    }

    /// Synchronous; blocks the main thread for the camera spin-up (~100-500 ms).
    pub fn start(&mut self) {
        if !self.running {
            unsafe {
                let _: () = msg_send![self.session, startRunning];
            }
            self.running = true;
        }
    }

    pub fn stop(&mut self) {
        if self.running {
            unsafe {
                let _: () = msg_send![self.session, stopRunning];
            }
            self.running = false;
        }
    }
}

fn ns_error(error: Id) -> String {
    if error.is_null() {
        return "unknown error".into();
    }
    unsafe {
        let desc: Id = msg_send![error, localizedDescription];
        if desc.is_null() {
            return "unknown error".into();
        }
        let utf8: *const std::os::raw::c_char = msg_send![desc, UTF8String];
        if utf8.is_null() {
            return "unknown error".into();
        }
        CStr::from_ptr(utf8).to_string_lossy().into_owned()
    }
}

// ---------------------------------------------------------------------------
// CPU comparison path: NV12 -> BGRA on the CPU (what a framework without a
// surface/CVPixelBuffer element would have to do every frame)
// ---------------------------------------------------------------------------

pub fn nv12_to_bgra(pb: &CVPixelBuffer) -> Option<(u32, u32, Vec<u8>)> {
    if pb.get_pixel_format() != kCVPixelFormatType_420YpCbCr8BiPlanarFullRange {
        return None;
    }
    pb.lock_base_address(kCVPixelBufferLock_ReadOnly);
    let width = pb.get_width_of_plane(0);
    let height = pb.get_height_of_plane(0);
    let result = unsafe {
        let y_base = pb.get_base_address_of_plane(0) as *const u8;
        let y_stride = pb.get_bytes_per_row_of_plane(0);
        let c_base = pb.get_base_address_of_plane(1) as *const u8;
        let c_stride = pb.get_bytes_per_row_of_plane(1);
        if y_base.is_null() || c_base.is_null() {
            None
        } else {
            let mut out = vec![0u8; width * height * 4];
            for row in 0..height {
                let y_row = y_base.add(row * y_stride);
                let c_row = c_base.add((row / 2) * c_stride);
                let out_row = &mut out[row * width * 4..(row + 1) * width * 4];
                for col in 0..width {
                    let y = *y_row.add(col) as i32;
                    let cb = *c_row.add((col / 2) * 2) as i32 - 128;
                    let cr = *c_row.add((col / 2) * 2 + 1) as i32 - 128;
                    // BT.601 full-range, 16.16 fixed point.
                    let r = y + ((91881 * cr) >> 16);
                    let g = y - ((22554 * cb + 46802 * cr) >> 16);
                    let b = y + ((116130 * cb) >> 16);
                    let px = &mut out_row[col * 4..col * 4 + 4];
                    px[0] = b.clamp(0, 255) as u8;
                    px[1] = g.clamp(0, 255) as u8;
                    px[2] = r.clamp(0, 255) as u8;
                    px[3] = 255;
                }
            }
            Some((width as u32, height as u32, out))
        }
    };
    pb.unlock_base_address(kCVPixelBufferLock_ReadOnly);
    result
}
