fn main() {
    // AVFoundation for AVCaptureSession/AVCaptureDevice/AVCaptureVideoDataOutput.
    // (CoreMedia/CoreVideo symbols come in via the gpui_media / core-video
    // crates' own link attributes, but linking them here too is harmless and
    // makes the requirement explicit.)
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=CoreMedia");
    println!("cargo:rustc-link-lib=framework=CoreVideo");
}
