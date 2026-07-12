mod app_state;
mod commands;
mod error;
mod models;
mod paths;
mod services;

use app_state::AppState;
use commands::settings_commands::{get_settings, save_settings};
use paths::AppPaths;

pub fn run() {
    let paths = AppPaths::resolve().expect("failed to resolve app paths");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState { paths })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings])
        .run(tauri::generate_context!())
        .expect("failed to run AI Switch");
}
