use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::core::sessions::{get_session_messages_core, list_sessions_core};
use crate::core::settings::{get_settings_core, save_settings_core};
use crate::core::terminals::{
    create_terminal_session_core, kill_terminal_session_core, list_terminal_sessions_core,
    resize_terminal_core, write_terminal_input_core,
};
use crate::error::AppError;
use crate::models::route_credential::{
    CreateApiRouteCredentialInput, ImportOfficialFilesInput, ImportOfficialTextInput,
    UpdateRouteCredentialInput,
};
use crate::models::route_pool::{RoutePoolRouteRequest, SetRoutePoolMembersInput};
use crate::models::settings::AppSettings;
use crate::services::route_config_service::RouteConfigService;
use crate::services::route_credential_service::RouteCredentialService;
use crate::services::route_pool_service::RoutePoolService;
use crate::services::route_proxy_service::RouteProxyService;
use crate::services::tailscale_service::TailscaleService;
use crate::services::web_service::{WebService, WebServiceConfig};
use crate::terminal_manager::CreateTerminalSessionInput;
use crate::web::event_bridge::EventEmitter;

pub async fn dispatch_command(
    state: Arc<AppState>,
    command: &str,
    args: Value,
) -> Result<Value, String> {
    match command {
        "health" => to_value(json!({ "ok": true })),
        "get_settings" => to_value(get_settings_core(&state.paths).await.map_err(to_error)?),
        "save_settings" => {
            let settings: AppSettings = parse_arg(&args, "settings")?;
            to_value(save_settings_core(&state.paths, settings).await.map_err(to_error)?)
        }
        "list_sessions" => {
            let platform = optional_string_arg(&args, "platform");
            to_value(list_sessions_core(platform).await?)
        }
        "get_session_messages" => {
            let provider_id = required_string_arg(&args, "providerId")?;
            let source_path = required_string_arg(&args, "sourcePath")?;
            to_value(get_session_messages_core(provider_id, source_path).await?)
        }
        "create_terminal_session" => {
            let input: CreateTerminalSessionInput = parse_arg(&args, "input")?;
            to_value(create_terminal_session_core(
                &state.terminals,
                EventEmitter::Web(Arc::clone(&state.event_broadcaster)),
                input,
            )?)
        }
        "write_terminal_input" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            let data = required_string_arg(&args, "data")?;
            write_terminal_input_core(&state.terminals, &session_id, &data)?;
            to_value(())
        }
        "resize_terminal" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            let cols = required_u16_arg(&args, "cols")?;
            let rows = required_u16_arg(&args, "rows")?;
            resize_terminal_core(&state.terminals, &session_id, cols, rows)?;
            to_value(())
        }
        "kill_terminal_session" => {
            let session_id = required_string_arg(&args, "sessionId")?;
            kill_terminal_session_core(&state.terminals, &session_id)?;
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
        "delete_route_credential" => {
            let id = required_string_arg(&args, "id")?;
            RouteCredentialService::delete(&state.pool, id)
                .await
                .map_err(to_error)?;
            to_value(())
        }
        "get_route_pool" => {
            let platform = required_string_arg(&args, "platform")?;
            to_value(
                RoutePoolService::get(&state.pool, platform)
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
        "start_route_proxy" => to_value(
            RouteProxyService::start(&state.route_proxy, state.pool.clone())
                .await
                .map_err(to_error)?,
        ),
        "stop_route_proxy" => to_value(
            RouteProxyService::stop(&state.route_proxy)
                .await
                .map_err(to_error)?,
        ),
        "get_route_proxy_status" => to_value(RouteProxyService::status(&state.route_proxy).await),
        "write_route_proxy_configs" => {
            let base_url = optional_string_arg(&args, "baseUrl");
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
                RouteConfigService::write_configs(&state.paths, &resolved)
                    .await
                    .map_err(to_error)?,
            )
        }
        "get_web_service_config" => {
            to_value(WebService::load_config(&state.paths).await.map_err(to_error)?)
        }
        "save_web_service_config" => {
            let config: WebServiceConfig = parse_arg(&args, "config")?;
            to_value(WebService::save_config(&state.paths, &config).await.map_err(to_error)?)
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
            to_value(WebService::stop(&state.web_service, &config).await)
        }
        "get_tailscale_status" => {
            let config = WebService::load_config(&state.paths).await.map_err(to_error)?;
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
            let config = WebService::load_config(&state.paths).await.map_err(to_error)?;
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
            let mut config = WebService::load_config(&state.paths).await.map_err(to_error)?;
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
                .map_err(|message| message)?,
            )
        }
        "disconnect_tailscale" => {
            let config = WebService::load_config(&state.paths).await.map_err(to_error)?;
            to_value(
                TailscaleService::disconnect(&state.tailscale, &state.paths, &config).await,
            )
        }
        other => Err(format!("Unknown command: {other}")),
    }
}

fn to_error(error: AppError) -> String {
    error.to_string()
}

fn to_value<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| format!("Could not serialize response: {error}"))
}

fn parse_arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    let value = args
        .get(key)
        .cloned()
        .ok_or_else(|| format!("Missing argument: {key}"))?;
    serde_json::from_value(value).map_err(|error| format!("Invalid argument {key}: {error}"))
}

fn optional_string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|value| match value {
            Value::Null => None,
            Value::String(text) => Some(text.clone()),
            other => Some(other.to_string()),
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn required_string_arg(args: &Value, key: &str) -> Result<String, String> {
    optional_string_arg(args, key).ok_or_else(|| format!("Missing argument: {key}"))
}

fn required_u16_arg(args: &Value, key: &str) -> Result<u16, String> {
    let value = args
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Missing argument: {key}"))?;
    u16::try_from(value).map_err(|_| format!("Argument {key} is outside u16 range"))
}
