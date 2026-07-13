use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::prompt_asset::{NewPromptAsset, PromptAsset, SetPromptAssetEnabledRequest};
use crate::services::prompt_asset_service::PromptAssetService;
use tauri::State;

#[tauri::command]
pub async fn list_prompt_assets(state: State<'_, AppState>) -> Result<Vec<PromptAsset>, ApiError> {
    PromptAssetService::list_prompt_assets(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_prompt_asset(
    state: State<'_, AppState>,
    request: NewPromptAsset,
) -> Result<PromptAsset, ApiError> {
    PromptAssetService::create_prompt_asset(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_prompt_asset_enabled(
    state: State<'_, AppState>,
    request: SetPromptAssetEnabledRequest,
) -> Result<PromptAsset, ApiError> {
    PromptAssetService::set_prompt_asset_enabled(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
