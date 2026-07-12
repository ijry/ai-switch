use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::target_app::TargetApp;
use crate::services::target_service::TargetService;
use tauri::State;

#[tauri::command]
pub async fn list_target_apps(state: State<'_, AppState>) -> Result<Vec<TargetApp>, ApiError> {
    TargetService::list_targets(&state.pool)
        .await
        .map_err(ApiError::from)
}
