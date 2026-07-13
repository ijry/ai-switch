use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::sync::{CreateSyncSnapshotRequest, NewSyncProfile, SyncProfile, SyncSnapshot};
use crate::services::sync_service::SyncService;
use tauri::State;

#[tauri::command]
pub async fn list_sync_profiles(state: State<'_, AppState>) -> Result<Vec<SyncProfile>, ApiError> {
    SyncService::list_sync_profiles(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_sync_profile(
    state: State<'_, AppState>,
    request: NewSyncProfile,
) -> Result<SyncProfile, ApiError> {
    SyncService::create_sync_profile(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_sync_snapshots(
    state: State<'_, AppState>,
) -> Result<Vec<SyncSnapshot>, ApiError> {
    SyncService::list_sync_snapshots(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_sync_snapshot(
    state: State<'_, AppState>,
    request: CreateSyncSnapshotRequest,
) -> Result<SyncSnapshot, ApiError> {
    SyncService::create_sync_snapshot(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
