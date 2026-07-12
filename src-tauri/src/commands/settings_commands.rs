use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::settings::AppSettings;
use crate::services::settings_service::SettingsService;
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, ApiError> {
    SettingsService::load(&state.paths)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, ApiError> {
    SettingsService::save(&state.paths, &settings)
        .await
        .map_err(ApiError::from)?;
    Ok(settings)
}
