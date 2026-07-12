use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::account::{NewOfficialAccount, OfficialAccount};
use crate::models::batch::{Batch, BatchGroup, NewBatch};
use crate::models::provider::{NewProvider, Provider};
use crate::services::batch_service::BatchService;
use serde::Deserialize;
use tauri::State;

#[derive(Debug, Deserialize)]
pub struct CreateProviderRequest {
    pub provider: NewProvider,
    pub batch_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub account: NewOfficialAccount,
    pub batch_id: Option<String>,
}

#[tauri::command]
pub async fn create_batch(state: State<'_, AppState>, input: NewBatch) -> Result<Batch, ApiError> {
    BatchService::create_batch(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_batch_groups(
    state: State<'_, AppState>,
    search: Option<String>,
) -> Result<Vec<BatchGroup>, ApiError> {
    BatchService::list_groups(&state.pool, search)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_provider(
    state: State<'_, AppState>,
    request: CreateProviderRequest,
) -> Result<Provider, ApiError> {
    BatchService::create_provider(&state.pool, request.provider, request.batch_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_official_account(
    state: State<'_, AppState>,
    request: CreateAccountRequest,
) -> Result<OfficialAccount, ApiError> {
    BatchService::create_official_account(&state.pool, request.account, request.batch_id)
        .await
        .map_err(ApiError::from)
}
