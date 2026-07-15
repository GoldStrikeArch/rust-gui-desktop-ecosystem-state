fn main() {
    // Parses tauri.conf.json, generates capability/permission schemas under
    // gen/schemas/ (including the http plugin's permission set), and embeds
    // the frontend assets manifest. Required for `tauri::generate_context!`.
    tauri_build::build()
}
