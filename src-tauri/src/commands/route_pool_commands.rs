use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::route_pool::{
    RoutePoolRouteOutcome, RoutePoolRouteRequest, RoutePoolState, SetRoutePoolMembersInput,
};
use crate::services::route_pool_service::RoutePoolService;
use tauri::State;

#[tauri::command]
pub async fn get_route_pool(
    state: State<'_, AppState>,
    platform: String,
    since: Option<String>,
    request_page: Option<i64>,
    request_page_size: Option<i64>,
) -> Result<RoutePoolState, ApiError> {
    RoutePoolService::get(
        &state.pool,
        platform,
        since,
        request_page,
        request_page_size,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_route_pool_members(
    state: State<'_, AppState>,
    input: SetRoutePoolMembersInput,
) -> Result<RoutePoolState, ApiError> {
    RoutePoolService::set_members(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn route_pool_route_once(
    state: State<'_, AppState>,
    request: RoutePoolRouteRequest,
) -> Result<RoutePoolRouteOutcome, ApiError> {
    RoutePoolService::route_once(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
