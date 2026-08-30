use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::route_credential::{
    CopyRouteCredentialInput, CreateApiRouteCredentialInput, ImportOfficialFilesInput,
    ImportOfficialTextInput, ReorderRouteCredentialInput, RouteCredential,
    RouteCredentialImportResult, RouteCredentialPage, RouteCredentialPageRequest,
    UpdateRouteCredentialInput,
};
use crate::services::route_credential_service::RouteCredentialService;
use crate::services::route_quota_service::{QuotaRefreshOutcome, RouteQuotaService};
use crate::services::route_recovery_service::{RecoveryRule, RouteRecoveryService};
use tauri::State;

#[tauri::command]
pub async fn list_route_credentials(
    state: State<'_, AppState>,
    platform: String,
) -> Result<Vec<RouteCredential>, ApiError> {
    let activity = state.route_proxy.activity();
    RouteCredentialService::list_with_activity(&state.pool, &activity, platform)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_route_credentials_page(
    state: State<'_, AppState>,
    input: RouteCredentialPageRequest,
) -> Result<RouteCredentialPage, ApiError> {
    let activity = state.route_proxy.activity();
    RouteCredentialService::page_with_activity(&state.pool, &activity, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn reorder_route_credentials(
    state: State<'_, AppState>,
    input: ReorderRouteCredentialInput,
) -> Result<RouteCredentialPage, ApiError> {
    RouteCredentialService::reorder(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_route_credential(
    state: State<'_, AppState>,
    id: String,
) -> Result<RouteCredential, ApiError> {
    let activity = state.route_proxy.activity();
    RouteCredentialService::get_with_activity(&state.pool, &activity, id)
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
pub async fn copy_route_credential(
    state: State<'_, AppState>,
    id: String,
    input: Option<CopyRouteCredentialInput>,
) -> Result<RouteCredential, ApiError> {
    RouteCredentialService::copy_with_options(&state.pool, id, input.unwrap_or_default())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_route_credential_recovery(
    state: State<'_, AppState>,
    id: String,
    rule: RecoveryRule,
) -> Result<RouteCredential, ApiError> {
    RouteRecoveryService::set_rule(&state.pool, id, rule)
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

#[tauri::command]
pub async fn archive_route_credentials(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<(), ApiError> {
    RouteCredentialService::archive(&state.pool, ids)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn restore_route_credentials(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<(), ApiError> {
    RouteCredentialService::restore(&state.pool, ids)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_route_credential_statuses(
    state: State<'_, AppState>,
    ids: Vec<String>,
    status: String,
) -> Result<(), ApiError> {
    RouteCredentialService::set_statuses(&state.pool, ids, status)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn refresh_route_credential_quota(
    state: State<'_, AppState>,
    id: String,
) -> Result<QuotaRefreshOutcome, ApiError> {
    RouteQuotaService::refresh_one(&state.pool, id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn refresh_route_credentials_quota(
    state: State<'_, AppState>,
    platform: String,
) -> Result<Vec<QuotaRefreshOutcome>, ApiError> {
    RouteQuotaService::refresh_platform(&state.pool, platform)
        .await
        .map_err(ApiError::from)
}
