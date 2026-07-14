use std::sync::Arc;

use tauri::State;

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::services::tailscale_service::{TailscaleLogin, TailscaleService, TailscaleStatus};
use crate::services::web_service::{WebServerStatus, WebService, WebServiceConfig};

#[tauri::command]
pub async fn get_web_service_config(
    state: State<'_, AppState>,
) -> Result<WebServiceConfig, ApiError> {
    WebService::load_config(&state.paths)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_web_service_config(
    state: State<'_, AppState>,
    config: WebServiceConfig,
) -> Result<WebServiceConfig, ApiError> {
    WebService::save_config(&state.paths, &config)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_web_server_status(
    state: State<'_, AppState>,
) -> Result<WebServerStatus, ApiError> {
    let config = WebService::load_config(&state.paths)
        .await
        .map_err(ApiError::from)?;
    Ok(WebService::status(&state.web_service, &config).await)
}

#[tauri::command]
pub async fn start_web_server(state: State<'_, AppState>) -> Result<WebServerStatus, ApiError> {
    let config = WebService::load_config(&state.paths)
        .await
        .map_err(ApiError::from)?;
    let app_state = Arc::new(state.inner().clone());
    WebService::start(app_state, config).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn stop_web_server(state: State<'_, AppState>) -> Result<WebServerStatus, ApiError> {
    let config = WebService::load_config(&state.paths)
        .await
        .map_err(ApiError::from)?;
    Ok(WebService::stop(&state.web_service, &config).await)
}

#[tauri::command]
pub async fn get_tailscale_status(state: State<'_, AppState>) -> Result<TailscaleStatus, ApiError> {
    let config = WebService::load_config(&state.paths)
        .await
        .map_err(ApiError::from)?;
    let web_status = WebService::status(&state.web_service, &config).await;
    Ok(TailscaleService::status(
        &state.tailscale,
        &state.paths,
        &config,
        Some(&web_status),
    )
    .await)
}

#[tauri::command]
pub async fn start_tailscale_login(state: State<'_, AppState>) -> Result<TailscaleLogin, ApiError> {
    let config = WebService::load_config(&state.paths)
        .await
        .map_err(ApiError::from)?;
    let web_status = WebService::status(&state.web_service, &config).await;
    Ok(TailscaleService::start_login(
        &state.tailscale,
        &state.paths,
        &config,
        Some(&web_status),
    )
    .await)
}

#[tauri::command]
pub async fn disconnect_tailscale(state: State<'_, AppState>) -> Result<TailscaleStatus, ApiError> {
    let config = WebService::load_config(&state.paths)
        .await
        .map_err(ApiError::from)?;
    Ok(TailscaleService::disconnect(&state.tailscale, &state.paths, &config).await)
}
