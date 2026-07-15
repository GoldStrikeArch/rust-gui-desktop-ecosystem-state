fn main() {
    // Parses tauri.conf.json, generates capability/permission schemas under
    // gen/schemas/, and embeds the frontend assets manifest. Because this
    // manual setup never enables tauri's `custom-protocol` feature, the
    // generated context is a "dev" context, which (on macOS) also embeds
    // ./Info.plist into the binary's __TEXT,__info_plist section — that is
    // how this unbundled cargo binary carries NSCameraUsageDescription /
    // NSMicrophoneUsageDescription for TCC.
    tauri_build::build()
}
