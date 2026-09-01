use crate::paths::AppPaths;
use crate::services::config_write_service::ConfigWriteRuntimeState;
use crate::services::deeplink_protocol_service::DeepLinkProtocolRuntime;
use crate::services::route_proxy_service::RouteProxyRuntimeState;
use crate::services::tailscale_service::TailscaleRuntimeState;
use crate::services::web_service::WebServiceRuntimeState;
use crate::terminal_manager::TerminalManager;
use crate::web::event_bridge::WebEventBroadcaster;
use crate::web::terminal_hub::TerminalHub;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub paths: AppPaths,
    pub pool: SqlitePool,
    pub config_writes: ConfigWriteRuntimeState,
    pub deeplink_protocols: DeepLinkProtocolRuntime,
    pub route_proxy: RouteProxyRuntimeState,
    pub web_service: WebServiceRuntimeState,
    pub tailscale: TailscaleRuntimeState,
    pub terminals: TerminalManager,
    pub terminal_hub: Arc<TerminalHub>,
    pub event_broadcaster: Arc<WebEventBroadcaster>,
}
