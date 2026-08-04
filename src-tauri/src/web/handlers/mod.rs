use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::commands::batch_commands::{CreateAccountRequest, UpdateAccountRequest};
use crate::core::sessions::{get_session_messages_core, list_sessions_core};
use crate::core::settings::{get_settings_core, save_settings_core};
use crate::core::terminals::{
    create_terminal_session_core, kill_terminal_session_core, list_terminal_sessions_core,
    resize_terminal_core, write_terminal_input_core,
};
use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
use crate::error::{ApiError, AppError};
use crate::models::batch::NewBatch;
use crate::models::route_credential::{
    CreateApiRouteCredentialInput, ImportOfficialFilesInput, ImportOfficialTextInput,
    UpdateRouteCredentialInput,
};
use crate::models::route_pool::{
    RouteModelsFetchRequest, RoutePoolModelTestRequest, RoutePoolRouteRequest,
    SetRoutePoolMembersInput,
};
use crate::models::settings::AppSettings;
use crate::services::batch_service::BatchService;
use crate::services::config_write_service::ConfigWriteCoordinator;
use crate::services::import_service::{ExampleJsonImportRequest, ImportService};
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::route_config_service::RouteConfigService;
use crate::services::route_credential_service::RouteCredentialService;
use crate::services::route_model_fetch_service::RouteModelFetchService;
use crate::services::route_model_test_service::RouteModelTestService;
use crate::services::route_pool_service::RoutePoolService;
use crate::services::route_proxy_https_service::RouteProxyHttpsService;
use crate::services::route_proxy_service::RouteProxyService;
use crate::services::route_quota_service::RouteQuotaService;
use crate::services::tailscale_service::TailscaleService;
use crate::services::target_service::TargetService;
use crate::services::web_service::{WebService, WebServiceConfig};
use crate::terminal_manager::CreateTerminalSessionInput;
use crate::web::event_bridge::EventEmitter;

pub async fn dispatch_command(
    state: Arc<AppState>,
    command: &str,
    args: Value,
) -> Result<Value, ApiError> {
    match command {
        "health" => to_value(json!({ "ok": true })),
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
        "get_settings" => to_value(get_settings_core(&state.paths).await.map_err(to_error)?),
        "save_settings" => {
            let settings: AppSettings = parse_arg(&args, "settings")?;
            to_value(
                save_settings_core(&state.paths, settings)
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
                    input,
                )
                .map_err(|message| command_error("web.terminal_create", message))?,
            )
        }
        "write_terminal_input" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            let data = required_raw_string_arg(&args, "data")?;
            write_terminal_input_core(&state.terminals, &session_id, &data)
                .map_err(|message| command_error("web.terminal_write", message))?;
            to_value(())
        }
        "resize_terminal" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            let cols = required_u16_arg(&args, "cols")?;
            let rows = required_u16_arg(&args, "rows")?;
            resize_terminal_core(&state.terminals, &session_id, cols, rows)
                .map_err(|message| command_error("web.terminal_resize", message))?;
            to_value(())
        }
        "kill_terminal_session" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            kill_terminal_session_core(&state.terminals, &session_id)
                .map_err(|message| command_error("web.terminal_kill", message))?;
            to_value(())
        }
        "list_terminal_sessions" => to_value(list_terminal_sessions_core(&state.terminals)),
        "list_route_credentials" => {
            let platform = required_string_arg(&args, "platform")?;
            to_value(
                RouteCredentialService::list(&state.pool, platform)
                    .await
                    .map_err(to_error)?,
            )
        }
        "get_route_credential" => {
            let id = required_string_arg(&args, "id")?;
            to_value(
                RouteCredentialService::get(&state.pool, id)
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
            to_value(
                RouteCredentialService::copy(&state.pool, id)
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
            to_value(
                RoutePoolService::route_once(&state.pool, request)
                    .await
                    .map_err(to_error)?,
            )
        }
        "route_pool_test_model" => {
            let request: RoutePoolModelTestRequest = parse_arg(&args, "request")?;
            if route_model_test_targets_single_account(&request) {
                to_value(
                    RouteModelTestService::test_model(&state.pool, request)
                        .await
                        .map_err(to_error)?,
                )
            } else {
                let base_url = route_model_test_proxy_base_url(state.as_ref()).await?;
                to_value(
                    RouteModelTestService::test_model_through_proxy(
                        &state.pool,
                        request,
                        &base_url,
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
        "stop_route_proxy" => to_value(
            RouteProxyService::stop(&state.route_proxy)
                .await
                .map_err(to_error)?,
        ),
        "get_route_proxy_status" => to_value(RouteProxyService::status(&state.route_proxy).await),
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
                )
                .await
                .map_err(to_error)?,
            )
        }
        "get_web_service_config" => to_value(
            WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?,
        ),
        "save_web_service_config" => {
            let config: WebServiceConfig = parse_arg(&args, "config")?;
            let saved = WebService::save_config(&state.paths, &config)
                .await
                .map_err(to_error)?;
            if saved.tailscale_enabled {
                let web_status = WebService::status(&state.web_service, &saved).await;
                if web_status.running {
                    let _ = TailscaleService::ensure_started(
                        &state.tailscale,
                        &state.paths,
                        &saved,
                        Some(&web_status),
                    )
                    .await;
                }
            }
            to_value(saved)
        }
        "get_web_server_status" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?;
            to_value(WebService::status(&state.web_service, &config).await)
        }
        "start_web_server" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?;
            to_value(
                WebService::start(Arc::clone(&state), config)
                    .await
                    .map_err(to_error)?,
            )
        }
        "stop_web_server" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?;
            to_value(WebService::stop(state.as_ref(), &config).await)
        }
        "get_tailscale_status" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?;
            let web_status = WebService::status(&state.web_service, &config).await;
            to_value(
                TailscaleService::status(
                    &state.tailscale,
                    &state.paths,
                    &config,
                    Some(&web_status),
                )
                .await,
            )
        }
        "start_tailscale_login" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?;
            let web_status = WebService::status(&state.web_service, &config).await;
            to_value(
                TailscaleService::start_login(
                    &state.tailscale,
                    &state.paths,
                    &config,
                    Some(&web_status),
                )
                .await,
            )
        }
        "start_tailscale_with_auth_key" => {
            let auth_key = required_string_arg(&args, "authKey")?;
            let mut config = WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?;
            let web_status = WebService::status(&state.web_service, &config).await;
            to_value(
                TailscaleService::start_with_auth_key(
                    &state.tailscale,
                    &state.paths,
                    &mut config,
                    Some(&web_status),
                    auth_key,
                )
                .await
                .map_err(|message| command_error("web.tailscale_start", message))?,
            )
        }
        "disconnect_tailscale" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(to_error)?;
            to_value(TailscaleService::disconnect(&state.tailscale, &state.paths, &config).await)
        }
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
                route_proxy: RouteProxyRuntimeState::default(),
                web_service: WebServiceRuntimeState::default(),
                tailscale: TailscaleRuntimeState::default(),
                terminals: TerminalManager::default(),
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
