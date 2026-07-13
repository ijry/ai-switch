use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::import_job::ImportJob;
use crate::services::import_service::{
    DeepLinkImportRequest, ExampleJsonExportOutcome, ExampleJsonImportRequest, ImportService,
    OfficialAccountJsonImportRequest,
};
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

#[tauri::command]
pub async fn export_example_json(
    state: State<'_, AppState>,
) -> Result<ExampleJsonExportOutcome, ApiError> {
    ImportService::export_example_json(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn import_official_account_json(
    state: State<'_, AppState>,
    request: OfficialAccountJsonImportRequest,
) -> Result<ImportJob, ApiError> {
    ImportService::import_official_account_json(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn import_deep_link(
    state: State<'_, AppState>,
    request: DeepLinkImportRequest,
) -> Result<ImportJob, ApiError> {
    ImportService::import_deep_link(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
