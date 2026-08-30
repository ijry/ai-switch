use crate::app_state::AppState;
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::server::{
    advertised_web_host, format_web_base_url, is_loopback_host, normalize_tls_paths,
    validate_sensitive_web_transport,
};
use crate::services::tailscale_service::{TailscaleLogin, TailscaleService, TailscaleStatus};
use crate::services::mobile_pairing::{
    MobilePairingPayload, MobilePairingRedeemResponse, MobilePairingStore, MobileTokenRegistry,
};
use crate::web::router::build_router_with_sensitive_command_gate;
use crate::web::static_assets::resolve_static_dir;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use url::Url;
use uuid::Uuid;

const MOBILE_TOKEN_REGISTRY_FILE: &str = "mobile-tokens.json";

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
    /// private = tailnet only; public = Tailscale Funnel internet access
    #[serde(default = "default_exposure_mode")]
    pub tailscale_exposure_mode: String,
    #[serde(default)]
    pub tls_enabled: bool,
    #[serde(default)]
    pub tls_cert_path: Option<String>,
    #[serde(default)]
    pub tls_key_path: Option<String>,
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
    config_reconciliation_lock: Arc<Mutex<()>>,
    mobile_pairing: MobilePairingStore,
}

#[derive(Default)]
struct WebServiceRuntimeInner {
    status: Option<WebServerStatus>,
    shutdown: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
    sensitive_command_gate: Option<Arc<AtomicBool>>,
}

fn default_exposure_mode() -> String {
    "private".to_string()
}

fn normalize_exposure_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "public" => "public".to_string(),
        _ => "private".to_string(),
    }
}

pub struct WebService;

impl WebService {
    pub fn mobile_token_registry(runtime: &WebServiceRuntimeState) -> MobileTokenRegistry {
        runtime.mobile_pairing.mobile_token_registry()
    }

    #[cfg(test)]
    pub(crate) fn mobile_pairing_store_for_test(
        runtime: &WebServiceRuntimeState,
    ) -> MobilePairingStore {
        runtime.mobile_pairing.clone()
    }

    pub async fn create_mobile_pairing(
        state: &AppState,
    ) -> Result<MobilePairingPayload, AppError> {
        let _guard = state.web_service.config_reconciliation_lock.lock().await;
        let config = Self::load_config(&state.paths).await?;
        let web_status = Self::status(&state.web_service, &config).await;
        if !web_status.running {
            return Err(AppError::Validation {
                code: "mobile_pairing.web_service_not_running",
                message: "Start the Web Service before creating a mobile pairing code".to_string(),
                details: None,
                recoverable: true,
            });
        }
        let tailscale = TailscaleService::status(
            &state.tailscale,
            &state.paths,
            &config,
            Some(&web_status),
        )
        .await;
        if tailscale.state != "connected" || !tailscale.serving {
            return Err(AppError::Validation {
                code: "mobile_pairing.remote_access_not_ready",
                message: "Secure network remote access is not ready".to_string(),
                details: tailscale.message.clone(),
                recoverable: true,
            });
        }
        let access_url = preferred_access_url(&tailscale.access_urls).ok_or_else(|| {
            AppError::Validation {
                code: "mobile_pairing.remote_url_missing",
                message: "No remote access URL is available".to_string(),
                details: None,
                recoverable: true,
            }
        })?;
        let (public_url, private_url) = if tailscale.public {
            (Some(access_url), None)
        } else {
            (None, Some(access_url))
        };
        state
            .web_service
            .mobile_pairing
            .create(
                public_url,
                private_url,
                SystemTime::now(),
                Duration::from_secs(5 * 60),
            )
            .await
            .map_err(|message| AppError::Validation {
                code: "mobile_pairing.create_failed",
                message,
                details: None,
                recoverable: true,
            })
    }

    pub async fn redeem_mobile_pairing(
        state: &AppState,
        code: String,
    ) -> Result<MobilePairingRedeemResponse, AppError> {
        let _guard = state.web_service.config_reconciliation_lock.lock().await;
        let now = SystemTime::now();
        let response = state
            .web_service
            .mobile_pairing
            .redeem(&code, now)
            .await
            .map_err(|message| AppError::Validation {
                code: "mobile_pairing.invalid_code",
                message,
                details: None,
                recoverable: true,
            })?;
        // The response already contains the usable token. A persistence error
        // must not discard it after consuming the one-time pairing code; the
        // in-memory registry remains authoritative for the current process.
        if let Err(error) = state
            .web_service
            .mobile_pairing
            .persist_tokens(&mobile_token_registry_path(&state.paths), now)
            .await
        {
            eprintln!("failed to persist mobile token registry: {error}");
        }
        Ok(response)
    }

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
        validate_enabled_tls_paths(&normalized)?;
        let contents = serde_json::to_string_pretty(&normalized)?;
        tokio::fs::write(&paths.web_service_file, contents).await?;
        Ok(normalized)
    }

    pub async fn save_config_and_reconcile(
        state: &AppState,
        config: &WebServiceConfig,
    ) -> Result<WebServiceConfig, AppError> {
        let _guard = state.web_service.config_reconciliation_lock.lock().await;
        Self::set_sensitive_command_policy(&state.web_service, false).await;
        let saved = Self::save_config(&state.paths, config).await?;
        Self::reconcile_sensitive_command_policy_locked(state, &saved).await;
        Ok(saved)
    }

    pub async fn status(
        runtime: &WebServiceRuntimeState,
        config: &WebServiceConfig,
    ) -> WebServerStatus {
        runtime
            .inner
            .lock()
            .await
            .status
            .clone()
            .unwrap_or(WebServerStatus {
                running: false,
                host: config.host.clone(),
                port: None,
                base_url: None,
            })
    }

    pub async fn start(state: Arc<AppState>) -> Result<WebServerStatus, AppError> {
        let _guard = state.web_service.config_reconciliation_lock.lock().await;
        Self::set_sensitive_command_policy(&state.web_service, false).await;
        let config = Self::load_config(&state.paths).await?;
        let status = Self::start_server_locked(Arc::clone(&state), config.clone()).await?;
        Self::reconcile_sensitive_command_policy_locked(&state, &config).await;
        Ok(status)
    }

    async fn start_server_locked(
        state: Arc<AppState>,
        config: WebServiceConfig,
    ) -> Result<WebServerStatus, AppError> {
        let config = Self::normalize_config(config);
        let tls_paths = validate_start_config(&config)?;
        if let Some(status) = state.web_service.inner.lock().await.status.clone() {
            if status.running {
                return Ok(status);
            }
        }

        let token = config.token.clone().unwrap_or_default();
        if let Err(error) = state
            .web_service
            .mobile_pairing
            .load_tokens(&mobile_token_registry_path(&state.paths), SystemTime::now())
            .await
        {
            // A corrupt registry must not prevent the primary-token web
            // service from starting; ignoring it fails closed for mobile
            // tokens while preserving local recovery.
            eprintln!("failed to load mobile token registry: {error}");
        }
        let static_dir = resolve_static_dir();
        let sensitive_command_gate = Arc::new(AtomicBool::new(false));
        let router = build_router_with_sensitive_command_gate(
            Arc::clone(&state),
            token,
            static_dir,
            Arc::clone(&sensitive_command_gate),
        );
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let (host, port, scheme, join_handle) = if let Some((certificate_path, private_key_path)) =
            tls_paths
        {
            let rustls_config = load_rustls_config(&certificate_path, &private_key_path).await?;
            let addr = resolve_bind_address(&config.host, config.port).await?;
            let listener = tokio::net::TcpListener::bind(addr).await.map_err(|error| {
                AppError::Filesystem {
                    code: "web_service.bind",
                    message: "Could not start web service".to_string(),
                    details: Some(error.to_string()),
                    recoverable: true,
                }
            })?;
            let addr = listener
                .local_addr()
                .map_err(|error| AppError::Filesystem {
                    code: "web_service.addr",
                    message: "Could not read web service address".to_string(),
                    details: Some(error.to_string()),
                    recoverable: true,
                })?;
            let listener = listener.into_std().map_err(|error| AppError::Filesystem {
                code: "web_service.bind",
                message: "Could not start web service".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            })?;
            let host = advertised_web_host(addr);
            let port = addr.port();
            let handle = axum_server::Handle::new();
            let join_handle = tokio::spawn(async move {
                let server = axum_server::from_tcp_rustls(listener, rustls_config)
                    .handle(handle.clone())
                    .serve(router.into_make_service_with_connect_info::<SocketAddr>());
                tokio::pin!(server);
                tokio::select! {
                    result = &mut server => {
                        if let Err(error) = result {
                            eprintln!("web service HTTPS server error: {error}");
                        }
                    }
                    _ = shutdown_rx => {
                        handle.graceful_shutdown(Some(Duration::from_secs(5)));
                        if let Err(error) = server.await {
                            eprintln!("web service HTTPS shutdown error: {error}");
                        }
                    }
                }
            });
            (host, port, "https", join_handle)
        } else {
            let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port))
                .await
                .map_err(|error| AppError::Filesystem {
                    code: "web_service.bind",
                    message: "Could not start web service".to_string(),
                    details: Some(error.to_string()),
                    recoverable: true,
                })?;
            let addr = listener
                .local_addr()
                .map_err(|error| AppError::Filesystem {
                    code: "web_service.addr",
                    message: "Could not read web service address".to_string(),
                    details: Some(error.to_string()),
                    recoverable: true,
                })?;
            let host = advertised_web_host(addr);
            let port = addr.port();
            let join_handle = tokio::spawn(async move {
                let server = axum::serve(listener, router).with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                });
                if let Err(error) = server.await {
                    eprintln!("web service server error: {error}");
                }
            });
            (host, port, "http", join_handle)
        };
        let base_url = format_web_base_url(scheme, &host, port);

        let status = WebServerStatus {
            running: true,
            host,
            port: Some(port),
            base_url: Some(base_url),
        };
        {
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
            inner.sensitive_command_gate = Some(sensitive_command_gate);
        }

        Ok(status)
    }

    pub async fn stop(state: &AppState) -> WebServerStatus {
        let _guard = state.web_service.config_reconciliation_lock.lock().await;
        Self::set_sensitive_command_policy(&state.web_service, false).await;
        let config = Self::load_config(&state.paths).await.unwrap_or_default();
        let (shutdown, join_handle) = {
            let mut inner = state.web_service.inner.lock().await;
            inner.status = None;
            inner.sensitive_command_gate = None;
            (inner.shutdown.take(), inner.join_handle.take())
        };
        if let Some(shutdown) = shutdown {
            let _ = shutdown.send(());
        }
        if let Some(handle) = join_handle {
            let _ = handle.await;
        }

        let _ = TailscaleService::disconnect(&state.tailscale, &state.paths, &config).await;

        WebServerStatus {
            running: false,
            host: config.host,
            port: None,
            base_url: None,
        }
    }

    pub async fn tailscale_status(state: &AppState) -> Result<TailscaleStatus, AppError> {
        let _guard = state.web_service.config_reconciliation_lock.lock().await;
        Self::set_sensitive_command_policy(&state.web_service, false).await;
        let config = Self::load_config(&state.paths).await?;
        let web_status = Self::status(&state.web_service, &config).await;
        let tailscale_status =
            TailscaleService::status(&state.tailscale, &state.paths, &config, Some(&web_status))
                .await;
        Self::apply_observed_tailscale_status_locked(
            &state.web_service,
            &config,
            &web_status,
            &tailscale_status,
        )
        .await;
        Ok(tailscale_status)
    }

    pub async fn start_tailscale_login(state: &AppState) -> Result<TailscaleLogin, AppError> {
        let _guard = state.web_service.config_reconciliation_lock.lock().await;
        Self::set_sensitive_command_policy(&state.web_service, false).await;
        let config = Self::load_config(&state.paths).await?;
        let web_status = Self::ensure_web_running_locked(state, &config).await?;
        Ok(TailscaleService::start_login(
            &state.tailscale,
            &state.paths,
            &config,
            Some(&web_status),
        )
        .await)
    }

    pub async fn start_tailscale_with_auth_key(
        state: &AppState,
        auth_key: String,
    ) -> Result<TailscaleStatus, AppError> {
        let _guard = state.web_service.config_reconciliation_lock.lock().await;
        Self::set_sensitive_command_policy(&state.web_service, false).await;
        let mut config = Self::load_config(&state.paths).await?;
        let web_status = Self::ensure_web_running_locked(state, &config).await?;
        let auth_key_was_present = config.tailscale_auth_key_present;
        let tailscale_status = TailscaleService::start_with_auth_key(
            &state.tailscale,
            &state.paths,
            &mut config,
            Some(&web_status),
            auth_key,
        )
        .await;
        if config.tailscale_auth_key_present != auth_key_was_present {
            config = Self::save_config(&state.paths, &config).await?;
        }
        let tailscale_status = tailscale_status.map_err(|message| AppError::Validation {
            code: "tailscale.auth_key",
            message,
            details: None,
            recoverable: true,
        })?;
        Self::apply_observed_tailscale_status_locked(
            &state.web_service,
            &config,
            &web_status,
            &tailscale_status,
        )
        .await;
        Ok(tailscale_status)
    }

    pub async fn disconnect_tailscale(state: &AppState) -> Result<TailscaleStatus, AppError> {
        let _guard = state.web_service.config_reconciliation_lock.lock().await;
        Self::set_sensitive_command_policy(&state.web_service, false).await;
        let config = Self::load_config(&state.paths).await?;
        let web_status = Self::status(&state.web_service, &config).await;
        let tailscale_status =
            TailscaleService::disconnect(&state.tailscale, &state.paths, &config).await;
        Self::apply_observed_tailscale_status_locked(
            &state.web_service,
            &config,
            &web_status,
            &tailscale_status,
        )
        .await;
        Ok(tailscale_status)
    }

    async fn ensure_web_running_locked(
        state: &AppState,
        config: &WebServiceConfig,
    ) -> Result<WebServerStatus, AppError> {
        let status = Self::status(&state.web_service, config).await;
        if status.running {
            return Ok(status);
        }
        Self::start_server_locked(Arc::new(state.clone()), config.clone()).await
    }

    async fn reconcile_sensitive_command_policy_locked(
        state: &AppState,
        config: &WebServiceConfig,
    ) {
        Self::set_sensitive_command_policy(&state.web_service, false).await;
        let status = Self::status(&state.web_service, config).await;
        if !status.running {
            return;
        }

        Self::reconcile_sensitive_command_gate(&state.web_service, config, &status, async {
            if config.tailscale_enabled {
                TailscaleService::ensure_started(
                    &state.tailscale,
                    &state.paths,
                    config,
                    Some(&status),
                )
                .await
            } else {
                TailscaleService::disconnect(&state.tailscale, &state.paths, config).await
            }
        })
        .await;
    }

    async fn apply_observed_tailscale_status_locked(
        runtime: &WebServiceRuntimeState,
        config: &WebServiceConfig,
        web_status: &WebServerStatus,
        tailscale_status: &TailscaleStatus,
    ) {
        let enabled =
            sensitive_commands_enabled_for_runtime(config, web_status, Some(tailscale_status));
        Self::set_sensitive_command_policy(runtime, enabled).await;
    }

    async fn reconcile_sensitive_command_gate<F>(
        runtime: &WebServiceRuntimeState,
        config: &WebServiceConfig,
        status: &WebServerStatus,
        tailscale_status: F,
    ) where
        F: std::future::Future<Output = TailscaleStatus>,
    {
        Self::set_sensitive_command_policy(runtime, false).await;
        let tailscale_status = tailscale_status.await;
        Self::apply_observed_tailscale_status_locked(runtime, config, status, &tailscale_status)
            .await;
    }

    async fn set_sensitive_command_policy(runtime: &WebServiceRuntimeState, enabled: bool) {
        let gate = runtime.inner.lock().await.sensitive_command_gate.clone();
        if let Some(gate) = gate {
            gate.store(enabled, Ordering::Release);
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
        let tls_cert_path = normalize_optional_path(config.tls_cert_path);
        let tls_key_path = normalize_optional_path(config.tls_key_path);

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
            tailscale_exposure_mode: normalize_exposure_mode(&config.tailscale_exposure_mode),
            tls_enabled: config.tls_enabled,
            tls_cert_path,
            tls_key_path,
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
            tailscale_exposure_mode: default_exposure_mode(),
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

fn normalize_optional_path(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn preferred_access_url(urls: &[String]) -> Option<String> {
    urls.iter()
        .map(String::as_str)
        .find(|url| url.contains(".ts.net"))
        .or_else(|| urls.iter().map(String::as_str).find(|url| !url.trim().is_empty()))
        .map(|url| url.trim_end_matches('/').to_string())
}

fn mobile_token_registry_path(paths: &AppPaths) -> PathBuf {
    paths.data_dir.join(MOBILE_TOKEN_REGISTRY_FILE)
}

fn validate_enabled_tls_paths(
    config: &WebServiceConfig,
) -> Result<Option<(PathBuf, PathBuf)>, AppError> {
    if !config.tls_enabled {
        return Ok(None);
    }
    normalize_tls_paths(
        config.tls_cert_path.as_deref(),
        config.tls_key_path.as_deref(),
    )?
    .ok_or_else(|| AppError::Validation {
        code: "web.tls_paths_incomplete",
        message: "Both TLS certificate and private-key paths are required".to_string(),
        details: None,
        recoverable: true,
    })
    .map(Some)
}

fn validate_start_config(
    config: &WebServiceConfig,
) -> Result<Option<(PathBuf, PathBuf)>, AppError> {
    let tls_paths = validate_enabled_tls_paths(config)?;
    validate_sensitive_web_transport(&config.host, config.tls_enabled)?;
    Ok(tls_paths)
}

fn sensitive_commands_enabled_for_runtime(
    config: &WebServiceConfig,
    status: &WebServerStatus,
    tailscale_status: Option<&TailscaleStatus>,
) -> bool {
    if !status.running {
        return false;
    }

    let Some(base_url) = status
        .base_url
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
    else {
        return false;
    };
    if base_url.scheme() == "https" {
        return true;
    }
    if base_url.scheme() != "http" || !is_loopback_host(&status.host) {
        return false;
    }

    let Some(tailscale_status) = tailscale_status else {
        return false;
    };
    if config.tailscale_enabled {
        return tailscale_status.state == "connected"
            && tailscale_status.serving
            && tailscale_status.access_urls.iter().any(|url| {
                Url::parse(url)
                    .ok()
                    .is_some_and(|parsed| parsed.scheme().eq_ignore_ascii_case("https"))
            });
    }

    !tailscale_status.serving && matches!(tailscale_status.state.as_str(), "disabled" | "stopped")
}

async fn resolve_bind_address(host: &str, port: u16) -> Result<SocketAddr, AppError> {
    tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| AppError::Validation {
            code: "web_service.addr",
            message: "Could not resolve web service address".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?
        .next()
        .ok_or_else(|| AppError::Validation {
            code: "web_service.addr",
            message: "Could not resolve web service address".to_string(),
            details: None,
            recoverable: true,
        })
}

async fn load_rustls_config(
    certificate_path: &std::path::Path,
    private_key_path: &std::path::Path,
) -> Result<axum_server::tls_rustls::RustlsConfig, AppError> {
    axum_server::tls_rustls::RustlsConfig::from_pem_file(certificate_path, private_key_path)
        .await
        .map_err(|error| AppError::Validation {
            code: "web.tls_material_invalid",
            message: "Could not load Web TLS certificate".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })
}

#[cfg(test)]
mod tests {
    use super::{
        load_rustls_config, sensitive_commands_enabled_for_runtime, validate_start_config,
        WebServerStatus, WebService, WebServiceConfig, WebServiceRuntimeState,
    };
    use crate::app_state::AppState;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::services::config_write_service::ConfigWriteRuntimeState;
    use crate::services::deeplink_protocol_service::DeepLinkProtocolRuntime;
    use crate::services::mobile_pairing::MobilePairingStore;
    use crate::services::route_proxy_service::RouteProxyRuntimeState;
    use crate::services::tailscale_service::{TailscaleRuntimeState, TailscaleStatus};
    use crate::services::tailscale_sidecar::SidecarControlClient;
    use crate::services::tailscale_types::{TailscaleLogin, TailscaleStartRequest};
    use crate::terminal_manager::TerminalManager;
    use crate::web::event_bridge::WebEventBroadcaster;
    use async_trait::async_trait;
    use rcgen::{generate_simple_self_signed, CertifiedKey};
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;
    use std::time::SystemTime;
    use tempfile::tempdir;
    use tokio::sync::Notify;

    #[tokio::test]
    async fn mobile_pairing_codes_are_hashed_expiring_and_single_use() {
        let store = MobilePairingStore::default();
        let payload = store
            .create(
                Some("https://public.example".to_string()),
                None,
                SystemTime::UNIX_EPOCH + Duration::from_secs(1_000),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        assert_eq!(payload.version, 1);
        assert!(!payload.pairing_code.is_empty());
        assert!(store.debug_contains_plaintext_code(&payload.pairing_code).await == false);

        let redeemed = store
            .redeem(&payload.pairing_code, SystemTime::UNIX_EPOCH + Duration::from_secs(1_001))
            .await
            .unwrap();
        assert!(redeemed.token.starts_with("ms_"));
        assert!(store
            .redeem(&payload.pairing_code, SystemTime::UNIX_EPOCH + Duration::from_secs(1_002))
            .await
            .is_err());
        assert!(store
            .is_mobile_token_valid(&redeemed.token, SystemTime::UNIX_EPOCH + Duration::from_secs(1_002))
            .await);
    }

    #[tokio::test]
    async fn mobile_pairing_rejects_expired_codes() {
        let store = MobilePairingStore::default();
        let payload = store
            .create(
                None,
                Some("https://private.example".to_string()),
                SystemTime::UNIX_EPOCH + Duration::from_secs(2_000),
                Duration::from_secs(1),
            )
            .await
            .unwrap();
        assert!(store
            .redeem(&payload.pairing_code, SystemTime::UNIX_EPOCH + Duration::from_secs(2_001))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn redeemed_mobile_tokens_are_persisted_for_a_new_runtime() {
        let (_temp, state) = concurrency_test_state().await;
        let store = WebService::mobile_pairing_store_for_test(&state.web_service);
        let payload = store
            .create(
                Some("https://public.example".to_string()),
                None,
                SystemTime::now(),
                Duration::from_secs(300),
            )
            .await
            .unwrap();
        let redeemed = WebService::redeem_mobile_pairing(state.as_ref(), payload.pairing_code)
            .await
            .unwrap();

        let restored = MobilePairingStore::default();
        restored
            .load_tokens(
                &state.paths.data_dir.join("mobile-tokens.json"),
                SystemTime::now(),
            )
            .await
            .unwrap();
        assert!(restored
            .is_mobile_token_valid(&redeemed.token, SystemTime::now())
            .await);
    }

    struct ControlledSidecarClient {
        status: TailscaleStatus,
        stop_result: Result<TailscaleStatus, String>,
        block_status: bool,
        block_stop: bool,
        status_started: Notify,
        release_status: Notify,
        stop_started: Notify,
        release_stop: Notify,
        start_calls: AtomicUsize,
    }

    impl ControlledSidecarClient {
        fn new(
            status: TailscaleStatus,
            stop_result: Result<TailscaleStatus, String>,
            block_status: bool,
            block_stop: bool,
        ) -> Self {
            Self {
                status,
                stop_result,
                block_status,
                block_stop,
                status_started: Notify::new(),
                release_status: Notify::new(),
                stop_started: Notify::new(),
                release_stop: Notify::new(),
                start_calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl SidecarControlClient for ControlledSidecarClient {
        async fn start(&self, _request: TailscaleStartRequest) -> Result<TailscaleStatus, String> {
            self.start_calls.fetch_add(1, Ordering::AcqRel);
            Ok(self.status.clone())
        }

        async fn login_oauth(&self) -> Result<TailscaleLogin, String> {
            Ok(TailscaleLogin {
                login_url: Some("https://login.tailscale.com/a/test".to_string()),
                message: "Complete browser sign-in".to_string(),
            })
        }

        async fn stop(&self) -> Result<TailscaleStatus, String> {
            self.stop_started.notify_one();
            if self.block_stop {
                self.release_stop.notified().await;
            }
            self.stop_result.clone()
        }

        async fn logout(&self) -> Result<TailscaleStatus, String> {
            Ok(TailscaleStatus::needs_login("Signed out"))
        }

        async fn status(&self) -> Result<TailscaleStatus, String> {
            self.status_started.notify_one();
            if self.block_status {
                self.release_status.notified().await;
            }
            Ok(self.status.clone())
        }
    }

    async fn concurrency_test_state() -> (tempfile::TempDir, Arc<AppState>) {
        let temp = tempdir().unwrap();
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = Arc::new(AppState {
            paths: crate::paths::AppPaths::from_data_dir(temp.path().join("app-data")),
            pool,
            config_writes: ConfigWriteRuntimeState::default(),
            deeplink_protocols: DeepLinkProtocolRuntime::default(),
            route_proxy: RouteProxyRuntimeState::default(),
            web_service: WebServiceRuntimeState::default(),
            tailscale: TailscaleRuntimeState::default(),
            terminals: TerminalManager::default(),
            event_broadcaster: Arc::new(WebEventBroadcaster::default()),
        });
        (temp, state)
    }

    fn public_tailscale_status() -> TailscaleStatus {
        TailscaleStatus {
            state: "connected".to_string(),
            device_name: Some("ai-switch".to_string()),
            tailnet_ip: Some("100.64.0.12".to_string()),
            magic_dns_name: Some("ai-switch.tailnet.ts.net".to_string()),
            login_url: None,
            access_urls: vec!["https://ai-switch.tailnet.ts.net".to_string()],
            serving: true,
            public: true,
            exposure_mode: Some("public".to_string()),
            public_port: Some(443),
            message: None,
        }
    }

    async fn seed_running_web_service(
        state: &AppState,
        config: &WebServiceConfig,
        gate: Arc<std::sync::atomic::AtomicBool>,
    ) {
        let mut runtime = state.web_service.inner.lock().await;
        runtime.status = Some(WebServerStatus {
            running: true,
            host: config.host.clone(),
            port: Some(config.port),
            base_url: Some(format!("http://{}:{}", config.host, config.port)),
        });
        runtime.sensitive_command_gate = Some(gate);
    }

    #[test]
    fn web_service_config_defaults_keep_auth_key_absent() {
        let config = WebServiceConfig::default();
        assert_eq!(config.tailscale_enabled, false);
        assert_eq!(config.tailscale_auth_key_present, false);
        assert!(config.tailscale_hostname.is_none());
    }

    #[test]
    fn advanced_tls_configuration_is_normalized_and_preserved() {
        let config = super::WebService::normalize_config(WebServiceConfig {
            tls_enabled: true,
            tls_cert_path: Some(" cert.pem ".to_string()),
            tls_key_path: Some(" key.pem ".to_string()),
            ..WebServiceConfig::default()
        });

        assert!(config.tls_enabled);
        assert_eq!(config.tls_cert_path.as_deref(), Some("cert.pem"));
        assert_eq!(config.tls_key_path.as_deref(), Some("key.pem"));
        validate_start_config(&config).unwrap();
    }

    #[test]
    fn enabled_tls_rejects_a_one_path_configuration() {
        let config = WebServiceConfig {
            host: "0.0.0.0".to_string(),
            tls_enabled: true,
            tls_cert_path: Some("cert.pem".to_string()),
            tls_key_path: None,
            ..WebServiceConfig::default()
        };

        let error = validate_start_config(&config).unwrap_err();
        assert!(matches!(
            error,
            crate::error::AppError::Validation {
                code: "web.tls_paths_incomplete",
                ..
            }
        ));
    }

    #[test]
    fn public_exposure_cannot_bypass_an_insecure_local_listener() {
        let config = WebServiceConfig {
            host: "0.0.0.0".to_string(),
            tls_enabled: false,
            tailscale_enabled: true,
            tailscale_exposure_mode: "public".to_string(),
            ..WebServiceConfig::default()
        };

        let error = validate_start_config(&config).unwrap_err();
        assert!(matches!(
            error,
            crate::error::AppError::Validation {
                code: "web.sensitive_transport_requires_tls",
                ..
            }
        ));
    }

    #[test]
    fn private_tailnet_http_does_not_register_sensitive_commands() {
        let private = WebServiceConfig {
            host: "127.0.0.1".to_string(),
            tls_enabled: false,
            tailscale_enabled: true,
            tailscale_exposure_mode: "private".to_string(),
            ..WebServiceConfig::default()
        };
        let public = WebServiceConfig {
            tailscale_exposure_mode: "public".to_string(),
            ..private.clone()
        };
        let web_status = WebServerStatus {
            running: true,
            host: "127.0.0.1".to_string(),
            port: Some(3090),
            base_url: Some("http://127.0.0.1:3090".to_string()),
        };
        let private_http_status = TailscaleStatus {
            state: "connected".to_string(),
            serving: true,
            public: false,
            exposure_mode: Some("private".to_string()),
            access_urls: vec!["http://ai-switch.tailnet.ts.net:3090".to_string()],
            ..TailscaleStatus::disabled()
        };
        let public_status = TailscaleStatus {
            access_urls: vec!["https://ai-switch.tailnet.ts.net".to_string()],
            public: true,
            exposure_mode: Some("public".to_string()),
            ..private_http_status.clone()
        };

        validate_start_config(&private).unwrap();
        assert!(!sensitive_commands_enabled_for_runtime(
            &private,
            &web_status,
            Some(&private_http_status),
        ));
        assert!(sensitive_commands_enabled_for_runtime(
            &public,
            &web_status,
            Some(&public_status),
        ));
    }

    #[test]
    fn private_tailnet_https_registers_sensitive_commands() {
        let config = WebServiceConfig {
            host: "127.0.0.1".to_string(),
            tls_enabled: false,
            tailscale_enabled: true,
            tailscale_exposure_mode: "private".to_string(),
            ..WebServiceConfig::default()
        };
        let web_status = WebServerStatus {
            running: true,
            host: "127.0.0.1".to_string(),
            port: Some(3090),
            base_url: Some("http://127.0.0.1:3090".to_string()),
        };
        let status = TailscaleStatus {
            state: "connected".to_string(),
            serving: true,
            public: false,
            exposure_mode: Some("private".to_string()),
            access_urls: vec!["https://ai-switch.tailnet.ts.net:3090".to_string()],
            ..TailscaleStatus::disabled()
        };

        assert!(sensitive_commands_enabled_for_runtime(
            &config,
            &web_status,
            Some(&status),
        ));
    }

    #[test]
    fn public_http_gate_stays_closed_until_rebind_is_confirmed() {
        let config = WebServiceConfig {
            host: "127.0.0.1".to_string(),
            tls_enabled: false,
            tailscale_enabled: true,
            tailscale_exposure_mode: "public".to_string(),
            ..WebServiceConfig::default()
        };
        let web_status = WebServerStatus {
            running: true,
            host: "127.0.0.1".to_string(),
            port: Some(3090),
            base_url: Some("http://127.0.0.1:3090".to_string()),
        };
        let failed = TailscaleStatus::error("rebind failed");
        let ready = TailscaleStatus {
            state: "connected".to_string(),
            serving: true,
            public: true,
            exposure_mode: Some("public".to_string()),
            access_urls: vec!["https://ai-switch.tailnet.ts.net".to_string()],
            ..TailscaleStatus::disabled()
        };

        assert!(!sensitive_commands_enabled_for_runtime(
            &config,
            &web_status,
            Some(&failed),
        ));
        assert!(sensitive_commands_enabled_for_runtime(
            &config,
            &web_status,
            Some(&ready),
        ));
    }

    #[test]
    fn local_http_gate_reopens_only_after_old_sidecar_stops() {
        let config = WebServiceConfig {
            host: "127.0.0.1".to_string(),
            tls_enabled: false,
            tailscale_enabled: false,
            ..WebServiceConfig::default()
        };
        let web_status = WebServerStatus {
            running: true,
            host: "127.0.0.1".to_string(),
            port: Some(3090),
            base_url: Some("http://127.0.0.1:3090".to_string()),
        };

        assert!(!sensitive_commands_enabled_for_runtime(
            &config,
            &web_status,
            Some(&TailscaleStatus::error("stop failed")),
        ));
        assert!(sensitive_commands_enabled_for_runtime(
            &config,
            &web_status,
            Some(&TailscaleStatus::disabled()),
        ));
    }

    #[tokio::test]
    async fn config_reconciliation_lock_serializes_updates() {
        let runtime = WebServiceRuntimeState::default();
        let first_guard = runtime.config_reconciliation_lock.lock().await;
        let second_runtime = runtime.clone();
        let acquired = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let second_acquired = Arc::clone(&acquired);
        let second = tokio::spawn(async move {
            let _guard = second_runtime.config_reconciliation_lock.lock().await;
            second_acquired.store(true, Ordering::Release);
        });

        tokio::task::yield_now().await;
        assert!(!acquired.load(Ordering::Acquire));
        drop(first_guard);
        second.await.unwrap();
        assert!(acquired.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn stop_and_start_are_serialized_until_old_sidecar_is_disconnected() {
        let (_temp, state) = concurrency_test_state().await;
        let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let config = WebServiceConfig {
            host: "127.0.0.1".to_string(),
            port,
            tailscale_enabled: false,
            ..WebServiceConfig::default()
        };
        WebService::save_config(&state.paths, &config)
            .await
            .unwrap();
        let old_gate = Arc::new(std::sync::atomic::AtomicBool::new(true));
        seed_running_web_service(&state, &config, Arc::clone(&old_gate)).await;
        let sidecar = Arc::new(ControlledSidecarClient::new(
            TailscaleStatus::disabled(),
            Ok(TailscaleStatus::disabled()),
            false,
            true,
        ));
        state
            .tailscale
            .set_client_for_test(Arc::clone(&sidecar) as Arc<dyn SidecarControlClient>)
            .await;

        let stop_state = Arc::clone(&state);
        let stop_task = tokio::spawn(async move { WebService::stop(&stop_state).await });
        sidecar.stop_started.notified().await;
        assert!(!old_gate.load(Ordering::Acquire));

        let start_state = Arc::clone(&state);
        let mut start_task = tokio::spawn(async move { WebService::start(start_state).await });
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut start_task)
                .await
                .is_err(),
            "start must wait until the old sidecar is disconnected"
        );

        sidecar.release_stop.notify_one();
        stop_task.await.unwrap();
        let started = start_task.await.unwrap().unwrap();
        assert!(started.running);
        let new_gate = state
            .web_service
            .inner
            .lock()
            .await
            .sensitive_command_gate
            .clone()
            .unwrap();
        assert!(new_gate.load(Ordering::Acquire));
        WebService::stop(&state).await;
    }

    #[tokio::test]
    async fn stale_status_observation_cannot_reopen_after_disconnect_failure() {
        let (_temp, state) = concurrency_test_state().await;
        let config = WebServiceConfig {
            tailscale_enabled: true,
            tailscale_exposure_mode: "public".to_string(),
            ..WebServiceConfig::default()
        };
        WebService::save_config(&state.paths, &config)
            .await
            .unwrap();
        let gate = Arc::new(std::sync::atomic::AtomicBool::new(true));
        seed_running_web_service(&state, &config, Arc::clone(&gate)).await;
        let sidecar = Arc::new(ControlledSidecarClient::new(
            public_tailscale_status(),
            Err("stop failed".to_string()),
            true,
            true,
        ));
        state
            .tailscale
            .set_client_for_test(Arc::clone(&sidecar) as Arc<dyn SidecarControlClient>)
            .await;

        let status_state = Arc::clone(&state);
        let status_task =
            tokio::spawn(async move { WebService::tailscale_status(&status_state).await });
        sidecar.status_started.notified().await;
        assert!(!gate.load(Ordering::Acquire));

        let disconnect_state = Arc::clone(&state);
        let disconnect_task =
            tokio::spawn(async move { WebService::disconnect_tailscale(&disconnect_state).await });
        assert!(
            tokio::time::timeout(Duration::from_millis(50), sidecar.stop_started.notified())
                .await
                .is_err(),
            "disconnect must wait for the in-flight status observation"
        );

        sidecar.release_status.notify_one();
        sidecar.stop_started.notified().await;
        assert!(!gate.load(Ordering::Acquire));
        sidecar.release_stop.notify_one();
        let disconnected = disconnect_task.await.unwrap().unwrap();
        assert_eq!(disconnected.state, "error");
        let observed = status_task.await.unwrap().unwrap();
        assert!(observed.public);
        assert!(!gate.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn stale_auth_key_config_cannot_overwrite_a_newer_disabled_save() {
        let (_temp, state) = concurrency_test_state().await;
        let initial = WebServiceConfig {
            tailscale_enabled: true,
            tailscale_exposure_mode: "public".to_string(),
            ..WebServiceConfig::default()
        };
        WebService::save_config(&state.paths, &initial)
            .await
            .unwrap();
        let gate = Arc::new(std::sync::atomic::AtomicBool::new(true));
        seed_running_web_service(&state, &initial, gate).await;
        let sidecar = Arc::new(ControlledSidecarClient::new(
            public_tailscale_status(),
            Ok(TailscaleStatus::disabled()),
            false,
            true,
        ));
        state
            .tailscale
            .set_client_for_test(Arc::clone(&sidecar) as Arc<dyn SidecarControlClient>)
            .await;
        let disabled = WebServiceConfig {
            tailscale_enabled: false,
            tailscale_exposure_mode: "private".to_string(),
            ..initial.clone()
        };

        let save_state = Arc::clone(&state);
        let save_task = tokio::spawn(async move {
            WebService::save_config_and_reconcile(&save_state, &disabled).await
        });
        sidecar.stop_started.notified().await;

        let auth_state = Arc::clone(&state);
        let mut auth_task = tokio::spawn(async move {
            WebService::start_tailscale_with_auth_key(&auth_state, "tskey-auth-stale".to_string())
                .await
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut auth_task)
                .await
                .is_err(),
            "auth-key start must wait for the pending config save"
        );

        sidecar.release_stop.notify_one();
        save_task.await.unwrap().unwrap();
        let auth_status = auth_task.await.unwrap().unwrap();
        assert_eq!(auth_status.state, "disabled");
        assert_eq!(sidecar.start_calls.load(Ordering::Acquire), 0);

        let saved = WebService::load_config(&state.paths).await.unwrap();
        assert!(!saved.tailscale_enabled);
        assert_eq!(saved.tailscale_exposure_mode, "private");
        assert!(!state.paths.tailscale_dir.join("auth-key").exists());
    }

    #[tokio::test]
    async fn sensitive_gate_closes_while_rebind_is_pending_and_after_failure() {
        let runtime = WebServiceRuntimeState::default();
        let gate = Arc::new(std::sync::atomic::AtomicBool::new(true));
        runtime.inner.lock().await.sensitive_command_gate = Some(Arc::clone(&gate));
        let config = WebServiceConfig {
            tailscale_enabled: true,
            tailscale_exposure_mode: "public".to_string(),
            ..WebServiceConfig::default()
        };
        let status = WebServerStatus {
            running: true,
            host: "127.0.0.1".to_string(),
            port: Some(3090),
            base_url: Some("http://127.0.0.1:3090".to_string()),
        };
        let rebind_started = Arc::new(tokio::sync::Notify::new());
        let release_rebind = Arc::new(tokio::sync::Notify::new());
        let task_runtime = runtime.clone();
        let task_started = Arc::clone(&rebind_started);
        let task_release = Arc::clone(&release_rebind);
        let task = tokio::spawn(async move {
            WebService::reconcile_sensitive_command_gate(
                &task_runtime,
                &config,
                &status,
                async move {
                    task_started.notify_one();
                    task_release.notified().await;
                    TailscaleStatus::error("rebind failed")
                },
            )
            .await;
        });

        rebind_started.notified().await;
        assert!(!gate.load(Ordering::Acquire));
        release_rebind.notify_one();
        task.await.unwrap();
        assert!(!gate.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn coordinated_tailscale_status_refreshes_the_running_gate() {
        let temp = tempdir().unwrap();
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState {
            paths: crate::paths::AppPaths::from_data_dir(temp.path().join("app-data")),
            pool,
            config_writes: ConfigWriteRuntimeState::default(),
            deeplink_protocols: DeepLinkProtocolRuntime::default(),
            route_proxy: RouteProxyRuntimeState::default(),
            web_service: WebServiceRuntimeState::default(),
            tailscale: TailscaleRuntimeState::default(),
            terminals: TerminalManager::default(),
            event_broadcaster: Arc::new(WebEventBroadcaster::default()),
        };
        let config = WebServiceConfig {
            tailscale_enabled: true,
            tailscale_exposure_mode: "public".to_string(),
            ..WebServiceConfig::default()
        };
        WebService::save_config(&state.paths, &config)
            .await
            .unwrap();
        let gate = Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let mut runtime = state.web_service.inner.lock().await;
            runtime.status = Some(WebServerStatus {
                running: true,
                host: "127.0.0.1".to_string(),
                port: Some(3090),
                base_url: Some("http://127.0.0.1:3090".to_string()),
            });
            runtime.sensitive_command_gate = Some(Arc::clone(&gate));
        }
        let public_client = Arc::new(ControlledSidecarClient::new(
            public_tailscale_status(),
            Ok(TailscaleStatus::stopped("stopped")),
            false,
            false,
        ));
        state
            .tailscale
            .set_client_for_test(public_client as Arc<dyn SidecarControlClient>)
            .await;

        WebService::tailscale_status(&state).await.unwrap();
        assert!(gate.load(Ordering::Acquire));

        let private_client = Arc::new(ControlledSidecarClient::new(
            TailscaleStatus {
                state: "connected".to_string(),
                serving: true,
                public: false,
                exposure_mode: Some("private".to_string()),
                ..TailscaleStatus::disabled()
            },
            Ok(TailscaleStatus::stopped("stopped")),
            false,
            false,
        ));
        state
            .tailscale
            .set_client_for_test(private_client as Arc<dyn SidecarControlClient>)
            .await;
        WebService::tailscale_status(&state).await.unwrap();
        assert!(!gate.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn configured_rustls_material_starts_an_https_listener() {
        let temp = tempdir().unwrap();
        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let certificate_path = temp.path().join("certificate.pem");
        let private_key_path = temp.path().join("private-key.pem");
        tokio::fs::write(&certificate_path, cert.pem())
            .await
            .unwrap();
        tokio::fs::write(&private_key_path, key_pair.serialize_pem())
            .await
            .unwrap();
        load_rustls_config(&certificate_path, &private_key_path)
            .await
            .unwrap();

        let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = Arc::new(AppState {
            paths: crate::paths::AppPaths::from_data_dir(temp.path().join("app-data")),
            pool,
            config_writes: ConfigWriteRuntimeState::default(),
            deeplink_protocols: DeepLinkProtocolRuntime::default(),
            route_proxy: RouteProxyRuntimeState::default(),
            web_service: WebServiceRuntimeState::default(),
            tailscale: TailscaleRuntimeState::default(),
            terminals: TerminalManager::default(),
            event_broadcaster: Arc::new(WebEventBroadcaster::default()),
        });
        let config = WebServiceConfig {
            host: "127.0.0.1".to_string(),
            port,
            tls_enabled: true,
            tls_cert_path: Some(certificate_path.display().to_string()),
            tls_key_path: Some(private_key_path.display().to_string()),
            ..WebServiceConfig::default()
        };
        WebService::save_config(&state.paths, &config)
            .await
            .unwrap();

        let status = WebService::start(Arc::clone(&state)).await.unwrap();
        assert_eq!(
            status.base_url.as_deref(),
            Some(format!("https://127.0.0.1:{port}").as_str())
        );
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        let mut response = None;
        for _ in 0..50 {
            match client
                .get(format!("https://127.0.0.1:{port}/health"))
                .send()
                .await
            {
                Ok(value) => {
                    response = Some(value);
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        assert_eq!(response.unwrap().status(), reqwest::StatusCode::OK);
        WebService::stop(&state).await;
    }

    #[tokio::test]
    async fn occupied_tls_port_fails_without_reporting_running() {
        let temp = tempdir().unwrap();
        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let certificate_path = temp.path().join("certificate.pem");
        let private_key_path = temp.path().join("private-key.pem");
        tokio::fs::write(&certificate_path, cert.pem())
            .await
            .unwrap();
        tokio::fs::write(&private_key_path, key_pair.serialize_pem())
            .await
            .unwrap();

        let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = occupied.local_addr().unwrap().port();
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = Arc::new(AppState {
            paths: crate::paths::AppPaths::from_data_dir(temp.path().join("app-data")),
            pool,
            config_writes: ConfigWriteRuntimeState::default(),
            deeplink_protocols: DeepLinkProtocolRuntime::default(),
            route_proxy: RouteProxyRuntimeState::default(),
            web_service: WebServiceRuntimeState::default(),
            tailscale: TailscaleRuntimeState::default(),
            terminals: TerminalManager::default(),
            event_broadcaster: Arc::new(WebEventBroadcaster::default()),
        });
        let config = WebServiceConfig {
            host: "127.0.0.1".to_string(),
            port,
            tls_enabled: true,
            tls_cert_path: Some(certificate_path.display().to_string()),
            tls_key_path: Some(private_key_path.display().to_string()),
            ..WebServiceConfig::default()
        };
        WebService::save_config(&state.paths, &config)
            .await
            .unwrap();

        let error = WebService::start(Arc::clone(&state)).await.unwrap_err();
        assert!(matches!(
            error,
            crate::error::AppError::Filesystem {
                code: "web_service.bind",
                ..
            }
        ));
        let status = WebService::status(&state.web_service, &config).await;
        assert!(!status.running);
        assert!(status.port.is_none());
        assert!(status.base_url.is_none());
    }
}
