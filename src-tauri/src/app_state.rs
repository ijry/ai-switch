use crate::paths::AppPaths;
use crate::services::route_proxy_service::RouteProxyRuntimeState;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub paths: AppPaths,
    pub pool: SqlitePool,
    pub route_proxy: RouteProxyRuntimeState,
}
