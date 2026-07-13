use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::wakeup::{
    ListWakeupRunsRequest, NewWakeupRun, NewWakeupTask, SetWakeupTaskEnabledRequest, WakeupRun,
    WakeupTask,
};
use crate::services::wakeup_service::WakeupService;
use tauri::State;

#[tauri::command]
pub async fn list_wakeup_tasks(state: State<'_, AppState>) -> Result<Vec<WakeupTask>, ApiError> {
    WakeupService::list_wakeup_tasks(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_wakeup_task(
    state: State<'_, AppState>,
    request: NewWakeupTask,
) -> Result<WakeupTask, ApiError> {
    WakeupService::create_wakeup_task(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_wakeup_task_enabled(
    state: State<'_, AppState>,
    request: SetWakeupTaskEnabledRequest,
) -> Result<WakeupTask, ApiError> {
    WakeupService::set_wakeup_task_enabled(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_wakeup_runs(
    state: State<'_, AppState>,
    request: ListWakeupRunsRequest,
) -> Result<Vec<WakeupRun>, ApiError> {
    WakeupService::list_wakeup_runs(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_wakeup_run(
    state: State<'_, AppState>,
    request: NewWakeupRun,
) -> Result<WakeupRun, ApiError> {
    WakeupService::create_wakeup_run(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
