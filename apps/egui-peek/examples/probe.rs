//! Verification tool: list cameras and their supported formats (no GUI).
//! Run: cargo run --release --example probe

use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};

fn main() {
    println!("nokhwa_check (camera authorized) = {}", nokhwa::nokhwa_check());
    let devices = nokhwa::query(nokhwa::utils::ApiBackend::AVFoundation).unwrap_or_default();
    for info in &devices {
        println!("device: {info}");
    }
    // Raw AVFoundation format list (what set_all actually matches against).
    match nokhwa_bindings_macos::AVCaptureDevice::new(&CameraIndex::Index(0)) {
        Ok(device) => match device.supported_formats_raw() {
            Ok(raw) => {
                for f in raw {
                    println!(
                        "  raw: {}x{} {:?} fps_list={:?}",
                        f.resolution.width, f.resolution.height, f.fourcc, f.fps_list
                    );
                }
            }
            Err(err) => println!("supported_formats_raw failed: {err}"),
        },
        Err(err) => println!("AVCaptureDevice::new failed: {err}"),
    }

    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);
    match nokhwa::Camera::new(CameraIndex::Index(0), requested) {
        Ok(mut camera) => {
            println!("opened with format: {}", camera.camera_format());
            match camera.compatible_camera_formats() {
                Ok(mut formats) => {
                    formats.sort();
                    for f in formats {
                        println!("  supported: {f}");
                    }
                }
                Err(err) => println!("compatible_camera_formats failed: {err}"),
            }
        }
        Err(err) => println!("Camera::new(None) failed: {err}"),
    }
}
