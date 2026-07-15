// Tasks (Tauri) — RCN GUI ecosystem research mini-app.
//
// Design choice (see GAPS.md): the task list lives in the Rust process,
// behind `tauri::State`. The webview frontend is a thin view that calls the
// three #[tauri::command] functions over Tauri's IPC bridge and re-renders
// from the returned canonical list. This deliberately exercises the IPC
// bridge, which is the architectural core of Tauri.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use tauri::State;

/// App state: the canonical task list, owned by the Rust core process.
#[derive(Default)]
struct Tasks(Mutex<Vec<String>>);

/// Appends the trimmed text as a new task. Empty/whitespace input is a
/// no-op. Returns the full, canonical task list.
#[tauri::command]
fn add_task(text: String, tasks: State<'_, Tasks>) -> Vec<String> {
    let trimmed = text.trim();
    let mut list = tasks.0.lock().unwrap();
    if !trimmed.is_empty() {
        list.push(trimmed.to_string());
    }
    list.clone()
}

/// Removes the task at `index` (ignores out-of-range indices, which can
/// happen if the UI races ahead of state). Returns the canonical list.
#[tauri::command]
fn delete_task(index: usize, tasks: State<'_, Tasks>) -> Vec<String> {
    let mut list = tasks.0.lock().unwrap();
    if index < list.len() {
        list.remove(index);
    }
    list.clone()
}

/// Returns the canonical task list (used by the frontend on load).
#[tauri::command]
fn get_tasks(tasks: State<'_, Tasks>) -> Vec<String> {
    tasks.0.lock().unwrap().clone()
}

fn main() {
    tauri::Builder::default()
        .manage(Tasks::default())
        .invoke_handler(tauri::generate_handler![add_task, delete_task, get_tasks])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
