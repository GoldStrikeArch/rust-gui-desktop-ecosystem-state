fn main() {
    // Parses tauri.conf.json, generates capability/permission schemas under
    // gen/schemas/, and embeds the frontend assets manifest. Required for
    // `tauri::generate_context!`.
    tauri_build::build()
}
