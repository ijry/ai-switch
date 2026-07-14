use std::sync::Arc;

use tauri::State;
use tauri_plugin_shell::ShellExt;

use crate::app_state::AppState;
use crate::error::{ApiError, AppError};
use crate::services::tailscale_service::{TailscaleLogin, TailscaleService, TailscaleStatus};
use crate::services::web_service::{WebServerStatus, WebService, WebServiceConfig};

async fn ensure_web_running(
    state: &AppState,
) -> Result<(WebServiceConfig, WebServerStatus), ApiError> {
    let config = WebService::load_config(&state.paths)
        .await
        .map_err(ApiError::from)?;
    let status = WebService::status(&state.web_service, &config).await;
    if status.running {
        return Ok((config, status));
    }
    let app_state = Arc::new(state.clone());
    let status = WebService::start(app_state, config.clone())
        .await
        .map_err(ApiError::from)?;
    Ok((config, status))
}

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
    let saved = WebService::save_config(&state.paths, &config)
        .await
        .map_err(ApiError::from)?;

    // Persist first, then rebind secure network so private/public switches take effect
    // without requiring a full web restart.
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
    Ok(WebService::stop(state.inner(), &config).await)
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
pub async fn start_tailscale_login(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<TailscaleLogin, ApiError> {
    // Remote proxy only works when the local web backend is listening.
    let (config, web_status) = ensure_web_running(state.inner()).await?;
    let mut login = TailscaleService::start_login(
        &state.tailscale,
        &state.paths,
        &config,
        Some(&web_status),
    )
    .await;

    if let Some(login_url) = login
        .login_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        #[allow(deprecated)]
        if let Err(error) = app.shell().open(login_url, None) {
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
    let (mut config, web_status) = ensure_web_running(state.inner()).await?;
    TailscaleService::start_with_auth_key(
        &state.tailscale,
        &state.paths,
        &mut config,
        Some(&web_status),
        auth_key,
    )
    .await
    .map_err(|message| {
        ApiError::from(AppError::Validation {
            code: "tailscale.auth_key",
            message,
            details: None,
            recoverable: true,
        })
    })
}

#[tauri::command]
pub async fn disconnect_tailscale(state: State<'_, AppState>) -> Result<TailscaleStatus, ApiError> {
    let config = WebService::load_config(&state.paths)
        .await
        .map_err(ApiError::from)?;
    Ok(TailscaleService::disconnect(&state.tailscale, &state.paths, &config).await)
}
