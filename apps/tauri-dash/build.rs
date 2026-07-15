fn main() {
    // Parses tauri.conf.json, generates the capability/permission schemas under
    // gen/schemas/, embeds the frontend assets manifest, and (on Windows) the
    // app icon/manifest. Required for `tauri::generate_context!` to work.
    tauri_build::build()
}
