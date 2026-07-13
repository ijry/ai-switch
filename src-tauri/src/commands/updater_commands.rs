use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::updater::{NewUpdateChannel, NewUpdateCheck, UpdateChannel, UpdateCheck};
use crate::services::updater_service::UpdaterService;
use tauri::State;

#[tauri::command]
pub async fn list_update_channels(
    state: State<'_, AppState>,
) -> Result<Vec<UpdateChannel>, ApiError> {
    UpdaterService::list_update_channels(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_update_channel(
    state: State<'_, AppState>,
    request: NewUpdateChannel,
) -> Result<UpdateChannel, ApiError> {
    UpdaterService::create_update_channel(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_update_checks(state: State<'_, AppState>) -> Result<Vec<UpdateCheck>, ApiError> {
    UpdaterService::list_update_checks(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_update_check(
    state: State<'_, AppState>,
    request: NewUpdateCheck,
) -> Result<UpdateCheck, ApiError> {
    UpdaterService::create_update_check(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
