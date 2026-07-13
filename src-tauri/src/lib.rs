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
mod tray;

use app_state::AppState;
use commands::automation_commands::{
    create_bulk_operation, create_item_tag, create_plugin_link, create_tag, list_bulk_operations,
    list_item_tags, list_plugin_links, list_tags, set_plugin_link_enabled,
};
use commands::batch_commands::{
    create_batch, create_official_account, create_provider, list_batch_groups,
    list_official_account_statuses, list_official_accounts, record_official_account_quota_snapshot,
    refresh_official_account_quota_snapshot,
};
use commands::import_commands::{
    export_example_json, import_deep_link, import_example_json, import_official_account_json,
};
use commands::instance_commands::{create_instance, list_instances, set_instance_status};
use commands::mcp_commands::{create_mcp_server, list_mcp_servers, set_mcp_server_enabled};
use commands::prompt_asset_commands::{
    create_prompt_asset, list_prompt_assets, set_prompt_asset_enabled,
};
use commands::provider_commands::{
    create_provider_from_preset, list_provider_presets, list_providers, switch_target_provider,
};
use commands::routing_commands::{
    create_failover_policy, create_proxy_profile, create_usage_event, list_failover_policies,
    list_proxy_profiles, list_usage_events,
};
use commands::session_commands::{
    create_session, create_session_event, list_session_events, list_sessions, set_session_status,
};
use commands::settings_commands::{get_settings, save_settings};
use commands::sync_commands::{
    create_sync_profile, create_sync_snapshot, list_sync_profiles, list_sync_snapshots,
};
use commands::target_commands::{
    list_target_apps, list_target_switch_statuses, rollback_config_snapshot,
};
use commands::tray_commands::refresh_tray_menu;
use commands::updater_commands::{
    create_update_channel, create_update_check, list_update_channels, list_update_checks,
};
use commands::wakeup_commands::{
    create_wakeup_run, create_wakeup_task, list_wakeup_runs, list_wakeup_tasks,
    set_wakeup_task_enabled,
};
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
        .setup(|app| {
            let handle = app.handle().clone();
            if let Err(error) = tauri::async_runtime::block_on(tray::setup_tray(&handle)) {
                eprintln!("AI Switch tray setup failed: {error}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            list_tags,
            create_tag,
            list_item_tags,
            create_item_tag,
            list_plugin_links,
            create_plugin_link,
            set_plugin_link_enabled,
            list_bulk_operations,
            create_bulk_operation,
            create_batch,
            list_batch_groups,
            create_provider,
            create_official_account,
            list_official_accounts,
            list_official_account_statuses,
            record_official_account_quota_snapshot,
            refresh_official_account_quota_snapshot,
            import_example_json,
            import_official_account_json,
            import_deep_link,
            export_example_json,
            list_mcp_servers,
            create_mcp_server,
            set_mcp_server_enabled,
            list_prompt_assets,
            create_prompt_asset,
            set_prompt_asset_enabled,
            list_proxy_profiles,
            create_proxy_profile,
            list_failover_policies,
            create_failover_policy,
            list_usage_events,
            create_usage_event,
            list_sync_profiles,
            create_sync_profile,
            list_sync_snapshots,
            create_sync_snapshot,
            list_sessions,
            create_session,
            set_session_status,
            list_session_events,
            create_session_event,
            list_update_channels,
            create_update_channel,
            list_update_checks,
            create_update_check,
            list_instances,
            create_instance,
            set_instance_status,
            list_wakeup_tasks,
            create_wakeup_task,
            set_wakeup_task_enabled,
            list_wakeup_runs,
            create_wakeup_run,
            list_target_apps,
            list_target_switch_statuses,
            rollback_config_snapshot,
            list_providers,
            list_provider_presets,
            create_provider_from_preset,
            switch_target_provider,
            refresh_tray_menu
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AI Switch");
}
