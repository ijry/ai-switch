use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::external_client_import::{
    ExternalClientImportOutcome, ExternalClientImportPreview, ImportExternalClientAccountsInput,
    PreviewExternalClientImportInput,
};
use crate::services::external_client_import_service;
use tauri::State;

#[tauri::command]
pub async fn preview_external_client_import(
    state: State<'_, AppState>,
    input: PreviewExternalClientImportInput,
) -> Result<ExternalClientImportPreview, ApiError> {
    external_client_import_service::preview_external_client_import(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn import_external_client_accounts(
    state: State<'_, AppState>,
    input: ImportExternalClientAccountsInput,
) -> Result<ExternalClientImportOutcome, ApiError> {
    external_client_import_service::import_external_client_accounts(&state.pool, input)
        .await
        .map_err(ApiError::from)
}
