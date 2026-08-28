use std::sync::Arc;

use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::services::tailscale_service::{TailscaleLogin, TailscaleStatus};
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
    let saved = WebService::save_config_and_reconcile(&state, &config)
        .await
        .map_err(ApiError::from)?;

    Ok(saved)
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
    let app_state = Arc::new(state.inner().clone());
    WebService::start(app_state).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn stop_web_server(state: State<'_, AppState>) -> Result<WebServerStatus, ApiError> {
    Ok(WebService::stop(state.inner()).await)
}

#[tauri::command]
pub async fn get_tailscale_status(state: State<'_, AppState>) -> Result<TailscaleStatus, ApiError> {
    WebService::tailscale_status(state.inner())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn start_tailscale_login(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<TailscaleLogin, ApiError> {
    let mut login = WebService::start_tailscale_login(state.inner())
        .await
        .map_err(ApiError::from)?;

    if let Some(login_url) = login
        .login_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Err(error) = app.opener().open_url(login_url, None::<&str>) {
            login.message = format!("Sign-in page ready, but browser open failed: {error}");
        }
    }

    Ok(login)
}

#[tauri::command]
pub async fn start_tailscale_with_auth_key(
    state: State<'_, AppState>,
    auth_key: String,
) -> Result<TailscaleStatus, ApiError> {
    WebService::start_tailscale_with_auth_key(state.inner(), auth_key)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn disconnect_tailscale(state: State<'_, AppState>) -> Result<TailscaleStatus, ApiError> {
    WebService::disconnect_tailscale(state.inner())
        .await
        .map_err(ApiError::from)
}
