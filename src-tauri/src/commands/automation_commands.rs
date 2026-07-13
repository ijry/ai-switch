use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::automation::{
    BulkOperation, ItemTag, NewBulkOperation, NewItemTag, NewPluginLink, NewTagRecord, PluginLink,
    SetPluginLinkEnabledRequest, TagRecord,
};
use crate::services::automation_service::AutomationService;
use tauri::State;

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagRecord>, ApiError> {
    AutomationService::list_tags(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_tag(
    state: State<'_, AppState>,
    request: NewTagRecord,
) -> Result<TagRecord, ApiError> {
    AutomationService::create_tag(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_item_tags(state: State<'_, AppState>) -> Result<Vec<ItemTag>, ApiError> {
    AutomationService::list_item_tags(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_item_tag(
    state: State<'_, AppState>,
    request: NewItemTag,
) -> Result<ItemTag, ApiError> {
    AutomationService::create_item_tag(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_plugin_links(state: State<'_, AppState>) -> Result<Vec<PluginLink>, ApiError> {
    AutomationService::list_plugin_links(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_plugin_link(
    state: State<'_, AppState>,
    request: NewPluginLink,
) -> Result<PluginLink, ApiError> {
    AutomationService::create_plugin_link(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_plugin_link_enabled(
    state: State<'_, AppState>,
    request: SetPluginLinkEnabledRequest,
) -> Result<PluginLink, ApiError> {
    AutomationService::set_plugin_link_enabled(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_bulk_operations(
    state: State<'_, AppState>,
) -> Result<Vec<BulkOperation>, ApiError> {
    AutomationService::list_bulk_operations(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_bulk_operation(
    state: State<'_, AppState>,
    request: NewBulkOperation,
) -> Result<BulkOperation, ApiError> {
    AutomationService::create_bulk_operation(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
