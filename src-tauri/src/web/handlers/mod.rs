use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::commands::batch_commands::{CreateAccountRequest, UpdateAccountRequest};
use crate::core::sessions::{get_session_messages_core, list_sessions_core};
use crate::core::settings::{get_settings_core, save_settings_core};
use crate::core::terminals::{
    create_terminal_session_core, kill_terminal_session_core, list_terminal_sessions_core,
    resize_terminal_core, resume_session_terminal_core, write_terminal_input_core,
};
use crate::core::usage_overview::get_usage_overview_core;
use crate::core::usage_stats::{
    get_model_price_configs_core, get_session_usage_stats_core, reload_model_price_overrides_core,
    save_model_price_configs_core,
};
use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
use crate::error::{ApiError, AppError};
use crate::models::batch::NewBatch;
use crate::models::external_client_import::{
    ImportExternalClientAccountsInput, PreviewExternalClientImportInput,
};
use crate::models::platform::PlatformId;
use crate::models::route_credential::{
    CopyRouteCredentialInput, CreateApiRouteCredentialInput, ImportOfficialFilesInput,
    ImportOfficialTextInput, ReorderRouteCredentialInput, RouteCredentialPageRequest,
    UpdateRouteCredentialInput, MASKED_SECRET_PAYLOAD,
};
use crate::models::route_credential_transfer::{
    ExportRouteCredentialsInput, ImportRouteCredentialsInput, PreviewRouteCredentialImportInput,
};
use crate::models::route_pool::{
    RouteModelsFetchRequest, RoutePoolModelTestRequest, RoutePoolRouteRequest,
    SetRoutePoolMembersInput,
};
use crate::models::settings::AppSettings;
use crate::services::agent_launch_service::AgentLaunchService;
use crate::services::batch_service::BatchService;
use crate::services::config_write_service::ConfigWriteCoordinator;
use crate::services::external_client_import_service;
use crate::services::import_service::{ExampleJsonImportRequest, ImportService};
use crate::services::model_pricing::ModelPriceConfig;
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::route_config_service::RouteConfigService;
use crate::services::route_credential_service::RouteCredentialService;
use crate::services::route_credential_transfer_service;
use crate::services::route_model_fetch_service::RouteModelFetchService;
use crate::services::route_model_test_service::RouteModelTestService;
use crate::services::route_pool_service::RoutePoolService;
use crate::services::route_proxy_https_service::RouteProxyHttpsService;
use crate::services::route_proxy_service::RouteProxyService;
use crate::services::route_quota_service::RouteQuotaService;
use crate::services::route_recovery_service::{RecoveryRule, RouteRecoveryService};
use crate::services::route_relay_balance_service::RouteRelayBalanceService;
use crate::services::target_service::TargetService;
use crate::services::web_service::{WebService, WebServiceConfig};
use crate::terminal_manager::CreateTerminalSessionInput;
use crate::web::event_bridge::EventEmitter;
use std::collections::HashMap;

pub fn is_sensitive_command(command: &str) -> bool {
    matches!(
        command,
        "export_route_credentials"
            | "preview_route_credential_import"
            | "import_route_credentials"
            | "preview_external_client_import"
            | "import_external_client_accounts"
            | "get_route_proxy_key"
            | "mcp_install_from_marketplace"
            | "mcp_upsert_local_server"
            | "mcp_set_server_apps"
            | "mcp_remove_server"
            | "skills_save"
            | "skills_delete"
            | "skills_install_package"
            | "create_mobile_pairing"
            | "create_terminal_session"
            | "get_web_service_config"
            | "save_web_service_config"
            | "get_web_server_status"
            | "start_web_server"
            | "stop_web_server"
            | "get_tailscale_status"
            | "start_tailscale_login"
            | "start_tailscale_with_auth_key"
            | "disconnect_tailscale"
    )
}

/// Projects stored secrets out of the two credential-listing responses.
///
/// Neither command is sensitive, and they should not become sensitive — the
/// paired phone needs the account list. But both serialize `secret_payload_json`
/// verbatim, so without this a 30-day pairing token reads every account's
/// plaintext api_key through an ordinary command, while `export_route_credentials`
/// returns 401 to that same token for that same data.
pub fn mask_listed_secret_payloads(command: &str, value: &mut Value) {
    let rows = match command {
        "list_route_credentials" => value.as_array_mut(),
        "list_route_credentials_page" => value.get_mut("items").and_then(Value::as_array_mut),
        _ => return,
    };
    let Some(rows) = rows else {
        return;
    };
    for row in rows {
        if let Some(payload) = row.get_mut("secret_payload_json") {
            *payload = Value::String(MASKED_SECRET_PAYLOAD.to_string());
        }
    }
}

pub async fn dispatch_command(
    state: Arc<AppState>,
    command: &str,
    args: Value,
) -> Result<Value, ApiError> {
    match command {
        "health" => to_value(json!({ "ok": true })),
        "mcp_scan_local" => to_value(crate::mcp::command::mcp_scan_local().await?),
        "mcp_list_marketplaces" => to_value(crate::mcp::command::mcp_list_marketplaces().await?),
        "mcp_search_marketplace" => {
            let provider_id = required_string_arg(&args, "providerId")?;
            let query = optional_string_arg(&args, "query")?;
            let limit = optional_i64_arg(&args, "limit")?
                .map(|value| {
                    u32::try_from(value).map_err(|_| {
                        invalid_argument("limit", Some("expected non-negative integer".to_string()))
                    })
                })
                .transpose()?;
            to_value(crate::mcp::command::mcp_search_marketplace(provider_id, query, limit).await?)
        }
        "mcp_get_marketplace_server_detail" => {
            let provider_id = required_string_arg(&args, "providerId")?;
            let server_id = required_string_arg(&args, "serverId")?;
            to_value(
                crate::mcp::command::mcp_get_marketplace_server_detail(provider_id, server_id)
                    .await?,
            )
        }
        "mcp_install_from_marketplace" => {
            let provider_id = required_string_arg(&args, "providerId")?;
            let server_id = required_string_arg(&args, "serverId")?;
            let apps = parse_arg(&args, "apps")?;
            let option_id = optional_string_arg(&args, "optionId")?;
            let protocol = optional_string_arg(&args, "protocol")?;
            let parameter_values = args.get("parameterValues").cloned();
            to_value(
                crate::mcp::command::mcp_install_from_marketplace(
                    provider_id,
                    server_id,
                    apps,
                    option_id,
                    protocol,
                    parameter_values,
                )
                .await?,
            )
        }
        "mcp_upsert_local_server" => {
            let server_id = required_string_arg(&args, "serverId")?;
            let spec: Value = parse_arg(&args, "spec")?;
            let apps = parse_arg(&args, "apps")?;
            to_value(crate::mcp::command::mcp_upsert_local_server(server_id, spec, apps).await?)
        }
        "mcp_set_server_apps" => {
            let server_id = required_string_arg(&args, "serverId")?;
            let apps = parse_arg(&args, "apps")?;
            to_value(crate::mcp::command::mcp_set_server_apps(server_id, apps).await?)
        }
        "mcp_remove_server" => {
            let server_id = required_string_arg(&args, "serverId")?;
            let apps = args
                .get("apps")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| invalid_argument("apps", Some(error.to_string())))?;
            to_value(crate::mcp::command::mcp_remove_server(server_id, apps).await?)
        }
        "skills_list_agents" => to_value(crate::skills::command::skills_list_agents().await?),
        "skills_list" => {
            let agent_type = parse_arg(&args, "agentType")?;
            let scope = parse_arg(&args, "scope")?;
            let workspace_path = optional_string_arg(&args, "workspacePath")?;
            to_value(crate::skills::command::skills_list(agent_type, scope, workspace_path).await?)
        }
        "skills_list_packages" => {
            let agent_type = parse_arg(&args, "agentType")?;
            let scope = parse_arg(&args, "scope")?;
            let workspace_path = optional_string_arg(&args, "workspacePath")?;
            to_value(
                crate::skills::command::skills_list_packages(agent_type, scope, workspace_path)
                    .await?,
            )
        }
        "skills_read_package" => {
            let package_id = required_string_arg(&args, "packageId")?;
            let agent_type = parse_arg(&args, "agentType")?;
            let scope = parse_arg(&args, "scope")?;
            let workspace_path = optional_string_arg(&args, "workspacePath")?;
            to_value(
                crate::skills::command::skills_read_package(
                    package_id,
                    agent_type,
                    scope,
                    workspace_path,
                )
                .await?,
            )
        }
        "skills_install_package" => {
            let package_id = required_string_arg(&args, "packageId")?;
            let agent_type = parse_arg(&args, "agentType")?;
            let scope = parse_arg(&args, "scope")?;
            let workspace_path = optional_string_arg(&args, "workspacePath")?;
            to_value(
                crate::skills::command::skills_install_package(
                    package_id,
                    agent_type,
                    scope,
                    workspace_path,
                )
                .await?,
            )
        }
        "skills_read" => {
            let agent_type = parse_arg(&args, "agentType")?;
            let scope = parse_arg(&args, "scope")?;
            let skill_id = required_string_arg(&args, "skillId")?;
            let workspace_path = optional_string_arg(&args, "workspacePath")?;
            to_value(
                crate::skills::command::skills_read(agent_type, scope, skill_id, workspace_path)
                    .await?,
            )
        }
        "skills_save" => {
            let agent_type = parse_arg(&args, "agentType")?;
            let scope = parse_arg(&args, "scope")?;
            let skill_id = required_string_arg(&args, "skillId")?;
            let content = required_raw_string_arg(&args, "content")?;
            let layout = args
                .get("layout")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| invalid_argument("layout", Some(error.to_string())))?;
            let workspace_path = optional_string_arg(&args, "workspacePath")?;
            to_value(
                crate::skills::command::skills_save(
                    agent_type,
                    scope,
                    skill_id,
                    content,
                    layout,
                    workspace_path,
                )
                .await?,
            )
        }
        "skills_delete" => {
            let agent_type = parse_arg(&args, "agentType")?;
            let scope = parse_arg(&args, "scope")?;
            let skill_id = required_string_arg(&args, "skillId")?;
            let workspace_path = optional_string_arg(&args, "workspacePath")?;
            to_value(
                crate::skills::command::skills_delete(agent_type, scope, skill_id, workspace_path)
                    .await?,
            )
        }
        "list_batch_groups" => {
            let search = optional_string_arg(&args, "search")?;
            to_value(
                BatchService::list_groups(&state.pool, search)
                    .await
                    .map_err(to_error)?,
            )
        }
        "create_batch" => {
            let input: NewBatch = parse_arg(&args, "input")?;
            to_value(
                BatchService::create_batch(&state.pool, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "create_official_account" => {
            let request: CreateAccountRequest = parse_arg(&args, "request")?;
            to_value(
                BatchService::create_official_account(
                    &state.pool,
                    request.account,
                    request.batch_id,
                )
                .await
                .map_err(to_error)?,
            )
        }
        "get_official_account" => {
            let id = required_string_arg(&args, "id")?;
            to_value(
                BatchService::get_official_account(&state.pool, id)
                    .await
                    .map_err(to_error)?,
            )
        }
        "update_official_account" => {
            let input: UpdateAccountRequest = parse_arg(&args, "input")?;
            to_value(
                BatchService::update_official_account(&state.pool, input.id, input.account)
                    .await
                    .map_err(to_error)?,
            )
        }
        "import_example_json" => {
            let request: ExampleJsonImportRequest = parse_arg(&args, "request")?;
            to_value(
                ImportService::import_example_json(&state.pool, request)
                    .await
                    .map_err(to_error)?,
            )
        }
        "list_platform_capabilities" => to_value(PlatformCapabilityService::list()),
        "list_target_apps" => to_value(
            TargetService::list_targets(&state.pool)
                .await
                .map_err(to_error)?,
        ),
        "list_target_config_statuses" => to_value(
            TargetService::list_config_statuses(&state.pool, &state.config_writes)
                .await
                .map_err(to_error)?,
        ),
        "list_config_write_clients" => {
            let platform = required_string_arg(&args, "platform")?;
            let platform = PlatformId::parse(&platform).map_err(to_error)?;
            to_value(
                TargetService::list_config_write_clients(&state.pool, platform)
                    .await
                    .map_err(to_error)?,
            )
        }
        "list_config_snapshots" => {
            let target_app_id = optional_string_arg(&args, "targetAppId")?;
            let limit = optional_i64_arg(&args, "limit")?
                .unwrap_or(50)
                .clamp(1, 200);
            ConfigWriteCoordinator::reconcile_prepared(&state.pool, &state.config_writes)
                .await
                .map_err(to_error)?;
            to_value(
                ConfigSnapshotRepository::list(&state.pool, target_app_id.as_deref(), limit)
                    .await
                    .map_err(to_error)?,
            )
        }
        "rollback_config_snapshot" => {
            let id = required_string_arg(&args, "id")?;
            to_value(
                ConfigWriteCoordinator::rollback(
                    &state.paths,
                    &state.pool,
                    &state.config_writes,
                    &id,
                )
                .await
                .map_err(to_error)?,
            )
        }
        "get_settings" => to_value(
            get_settings_core(&state.paths, &state.deeplink_protocols)
                .await
                .map_err(to_error)?,
        ),
        "save_settings" => {
            let settings: AppSettings = parse_arg(&args, "settings")?;
            to_value(
                save_settings_core(&state.paths, &state.deeplink_protocols, settings)
                    .await
                    .map_err(to_error)?,
            )
        }
        "list_sessions" => {
            let platform = optional_string_arg(&args, "platform")?;
            to_value(
                list_sessions_core(platform)
                    .await
                    .map_err(|message| command_error("web.session_scan", message))?,
            )
        }
        "get_session_usage_stats" => {
            let since = optional_string_arg(&args, "since")?;
            to_value(
                get_session_usage_stats_core(since)
                    .await
                    .map_err(to_error)?,
            )
        }
        "reload_model_price_overrides" => to_value(
            reload_model_price_overrides_core()
                .await
                .map_err(to_error)?,
        ),
        "get_model_price_configs" => {
            to_value(get_model_price_configs_core().await.map_err(to_error)?)
        }
        "save_model_price_configs" => {
            let configs: HashMap<String, ModelPriceConfig> = parse_arg(&args, "configs")?;
            to_value(
                save_model_price_configs_core(configs)
                    .await
                    .map_err(to_error)?,
            )
        }
        "get_usage_overview" => {
            let since = optional_string_arg(&args, "since")?;
            let page = optional_i64_arg(&args, "page")?;
            let page_size = optional_i64_arg(&args, "page_size")?;
            // The browser's own offset, so a phone in another timezone gets its
            // buckets cut at its midnight rather than the server's.
            let utc_offset_minutes = optional_i64_arg(&args, "utc_offset_minutes")?
                .and_then(|value| i32::try_from(value).ok());
            to_value(
                get_usage_overview_core(&state.pool, since, page, page_size, utc_offset_minutes)
                    .await
                    .map_err(to_error)?,
            )
        }
        "get_session_messages" => {
            let provider_id = required_string_arg(&args, "providerId")?;
            let source_path = required_string_arg(&args, "sourcePath")?;
            to_value(
                get_session_messages_core(provider_id, source_path)
                    .await
                    .map_err(|message| command_error("web.session_read", message))?,
            )
        }
        "create_terminal_session" => {
            let input: CreateTerminalSessionInput = parse_arg(&args, "input")?;
            to_value(
                create_terminal_session_core(
                    &state.terminals,
                    EventEmitter::Web(Arc::clone(&state.event_broadcaster)),
                    Arc::clone(&state.terminal_hub),
                    input,
                )
                .map_err(|message| command_error("web.terminal_create", message))?,
            )
        }
        "resume_session_terminal" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            let cols = required_u16_arg(&args, "cols")?;
            let rows = required_u16_arg(&args, "rows")?;
            to_value(
                resume_session_terminal_core(
                    &state.terminals,
                    EventEmitter::Web(Arc::clone(&state.event_broadcaster)),
                    Arc::clone(&state.terminal_hub),
                    &session_id,
                    cols,
                    rows,
                )
                .await
                .map_err(|message| command_error("web.terminal_resume", message))?,
            )
        }
        "write_terminal_input" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            require_terminal_subscriber(&state, &session_id)?;
            let data = required_raw_string_arg(&args, "data")?;
            write_terminal_input_core(&state.terminals, &session_id, &data)
                .map_err(|message| command_error("web.terminal_write", message))?;
            to_value(())
        }
        "resize_terminal" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            require_terminal_subscriber(&state, &session_id)?;
            let cols = required_u16_arg(&args, "cols")?;
            let rows = required_u16_arg(&args, "rows")?;
            resize_terminal_core(&state.terminals, &session_id, cols, rows)
                .map_err(|message| command_error("web.terminal_resize", message))?;
            to_value(())
        }
        "kill_terminal_session" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            require_terminal_subscriber(&state, &session_id)?;
            kill_terminal_session_core(&state.terminals, &session_id)
                .map_err(|message| command_error("web.terminal_kill", message))?;
            to_value(())
        }
        "list_terminal_sessions" => to_value(list_terminal_sessions_core(&state.terminals)),
        "list_agent_launch_options" => {
            to_value(AgentLaunchService::list_options(&state.pool).await?)
        }
        "list_route_credentials" => {
            let platform = required_string_arg(&args, "platform")?;
            let activity = state.route_proxy.activity();
            to_value(
                RouteCredentialService::list_with_activity(&state.pool, &activity, platform)
                    .await
                    .map_err(to_error)?,
            )
        }
        "export_route_credentials" => {
            let input: ExportRouteCredentialsInput = parse_arg(&args, "input")?;
            to_value(
                route_credential_transfer_service::export_route_credentials(&state.pool, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "list_route_credentials_page" => {
            let input: RouteCredentialPageRequest = parse_arg(&args, "input")?;
            let activity = state.route_proxy.activity();
            to_value(
                RouteCredentialService::page_with_activity(&state.pool, &activity, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "reorder_route_credentials" => {
            let input: ReorderRouteCredentialInput = parse_arg(&args, "input")?;
            to_value(
                RouteCredentialService::reorder(&state.pool, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "get_route_credential" => {
            let id = required_string_arg(&args, "id")?;
            let activity = state.route_proxy.activity();
            to_value(
                RouteCredentialService::get_with_activity(&state.pool, &activity, id)
                    .await
                    .map_err(to_error)?,
            )
        }
        "create_api_route_credential" => {
            let input: CreateApiRouteCredentialInput = parse_arg(&args, "input")?;
            to_value(
                RouteCredentialService::create_api(&state.pool, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "import_official_route_credentials_from_text" => {
            let input: ImportOfficialTextInput = parse_arg(&args, "input")?;
            to_value(
                RouteCredentialService::import_official_text(&state.pool, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "import_official_route_credentials_from_files" => {
            let input: ImportOfficialFilesInput = parse_arg(&args, "input")?;
            to_value(
                RouteCredentialService::import_official_files(&state.pool, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "update_route_credential" => {
            let id = required_string_arg(&args, "id")?;
            let input: UpdateRouteCredentialInput = parse_arg(&args, "input")?;
            to_value(
                RouteCredentialService::update(&state.pool, id, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "copy_route_credential" => {
            let id = required_string_arg(&args, "id")?;
            let input = args
                .get("input")
                .filter(|value| !value.is_null())
                .cloned()
                .map(serde_json::from_value::<CopyRouteCredentialInput>)
                .transpose()
                .map_err(|error| invalid_argument("input", Some(error.to_string())))?
                .unwrap_or_default();
            to_value(
                RouteCredentialService::copy_with_options(&state.pool, id, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "set_route_credential_recovery" => {
            let id = required_string_arg(&args, "id")?;
            let rule: RecoveryRule = parse_arg(&args, "rule")?;
            to_value(
                RouteRecoveryService::set_rule(&state.pool, id, rule)
                    .await
                    .map_err(to_error)?,
            )
        }
        "set_route_credential_model_status" => {
            let id = required_string_arg(&args, "id")?;
            let model_key = required_string_arg(&args, "model_key")?;
            let status = required_string_arg(&args, "status")?;
            to_value(
                RouteCredentialService::set_model_status(&state.pool, id, model_key, status)
                    .await
                    .map_err(to_error)?,
            )
        }
        "clear_route_credential_model_state" => {
            let id = required_string_arg(&args, "id")?;
            let model_key = required_string_arg(&args, "model_key")?;
            to_value(
                RouteCredentialService::clear_model_state(&state.pool, id, model_key)
                    .await
                    .map_err(to_error)?,
            )
        }
        "delete_route_credential" => {
            let id = required_string_arg(&args, "id")?;
            RouteCredentialService::delete(&state.pool, id)
                .await
                .map_err(to_error)?;
            to_value(())
        }
        "archive_route_credentials" => {
            let ids: Vec<String> = parse_arg(&args, "ids")?;
            RouteCredentialService::archive(&state.pool, ids)
                .await
                .map_err(to_error)?;
            to_value(())
        }
        "restore_route_credentials" => {
            let ids: Vec<String> = parse_arg(&args, "ids")?;
            RouteCredentialService::restore(&state.pool, ids)
                .await
                .map_err(to_error)?;
            to_value(())
        }
        "set_route_credential_statuses" => {
            let ids: Vec<String> = parse_arg(&args, "ids")?;
            let status = required_string_arg(&args, "status")?;
            RouteCredentialService::set_statuses(&state.pool, ids, status)
                .await
                .map_err(to_error)?;
            to_value(())
        }
        "refresh_route_credential_quota" => {
            let id = required_string_arg(&args, "id")?;
            to_value(
                RouteQuotaService::refresh_one(&state.pool, id)
                    .await
                    .map_err(to_error)?,
            )
        }
        "refresh_route_credentials_quota" => {
            let platform = required_string_arg(&args, "platform")?;
            to_value(
                RouteQuotaService::refresh_platform(&state.pool, platform)
                    .await
                    .map_err(to_error)?,
            )
        }
        "refresh_route_credential_relay_balance" => {
            let id = required_string_arg(&args, "id")?;
            to_value(
                RouteRelayBalanceService::refresh_one(&state.pool, id)
                    .await
                    .map_err(to_error)?,
            )
        }
        "refresh_route_credentials_relay_balance" => {
            let platform = required_string_arg(&args, "platform")?;
            to_value(
                RouteRelayBalanceService::refresh_platform(&state.pool, platform)
                    .await
                    .map_err(to_error)?,
            )
        }
        "get_route_pool" => {
            let platform = required_string_arg(&args, "platform")?;
            let since = optional_string_arg(&args, "since")?;
            let request_page = optional_i64_arg(&args, "request_page")?;
            let request_page_size = optional_i64_arg(&args, "request_page_size")?;
            to_value(
                RoutePoolService::get(
                    &state.pool,
                    platform,
                    since,
                    request_page,
                    request_page_size,
                )
                .await
                .map_err(to_error)?,
            )
        }
        "preview_route_credential_import" => {
            let input: PreviewRouteCredentialImportInput = parse_arg(&args, "input")?;
            to_value(
                crate::services::route_credential_transfer_import_service::preview_route_credential_import(
                    &state.pool,
                    input,
                )
                .await
                .map_err(to_error)?,
            )
        }
        "import_route_credentials" => {
            let input: ImportRouteCredentialsInput = parse_arg(&args, "input")?;
            to_value(
                crate::services::route_credential_transfer_import_service::import_route_credentials(
                    &state.pool,
                    input,
                )
                .await
                .map_err(to_error)?,
            )
        }
        "preview_external_client_import" => {
            let input: PreviewExternalClientImportInput = parse_arg(&args, "input")?;
            to_value(
                external_client_import_service::preview_external_client_import(&state.pool, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "import_external_client_accounts" => {
            let input: ImportExternalClientAccountsInput = parse_arg(&args, "input")?;
            to_value(
                external_client_import_service::import_external_client_accounts(&state.pool, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "set_route_pool_members" => {
            let input: SetRoutePoolMembersInput = parse_arg(&args, "input")?;
            to_value(
                RoutePoolService::set_members(&state.pool, input)
                    .await
                    .map_err(to_error)?,
            )
        }
        "route_pool_route_once" => {
            let request: RoutePoolRouteRequest = parse_arg(&args, "request")?;
            let activity = state.route_proxy.activity();
            to_value(
                RoutePoolService::route_once_with_activity(&state.pool, &activity, request)
                    .await
                    .map_err(to_error)?,
            )
        }
        "subscribe_route_proxy_live_log" => {
            let platform = required_string_arg(&args, "platform")?;
            to_value(state.route_proxy.live_log().subscribe(&platform))
        }
        "unsubscribe_route_proxy_live_log" => {
            state.route_proxy.live_log().unsubscribe();
            to_value(Value::Null)
        }
        "route_pool_test_model" => {
            let request: RoutePoolModelTestRequest = parse_arg(&args, "request")?;
            if route_model_test_targets_single_account(&request) {
                let activity = state.route_proxy.activity();
                to_value(
                    RouteModelTestService::test_model_with_activity(
                        &state.pool,
                        &activity,
                        request,
                    )
                    .await
                    .map_err(to_error)?,
                )
            } else {
                let base_url = route_model_test_proxy_base_url(state.as_ref()).await?;
                let root_certificate_pem = if base_url.starts_with("https://") {
                    Some(
                        RouteProxyHttpsService::load_root_certificate_pem(&state.paths)
                            .await
                            .map_err(to_error)?,
                    )
                } else {
                    None
                };
                to_value(
                    RouteModelTestService::test_model_through_proxy_with_root_certificate(
                        &state.pool,
                        request,
                        &base_url,
                        root_certificate_pem.as_deref(),
                    )
                    .await
                    .map_err(to_error)?,
                )
            }
        }
        "fetch_route_models" => {
            let request: RouteModelsFetchRequest = parse_arg(&args, "request")?;
            to_value(
                RouteModelFetchService::fetch(request)
                    .await
                    .map_err(to_error)?,
            )
        }
        "start_route_proxy" => to_value(
            RouteProxyHttpsService::start_proxy(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "stop_route_proxy" => {
            let status = RouteProxyService::stop(&state.route_proxy)
                .await
                .map_err(to_error)?;
            RouteProxyHttpsService::clear_auto_start(&state.paths)
                .await
                .map_err(to_error)?;
            to_value(status)
        }
        "get_route_proxy_status" => to_value(RouteProxyService::status(&state.route_proxy).await),
        "get_route_proxy_key" => {
            let platform = required_string_arg(&args, "platform")?;
            to_value(
                RouteProxyService::get_or_create_platform_key(&state.pool, &platform)
                    .await
                    .map_err(to_error)?,
            )
        }
        "get_route_proxy_https_status" => to_value(
            RouteProxyHttpsService::status_for_state(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "enable_route_proxy_https" => to_value(
            RouteProxyHttpsService::enable(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "disable_route_proxy_https" => to_value(
            RouteProxyHttpsService::disable(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "reimport_route_proxy_root_ca" => to_value(
            RouteProxyHttpsService::reimport_root_ca(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "regenerate_route_proxy_https_certificates" => to_value(
            RouteProxyHttpsService::regenerate_certificates(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "uninstall_route_proxy_root_ca" => to_value(
            RouteProxyHttpsService::uninstall_root_ca(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "delete_route_proxy_https_certificates" => to_value(
            RouteProxyHttpsService::delete_certificates(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "write_route_proxy_configs" => {
            let base_url = optional_string_arg(&args, "baseUrl")?;
            let platform = optional_string_arg(&args, "platform")?.ok_or_else(|| {
                to_error(AppError::Validation {
                    code: "validation.route_config_platform_required",
                    message: "Route config platform is required".to_string(),
                    details: None,
                    recoverable: true,
                })
            })?;
            let client_keys = optional_string_array_arg(&args, "clientKeys")?;
            let status = RouteProxyService::status(&state.route_proxy).await;
            let resolved = base_url
                .filter(|value| !value.is_empty())
                .or(status.base_url)
                .ok_or_else(|| {
                    to_error(AppError::Validation {
                        code: "validation.route_proxy_not_running",
                        message: "Start the route proxy before writing config files".to_string(),
                        details: None,
                        recoverable: true,
                    })
                })?;
            to_value(
                RouteConfigService::write_configs(
                    &state.paths,
                    &state.pool,
                    &state.config_writes,
                    &resolved,
                    &platform,
                    client_keys.as_deref(),
                )
                .await
                .map_err(to_error)?,
            )
        }
        "route_config_write_is_stale" => {
            let base_url = optional_string_arg(&args, "baseUrl")?;
            let platform = optional_string_arg(&args, "platform")?.ok_or_else(|| {
                to_error(AppError::Validation {
                    code: "validation.route_config_platform_required",
                    message: "Route config platform is required".to_string(),
                    details: None,
                    recoverable: true,
                })
            })?;
            let client_keys = optional_string_array_arg(&args, "clientKeys")?;
            let status = RouteProxyService::status(&state.route_proxy).await;
            match base_url
                .filter(|value| !value.is_empty())
                .or(status.base_url)
            {
                Some(resolved) => to_value(
                    RouteConfigService::config_write_is_stale(
                        &state.paths,
                        &state.pool,
                        &resolved,
                        &platform,
                        client_keys.as_deref(),
                    )
                    .await,
                ),
                None => to_value(false),
            }
        }
        "get_web_service_config" => to_value(
            WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?,
        ),
        "save_web_service_config" => {
            let config: WebServiceConfig = parse_arg(&args, "config")?;
            let saved = WebService::save_config_and_reconcile(&state, &config)
                .await
                .map_err(to_error)?;
            to_value(saved)
        }
        "get_web_server_status" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?;
            to_value(WebService::status(&state.web_service, &config).await)
        }
        "start_web_server" => to_value(
            WebService::start(Arc::clone(&state))
                .await
                .map_err(to_error)?,
        ),
        "stop_web_server" => to_value(WebService::stop(state.as_ref()).await),
        "get_tailscale_status" => to_value(
            WebService::tailscale_status(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "create_mobile_pairing" => to_value(
            WebService::create_mobile_pairing(
                state.as_ref(),
                args.get("force").and_then(Value::as_bool).unwrap_or(false),
            )
            .await
            .map_err(to_error)?,
        ),
        "start_tailscale_login" => to_value(
            WebService::start_tailscale_login(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        "start_tailscale_with_auth_key" => {
            let auth_key = required_string_arg(&args, "authKey")?;
            to_value(
                WebService::start_tailscale_with_auth_key(state.as_ref(), auth_key)
                    .await
                    .map_err(to_error)?,
            )
        }
        "disconnect_tailscale" => to_value(
            WebService::disconnect_tailscale(state.as_ref())
                .await
                .map_err(to_error)?,
        ),
        other => Err(ApiError::from(AppError::Validation {
            code: "web.command_unknown",
            message: "Web command is not recognized".to_string(),
            details: Some(other.to_string()),
            recoverable: false,
        })),
    }
}

fn command_error(code: &'static str, message: String) -> ApiError {
    ApiError::from(AppError::Validation {
        code,
        message,
        details: None,
        recoverable: true,
    })
}

fn require_terminal_subscriber(state: &Arc<AppState>, session_id: &str) -> Result<(), ApiError> {
    if state.terminal_hub.has_subscriber(session_id) {
        return Ok(());
    }
    Err(command_error(
        "web.terminal_not_subscribed",
        format!("No active terminal subscription for session {session_id}."),
    ))
}

fn to_error(error: AppError) -> ApiError {
    ApiError::from(error)
}

fn to_value<T: Serialize>(value: T) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|error| {
        ApiError::from(AppError::Validation {
            code: "web.response_serialize",
            message: "Could not serialize the Web response".to_string(),
            details: Some(error.to_string()),
            recoverable: false,
        })
    })
}

fn parse_arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, ApiError> {
    let value = args
        .get(key)
        .cloned()
        .ok_or_else(|| missing_argument(key))?;
    serde_json::from_value(value).map_err(|error| invalid_argument(key, Some(error.to_string())))
}

fn optional_string_arg(args: &Value, key: &str) -> Result<Option<String>, ApiError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            Ok(Some(text.trim().to_string()).filter(|value| !value.is_empty()))
        }
        Some(_) => Err(invalid_argument(key, Some("expected string".to_string()))),
    }
}

fn optional_string_array_arg(args: &Value, key: &str) -> Result<Option<Vec<String>>, ApiError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| {
                        invalid_argument(key, Some("expected array of strings".to_string()))
                    })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(invalid_argument(
            key,
            Some("expected array of strings".to_string()),
        )),
    }
}

fn route_model_test_targets_single_account(request: &RoutePoolModelTestRequest) -> bool {
    request
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|account_id| !account_id.is_empty())
        .is_some()
}

async fn route_model_test_proxy_base_url(state: &AppState) -> Result<String, ApiError> {
    let status = RouteProxyService::status(&state.route_proxy).await;
    let status = if status
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|base_url| !base_url.is_empty())
        .is_some()
    {
        status
    } else {
        RouteProxyHttpsService::start_proxy(state)
            .await
            .map_err(ApiError::from)?
    };

    status
        .base_url
        .map(|base_url| base_url.trim().to_string())
        .filter(|base_url| !base_url.is_empty())
        .ok_or_else(|| {
            ApiError::from(AppError::Validation {
                code: "validation.route_proxy_not_running",
                message: "Start the route proxy before testing the route pool".to_string(),
                details: None,
                recoverable: true,
            })
        })
}

fn optional_i64_arg(args: &Value, key: &str) -> Result<Option<i64>, ApiError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or_else(|| invalid_argument(key, Some("expected integer".to_string()))),
        Some(Value::String(text)) => text
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| invalid_argument(key, Some("expected integer".to_string()))),
        Some(_) => Err(invalid_argument(key, Some("expected integer".to_string()))),
    }
}

fn required_string_arg(args: &Value, key: &str) -> Result<String, ApiError> {
    optional_string_arg(args, key)?.ok_or_else(|| missing_argument(key))
}

fn required_raw_string_arg(args: &Value, key: &str) -> Result<String, ApiError> {
    match args.get(key) {
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Null) | None => Err(missing_argument(key)),
        Some(_) => Err(invalid_argument(key, Some("expected string".to_string()))),
    }
}

fn required_u16_arg(args: &Value, key: &str) -> Result<u16, ApiError> {
    let value = args
        .get(key)
        .ok_or_else(|| missing_argument(key))?
        .as_u64()
        .ok_or_else(|| invalid_argument(key, Some("expected unsigned integer".to_string())))?;
    u16::try_from(value).map_err(|_| invalid_argument(key, Some("outside u16 range".to_string())))
}

fn missing_argument(key: &str) -> ApiError {
    ApiError::from(AppError::Validation {
        code: "web.argument_missing",
        message: "A required Web command argument is missing".to_string(),
        details: Some(key.to_string()),
        recoverable: true,
    })
}

fn invalid_argument(key: &str, reason: Option<String>) -> ApiError {
    ApiError::from(AppError::Validation {
        code: "web.argument_invalid",
        message: "A Web command argument is invalid".to_string(),
        details: Some(match reason {
            Some(reason) => format!("{key}: {reason}"),
            None => key.to_string(),
        }),
        recoverable: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::services::config_write_service::ConfigWriteRuntimeState;
    use crate::services::route_proxy_service::RouteProxyRuntimeState;
    use crate::services::tailscale_service::TailscaleRuntimeState;
    use crate::services::web_service::WebServiceRuntimeState;
    use crate::terminal_manager::TerminalManager;
    use crate::web::event_bridge::WebEventBroadcaster;
    use std::sync::Arc;
    use tempfile::tempdir;

    struct TestState {
        _temp: tempfile::TempDir,
        state: Arc<AppState>,
    }

    async fn test_state() -> TestState {
        let temp = tempdir().expect("temp dir");
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        TestState {
            state: Arc::new(AppState {
                paths: crate::paths::AppPaths::from_data_dir(temp.path().join("app-data")),
                pool,
                config_writes: ConfigWriteRuntimeState::default(),
                deeplink_protocols:
                    crate::services::deeplink_protocol_service::DeepLinkProtocolRuntime::default(),
                route_proxy: RouteProxyRuntimeState::default(),
                web_service: WebServiceRuntimeState::default(),
                tailscale: TailscaleRuntimeState::default(),
                terminals: TerminalManager::default(),
                terminal_hub: Arc::new(crate::web::terminal_hub::TerminalHub::default()),
                event_broadcaster: Arc::new(WebEventBroadcaster::default()),
            }),
            _temp: temp,
        }
    }

    #[test]
    fn required_raw_string_arg_preserves_terminal_control_input() {
        let args = json!({ "data": "\r" });

        assert_eq!(required_raw_string_arg(&args, "data").unwrap(), "\r");
    }

    #[test]
    fn required_raw_string_arg_allows_empty_terminal_input() {
        let args = json!({ "data": "" });

        assert_eq!(required_raw_string_arg(&args, "data").unwrap(), "");
    }

    #[test]
    fn required_string_arg_still_rejects_blank_regular_input() {
        let args = json!({ "data": "\r" });

        let error = required_string_arg(&args, "data").unwrap_err();
        assert_eq!(error.code, "web.argument_missing");
        assert_eq!(error.details.as_deref(), Some("data"));
    }

    #[test]
    fn argument_helpers_report_stable_missing_and_invalid_codes() {
        let missing = parse_arg::<String>(&json!({}), "value").unwrap_err();
        assert_eq!(missing.code, "web.argument_missing");
        assert_eq!(missing.details.as_deref(), Some("value"));

        let invalid = optional_i64_arg(&json!({ "limit": [] }), "limit").unwrap_err();
        assert_eq!(invalid.code, "web.argument_invalid");
        assert_eq!(invalid.details.as_deref(), Some("limit: expected integer"));

        let negative = u32::try_from(-1_i64).map_err(|_| {
            invalid_argument("limit", Some("expected non-negative integer".to_string()))
        });
        let negative = negative.unwrap_err();
        assert_eq!(negative.code, "web.argument_invalid");
        assert_eq!(
            negative.details.as_deref(),
            Some("limit: expected non-negative integer")
        );
    }

    #[test]
    fn mcp_and_skill_mutations_are_sensitive_web_commands() {
        for command in [
            "mcp_install_from_marketplace",
            "mcp_upsert_local_server",
            "mcp_set_server_apps",
            "mcp_remove_server",
            "skills_save",
            "skills_delete",
            "create_mobile_pairing",
        ] {
            assert!(is_sensitive_command(command), "{command} must require auth");
        }
        assert!(!is_sensitive_command("mcp_scan_local"));
        assert!(!is_sensitive_command("skills_list"));
    }

    #[test]
    fn creating_terminal_sessions_is_a_sensitive_web_command() {
        assert!(is_sensitive_command("create_terminal_session"));
    }

    #[test]
    fn resuming_session_terminals_is_not_a_sensitive_web_command() {
        assert!(!is_sensitive_command("resume_session_terminal"));
        assert!(!is_sensitive_command("write_terminal_input"));
        assert!(!is_sensitive_command("resize_terminal"));
        assert!(!is_sensitive_command("kill_terminal_session"));
    }

    #[tokio::test]
    async fn terminal_writes_require_an_active_subscriber() {
        let fixture = test_state().await;
        let error = dispatch_command(
            Arc::clone(&fixture.state),
            "write_terminal_input",
            json!({ "sessionId": "unsubscribed-session", "data": "ls\n" }),
        )
        .await
        .expect_err("write without subscriber must fail");
        assert_eq!(error.code, "web.terminal_not_subscribed");
    }

    #[tokio::test]
    async fn resuming_an_unknown_session_reports_a_resume_error() {
        let fixture = test_state().await;
        let error = dispatch_command(
            Arc::clone(&fixture.state),
            "resume_session_terminal",
            json!({ "sessionId": "missing-session", "cols": 100, "rows": 30 }),
        )
        .await
        .expect_err("unknown session must fail");
        assert_eq!(error.code, "web.terminal_resume");
        assert!(error.message.contains("missing-session"));
    }

    #[tokio::test]
    async fn dispatches_marketplace_and_skill_catalog_commands() {
        let fixture = test_state().await;
        let marketplaces = dispatch_command(
            Arc::clone(&fixture.state),
            "mcp_list_marketplaces",
            json!({}),
        )
        .await
        .expect("marketplaces");
        assert_eq!(marketplaces.as_array().map(Vec::len), Some(2));

        let agents = dispatch_command(fixture.state, "skills_list_agents", json!({}))
            .await
            .expect("skill agents");
        assert_eq!(agents.as_array().map(Vec::len), Some(11));
    }

    #[tokio::test]
    async fn dispatch_unknown_command_returns_structured_error() {
        let fixture = test_state().await;
        let error = dispatch_command(fixture.state, "not_a_command", json!({}))
            .await
            .unwrap_err();

        assert_eq!(error.code, "web.command_unknown");
        assert_eq!(error.details.as_deref(), Some("not_a_command"));
        assert!(!error.recoverable);
    }

    #[tokio::test]
    async fn saving_a_masked_secret_payload_keeps_the_stored_key() {
        // The paired phone is shown MASKED_SECRET_PAYLOAD instead of the real
        // secret. It loads the same edit drawer, so saving from there must not
        // write the mask over the key — masking a read has to be safe on its own.
        let fixture = test_state().await;
        let credential = dispatch_command(
            Arc::clone(&fixture.state),
            "create_api_route_credential",
            json!({
                "input": {
                    "platform": "codex",
                    "display_name": "Phone edited",
                    "api_key": "sk-real-key",
                    "base_url": "https://api.example.com/v1",
                    "interface_format": "openai",
                    "model_mappings_json": "[]"
                }
            }),
        )
        .await
        .expect("create credential");
        let id = credential["id"]
            .as_str()
            .expect("credential id")
            .to_string();
        let config_json = credential["config_json"]
            .as_str()
            .expect("config json")
            .to_string();

        let updated = dispatch_command(
            fixture.state,
            "update_route_credential",
            json!({
                "id": id,
                "input": {
                    "display_name": "Renamed from the phone",
                    "email": null,
                    "status": "ok",
                    "route_priority": 1,
                    "max_concurrency": 1,
                    "secret_payload_json": MASKED_SECRET_PAYLOAD,
                    "config_json": config_json,
                    "preview_json": "{}"
                }
            }),
        )
        .await
        .expect("update credential");

        assert_eq!(updated["display_name"], "Renamed from the phone");
        let secret = updated["secret_payload_json"]
            .as_str()
            .expect("secret payload");
        assert!(
            secret.contains("sk-real-key"),
            "the mask must not overwrite the stored key, got {secret}"
        );
    }

    #[tokio::test]
    async fn saving_a_masked_payload_merged_with_an_empty_key_keeps_the_stored_key() {
        // What the edit drawer actually sends back: it re-serializes the payload it
        // loaded and merges its (empty) api_key field in, so the mask arrives as
        // `{"__masked":true,"api_key":""}`. A whole-string comparison would miss
        // this and blank out the key.
        let fixture = test_state().await;
        let credential = dispatch_command(
            Arc::clone(&fixture.state),
            "create_api_route_credential",
            json!({
                "input": {
                    "platform": "codex",
                    "display_name": "Phone edited",
                    "api_key": "sk-real-key",
                    "base_url": "https://api.example.com/v1",
                    "interface_format": "openai",
                    "model_mappings_json": "[]"
                }
            }),
        )
        .await
        .expect("create credential");
        let id = credential["id"]
            .as_str()
            .expect("credential id")
            .to_string();
        let config_json = credential["config_json"]
            .as_str()
            .expect("config json")
            .to_string();

        let updated = dispatch_command(
            fixture.state,
            "update_route_credential",
            json!({
                "id": id,
                "input": {
                    "display_name": "Phone edited",
                    "email": null,
                    "status": "ok",
                    "route_priority": 1,
                    "max_concurrency": 1,
                    "secret_payload_json": r#"{"__masked": true, "api_key": ""}"#,
                    "config_json": config_json,
                    "preview_json": "{}"
                }
            }),
        )
        .await
        .expect("update credential");

        let secret = updated["secret_payload_json"]
            .as_str()
            .expect("secret payload");
        assert!(
            secret.contains("sk-real-key"),
            "the drawer's re-serialized mask must not overwrite the stored key, got {secret}"
        );
    }

    #[tokio::test]
    async fn dispatches_route_credential_recovery_rule() {
        let fixture = test_state().await;
        let credential = dispatch_command(
            Arc::clone(&fixture.state),
            "create_api_route_credential",
            json!({
                "input": {
                    "platform": "codex",
                    "display_name": "Scheduled account",
                    "api_key": "sk-test",
                    "base_url": "https://api.example.com/v1",
                    "interface_format": "openai",
                    "model_mappings_json": "[]"
                }
            }),
        )
        .await
        .expect("create credential");
        let id = credential["id"].as_str().expect("credential id");

        let updated = dispatch_command(
            fixture.state,
            "set_route_credential_recovery",
            json!({
                "id": id,
                "rule": {
                    "mode": "scheduled",
                    "times": ["3:00", "15:00"]
                }
            }),
        )
        .await
        .expect("set recovery rule");
        let config: Value =
            serde_json::from_str(updated["config_json"].as_str().expect("config json"))
                .expect("config value");

        assert_eq!(config["recovery"]["mode"], "scheduled");
        assert_eq!(config["recovery"]["times"], json!(["03:00", "15:00"]));
    }

    #[tokio::test]
    async fn dispatches_route_credential_copy_options_and_keeps_legacy_calls_working() {
        let fixture = test_state().await;
        let credential = dispatch_command(
            Arc::clone(&fixture.state),
            "create_api_route_credential",
            json!({
                "input": {
                    "platform": "claude",
                    "display_name": "Claude API",
                    "api_key": "sk-source",
                    "base_url": "https://api.example.com",
                    "interface_format": "anthropic",
                    "model_mappings_json": "[]"
                }
            }),
        )
        .await
        .expect("create credential");
        let id = credential["id"].as_str().expect("credential id");

        let copied = dispatch_command(
            Arc::clone(&fixture.state),
            "copy_route_credential",
            json!({
                "id": id,
                "input": {
                    "target_platform": "codex",
                    "api_key": "sk-override"
                }
            }),
        )
        .await
        .expect("copy credential");
        let copied_config: Value =
            serde_json::from_str(copied["config_json"].as_str().expect("config json"))
                .expect("config value");
        let copied_secret: Value = serde_json::from_str(
            copied["secret_payload_json"]
                .as_str()
                .expect("secret payload json"),
        )
        .expect("secret value");

        assert_eq!(copied["platform"], "codex");
        assert_eq!(copied_config["base_url"], "https://api.example.com/v1");
        assert_eq!(copied_secret["api_key"], "sk-override");

        let legacy_copy =
            dispatch_command(fixture.state, "copy_route_credential", json!({ "id": id }))
                .await
                .expect("legacy copy");
        assert_eq!(legacy_copy["platform"], "claude");
    }

    #[tokio::test]
    async fn dispatches_export_with_the_exact_input_argument() {
        let fixture = test_state().await;
        let result = dispatch_command(
            Arc::clone(&fixture.state),
            "export_route_credentials",
            json!({
                "input": {
                    "selection_context": {"platform": "claude", "pool_scope": "in_pool"},
                    "credential_ids": []
                }
            }),
        )
        .await
        .unwrap();

        assert!(result["json_text"].is_null());
        assert_eq!(result["errors"][0]["code"], "transfer.selection_empty");

        let error = dispatch_command(
            fixture.state,
            "export_route_credentials",
            json!({"Input": {}}),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "web.argument_missing");
        assert_eq!(error.details.as_deref(), Some("input"));
    }

    #[tokio::test]
    async fn dispatches_import_preview_and_commit_with_the_exact_input_argument() {
        let fixture = test_state().await;
        let input = json!({
            "text": "[]",
            "ambiguous_platform_choices": [],
            "restore_pool_membership": false
        });

        let preview = dispatch_command(
            Arc::clone(&fixture.state),
            "preview_route_credential_import",
            json!({
                "input": {
                    "text": "[]",
                    "ambiguous_platform_choices": []
                }
            }),
        )
        .await
        .expect("import preview");
        assert_eq!(preview["counts"]["total"], 0);

        let outcome = dispatch_command(
            fixture.state,
            "import_route_credentials",
            json!({ "input": input }),
        )
        .await
        .expect("import commit");
        assert_eq!(outcome["imported"], 0);
        assert_eq!(outcome["failed"], 0);
    }

    #[tokio::test]
    async fn web_dispatch_never_exposes_the_desktop_save_command() {
        let fixture = test_state().await;
        let error = dispatch_command(
            fixture.state,
            "save_route_credential_export",
            json!({
                "suggested_file_name": "credentials.json",
                "json_text": "[]"
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.code, "web.command_unknown");
    }

    #[tokio::test]
    async fn dispatch_get_route_proxy_https_status_returns_a_serializable_status() {
        let fixture = test_state().await;
        let result = dispatch_command(fixture.state, "get_route_proxy_https_status", json!({}))
            .await
            .expect("HTTPS status response");

        assert_eq!(result.get("enabled").and_then(Value::as_bool), Some(false));
        assert_eq!(
            result.get("certReady").and_then(Value::as_bool),
            Some(false)
        );
        assert!(result
            .get("certificateDir")
            .and_then(Value::as_str)
            .is_some());
        assert!(result
            .get("manualInstructions")
            .and_then(Value::as_array)
            .is_some());
    }

    #[tokio::test]
    async fn dispatch_list_platform_capabilities_returns_phase_a_matrix() {
        let fixture = test_state().await;
        let result = dispatch_command(fixture.state, "list_platform_capabilities", json!({}))
            .await
            .expect("platform capability response");

        let rows = result.as_array().expect("capability rows");
        assert_eq!(rows.len(), 7);
        let hermes = rows
            .iter()
            .find(|row| row.get("platform").and_then(Value::as_str) == Some("hermes"))
            .expect("Hermes capability");
        assert_eq!(
            hermes.get("support_level").and_then(Value::as_str),
            Some("partial")
        );
        assert_eq!(
            hermes
                .pointer("/operations/config_write/availability")
                .and_then(Value::as_str),
            Some("unavailable")
        );
    }

    #[tokio::test]
    async fn dispatches_target_config_statuses_and_snapshot_summaries() {
        let fixture = test_state().await;
        let statuses = dispatch_command(
            Arc::clone(&fixture.state),
            "list_target_config_statuses",
            json!({}),
        )
        .await
        .expect("target statuses");
        assert!(statuses.as_array().is_some_and(|rows| rows
            .iter()
            .any(|row| row.pointer("/target/key") == Some(&json!("codex")))));

        let snapshots = dispatch_command(
            fixture.state,
            "list_config_snapshots",
            json!({ "limit": 5 }),
        )
        .await
        .expect("snapshot summaries");
        assert_eq!(snapshots, json!([]));
    }
}
