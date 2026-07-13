use crate::app_state::AppState;
use crate::error::ApiError;
use crate::services::route_config_service::{RouteConfigService, RouteConfigWriteOutcome};
use crate::services::route_proxy_service::{RouteProxyService, RouteProxyStatus};
use tauri::State;

#[tauri::command]
pub async fn start_route_proxy(state: State<'_, AppState>) -> Result<RouteProxyStatus, ApiError> {
    RouteProxyService::start(&state.route_proxy, state.pool.clone())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn stop_route_proxy(state: State<'_, AppState>) -> Result<RouteProxyStatus, ApiError> {
    RouteProxyService::stop(&state.route_proxy)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_route_proxy_status(
    state: State<'_, AppState>,
) -> Result<RouteProxyStatus, ApiError> {
    Ok(RouteProxyService::status(&state.route_proxy).await)
}

#[tauri::command]
pub async fn write_route_proxy_configs(
    state: State<'_, AppState>,
    base_url: Option<String>,
) -> Result<Vec<RouteConfigWriteOutcome>, ApiError> {
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

    RouteConfigService::write_configs(&state.paths, &resolved)
        .await
        .map_err(ApiError::from)
}
