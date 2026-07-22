mod adapters;
mod app_state;
mod commands;
mod config_writer;
mod core;
mod database;
mod error;
mod importers;
mod models;
mod paths;
mod security;
pub mod server;
mod services;
mod session_manager;
mod terminal_manager;
mod web;

use app_state::AppState;
use commands::batch_commands::{
    create_batch, create_official_account, create_provider, get_official_account,
    list_batch_groups, update_official_account,
};
use commands::import_commands::import_example_json;
use commands::route_credential_commands::{
    create_api_route_credential, delete_route_credential, get_route_credential,
    import_official_route_credentials_from_files, import_official_route_credentials_from_text,
    list_route_credentials, refresh_route_credential_quota, refresh_route_credentials_quota,
    update_route_credential,
};
use commands::route_pool_commands::{
    fetch_route_models, get_route_pool, route_pool_route_once, route_pool_test_model,
    set_route_pool_members,
};
use commands::route_proxy_commands::{
    get_route_proxy_status, start_route_proxy, stop_route_proxy, write_route_proxy_configs,
};
use commands::session_commands::{get_session_messages, list_sessions};
use commands::settings_commands::{get_settings, save_settings};
use commands::target_commands::list_target_apps;
use commands::terminal_commands::{
    create_terminal_session, kill_terminal_session, list_terminal_sessions, resize_terminal,
    write_terminal_input,
};
use commands::web_service_commands::{
    disconnect_tailscale, get_tailscale_status, get_web_server_status, get_web_service_config,
    save_web_service_config, start_tailscale_login, start_tailscale_with_auth_key,
    start_web_server, stop_web_server,
};
use database::open_migrated_pool;
use paths::AppPaths;
use services::route_proxy_service::RouteProxyRuntimeState;
use services::tailscale_service::TailscaleRuntimeState;
use services::web_service::{WebService, WebServiceRuntimeState};
use std::sync::Arc;
use services::deeplink_service::{parse_deeplink_url, DeepLinkErrorPayload};
use tauri::{Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;
use terminal_manager::TerminalManager;
use web::event_bridge::WebEventBroadcaster;

fn is_deeplink_url(value: &str) -> bool {
    value.starts_with("ccswitch://") || value.starts_with("aiswitch://")
}

fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn handle_deeplink_url(app: &tauri::AppHandle, url_str: &str, source: &str) -> bool {
    if !is_deeplink_url(url_str) {
        return false;
    }

    match parse_deeplink_url(url_str) {
        Ok(payload) => {
            let _ = app.emit("deeplink-import", payload);
            focus_main_window(app);
        }
        Err(message) => {
            let _ = app.emit(
                "deeplink-error",
                DeepLinkErrorPayload {
                    message,
                    source: source.to_string(),
                },
            );
            focus_main_window(app);
        }
    }

    true
}

pub fn run() {
    let paths = AppPaths::resolve().expect("failed to resolve app paths");
    let pool = tauri::async_runtime::block_on(async {
        paths.ensure().await.expect("failed to ensure app paths");
        open_migrated_pool(&paths.database_file, &paths.backups_dir)
            .await
            .expect("failed to open database after migration repair")
    });

    let mut builder = tauri::Builder::default();

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            for arg in &args {
                if handle_deeplink_url(app, arg, "single_instance") {
                    break;
                }
            }
            focus_main_window(app);
        }));
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .manage(AppState {
            paths,
            pool,
            route_proxy: RouteProxyRuntimeState::default(),
            web_service: WebServiceRuntimeState::default(),
            tailscale: TailscaleRuntimeState::default(),
            terminals: TerminalManager::default(),
            event_broadcaster: Arc::new(WebEventBroadcaster::new()),
        })
        .setup(|app| {
            let state = app.state::<AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                let Ok(config) = WebService::load_config(&state.paths).await else {
                    return;
                };
                if !config.auto_start {
                    return;
                }
                let _ = WebService::start(Arc::new(state), config).await;
            });

            #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
            {
                if let Err(err) = app.deep_link().register_all() {
                    eprintln!("failed to register deep link schemes: {err}");
                }
            }

            app.deep_link().on_open_url({
                let app_handle = app.handle().clone();
                move |event| {
                    for url in event.urls() {
                        if handle_deeplink_url(&app_handle, url.as_str(), "on_open_url") {
                            break;
                        }
                    }
                }
            });

            for arg in std::env::args().skip(1) {
                if handle_deeplink_url(&app.handle(), &arg, "argv") {
                    break;
                }
            }

            Ok(())
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
            refresh_route_credential_quota,
            refresh_route_credentials_quota,
            import_example_json,
            get_route_pool,
            set_route_pool_members,
            route_pool_route_once,
            route_pool_test_model,
            fetch_route_models,
            start_route_proxy,
            stop_route_proxy,
            get_route_proxy_status,
            write_route_proxy_configs,
            list_sessions,
            get_session_messages,
            list_target_apps,
            create_terminal_session,
            write_terminal_input,
            resize_terminal,
            kill_terminal_session,
            list_terminal_sessions,
            get_web_service_config,
            save_web_service_config,
            get_web_server_status,
            start_web_server,
            stop_web_server,
            get_tailscale_status,
            start_tailscale_login,
            start_tailscale_with_auth_key,
            disconnect_tailscale
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AI Switch");
}
