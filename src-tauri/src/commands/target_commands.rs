use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::target_app::TargetApp;
use crate::models::target_state::TargetSwitchStatus;
use crate::services::provider_switch_service::ProviderSwitchService;
use crate::services::target_service::TargetService;
use tauri::State;

#[tauri::command]
pub async fn list_target_apps(state: State<'_, AppState>) -> Result<Vec<TargetApp>, ApiError> {
    TargetService::list_targets(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_target_switch_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<TargetSwitchStatus>, ApiError> {
    ProviderSwitchService::list_target_switch_statuses(&state.pool)
        .await
        .map_err(ApiError::from)
}
