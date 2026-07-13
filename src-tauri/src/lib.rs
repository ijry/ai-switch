mod adapters;
mod app_state;
mod commands;
mod config_writer;
mod database;
mod error;
mod importers;
mod models;
mod paths;
mod security;
mod services;

use app_state::AppState;
use commands::batch_commands::{
    create_batch, create_official_account, create_provider, get_official_account,
    list_batch_groups, update_official_account,
};
use commands::import_commands::import_example_json;
use commands::route_credential_commands::{
    create_api_route_credential, delete_route_credential, get_route_credential,
    import_official_route_credentials_from_files, import_official_route_credentials_from_text,
    list_route_credentials, update_route_credential,
};
use commands::route_pool_commands::{
    get_route_pool, route_pool_route_once, set_route_pool_members,
};
use commands::route_proxy_commands::{
    get_route_proxy_status, start_route_proxy, stop_route_proxy, write_route_proxy_configs,
};
use commands::settings_commands::{get_settings, save_settings};
use commands::target_commands::list_target_apps;
use database::open_migrated_pool;
use paths::AppPaths;
use services::route_proxy_service::RouteProxyRuntimeState;

pub fn run() {
    let paths = AppPaths::resolve().expect("failed to resolve app paths");
    let pool = tauri::async_runtime::block_on(async {
        paths.ensure().await.expect("failed to ensure app paths");
        open_migrated_pool(&paths.database_file, &paths.backups_dir)
            .await
            .expect("failed to open database after migration repair")
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            paths,
            pool,
            route_proxy: RouteProxyRuntimeState::default(),
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            create_batch,
            list_batch_groups,
            create_provider,
            create_official_account,
            get_official_account,
            update_official_account,
            list_route_credentials,
            get_route_credential,
            create_api_route_credential,
            import_official_route_credentials_from_text,
            import_official_route_credentials_from_files,
            update_route_credential,
            delete_route_credential,
            import_example_json,
            get_route_pool,
            set_route_pool_members,
            route_pool_route_once,
            start_route_proxy,
            stop_route_proxy,
            get_route_proxy_status,
            write_route_proxy_configs,
            list_target_apps
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AI Switch");
}
