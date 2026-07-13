use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::tray::TrayMenuStatus;
use crate::tray;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn refresh_tray_menu(
    app: AppHandle,
    _state: State<'_, AppState>,
) -> Result<TrayMenuStatus, ApiError> {
    tray::refresh_tray_menu(&app).await.map_err(ApiError::from)
}
