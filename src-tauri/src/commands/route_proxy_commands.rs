use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::config_snapshot::ConfigWriteOutcome;
use crate::services::route_config_service::RouteConfigService;
use crate::services::route_proxy_https_service::RouteProxyHttpsService;
use crate::services::route_proxy_service::{RouteProxyService, RouteProxyStatus};
use tauri::State;

#[tauri::command]
pub async fn start_route_proxy(state: State<'_, AppState>) -> Result<RouteProxyStatus, ApiError> {
    RouteProxyHttpsService::start_proxy(&state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn stop_route_proxy(state: State<'_, AppState>) -> Result<RouteProxyStatus, ApiError> {
    let status = RouteProxyService::stop(&state.route_proxy)
        .await
        .map_err(ApiError::from)?;
    RouteProxyHttpsService::clear_auto_start(&state.paths)
        .await
        .map_err(ApiError::from)?;
    Ok(status)
}

#[tauri::command]
pub async fn get_route_proxy_status(
    state: State<'_, AppState>,
) -> Result<RouteProxyStatus, ApiError> {
    Ok(RouteProxyService::status(&state.route_proxy).await)
}

#[tauri::command]
pub async fn get_route_proxy_key(
    state: State<'_, AppState>,
    platform: String,
) -> Result<String, ApiError> {
    RouteProxyService::get_or_create_platform_key(&state.pool, &platform)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn write_route_proxy_configs(
    state: State<'_, AppState>,
    base_url: Option<String>,
    platform: String,
) -> Result<Vec<ConfigWriteOutcome>, ApiError> {
    let status = RouteProxyService::status(&state.route_proxy).await;
    let resolved = base_url
        .and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .or(status.base_url)
        .ok_or_else(|| {
            ApiError::from(crate::error::AppError::Validation {
                code: "validation.route_proxy_not_running",
                message: "Start the route proxy before writing config files".to_string(),
                details: None,
                recoverable: true,
            })
        })?;

    RouteConfigService::write_configs(
        &state.paths,
        &state.pool,
        &state.config_writes,
        &resolved,
        &platform,
    )
    .await
    .map_err(ApiError::from)
}

/// Whether writing config now would change the file on disk — the app only
/// writes on demand, so model-slot and client-config edits sit unapplied until
/// the user asks for a write.
#[tauri::command]
pub async fn route_config_write_is_stale(
    state: State<'_, AppState>,
    base_url: Option<String>,
    platform: String,
) -> Result<bool, ApiError> {
    let status = RouteProxyService::status(&state.route_proxy).await;
    let Some(resolved) = base_url
        .and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .or(status.base_url)
    else {
        // Proxy not running: writing is unavailable, so there is nothing to nudge about.
        return Ok(false);
    };

    Ok(
        RouteConfigService::config_write_is_stale(&state.paths, &state.pool, &resolved, &platform)
            .await,
    )
}
