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

// rfd uses TaskDialogIndirect, which requires the Common Controls v6 manifest.
// Tauri links its generated resource into application binaries, but not lib tests.
#[cfg(all(test, target_os = "windows"))]
#[link(name = "resource", kind = "static")]
unsafe extern "C" {}

use app_state::AppState;
use commands::batch_commands::{
    create_batch, create_official_account, create_provider, get_official_account,
    list_batch_groups, update_official_account,
};
use commands::import_commands::import_example_json;
use commands::platform_commands::list_platform_capabilities;
use commands::route_credential_commands::{
    archive_route_credentials, copy_route_credential, create_api_route_credential,
    delete_route_credential,
    get_route_credential, import_official_route_credentials_from_files,
    import_official_route_credentials_from_text, list_route_credentials,
    list_route_credentials_page, reorder_route_credentials,
    refresh_route_credential_quota, refresh_route_credentials_quota, update_route_credential,
    restore_route_credentials,
};
use commands::route_credential_transfer_commands::{
    export_route_credentials, import_route_credentials, preview_route_credential_import,
    save_route_credential_export,
};
use commands::route_pool_commands::{
    fetch_route_models, get_route_pool, route_pool_route_once, route_pool_test_model,
    set_route_pool_members,
};
use commands::route_proxy_commands::{
    get_route_proxy_key, get_route_proxy_status, start_route_proxy, stop_route_proxy,
    write_route_proxy_configs,
};
use commands::route_proxy_https_commands::{
    delete_route_proxy_https_certificates, disable_route_proxy_https, enable_route_proxy_https,
    get_route_proxy_https_status, open_route_proxy_https_certificate_dir,
    regenerate_route_proxy_https_certificates, reimport_route_proxy_root_ca,
    uninstall_route_proxy_root_ca,
};
use commands::session_commands::{get_session_messages, list_sessions};
use commands::settings_commands::{get_settings, save_settings};
use commands::target_commands::{
    list_config_snapshots, list_target_apps, list_target_config_statuses, rollback_config_snapshot,
};
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
use services::config_write_service::ConfigWriteRuntimeState;
use services::deeplink_protocol_service::{
    DeepLinkProtocolRegistrar, DeepLinkProtocolRuntime, DeepLinkProtocolStatus,
    UNSUPPORTED_REASON,
};
use services::deeplink_service::{parse_deeplink_url, DeepLinkErrorPayload};
use services::route_proxy_service::RouteProxyRuntimeState;
use services::route_proxy_https_service::RouteProxyHttpsService;
use services::tailscale_service::TailscaleRuntimeState;
use services::web_service::{WebService, WebServiceRuntimeState};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_deep_link::DeepLinkExt;
use terminal_manager::TerminalManager;
use web::event_bridge::WebEventBroadcaster;

fn is_deeplink_url(app: &tauri::AppHandle, value: &str) -> bool {
    value.starts_with("aiswitch://")
        || (value.starts_with("ccswitch://")
            && app
                .state::<AppState>()
                .deeplink_protocols
                .ccswitch_enabled())
}

fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn handle_deeplink_url(app: &tauri::AppHandle, url_str: &str, source: &str) -> bool {
    if !is_deeplink_url(app, url_str) {
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

struct TauriDeepLinkRegistrar {
    app: tauri::AppHandle,
}

impl DeepLinkProtocolRegistrar for TauriDeepLinkRegistrar {
    fn status(&self) -> DeepLinkProtocolStatus {
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        {
            match self.app.deep_link().is_registered("ccswitch") {
                Ok(registered) => DeepLinkProtocolStatus {
                    supported: true,
                    ccswitch_registered: registered,
                    reason: None,
                },
                Err(error) => DeepLinkProtocolStatus {
                    supported: false,
                    ccswitch_registered: false,
                    reason: Some(error.to_string()),
                },
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            DeepLinkProtocolStatus {
                supported: false,
                ccswitch_registered: false,
                reason: Some(UNSUPPORTED_REASON.to_string()),
            }
        }
    }

    fn set_ccswitch_enabled(&self, enabled: bool) -> Result<(), crate::error::AppError> {
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        {
            if enabled {
                self.app.deep_link().register("ccswitch").map_err(|error| {
                    crate::error::AppError::Validation {
                        code: "capability.deeplink_compat_register",
                        message: "Could not register cc-switch deep link".into(),
                        details: Some(error.to_string()),
                        recoverable: true,
                    }
                })
            } else {
                let owns_scheme = self.app.deep_link().is_registered("ccswitch").map_err(|error| {
                    crate::error::AppError::Validation {
                        code: "capability.deeplink_compat_status",
                        message: "Could not inspect cc-switch deep link ownership".into(),
                        details: Some(error.to_string()),
                        recoverable: true,
                    }
                })?;
                if owns_scheme {
                    self.app.deep_link().unregister("ccswitch").map_err(|error| {
                        crate::error::AppError::Validation {
                            code: "capability.deeplink_compat_unregister",
                            message: "Could not unregister cc-switch deep link".into(),
                            details: Some(error.to_string()),
                            recoverable: true,
                        }
                    })?;
                }
                Ok(())
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            if enabled {
                Err(crate::error::AppError::Validation {
                    code: UNSUPPORTED_REASON,
                    message: "cc-switch deep-link compatibility is unavailable on this runtime".into(),
                    details: None,
                    recoverable: true,
                })
            } else {
                Ok(())
            }
        }
    }
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
    let tray_quit_requested = Arc::new(AtomicBool::new(false));
    let close_tray_quit_requested = Arc::clone(&tray_quit_requested);

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
        .on_window_event(move |window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if !close_tray_quit_requested.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .manage(AppState {
            paths,
            pool,
            config_writes: ConfigWriteRuntimeState::default(),
            deeplink_protocols: DeepLinkProtocolRuntime::default(),
            route_proxy: RouteProxyRuntimeState::default(),
            web_service: WebServiceRuntimeState::default(),
            tailscale: TailscaleRuntimeState::default(),
            terminals: TerminalManager::default(),
            event_broadcaster: Arc::new(WebEventBroadcaster::new()),
        })
        .setup(move |app| {
            let show_item = MenuItemBuilder::with_id("tray-show", "显示主窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("tray-quit", "退出 AI Switch").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;
            let tray = app.tray_by_id("main").ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "AI Switch tray icon is unavailable")
            })?;
            tray.set_menu(Some(tray_menu))?;
            tray.on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    focus_main_window(tray.app_handle());
                }
            });
            let tray_quit_requested = Arc::clone(&tray_quit_requested);
            app.on_menu_event(move |app, event| match event.id().as_ref() {
                "tray-show" => focus_main_window(app),
                "tray-quit" => {
                    tray_quit_requested.store(true, Ordering::SeqCst);
                    app.exit(0);
                }
                _ => {}
            });

            let state = app.state::<AppState>().inner().clone();
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                state.deeplink_protocols.attach_registrar(Arc::new(TauriDeepLinkRegistrar {
                    app: app.handle().clone(),
                }));
                if let Ok(settings) = tauri::async_runtime::block_on(
                    services::settings_service::SettingsService::load(&state.paths),
                ) {
                    if settings.ccswitch_deeplink_compat_enabled {
                        let _ = state.deeplink_protocols.set_ccswitch_enabled(true);
                    }
                }
            }
            tauri::async_runtime::spawn(async move {
                let Ok(config) = WebService::load_config(&state.paths).await else {
                    return;
                };
                if !config.auto_start {
                    return;
                }
                let _ = WebService::start(Arc::new(state)).await;
            });

            let route_proxy_state = app.state::<AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                let Ok(config) = RouteProxyHttpsService::load_config(&route_proxy_state.paths).await else {
                    return;
                };
                if !config.auto_start {
                    return;
                }
                if let Err(error) = RouteProxyHttpsService::start_proxy(&route_proxy_state).await {
                    eprintln!("failed to restore route proxy: {error}");
                }
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
            list_platform_capabilities,
            list_route_credentials,
            list_route_credentials_page,
            reorder_route_credentials,
            get_route_credential,
            create_api_route_credential,
            copy_route_credential,
            import_official_route_credentials_from_text,
            import_official_route_credentials_from_files,
            update_route_credential,
            delete_route_credential,
            archive_route_credentials,
            restore_route_credentials,
            refresh_route_credential_quota,
            refresh_route_credentials_quota,
            export_route_credentials,
            preview_route_credential_import,
            import_route_credentials,
            save_route_credential_export,
            import_example_json,
            get_route_pool,
            set_route_pool_members,
            route_pool_route_once,
            route_pool_test_model,
            fetch_route_models,
            start_route_proxy,
            stop_route_proxy,
            get_route_proxy_status,
            get_route_proxy_key,
            write_route_proxy_configs,
            get_route_proxy_https_status,
            enable_route_proxy_https,
            disable_route_proxy_https,
            reimport_route_proxy_root_ca,
            regenerate_route_proxy_https_certificates,
            uninstall_route_proxy_root_ca,
            delete_route_proxy_https_certificates,
            open_route_proxy_https_certificate_dir,
            list_sessions,
            get_session_messages,
            list_target_apps,
            list_target_config_statuses,
            list_config_snapshots,
            rollback_config_snapshot,
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
