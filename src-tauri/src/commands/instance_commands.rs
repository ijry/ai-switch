use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::instance::{ManagedInstance, NewManagedInstance, SetInstanceStatusRequest};
use crate::services::instance_service::InstanceService;
use tauri::State;

#[tauri::command]
pub async fn list_instances(state: State<'_, AppState>) -> Result<Vec<ManagedInstance>, ApiError> {
    InstanceService::list_instances(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_instance(
    state: State<'_, AppState>,
    request: NewManagedInstance,
) -> Result<ManagedInstance, ApiError> {
    InstanceService::create_instance(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_instance_status(
    state: State<'_, AppState>,
    request: SetInstanceStatusRequest,
) -> Result<ManagedInstance, ApiError> {
    InstanceService::set_instance_status(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
