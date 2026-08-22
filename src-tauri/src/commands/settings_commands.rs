use crate::app_state::AppState;
use crate::core::settings::{get_settings_core, save_settings_core};
use crate::error::ApiError;
use crate::models::settings::{AppSettings, AppSettingsView};
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettingsView, ApiError> {
    get_settings_core(&state.paths, &state.deeplink_protocols)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettingsView, ApiError> {
    save_settings_core(
        &state.paths,
        &state.deeplink_protocols,
        &state.route_proxy,
        settings,
    )
    .await
    .map_err(ApiError::from)
}
