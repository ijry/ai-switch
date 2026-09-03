use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use crate::app_state::AppState;
use crate::database::open_migrated_pool;
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::services::config_write_service::ConfigWriteRuntimeState;
use crate::services::deeplink_protocol_service::DeepLinkProtocolRuntime;
use crate::services::route_proxy_service::RouteProxyRuntimeState;
use crate::services::route_recovery_service::RouteRecoveryService;
use crate::services::tailscale_service::TailscaleRuntimeState;
use crate::services::web_service::WebServiceRuntimeState;
use crate::terminal_manager::TerminalManager;
use crate::web::event_bridge::{EventEmitter, WebEventBroadcaster};
use crate::web::router::build_router;
use crate::web::static_assets::{
    locate_static_dir, resolve_static_dir, static_dir_candidates_report,
};

pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    host.parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

pub(crate) fn format_web_base_url(scheme: &str, host: &str, port: u16) -> String {
    let host = host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    let authority_host = match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{host}]"),
        _ => host.to_string(),
    };
    format!("{scheme}://{authority_host}:{port}")
}

pub(crate) fn advertised_web_host(address: SocketAddr) -> String {
    match address.ip() {
        IpAddr::V4(address) if address.is_unspecified() => Ipv4Addr::LOCALHOST.to_string(),
        IpAddr::V6(address) if address.is_unspecified() => Ipv6Addr::LOCALHOST.to_string(),
        address => address.to_string(),
    }
}

fn bound_web_base_url(scheme: &str, address: SocketAddr) -> String {
    format_web_base_url(scheme, &advertised_web_host(address), address.port())
}

async fn bind_server_listener(address: SocketAddr) -> Result<tokio::net::TcpListener, String> {
    tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("Could not bind server: {error}"))
}

pub fn validate_sensitive_web_transport(host: &str, tls_enabled: bool) -> Result<(), AppError> {
    if tls_enabled || is_loopback_host(host) {
        return Ok(());
    }

    Err(AppError::Validation {
        code: "web.sensitive_transport_requires_tls",
        message: "Sensitive Web commands require TLS on non-loopback listeners".to_string(),
        details: Some(host.trim().to_string()),
        recoverable: true,
    })
}

pub(crate) fn normalize_tls_paths(
    certificate_path: Option<&str>,
    private_key_path: Option<&str>,
) -> Result<Option<(PathBuf, PathBuf)>, AppError> {
    let certificate_path = certificate_path
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let private_key_path = private_key_path
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (certificate_path, private_key_path) {
        (None, None) => Ok(None),
        (Some(certificate_path), Some(private_key_path)) => Ok(Some((
            PathBuf::from(certificate_path),
            PathBuf::from(private_key_path),
        ))),
        _ => Err(AppError::Validation {
            code: "web.tls_paths_incomplete",
            message: "Both TLS certificate and private-key paths are required".to_string(),
            details: None,
            recoverable: true,
        }),
    }
}

/// The standalone server has no second line of access control — the token is
/// the only one. Refuse to start without a usable one, the same failure posture
/// as "no TLS on a non-loopback listener".
pub(crate) fn resolve_server_token(raw: Option<String>) -> Result<String, AppError> {
    const MINIMUM_TOKEN_LENGTH: usize = 16;

    let token = raw.unwrap_or_default().trim().to_string();
    if token.is_empty() {
        return Err(AppError::Validation {
            code: "web.token_required",
            message: "AI_SWITCH_TOKEN must be set: it is the only access control the \
                      standalone server has, and ordinary commands can read stored API keys"
                .to_string(),
            details: None,
            recoverable: false,
        });
    }
    if token.chars().count() < MINIMUM_TOKEN_LENGTH {
        return Err(AppError::Validation {
            code: "web.token_too_short",
            message: format!("AI_SWITCH_TOKEN must be at least {MINIMUM_TOKEN_LENGTH} characters"),
            details: None,
            recoverable: false,
        });
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_http_is_accepted_for_sensitive_commands() {
        for host in [
            "localhost",
            "LOCALHOST",
            "127.0.0.1",
            "127.42.0.9",
            "::1",
            "[::1]",
        ] {
            assert!(is_loopback_host(host), "expected loopback host: {host}");
            validate_sensitive_web_transport(host, false).unwrap();
        }
    }

    #[test]
    fn non_loopback_http_is_rejected_before_binding() {
        for host in [
            "0.0.0.0",
            "192.168.1.10",
            "100.64.0.12",
            "localhost.example",
        ] {
            assert!(!is_loopback_host(host), "unexpected loopback host: {host}");
            let error = validate_sensitive_web_transport(host, false).unwrap_err();
            assert!(matches!(
                error,
                crate::error::AppError::Validation {
                    code: "web.sensitive_transport_requires_tls",
                    ..
                }
            ));
        }
    }

    #[test]
    fn configured_tls_allows_non_loopback_hosts() {
        validate_sensitive_web_transport("0.0.0.0", true).unwrap();
    }

    #[test]
    fn a_usable_token_is_required_before_startup() {
        assert_eq!(
            resolve_server_token(Some("  0123456789abcdef  ".to_string())).unwrap(),
            "0123456789abcdef"
        );
        for raw in [None, Some(String::new()), Some("   ".to_string())] {
            let error = resolve_server_token(raw).unwrap_err();
            assert!(matches!(
                error,
                AppError::Validation {
                    code: "web.token_required",
                    ..
                }
            ));
        }
        let error = resolve_server_token(Some("short".to_string())).unwrap_err();
        assert!(matches!(
            error,
            AppError::Validation {
                code: "web.token_too_short",
                ..
            }
        ));
    }

    #[test]
    fn tls_paths_require_both_certificate_and_key() {
        assert!(normalize_tls_paths(None, None).unwrap().is_none());
        assert!(normalize_tls_paths(Some(" cert.pem "), Some(" key.pem "))
            .unwrap()
            .is_some());
        for paths in [
            (Some("cert.pem"), None),
            (None, Some("key.pem")),
            (Some(" "), Some("key.pem")),
        ] {
            let error = normalize_tls_paths(paths.0, paths.1).unwrap_err();
            assert!(matches!(
                error,
                crate::error::AppError::Validation {
                    code: "web.tls_paths_incomplete",
                    ..
                }
            ));
        }
    }

    #[test]
    fn web_base_urls_bracket_ipv6_hosts() {
        assert_eq!(
            format_web_base_url("http", "::1", 3090),
            "http://[::1]:3090"
        );
        assert_eq!(
            format_web_base_url("https", "[::1]", 3090),
            "https://[::1]:3090"
        );
        assert_eq!(
            format_web_base_url("https", "127.0.0.1", 3090),
            "https://127.0.0.1:3090"
        );
    }

    #[test]
    fn advertised_hosts_replace_unspecified_bind_addresses() {
        let ipv4: std::net::SocketAddr = "0.0.0.0:3090".parse().unwrap();
        let ipv6: std::net::SocketAddr = "[::]:3090".parse().unwrap();

        assert_eq!(advertised_web_host(ipv4), "127.0.0.1");
        assert_eq!(advertised_web_host(ipv6), "::1");
    }

    #[test]
    fn bound_base_url_uses_the_actual_listener_port() {
        let address: std::net::SocketAddr = "127.0.0.1:43123".parse().unwrap();
        assert_eq!(
            bound_web_base_url("https", address),
            "https://127.0.0.1:43123"
        );
    }

    #[tokio::test]
    async fn standalone_listener_rejects_an_occupied_port_before_startup() {
        let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = reservation.local_addr().unwrap();

        let error = bind_server_listener(address).await.unwrap_err();
        assert!(error.starts_with("Could not bind server:"));
    }
}

/// Resolves when the process is asked to stop.
///
/// Ctrl+C alone is not enough. `systemd stop`, `docker stop` and every container
/// orchestrator send SIGTERM, and the default disposition terminates the process
/// outright — so [`shutdown_runtime`] never runs, the tailscale sidecar and every
/// PTY child survive the "graceful" stop, and in-flight requests are cut. That is
/// the most common way this binary is stopped in the deployment it is built for.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        // A failed registration must not turn into "never shut down": fall back to
        // Ctrl+C alone rather than hanging on a signal stream that does not exist.
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    // Windows has no SIGTERM; `ctrl_c` already covers Ctrl+C, Ctrl+Break and the
    // console close/logoff/shutdown events.
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Tauri reclaims these through `RunEvent::Exit`; the standalone server has no
/// such hook, so it has to do it before returning or PTY children and the
/// tailscale sidecar are left running.
async fn shutdown_runtime(state: &Arc<AppState>) {
    crate::services::tailscale_service::TailscaleService::shutdown(&state.tailscale).await;
    state.terminals.kill_all();
}

pub async fn run_from_env() -> Result<(), String> {
    let host = std::env::var("AI_SWITCH_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("AI_SWITCH_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3090);
    let token = resolve_server_token(std::env::var("AI_SWITCH_TOKEN").ok())
        .map_err(|error| error.to_string())?;

    let tls_certificate_path = std::env::var("AI_SWITCH_TLS_CERT_PATH").ok();
    let tls_private_key_path = std::env::var("AI_SWITCH_TLS_KEY_PATH").ok();
    let tls_paths = normalize_tls_paths(
        tls_certificate_path.as_deref(),
        tls_private_key_path.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    validate_sensitive_web_transport(&host, tls_paths.is_some())
        .map_err(|error| error.to_string())?;
    let static_dir = match locate_static_dir() {
        Some(dir) => {
            println!("Serving AI Switch web assets from {}", dir.display());
            dir
        }
        None => {
            // With only a "listening on ..." line to go on, a 404 in the browser
            // gives the operator no way to tell a missing bundle from a broken
            // service.
            eprintln!(
                "WARNING: no AI Switch web assets found; the browser UI will not load.\n\
                 The HTTP API still works. Paths tried, in order:\n{}",
                static_dir_candidates_report()
            );
            resolve_static_dir()
        }
    };

    let paths = AppPaths::resolve().map_err(|error| error.to_string())?;
    paths.ensure().await.map_err(|error| error.to_string())?;
    let pool = open_migrated_pool(&paths.database_file, &paths.backups_dir)
        .await
        .map_err(|error| error.to_string())?;
    let state = Arc::new(AppState {
        paths,
        pool,
        config_writes: ConfigWriteRuntimeState::default(),
        deeplink_protocols: DeepLinkProtocolRuntime::default(),
        route_proxy: RouteProxyRuntimeState::default(),
        web_service: WebServiceRuntimeState::default(),
        tailscale: TailscaleRuntimeState::default(),
        terminals: TerminalManager::default(),
        terminal_hub: Arc::new(crate::web::terminal_hub::TerminalHub::default()),
        event_broadcaster: Arc::new(WebEventBroadcaster::new()),
    });
    state
        .route_proxy
        .activity()
        .set_emitter(EventEmitter::Web(Arc::clone(&state.event_broadcaster)));
    state
        .route_proxy
        .live_log()
        .set_emitter(EventEmitter::Web(Arc::clone(&state.event_broadcaster)));

    // Auto-recovery scheduler for the standalone server binary.
    {
        let pool = state.pool.clone();
        let activity = state.route_proxy.activity();
        tokio::spawn(async move {
            RouteRecoveryService::run_loop(pool, activity).await;
        });
    }

    // The desktop setup() hook does the same thing; an unattended server needs
    // it more, since nobody is there to press start after a reboot.
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            crate::services::route_proxy_https_service::restore_auto_started_proxy(&state).await;
        });
    }

    let shutdown_state = Arc::clone(&state);
    let router = build_router(state, token, static_dir);
    let addr = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|error| format!("Invalid server address: {error}"))?
        .next()
        .ok_or_else(|| "Invalid server address: no addresses resolved".to_string())?;

    let rustls_config = if let Some((certificate_path, private_key_path)) = tls_paths {
        Some(
            axum_server::tls_rustls::RustlsConfig::from_pem_file(
                certificate_path,
                private_key_path,
            )
            .await
            .map_err(|error| format!("Could not load HTTPS certificate: {error}"))?,
        )
    } else {
        None
    };
    let listener = bind_server_listener(addr).await?;
    let bound_address = listener
        .local_addr()
        .map_err(|error| format!("Could not read server address: {error}"))?;

    if let Some(rustls_config) = rustls_config {
        let listener = listener
            .into_std()
            .map_err(|error| format!("Could not prepare HTTPS listener: {error}"))?;
        println!(
            "AI Switch server listening on {}",
            bound_web_base_url("https", bound_address)
        );
        // axum_server has no `with_graceful_shutdown`, so the signal goes
        // through its handle instead.
        let handle = axum_server::Handle::new();
        {
            let handle = handle.clone();
            tokio::spawn(async move {
                shutdown_signal().await;
                handle.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
            });
        }
        let result = axum_server::from_tcp_rustls(listener, rustls_config)
            .handle(handle)
            .serve(router.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .map_err(|error| format!("HTTPS server error: {error}"));
        shutdown_runtime(&shutdown_state).await;
        result
    } else {
        println!(
            "AI Switch server listening on {}",
            bound_web_base_url("http", bound_address)
        );
        let result = axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(|error| format!("Server error: {error}"));
        shutdown_runtime(&shutdown_state).await;
        result
    }
}
