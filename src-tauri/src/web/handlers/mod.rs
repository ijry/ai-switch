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
use crate::models::settings::AppSettings;
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
        "get_settings" => to_value(get_settings_core(&state.paths).await.map_err(|error| error.to_string())?),
        "save_settings" => {
            let settings: AppSettings = parse_arg(&args, "settings")?;
            to_value(save_settings_core(&state.paths, settings).await.map_err(|error| error.to_string())?)
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
        "get_web_service_config" => {
            to_value(WebService::load_config(&state.paths).await.map_err(|error| error.to_string())?)
        }
        "save_web_service_config" => {
            let config: WebServiceConfig = parse_arg(&args, "config")?;
            to_value(
                WebService::save_config(&state.paths, &config)
                    .await
                    .map_err(|error| error.to_string())?,
            )
        }
        "get_web_server_status" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(|error| error.to_string())?;
            to_value(WebService::status(&state.web_service, &config).await)
        }
        "start_web_server" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(|error| error.to_string())?;
            to_value(
                WebService::start(Arc::clone(&state), config)
                    .await
                    .map_err(|error| error.to_string())?,
            )
        }
        "stop_web_server" => {
            let config = WebService::load_config(&state.paths)
                .await
                .map_err(|error| error.to_string())?;
            to_value(WebService::stop(&state.web_service, &config).await)
        }
        "get_tailscale_status" => to_value(TailscaleService::status().await),
        "start_tailscale_login" => to_value(TailscaleService::start_login().await),
        "disconnect_tailscale" => to_value(TailscaleService::disconnect().await),
        other => Err(format!("Unknown command: {other}")),
    }
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
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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
