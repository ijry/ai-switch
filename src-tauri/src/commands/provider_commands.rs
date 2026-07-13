use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::provider::Provider;
use crate::models::provider_preset::{
    CreateProviderFromPresetOutcome, CreateProviderFromPresetRequest, ProviderPreset,
};
use crate::models::provider_switch::{ProviderSwitchOutcome, ProviderSwitchRequest};
use crate::services::provider_preset_service::ProviderPresetService;
use crate::services::provider_switch_service::ProviderSwitchService;
use tauri::State;

#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> Result<Vec<Provider>, ApiError> {
    ProviderSwitchService::list_providers(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_provider_presets() -> Result<Vec<ProviderPreset>, ApiError> {
    Ok(ProviderPresetService::list_presets())
}

#[tauri::command]
pub async fn create_provider_from_preset(
    state: State<'_, AppState>,
    request: CreateProviderFromPresetRequest,
) -> Result<CreateProviderFromPresetOutcome, ApiError> {
    ProviderPresetService::create_provider_from_preset(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn switch_target_provider(
    state: State<'_, AppState>,
    request: ProviderSwitchRequest,
) -> Result<ProviderSwitchOutcome, ApiError> {
    ProviderSwitchService::switch_provider(&state.pool, &state.paths, request)
        .await
        .map_err(ApiError::from)
}
