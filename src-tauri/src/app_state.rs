use crate::paths::AppPaths;
use crate::services::route_proxy_service::RouteProxyRuntimeState;
use crate::terminal_manager::TerminalManager;
use crate::web::event_bridge::WebEventBroadcaster;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub paths: AppPaths,
    pub pool: SqlitePool,
    pub route_proxy: RouteProxyRuntimeState,
    pub terminals: TerminalManager,
    pub event_broadcaster: Arc<WebEventBroadcaster>,
}
