use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::route_credential::{
    CreateApiRouteCredentialInput, ImportOfficialFilesInput, ImportOfficialTextInput,
    RouteCredential, RouteCredentialImportResult, UpdateRouteCredentialInput,
};
use crate::services::route_credential_service::RouteCredentialService;
use tauri::State;

#[tauri::command]
pub async fn list_route_credentials(
    state: State<'_, AppState>,
    platform: String,
) -> Result<Vec<RouteCredential>, ApiError> {
    RouteCredentialService::list(&state.pool, platform)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_route_credential(
    state: State<'_, AppState>,
    id: String,
) -> Result<RouteCredential, ApiError> {
    RouteCredentialService::get(&state.pool, id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_api_route_credential(
    state: State<'_, AppState>,
    input: CreateApiRouteCredentialInput,
) -> Result<RouteCredential, ApiError> {
    RouteCredentialService::create_api(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn import_official_route_credentials_from_text(
    state: State<'_, AppState>,
    input: ImportOfficialTextInput,
) -> Result<RouteCredentialImportResult, ApiError> {
    RouteCredentialService::import_official_text(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn import_official_route_credentials_from_files(
    state: State<'_, AppState>,
    input: ImportOfficialFilesInput,
) -> Result<RouteCredentialImportResult, ApiError> {
    RouteCredentialService::import_official_files(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn update_route_credential(
    state: State<'_, AppState>,
    id: String,
    input: UpdateRouteCredentialInput,
) -> Result<RouteCredential, ApiError> {
    RouteCredentialService::update(&state.pool, id, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn delete_route_credential(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), ApiError> {
    RouteCredentialService::delete(&state.pool, id)
        .await
        .map_err(ApiError::from)
}
