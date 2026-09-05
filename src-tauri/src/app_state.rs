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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Runtime mirror of `AppSettings::close_to_tray`.
///
/// The window close handler runs on the main thread and has to answer
/// synchronously, so it cannot await a read of settings.json. Every write path
/// goes through `save_settings_core`, which refreshes this after the file lands.
#[derive(Clone)]
pub struct CloseToTrayRuntime {
    enabled: Arc<AtomicBool>,
}

impl Default for CloseToTrayRuntime {
    /// Matches `AppSettings::defaults_for_data_dir`, so a state built without an
    /// explicit value behaves like a fresh install rather than like the one
    /// setting nobody chose.
    fn default() -> Self {
        Self::new(true)
    }
}

impl CloseToTrayRuntime {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }
}

#[derive(Clone)]
pub struct AppState {
    pub paths: AppPaths,
    pub pool: SqlitePool,
    pub config_writes: ConfigWriteRuntimeState,
    pub deeplink_protocols: DeepLinkProtocolRuntime,
    pub close_to_tray: CloseToTrayRuntime,
    pub route_proxy: RouteProxyRuntimeState,
    pub web_service: WebServiceRuntimeState,
    pub tailscale: TailscaleRuntimeState,
    pub terminals: TerminalManager,
    pub terminal_hub: Arc<TerminalHub>,
    pub event_broadcaster: Arc<WebEventBroadcaster>,
}
