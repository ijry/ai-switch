use std::sync::atomic::Ordering;

use crate::app_state::AppState;
use crate::core::settings::{get_settings_core, save_settings_core};
use crate::error::ApiError;
use crate::models::settings::{AppSettings, AppSettingsView};
use crate::CloseToTrayState;
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettingsView, ApiError> {
    get_settings_core(&state.paths, &state.deeplink_protocols)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_settings(
    close_to_tray: State<'_, CloseToTrayState>,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettingsView, ApiError> {
    let view = save_settings_core(&state.paths, &state.deeplink_protocols, settings.clone())
        .await
        .map_err(ApiError::from)?;
    // Keep the runtime mirror in sync so the close button reacts without
    // re-reading settings.json on every click.
    close_to_tray
        .enabled
        .store(settings.close_to_tray, Ordering::SeqCst);
    Ok(view)
}
