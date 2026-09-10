use super::models::*;
use super::repository::ImageGenerationRepository;
use super::service::{
    GenerateImageInput, GenerateImageOutcome, ImageAssetContent, ImageGenerationService,
};
use crate::app_state::AppState;
use crate::error::ApiError;
use tauri::State;

#[tauri::command]
pub async fn imagegen_list_sessions(
    state: State<'_, AppState>,
    include_archived: Option<bool>,
) -> Result<Vec<ImageSession>, ApiError> {
    ImageGenerationRepository::list_sessions(&state.pool, include_archived.unwrap_or(false))
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
pub async fn imagegen_create_session(
    state: State<'_, AppState>,
    input: CreateImageSessionInput,
) -> Result<ImageSession, ApiError> {
    ImageGenerationRepository::create_session(&state.pool, input)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
pub async fn imagegen_update_session(
    state: State<'_, AppState>,
    input: UpdateImageSessionInput,
) -> Result<ImageSession, ApiError> {
    ImageGenerationRepository::update_session(&state.pool, input)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
pub async fn imagegen_delete_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), ApiError> {
    ImageGenerationRepository::delete_session(&state.pool, &id)
        .await
        .map_err(ApiError::from)?;
    super::storage::delete_session_assets(&state.paths.imagegen_dir, &id)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
pub async fn imagegen_get_conversation(
    state: State<'_, AppState>,
    id: String,
) -> Result<ImageConversation, ApiError> {
    ImageGenerationRepository::conversation(&state.pool, &id)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
pub async fn imagegen_list_models(
    state: State<'_, AppState>,
    platform: String,
) -> Result<Vec<ImageModelOption>, ApiError> {
    ImageGenerationService::list_models(&state, &platform)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
pub async fn imagegen_generate(
    state: State<'_, AppState>,
    input: GenerateImageInput,
) -> Result<GenerateImageOutcome, ApiError> {
    ImageGenerationService::generate(&state, input)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
pub async fn imagegen_read_asset(
    state: State<'_, AppState>,
    id: String,
) -> Result<ImageAssetContent, ApiError> {
    ImageGenerationService::read_asset(&state, &id)
        .await
        .map_err(ApiError::from)
}
