use crate::app_state::AppState;
use crate::error::{ApiError, AppError};
use crate::models::route_proxy_https::{RouteProxyHttpsOperationOutcome, RouteProxyHttpsStatus};
use crate::services::route_proxy_https_service::RouteProxyHttpsService;
use tauri::{AppHandle, State};
use tauri_plugin_shell::ShellExt;

#[tauri::command]
pub async fn get_route_proxy_https_status(
    state: State<'_, AppState>,
) -> Result<RouteProxyHttpsStatus, ApiError> {
    RouteProxyHttpsService::status_for_state(&state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn enable_route_proxy_https(
    state: State<'_, AppState>,
) -> Result<RouteProxyHttpsOperationOutcome, ApiError> {
    RouteProxyHttpsService::enable(&state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn disable_route_proxy_https(
    state: State<'_, AppState>,
) -> Result<RouteProxyHttpsOperationOutcome, ApiError> {
    RouteProxyHttpsService::disable(&state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn reimport_route_proxy_root_ca(
    state: State<'_, AppState>,
) -> Result<RouteProxyHttpsOperationOutcome, ApiError> {
    RouteProxyHttpsService::reimport_root_ca(&state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn regenerate_route_proxy_https_certificates(
    state: State<'_, AppState>,
) -> Result<RouteProxyHttpsOperationOutcome, ApiError> {
    RouteProxyHttpsService::regenerate_certificates(&state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn uninstall_route_proxy_root_ca(
    state: State<'_, AppState>,
) -> Result<RouteProxyHttpsOperationOutcome, ApiError> {
    RouteProxyHttpsService::uninstall_root_ca(&state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_route_proxy_https_certificates(
    state: State<'_, AppState>,
) -> Result<RouteProxyHttpsStatus, ApiError> {
    RouteProxyHttpsService::delete_certificates(&state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn open_route_proxy_https_certificate_dir(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ApiError> {
    if !state.paths.route_proxy_https_dir.is_dir() {
        return Err(ApiError::from(AppError::Validation {
            code: "validation.route_proxy_https_certificate_dir_missing",
            message: "Generate local HTTPS certificates before opening their directory".to_string(),
            details: None,
            recoverable: true,
        }));
    }
    app.shell()
        .open(
            state.paths.route_proxy_https_dir.to_string_lossy().as_ref(),
            None,
        )
        .map_err(|error| {
            ApiError::from(AppError::Filesystem {
                code: "filesystem.route_proxy_https_open_certificate_dir",
                message: "Could not open the local HTTPS certificate directory".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            })
        })
}
