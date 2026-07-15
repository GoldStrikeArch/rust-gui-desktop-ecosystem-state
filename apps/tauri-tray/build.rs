fn main() {
    // Parses tauri.conf.json, generates the capability/permission schemas under
    // gen/schemas/, embeds the frontend assets manifest and the default window
    // icon. Required for `tauri::generate_context!` to work.
    tauri_build::build()
}
