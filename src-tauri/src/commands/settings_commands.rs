use crate::app_state::AppState;
use crate::core::settings::{get_settings_core, save_settings_core};
use crate::error::ApiError;
use crate::models::settings::AppSettings;
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, ApiError> {
    get_settings_core(&state.paths).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, ApiError> {
    save_settings_core(&state.paths, settings)
        .await
        .map_err(ApiError::from)
}
