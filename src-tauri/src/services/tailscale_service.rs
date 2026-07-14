use crate::paths::AppPaths;
use crate::services::tailscale_sidecar::{
    resolve_sidecar_path, HttpSidecarControlClient, SidecarControlClient,
};
use crate::services::tailscale_types::TailscaleStartRequest;
use crate::services::web_service::{WebServerStatus, WebService, WebServiceConfig};
use std::sync::Arc;
use tokio::sync::Mutex;

pub use crate::services::tailscale_types::{TailscaleLogin, TailscaleStatus};

const AUTH_KEY_FILE: &str = "auth-key";
const MISSING_COMPONENT_MESSAGE: &str =
    "Built-in network component is missing. Reinstall AI Switch to restore remote access.";

#[derive(Clone, Default)]
pub struct TailscaleRuntimeState {
    inner: Arc<Mutex<TailscaleRuntimeInner>>,
}

#[derive(Default)]
struct TailscaleRuntimeInner {
    client: Option<Arc<dyn SidecarControlClient>>,
    last_status: Option<TailscaleStatus>,
}

pub struct TailscaleService;

impl TailscaleService {
    pub async fn status(
        runtime: &TailscaleRuntimeState,
        paths: &AppPaths,
        config: &WebServiceConfig,
        web_status: Option<&WebServerStatus>,
    ) -> TailscaleStatus {
        Self::status_with_client(runtime, paths, config, web_status, resolve_live_client).await
    }

    pub async fn start_login(
        runtime: &TailscaleRuntimeState,
        paths: &AppPaths,
        config: &WebServiceConfig,
        web_status: Option<&WebServerStatus>,
    ) -> TailscaleLogin {
        Self::start_login_with_client(runtime, paths, config, web_status, resolve_live_client).await
    }

    pub async fn start_with_auth_key(
        runtime: &TailscaleRuntimeState,
        paths: &AppPaths,
        config: &mut WebServiceConfig,
        web_status: Option<&WebServerStatus>,
        auth_key: String,
    ) -> Result<TailscaleStatus, String> {
        Self::start_with_auth_key_with_client(
            runtime,
            paths,
            config,
            web_status,
            auth_key,
            resolve_live_client,
        )
        .await
    }

    pub async fn disconnect(
        runtime: &TailscaleRuntimeState,
        paths: &AppPaths,
        config: &WebServiceConfig,
    ) -> TailscaleStatus {
        Self::disconnect_with_client(runtime, paths, config, resolve_live_client).await
    }

    pub async fn ensure_started(
        runtime: &TailscaleRuntimeState,
        paths: &AppPaths,
        config: &WebServiceConfig,
        web_status: Option<&WebServerStatus>,
    ) -> TailscaleStatus {
        Self::ensure_started_with_client(runtime, paths, config, web_status, resolve_live_client)
            .await
    }

    pub async fn status_with_client<F>(
        runtime: &TailscaleRuntimeState,
        paths: &AppPaths,
        config: &WebServiceConfig,
        web_status: Option<&WebServerStatus>,
        factory: F,
    ) -> TailscaleStatus
    where
        F: FnOnce() -> Option<Arc<dyn SidecarControlClient>>,
    {
        if !config.tailscale_enabled {
            return TailscaleStatus::disabled();
        }

        let client = match Self::ensure_client(runtime, factory).await {
            Ok(client) => client,
            Err(status) => return status,
        };

        match client.status().await {
            Ok(status) => {
                let web_running = web_status.map(|status| status.running).unwrap_or(false);
                // Connected OAuth sessions may come online after login without a proxy listener.
                // Rebind once so MagicDNS stops 502'ing on a dead upstream.
                if status.state == "connected" && web_running && !status.serving {
                    let request = Self::build_start_request(paths, config, web_status, None);
                    if let Ok(rebound) = client.start(request).await {
                        return Self::finalize_status(rebound, config, web_status, paths);
                    }
                }
                Self::finalize_status(status, config, web_status, paths)
            }
            Err(error) => TailscaleStatus::error(error),
        }
    }

    pub async fn start_login_with_client<F>(
        runtime: &TailscaleRuntimeState,
        paths: &AppPaths,
        config: &WebServiceConfig,
        web_status: Option<&WebServerStatus>,
        factory: F,
    ) -> TailscaleLogin
    where
        F: FnOnce() -> Option<Arc<dyn SidecarControlClient>>,
    {
        if !config.tailscale_enabled {
            return TailscaleLogin {
                login_url: None,
                message: "Enable secure network first".to_string(),
            };
        }

        let client = match Self::ensure_client(runtime, factory).await {
            Ok(client) => client,
            Err(status) => {
                return TailscaleLogin {
                    login_url: None,
                    message: status
                        .message
                        .unwrap_or_else(|| MISSING_COMPONENT_MESSAGE.to_string()),
                };
            }
        };

        let request = Self::build_start_request(paths, config, web_status, None);
        if let Err(error) = client.start(request).await {
            return TailscaleLogin {
                login_url: None,
                message: error,
            };
        }

        match client.login_oauth().await {
            Ok(login) => login,
            Err(error) => TailscaleLogin {
                login_url: None,
                message: error,
            },
        }
    }

    pub async fn start_with_auth_key_with_client<F>(
        runtime: &TailscaleRuntimeState,
        paths: &AppPaths,
        config: &mut WebServiceConfig,
        web_status: Option<&WebServerStatus>,
        auth_key: String,
        factory: F,
    ) -> Result<TailscaleStatus, String>
    where
        F: FnOnce() -> Option<Arc<dyn SidecarControlClient>>,
    {
        if !config.tailscale_enabled {
            return Ok(TailscaleStatus::disabled());
        }

        let auth_key = auth_key.trim().to_string();
        if auth_key.is_empty() {
            return Err("Auth key is required".to_string());
        }

        let client = match Self::ensure_client(runtime, factory).await {
            Ok(client) => client,
            Err(status) => {
                return Ok(status);
            }
        };

        Self::persist_auth_key(paths, &auth_key).await?;
        config.tailscale_auth_key_present = true;
        WebService::save_config(paths, config)
            .await
            .map_err(|error| error.to_string())?;

        let request = Self::build_start_request(paths, config, web_status, Some(auth_key));
        let status = client.start(request).await?;
        Ok(Self::finalize_status(status, config, web_status, paths))
    }

    pub async fn ensure_started_with_client<F>(
        runtime: &TailscaleRuntimeState,
        paths: &AppPaths,
        config: &WebServiceConfig,
        web_status: Option<&WebServerStatus>,
        factory: F,
    ) -> TailscaleStatus
    where
        F: FnOnce() -> Option<Arc<dyn SidecarControlClient>>,
    {
        if !config.tailscale_enabled {
            return TailscaleStatus::disabled();
        }

        let auth_key = match Self::load_auth_key(paths).await {
            Ok(value) => value,
            Err(error) => return TailscaleStatus::error(error),
        };

        let client = match Self::ensure_client(runtime, factory).await {
            Ok(client) => client,
            Err(status) => return status,
        };

        // Prefer rebinding an already-connected session so starting web after OAuth works.
        if let Ok(existing) = client.status().await {
            if existing.state == "connected" || auth_key.is_some() || config.tailscale_auth_key_present
            {
                let request = Self::build_start_request(paths, config, web_status, auth_key);
                return match client.start(request).await {
                    Ok(status) => Self::finalize_status(status, config, web_status, paths),
                    Err(error) => TailscaleStatus::error(error),
                };
            }
        } else if auth_key.is_some() || config.tailscale_auth_key_present {
            let request = Self::build_start_request(paths, config, web_status, auth_key);
            return match client.start(request).await {
                Ok(status) => Self::finalize_status(status, config, web_status, paths),
                Err(error) => TailscaleStatus::error(error),
            };
        }

        // Also attempt start from persisted tsnet state (OAuth) so app restart can recover.
        let request = Self::build_start_request(paths, config, web_status, None);
        match client.start(request).await {
            Ok(status) => {
                let finalized = Self::finalize_status(status, config, web_status, paths);
                if finalized.state == "connected" {
                    finalized
                } else {
                    TailscaleStatus::stopped("Secure network is waiting for sign-in")
                }
            }
            Err(_) => TailscaleStatus::stopped("Secure network is waiting for sign-in"),
        }
    }

    pub async fn disconnect_with_client<F>(
        runtime: &TailscaleRuntimeState,
        _paths: &AppPaths,
        config: &WebServiceConfig,
        factory: F,
    ) -> TailscaleStatus
    where
        F: FnOnce() -> Option<Arc<dyn SidecarControlClient>>,
    {
        let client = {
            let inner = runtime.inner.lock().await;
            if let Some(client) = inner.client.clone() {
                Some(client)
            } else {
                drop(inner);
                factory()
            }
        };

        let Some(client) = client else {
            return if config.tailscale_enabled {
                TailscaleStatus::stopped("Secure network stopped")
            } else {
                TailscaleStatus::disabled()
            };
        };

        let status = match client.stop().await {
            Ok(status) => status,
            Err(error) => TailscaleStatus::error(error),
        };

        let mut inner = runtime.inner.lock().await;
        inner.client = None;
        inner.last_status = Some(status.clone());
        status
    }

    async fn ensure_client<F>(
        runtime: &TailscaleRuntimeState,
        factory: F,
    ) -> Result<Arc<dyn SidecarControlClient>, TailscaleStatus>
    where
        F: FnOnce() -> Option<Arc<dyn SidecarControlClient>>,
    {
        let mut inner = runtime.inner.lock().await;
        if let Some(client) = inner.client.clone() {
            return Ok(client);
        }

        match factory() {
            Some(client) => {
                inner.client = Some(Arc::clone(&client));
                Ok(client)
            }
            None => Err(TailscaleStatus::error(MISSING_COMPONENT_MESSAGE)),
        }
    }

    fn build_start_request(
        paths: &AppPaths,
        config: &WebServiceConfig,
        web_status: Option<&WebServerStatus>,
        auth_key: Option<String>,
    ) -> TailscaleStartRequest {
        let hostname = config
            .tailscale_hostname
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "ai-switch".to_string());
        let port = web_status
            .and_then(|status| status.port)
            .unwrap_or(config.port);
        let backend_host = if config.host == "0.0.0.0" {
            "127.0.0.1".to_string()
        } else {
            config.host.clone()
        };

        TailscaleStartRequest {
            state_dir: paths.tailscale_dir.to_string_lossy().to_string(),
            hostname,
            auth_key,
            backend_addr: format!("{backend_host}:{port}"),
            serve_port: port,
        }
    }

    fn finalize_status(
        mut status: TailscaleStatus,
        config: &WebServiceConfig,
        web_status: Option<&WebServerStatus>,
        _paths: &AppPaths,
    ) -> TailscaleStatus {
        if status.state == "connected" {
            let web_running = web_status.map(|status| status.running).unwrap_or(false);
            // Remote URLs are only useful when both the local web backend and the sidecar
            // reverse proxy are actually ready. Advertising earlier produces MagicDNS 502s.
            if web_running && status.serving {
                let port = web_status
                    .and_then(|status| status.port)
                    .unwrap_or(config.port);
                let mut access_urls = Vec::new();
                if let Some(ip) = status.tailnet_ip.as_deref() {
                    access_urls.push(format!("http://{ip}:{port}"));
                }
                if let Some(name) = status.magic_dns_name.as_deref() {
                    access_urls.push(format!("http://{name}:{port}"));
                }
                status.access_urls = access_urls;
                status.serving = !status.access_urls.is_empty();
            } else {
                status.access_urls = Vec::new();
                status.serving = false;
                if status.message.as_deref().unwrap_or("").trim().is_empty() {
                    status.message = Some(if web_running {
                        "Secure network is connected but remote proxy is not ready".to_string()
                    } else {
                        "Start the web service to publish remote access".to_string()
                    });
                }
            }
        }
        status
    }

    async fn persist_auth_key(paths: &AppPaths, auth_key: &str) -> Result<(), String> {
        paths.ensure().await.map_err(|error| error.to_string())?;
        let path = paths.tailscale_dir.join(AUTH_KEY_FILE);
        tokio::fs::write(path, auth_key)
            .await
            .map_err(|error| format!("Could not save auth key: {error}"))
    }

    async fn load_auth_key(paths: &AppPaths) -> Result<Option<String>, String> {
        let path = paths.tailscale_dir.join(AUTH_KEY_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let value = tokio::fs::read_to_string(path)
            .await
            .map_err(|error| format!("Could not read auth key: {error}"))?;
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed))
        }
    }
}

fn resolve_live_client() -> Option<Arc<dyn SidecarControlClient>> {
    let path = resolve_sidecar_path()?;
    match HttpSidecarControlClient::new(path) {
        Ok(client) => Some(Arc::new(client) as Arc<dyn SidecarControlClient>),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{TailscaleRuntimeState, TailscaleService};
    use crate::paths::AppPaths;
    use crate::services::tailscale_sidecar::{FakeSidecarControlClient, SidecarControlClient};
    use crate::services::web_service::{WebServerStatus, WebServiceConfig};
    use std::sync::Arc;
    use tempfile::tempdir;

    fn test_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        (dir, paths)
    }

    #[tokio::test]
    async fn status_is_error_when_sidecar_binary_missing() {
        let runtime = TailscaleRuntimeState::default();
        let (_dir, paths) = test_paths();
        let config = WebServiceConfig {
            tailscale_enabled: true,
            ..WebServiceConfig::default()
        };
        let status =
            TailscaleService::status_with_client(&runtime, &paths, &config, None, || None).await;
        assert_eq!(status.state, "error");
        let message = status.message.unwrap().to_lowercase();
        assert!(
            message.contains("built-in network component"),
            "unexpected message: {message}"
        );
    }

    #[tokio::test]
    async fn auth_key_connect_sets_connected_and_access_urls() {
        let runtime = TailscaleRuntimeState::default();
        let (_dir, paths) = test_paths();
        paths.ensure().await.expect("ensure paths");
        let mut config = WebServiceConfig {
            tailscale_enabled: true,
            port: 3090,
            host: "127.0.0.1".to_string(),
            tailscale_hostname: Some("ai-switch".to_string()),
            ..WebServiceConfig::default()
        };
        let web_status = WebServerStatus {
            running: true,
            host: "127.0.0.1".to_string(),
            port: Some(3090),
            base_url: Some("http://127.0.0.1:3090".to_string()),
        };
        let client = Arc::new(FakeSidecarControlClient::default()) as Arc<dyn SidecarControlClient>;
        let client_for_factory = Arc::clone(&client);

        let status = TailscaleService::start_with_auth_key_with_client(
            &runtime,
            &paths,
            &mut config,
            Some(&web_status),
            "tskey-auth-test".to_string(),
            move || Some(client_for_factory),
        )
        .await
        .expect("auth key connect");

        assert_eq!(status.state, "connected");
        assert!(status
            .access_urls
            .iter()
            .any(|url| url == "http://100.64.0.12:3090"));
        assert!(status
            .access_urls
            .iter()
            .any(|url| url == "http://ai-switch.tailnet.ts.net:3090"));
        assert!(status.serving);
        assert!(config.tailscale_auth_key_present);
        assert!(paths.tailscale_dir.join("auth-key").exists());
    }

    #[tokio::test]
    async fn disabled_config_returns_disabled_status() {
        let runtime = TailscaleRuntimeState::default();
        let (_dir, paths) = test_paths();
        let config = WebServiceConfig::default();
        let status = TailscaleService::status_with_client(
            &runtime,
            &paths,
            &config,
            None,
            || panic!("factory should not run when disabled"),
        )
        .await;
        assert_eq!(status.state, "disabled");
    }

    #[tokio::test]
    async fn connected_without_web_hides_access_urls() {
        let runtime = TailscaleRuntimeState::default();
        let (_dir, paths) = test_paths();
        paths.ensure().await.expect("ensure paths");
        let mut config = WebServiceConfig {
            tailscale_enabled: true,
            port: 10086,
            host: "127.0.0.1".to_string(),
            tailscale_hostname: Some("ai-switch".to_string()),
            ..WebServiceConfig::default()
        };
        let client = Arc::new(FakeSidecarControlClient::default()) as Arc<dyn SidecarControlClient>;
        let client_for_factory = Arc::clone(&client);

        let status = TailscaleService::start_with_auth_key_with_client(
            &runtime,
            &paths,
            &mut config,
            None,
            "tskey-auth-test".to_string(),
            move || Some(client_for_factory),
        )
        .await
        .expect("auth key connect");

        assert_eq!(status.state, "connected");
        assert!(!status.serving);
        assert!(status.access_urls.is_empty());
        assert!(status
            .message
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("web service"));
    }

    #[tokio::test]
    async fn starting_web_with_saved_auth_starts_sidecar() {
        let runtime = TailscaleRuntimeState::default();
        let (_dir, paths) = test_paths();
        paths.ensure().await.expect("ensure paths");
        tokio::fs::write(paths.tailscale_dir.join("auth-key"), "tskey-auth-saved")
            .await
            .expect("write auth key");
        let config = WebServiceConfig {
            tailscale_enabled: true,
            tailscale_auth_key_present: true,
            port: 3090,
            host: "127.0.0.1".to_string(),
            tailscale_hostname: Some("ai-switch".to_string()),
            ..WebServiceConfig::default()
        };
        let web_status = WebServerStatus {
            running: true,
            host: "127.0.0.1".to_string(),
            port: Some(3090),
            base_url: Some("http://127.0.0.1:3090".to_string()),
        };
        let client = Arc::new(FakeSidecarControlClient::default());
        let client_for_factory = Arc::clone(&client) as Arc<dyn SidecarControlClient>;
        let status = TailscaleService::ensure_started_with_client(
            &runtime,
            &paths,
            &config,
            Some(&web_status),
            move || Some(client_for_factory),
        )
        .await;
        assert_eq!(status.state, "connected");
        assert!(status.serving);
        let last = client.last_start().expect("start called");
        assert_eq!(last.auth_key.as_deref(), Some("tskey-auth-saved"));
        assert_eq!(last.backend_addr, "127.0.0.1:3090");
    }

    #[tokio::test]
    async fn stopping_web_service_stops_sidecar() {
        let runtime = TailscaleRuntimeState::default();
        let (_dir, paths) = test_paths();
        paths.ensure().await.expect("ensure paths");
        let mut config = WebServiceConfig {
            tailscale_enabled: true,
            ..WebServiceConfig::default()
        };
        let client = Arc::new(FakeSidecarControlClient::default()) as Arc<dyn SidecarControlClient>;
        let client_for_factory = Arc::clone(&client);
        let _ = TailscaleService::start_with_auth_key_with_client(
            &runtime,
            &paths,
            &mut config,
            None,
            "tskey-auth-test".to_string(),
            move || Some(client_for_factory),
        )
        .await
        .expect("connect");

        let status =
            TailscaleService::disconnect_with_client(&runtime, &paths, &config, || None).await;
        assert_eq!(status.state, "stopped");
    }
}
