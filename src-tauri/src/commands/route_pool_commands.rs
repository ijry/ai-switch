use crate::app_state::AppState;
use crate::error::{ApiError, AppError};
use crate::models::route_pool::{
    FetchedRouteModel, RouteModelsFetchRequest, RoutePoolModelTestOutcome,
    RoutePoolModelTestRequest, RoutePoolRouteOutcome, RoutePoolRouteRequest, RoutePoolState,
    SetRoutePoolMembersInput,
};
use crate::services::route_model_fetch_service::RouteModelFetchService;
use crate::services::route_model_test_service::RouteModelTestService;
use crate::services::route_pool_service::RoutePoolService;
use crate::services::route_proxy_https_service::RouteProxyHttpsService;
use crate::services::route_proxy_service::RouteProxyService;
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

#[tauri::command]
pub async fn route_pool_test_model(
    state: State<'_, AppState>,
    request: RoutePoolModelTestRequest,
) -> Result<RoutePoolModelTestOutcome, ApiError> {
    if route_model_test_targets_single_account(&request) {
        return RouteModelTestService::test_model(&state.pool, request)
            .await
            .map_err(ApiError::from);
    }

    let base_url = route_model_test_proxy_base_url(&state).await?;
    RouteModelTestService::test_model_through_proxy(&state.pool, request, &base_url)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn fetch_route_models(
    request: RouteModelsFetchRequest,
) -> Result<Vec<FetchedRouteModel>, ApiError> {
    RouteModelFetchService::fetch(request)
        .await
        .map_err(ApiError::from)
}

fn route_model_test_targets_single_account(request: &RoutePoolModelTestRequest) -> bool {
    request
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|account_id| !account_id.is_empty())
        .is_some()
}

async fn route_model_test_proxy_base_url(state: &AppState) -> Result<String, ApiError> {
    let status = RouteProxyService::status(&state.route_proxy).await;
    let status = if status
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|base_url| !base_url.is_empty())
        .is_some()
    {
        status
    } else {
        RouteProxyHttpsService::start_proxy(state)
            .await
            .map_err(ApiError::from)?
    };

    status
        .base_url
        .map(|base_url| base_url.trim().to_string())
        .filter(|base_url| !base_url.is_empty())
        .ok_or_else(|| {
            ApiError::from(AppError::Validation {
                code: "validation.route_proxy_not_running",
                message: "Start the route proxy before testing the route pool".to_string(),
                details: None,
                recoverable: true,
            })
        })
}
