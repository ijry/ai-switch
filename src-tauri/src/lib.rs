mod app_state;
mod commands;
mod database;
mod error;
mod models;
mod paths;
mod services;

use app_state::AppState;
use commands::batch_commands::{
    create_batch, create_official_account, create_provider, list_batch_groups,
};
use commands::settings_commands::{get_settings, save_settings};
use commands::target_commands::list_target_apps;
use database::{create_pool, run_migrations};
use paths::AppPaths;

pub fn run() {
    let paths = AppPaths::resolve().expect("failed to resolve app paths");
    let pool = tauri::async_runtime::block_on(async {
        paths.ensure().await.expect("failed to ensure app paths");
        let pool = create_pool(&paths.database_file)
            .await
            .expect("failed to create database pool");
        run_migrations(&pool)
            .await
            .expect("failed to run database migrations");
        pool
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState { paths, pool })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            create_batch,
            list_batch_groups,
            create_provider,
            create_official_account,
            list_target_apps
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AI Switch");
}
