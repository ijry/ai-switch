use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::disk_space::DiskSpaceStatus;
use crate::services::disk_space_service::DiskSpaceService;
use tauri::State;

/// Free space on the volumes the app writes to.
///
/// Never actually fails — a volume that will not report its size is left out of
/// the result. The `Result` is Tauri's requirement: an async command that borrows
/// state has to return one.
#[tauri::command]
pub async fn get_disk_space_status(
    state: State<'_, AppState>,
) -> Result<DiskSpaceStatus, ApiError> {
    Ok(DiskSpaceService::status(&state.paths.data_dir))
}
