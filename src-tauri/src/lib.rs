mod adapters;
mod app_state;
mod commands;
mod config_writer;
mod core;
mod database;
mod error;
mod importers;
mod mcp;
mod models;
mod paths;
mod security;
pub mod server;
mod services;
mod session_manager;
mod skills;
mod terminal_manager;
mod web;

// rfd uses TaskDialogIndirect, which requires the Common Controls v6 manifest.
// Tauri links its generated resource into application binaries, but not lib tests.
#[cfg(all(test, target_os = "windows"))]
#[link(name = "resource", kind = "static")]
unsafe extern "C" {}

use app_state::{AppState, CloseToTrayRuntime};
use commands::batch_commands::{
    create_batch, create_official_account, create_provider, get_official_account,
    list_batch_groups, update_official_account,
};
use commands::disk_space_commands::get_disk_space_status;
use commands::external_client_import_commands::{
    import_external_client_accounts, preview_external_client_import,
};
use commands::import_commands::import_example_json;
use commands::platform_commands::list_platform_capabilities;
use commands::route_credential_commands::{
    archive_route_credentials, clear_route_credential_model_state, copy_route_credential,
    create_api_route_credential, delete_route_credential, get_route_credential,
    import_official_route_credentials_from_files, import_official_route_credentials_from_text,
    list_route_credentials, list_route_credentials_page, refresh_route_credential_quota,
    refresh_route_credential_relay_balance, refresh_route_credentials_quota,
    refresh_route_credentials_relay_balance, reorder_route_credentials, restore_route_credentials,
    set_route_credential_model_status, set_route_credential_recovery,
    set_route_credential_statuses, update_route_credential,
};
use commands::route_credential_transfer_commands::{
    export_route_credentials, import_route_credentials, preview_route_credential_import,
    save_route_credential_export,
};
use commands::route_pool_commands::{
    fetch_route_models, get_route_pool, route_pool_route_once, route_pool_test_model,
    set_route_pool_members, subscribe_route_proxy_live_log, unsubscribe_route_proxy_live_log,
};
use commands::route_proxy_commands::{
    get_route_proxy_key, get_route_proxy_status, route_config_write_is_stale, start_route_proxy,
    stop_route_proxy, write_route_proxy_configs,
};
use commands::route_proxy_https_commands::{
    delete_route_proxy_https_certificates, disable_route_proxy_https, enable_route_proxy_https,
    get_route_proxy_https_status, open_route_proxy_https_certificate_dir,
    regenerate_route_proxy_https_certificates, reimport_route_proxy_root_ca,
    uninstall_route_proxy_root_ca,
};
use commands::session_commands::{get_session_messages, list_sessions, open_session_terminal};
use commands::settings_commands::{get_settings, save_settings};
use commands::target_commands::{
    list_config_snapshots, list_config_write_clients, list_target_apps,
    list_target_config_statuses, rollback_config_snapshot,
};
use commands::terminal_commands::{
    create_terminal_session, kill_terminal_session, list_agent_launch_options,
    list_terminal_sessions, resize_terminal, write_terminal_input,
};
use commands::usage_stats_commands::{
    get_model_price_configs, get_session_usage_stats, get_usage_overview,
    reload_model_price_overrides, save_model_price_configs,
};
use commands::web_service_commands::{
    create_mobile_pairing, disconnect_tailscale, get_tailscale_status, get_web_server_status,
    get_web_service_config, save_web_service_config, start_tailscale_login,
    start_tailscale_with_auth_key, start_web_server, stop_web_server,
};
use database::open_migrated_pool;
use mcp::command::{
    mcp_get_marketplace_server_detail, mcp_install_from_marketplace, mcp_list_marketplaces,
    mcp_remove_server, mcp_scan_local, mcp_search_marketplace, mcp_set_server_apps,
    mcp_upsert_local_server,
};
use paths::AppPaths;
use services::config_write_service::ConfigWriteRuntimeState;
use services::deeplink_protocol_service::{
    DeepLinkProtocolRegistrar, DeepLinkProtocolRuntime, DeepLinkProtocolStatus, UNSUPPORTED_REASON,
};
use services::deeplink_service::{parse_deeplink_url, DeepLinkErrorPayload};
use services::route_proxy_https_service::RouteProxyHttpsService;
use services::route_proxy_service::RouteProxyRuntimeState;
use services::route_recovery_service::RouteRecoveryService;
use services::tailscale_service::{TailscaleRuntimeState, TailscaleService};
use services::web_service::{WebService, WebServiceRuntimeState};
use skills::command::{
    skills_delete, skills_install_package, skills_list, skills_list_agents, skills_list_packages,
    skills_read, skills_read_package, skills_save,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_deep_link::DeepLinkExt;
use terminal_manager::TerminalManager;
use web::event_bridge::{EventEmitter, WebEventBroadcaster};

const AUTOSTART_ARG: &str = "--autostart";

fn is_autostart_launch<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == AUTOSTART_ARG)
}

fn is_deeplink_url(app: &tauri::AppHandle, value: &str) -> bool {
    value.starts_with("aiswitch://")
        || (value.starts_with("ccswitch://")
            && app
                .state::<AppState>()
                .deeplink_protocols
                .ccswitch_enabled())
}

/// Hides the macOS Dock icon so a window closed to the tray leaves only the
/// tray item behind. Idempotent, and a no-op everywhere else.
///
/// Deliberately `set_activation_policy` rather than `set_dock_visibility`: the
/// latter goes through tao's `TransformProcessType`, which drops any hide that
/// lands within a second of a show — exactly the show-from-tray-then-close
/// sequence this feature is built on — and leaves a stale Dock tile behind.
fn hide_dock_icon(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        if let Err(error) = app.set_activation_policy(tauri::ActivationPolicy::Accessory) {
            eprintln!("failed to hide the Dock icon: {error}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Restores the macOS Dock icon when the main window comes back.
fn restore_dock_icon(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        if let Err(error) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
            eprintln!("failed to restore the Dock icon: {error}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        restore_dock_icon(app);
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
                let owns_scheme =
                    self.app
                        .deep_link()
                        .is_registered("ccswitch")
                        .map_err(|error| crate::error::AppError::Validation {
                            code: "capability.deeplink_compat_status",
                            message: "Could not inspect cc-switch deep link ownership".into(),
                            details: Some(error.to_string()),
                            recoverable: true,
                        })?;
                if owns_scheme {
                    self.app
                        .deep_link()
                        .unregister("ccswitch")
                        .map_err(|error| crate::error::AppError::Validation {
                            code: "capability.deeplink_compat_unregister",
                            message: "Could not unregister cc-switch deep link".into(),
                            details: Some(error.to_string()),
                            recoverable: true,
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
                    message: "cc-switch deep-link compatibility is unavailable on this runtime"
                        .into(),
                    details: None,
                    recoverable: true,
                })
            } else {
                Ok(())
            }
        }
    }
}

/// Record why the app cannot start, then exit.
///
/// Release builds set `windows_subsystem = "windows"` (see `main.rs`), so
/// panicking here shows the user nothing whatsoever: no console, no window, no
/// trace. The failures that reach this point are the ones most worth reporting —
/// a migration conflict deliberately leaves a populated database untouched and
/// explains that in its message — so the reason has to land somewhere findable.
fn report_fatal_startup_error(paths: &AppPaths, error: &crate::error::AppError) -> ! {
    let report = format!(
        "[{}] AI Switch could not start: {error} ({})\n{error:?}\n",
        chrono::Utc::now().to_rfc3339(),
        error.code(),
    );
    eprintln!("{report}");

    let _ = std::fs::create_dir_all(&paths.logs_dir);
    let log_path = paths.logs_dir.join("startup-error.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        use std::io::Write;
        let _ = file.write_all(report.as_bytes());
    }

    std::process::exit(1);
}

pub fn run() {
    let paths = AppPaths::resolve().expect("failed to resolve app paths");
    let pool = tauri::async_runtime::block_on(async {
        if let Err(error) = paths.ensure().await {
            report_fatal_startup_error(&paths, &error);
        }
        match open_migrated_pool(&paths.database_file, &paths.backups_dir).await {
            Ok(pool) => pool,
            Err(error) => report_fatal_startup_error(&paths, &error),
        }
    });

    let launched_from_autostart = is_autostart_launch(std::env::args().skip(1));
    // Read once here so the close handler, which runs on the main thread and
    // cannot await, has an answer from the very first click. Unreadable settings
    // fall back to the tray, which is what the app did before this was an option.
    let close_to_tray = CloseToTrayRuntime::new(
        tauri::async_runtime::block_on(services::settings_service::SettingsService::load(&paths))
            .map(|settings| settings.close_to_tray)
            .unwrap_or(true),
    );
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
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args([AUTOSTART_ARG])
                .app_name("AI Switch")
                .build(),
        )
        .on_window_event(move |window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Tray "quit" closes the window for real; the close button is
                // governed by the setting.
                if close_tray_quit_requested.load(Ordering::SeqCst) {
                    return;
                }
                let app = window.app_handle();
                if app.state::<AppState>().close_to_tray.enabled() {
                    api.prevent_close();
                    let _ = window.hide();
                    hide_dock_icon(app);
                } else {
                    // Route through the same exit the tray menu uses, so the
                    // route proxy and terminal children get torn down instead of
                    // being orphaned by a bare window close. The flag also keeps
                    // this branch from re-entering on the way out.
                    close_tray_quit_requested.store(true, Ordering::SeqCst);
                    app.exit(0);
                }
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .manage(AppState {
            paths,
            pool,
            config_writes: ConfigWriteRuntimeState::default(),
            deeplink_protocols: DeepLinkProtocolRuntime::default(),
            close_to_tray,
            route_proxy: RouteProxyRuntimeState::default(),
            web_service: WebServiceRuntimeState::default(),
            tailscale: TailscaleRuntimeState::default(),
            terminals: TerminalManager::default(),
            terminal_hub: Arc::new(crate::web::terminal_hub::TerminalHub::default()),
            event_broadcaster: Arc::new(WebEventBroadcaster::new()),
        })
        .setup(move |app| {
            if launched_from_autostart {
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(error) = window.hide() {
                        eprintln!("failed to hide window for autostart launch: {error}");
                    }
                }
                // Started straight into the tray, so the Dock icon should not be
                // the one thing that shows up — same rule the close button follows.
                if app.state::<AppState>().close_to_tray.enabled() {
                    hide_dock_icon(app.handle());
                }
            }

            let show_item = MenuItemBuilder::with_id("tray-show", "显示主窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("tray-quit", "退出 AI Switch").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;
            let tray = app.tray_by_id("main").ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "AI Switch tray icon is unavailable",
                )
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
            state
                .route_proxy
                .activity()
                .set_emitter(EventEmitter::Tauri(app.handle().clone()));
            state
                .route_proxy
                .live_log()
                .set_emitter(EventEmitter::Tauri(app.handle().clone()));
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                state
                    .deeplink_protocols
                    .attach_registrar(Arc::new(TauriDeepLinkRegistrar {
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
                services::route_proxy_https_service::restore_auto_started_proxy(&route_proxy_state)
                    .await;
            });

            // Auto-recovery scheduler: periodically re-enable accounts per their
            // configured recovery rule (scheduled times / health-check probe).
            let recovery_state = app.state::<AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                RouteRecoveryService::run_loop(
                    recovery_state.pool.clone(),
                    recovery_state.route_proxy.activity(),
                )
                .await;
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
            get_disk_space_status,
            list_route_credentials,
            list_route_credentials_page,
            reorder_route_credentials,
            get_route_credential,
            create_api_route_credential,
            copy_route_credential,
            set_route_credential_recovery,
            set_route_credential_model_status,
            clear_route_credential_model_state,
            import_official_route_credentials_from_text,
            import_official_route_credentials_from_files,
            update_route_credential,
            delete_route_credential,
            archive_route_credentials,
            restore_route_credentials,
            set_route_credential_statuses,
            refresh_route_credential_quota,
            refresh_route_credentials_quota,
            refresh_route_credential_relay_balance,
            refresh_route_credentials_relay_balance,
            export_route_credentials,
            preview_route_credential_import,
            import_route_credentials,
            save_route_credential_export,
            preview_external_client_import,
            import_external_client_accounts,
            import_example_json,
            get_route_pool,
            set_route_pool_members,
            route_pool_route_once,
            route_pool_test_model,
            fetch_route_models,
            subscribe_route_proxy_live_log,
            unsubscribe_route_proxy_live_log,
            start_route_proxy,
            stop_route_proxy,
            get_route_proxy_status,
            get_route_proxy_key,
            write_route_proxy_configs,
            route_config_write_is_stale,
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
            open_session_terminal,
            get_model_price_configs,
            save_model_price_configs,
            get_session_usage_stats,
            get_usage_overview,
            reload_model_price_overrides,
            list_target_apps,
            list_target_config_statuses,
            list_config_write_clients,
            list_config_snapshots,
            rollback_config_snapshot,
            create_terminal_session,
            write_terminal_input,
            resize_terminal,
            kill_terminal_session,
            list_terminal_sessions,
            list_agent_launch_options,
            get_web_service_config,
            save_web_service_config,
            get_web_server_status,
            start_web_server,
            stop_web_server,
            get_tailscale_status,
            create_mobile_pairing,
            start_tailscale_login,
            start_tailscale_with_auth_key,
            disconnect_tailscale,
            mcp_scan_local,
            mcp_list_marketplaces,
            mcp_search_marketplace,
            mcp_get_marketplace_server_detail,
            mcp_install_from_marketplace,
            mcp_upsert_local_server,
            mcp_set_server_apps,
            mcp_remove_server,
            skills_list_agents,
            skills_list,
            skills_read,
            skills_save,
            skills_delete,
            skills_list_packages,
            skills_read_package,
            skills_install_package
        ])
        .build(tauri::generate_context!())
        .expect("failed to build AI Switch")
        .run(|app_handle, event| {
            // The tray-quit path calls `app.exit(0)`, which terminates without
            // dropping Tauri-managed state — so neither `kill_on_drop` nor any
            // `Drop` impl runs and child processes are left behind. This is the
            // only hook where we still get to end them.
            //
            // Note this is a backstop, not the whole fix: the updater's
            // `relaunch()` goes through `AppHandle::restart`, which documents
            // that it may skip these events entirely. The sidecar's stdin
            // watchdog is what covers that path.
            if let RunEvent::Exit = event {
                let state = app_handle.state::<AppState>();
                tauri::async_runtime::block_on(TailscaleService::shutdown(&state.tailscale));
                state.terminals.kill_all();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::is_autostart_launch;

    #[test]
    fn recognizes_the_exact_autostart_argument() {
        assert!(is_autostart_launch([
            "ai-switch".to_string(),
            "--autostart".to_string(),
        ]));
    }

    #[test]
    fn ignores_normal_and_similar_arguments() {
        assert!(!is_autostart_launch([
            "ai-switch".to_string(),
            "--autostart=true".to_string(),
            "--auto-start".to_string(),
        ]));
    }
}
