use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::import_job::ImportJob;
use crate::services::import_service::{ExampleJsonImportRequest, ImportService};
use tauri::State;

#[tauri::command]
pub async fn import_example_json(
    state: State<'_, AppState>,
    request: ExampleJsonImportRequest,
) -> Result<ImportJob, ApiError> {
    ImportService::import_example_json(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
