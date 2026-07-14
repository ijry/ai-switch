use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use crate::app_state::AppState;
use crate::database::open_migrated_pool;
use crate::paths::AppPaths;
use crate::services::route_proxy_service::RouteProxyRuntimeState;
use crate::services::web_service::WebServiceRuntimeState;
use crate::terminal_manager::TerminalManager;
use crate::web::event_bridge::WebEventBroadcaster;
use crate::web::router::build_router;

pub async fn run_from_env() -> Result<(), String> {
    let host = std::env::var("AI_SWITCH_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("AI_SWITCH_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3090);
    let token = std::env::var("AI_SWITCH_TOKEN").unwrap_or_default();
    let static_dir = std::env::var("AI_SWITCH_STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../dist"));

    let paths = AppPaths::resolve().map_err(|error| error.to_string())?;
    paths.ensure().await.map_err(|error| error.to_string())?;
    let pool = open_migrated_pool(&paths.database_file, &paths.backups_dir)
        .await
        .map_err(|error| error.to_string())?;
    let state = Arc::new(AppState {
        paths,
        pool,
        route_proxy: RouteProxyRuntimeState::default(),
        web_service: WebServiceRuntimeState::default(),
        terminals: TerminalManager::default(),
        event_broadcaster: Arc::new(WebEventBroadcaster::new()),
    });

    let router = build_router(state, token, static_dir);
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|error| format!("Invalid server address: {error}"))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|error| format!("Could not bind server: {error}"))?;

    axum::serve(listener, router)
        .await
        .map_err(|error| format!("Server error: {error}"))
}
