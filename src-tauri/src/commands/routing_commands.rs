use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::routing::{
    FailoverPolicy, NewFailoverPolicy, NewProxyProfile, NewUsageEvent, ProxyProfile, UsageEvent,
};
use crate::services::routing_service::RoutingService;
use tauri::State;

#[tauri::command]
pub async fn list_proxy_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<ProxyProfile>, ApiError> {
    RoutingService::list_proxy_profiles(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_proxy_profile(
    state: State<'_, AppState>,
    request: NewProxyProfile,
) -> Result<ProxyProfile, ApiError> {
    RoutingService::create_proxy_profile(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_failover_policies(
    state: State<'_, AppState>,
) -> Result<Vec<FailoverPolicy>, ApiError> {
    RoutingService::list_failover_policies(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_failover_policy(
    state: State<'_, AppState>,
    request: NewFailoverPolicy,
) -> Result<FailoverPolicy, ApiError> {
    RoutingService::create_failover_policy(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_usage_events(state: State<'_, AppState>) -> Result<Vec<UsageEvent>, ApiError> {
    RoutingService::list_usage_events(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_usage_event(
    state: State<'_, AppState>,
    request: NewUsageEvent,
) -> Result<UsageEvent, ApiError> {
    RoutingService::create_usage_event(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
