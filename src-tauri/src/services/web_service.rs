use crate::app_state::AppState;
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::web::router::build_router;
use crate::web::static_assets::resolve_static_dir;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebServiceConfig {
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
    pub auto_start: bool,
    pub tailscale_enabled: bool,
    #[serde(default)]
    pub tailscale_hostname: Option<String>,
    #[serde(default)]
    pub tailscale_auth_key_present: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebServerStatus {
    pub running: bool,
    pub host: String,
    pub port: Option<u16>,
    pub base_url: Option<String>,
}

#[derive(Clone, Default)]
pub struct WebServiceRuntimeState {
    inner: Arc<Mutex<WebServiceRuntimeInner>>,
}

#[derive(Default)]
struct WebServiceRuntimeInner {
    status: Option<WebServerStatus>,
    shutdown: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

pub struct WebService;

impl WebService {
    pub async fn load_config(paths: &AppPaths) -> Result<WebServiceConfig, AppError> {
        paths.ensure().await?;
        if !paths.web_service_file.exists() {
            let config = WebServiceConfig::default();
            Self::save_config(paths, &config).await?;
            return Ok(config);
        }

        let contents = tokio::fs::read_to_string(&paths.web_service_file).await?;
        let config: WebServiceConfig = serde_json::from_str(&contents)?;
        let normalized = Self::normalize_config(config.clone());
        if normalized != config {
            Self::save_config(paths, &normalized).await?;
        }
        Ok(normalized)
    }

    pub async fn save_config(
        paths: &AppPaths,
        config: &WebServiceConfig,
    ) -> Result<WebServiceConfig, AppError> {
        paths.ensure().await?;
        let normalized = Self::normalize_config(config.clone());
        let contents = serde_json::to_string_pretty(&normalized)?;
        tokio::fs::write(&paths.web_service_file, contents).await?;
        Ok(normalized)
    }

    pub async fn status(runtime: &WebServiceRuntimeState, config: &WebServiceConfig) -> WebServerStatus {
        runtime.inner.lock().await.status.clone().unwrap_or(WebServerStatus {
            running: false,
            host: config.host.clone(),
            port: None,
            base_url: None,
        })
    }

    pub async fn start(
        state: Arc<AppState>,
        config: WebServiceConfig,
    ) -> Result<WebServerStatus, AppError> {
        let config = Self::normalize_config(config);
        if let Some(status) = state.web_service.inner.lock().await.status.clone() {
            if status.running {
                return Ok(status);
            }
        }

        let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port))
            .await
            .map_err(|error| AppError::Filesystem {
                code: "web_service.bind",
                message: "Could not start web service".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            })?;
        let addr = listener.local_addr().map_err(|error| AppError::Filesystem {
            code: "web_service.addr",
            message: "Could not read web service address".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;
        let host = advertised_host(addr, &config.host);
        let port = addr.port();
        let base_url = format!("http://{host}:{port}");
        let token = config.token.clone().unwrap_or_default();
        let static_dir = resolve_static_dir();
        let router = build_router(Arc::clone(&state), token, static_dir);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let join_handle = tokio::spawn(async move {
            let server = axum::serve(listener, router).with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            let _ = server.await;
        });

        let status = WebServerStatus {
            running: true,
            host,
            port: Some(port),
            base_url: Some(base_url),
        };
        let mut inner = state.web_service.inner.lock().await;
        if let Some(existing) = &inner.status {
            if existing.running {
                let _ = shutdown_tx.send(());
                return Ok(existing.clone());
            }
        }
        inner.status = Some(status.clone());
        inner.shutdown = Some(shutdown_tx);
        inner.join_handle = Some(join_handle);
        Ok(status)
    }

    pub async fn stop(runtime: &WebServiceRuntimeState, config: &WebServiceConfig) -> WebServerStatus {
        let (shutdown, join_handle) = {
            let mut inner = runtime.inner.lock().await;
            inner.status = None;
            (inner.shutdown.take(), inner.join_handle.take())
        };
        if let Some(shutdown) = shutdown {
            let _ = shutdown.send(());
        }
        if let Some(handle) = join_handle {
            let _ = handle.await;
        }
        WebServerStatus {
            running: false,
            host: config.host.clone(),
            port: None,
            base_url: None,
        }
    }

    fn normalize_config(config: WebServiceConfig) -> WebServiceConfig {
        let defaults = WebServiceConfig::default();
        let host = config.host.trim();
        let token = config
            .token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or(defaults.token);
        let hostname = config
            .tailscale_hostname
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        WebServiceConfig {
            host: if host.is_empty() {
                defaults.host
            } else {
                host.to_string()
            },
            port: if config.port == 0 {
                defaults.port
            } else {
                config.port
            },
            token,
            auto_start: config.auto_start,
            tailscale_enabled: config.tailscale_enabled,
            tailscale_hostname: hostname,
            tailscale_auth_key_present: config.tailscale_auth_key_present,
        }
    }
}

impl Default for WebServiceConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3090,
            token: Some(Uuid::new_v4().to_string()),
            auto_start: false,
            tailscale_enabled: false,
            tailscale_hostname: None,
            tailscale_auth_key_present: false,
        }
    }
}

fn advertised_host(addr: SocketAddr, configured_host: &str) -> String {
    if configured_host == "0.0.0.0" {
        "127.0.0.1".to_string()
    } else {
        addr.ip().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::WebServiceConfig;

    #[test]
    fn web_service_config_defaults_keep_auth_key_absent() {
        let config = WebServiceConfig::default();
        assert_eq!(config.tailscale_enabled, false);
        assert_eq!(config.tailscale_auth_key_present, false);
        assert!(config.tailscale_hostname.is_none());
    }
}
