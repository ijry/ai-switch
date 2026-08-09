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
use crate::services::tailscale_service::TailscaleRuntimeState;
use crate::services::web_service::WebServiceRuntimeState;
use crate::terminal_manager::TerminalManager;
use crate::web::event_bridge::{EventEmitter, WebEventBroadcaster};
use crate::web::router::build_router;
use crate::web::static_assets::resolve_static_dir;

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

pub async fn run_from_env() -> Result<(), String> {
    let host = std::env::var("AI_SWITCH_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("AI_SWITCH_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3090);
    let token = std::env::var("AI_SWITCH_TOKEN").unwrap_or_default();
    let tls_certificate_path = std::env::var("AI_SWITCH_TLS_CERT_PATH").ok();
    let tls_private_key_path = std::env::var("AI_SWITCH_TLS_KEY_PATH").ok();
    let tls_paths = normalize_tls_paths(
        tls_certificate_path.as_deref(),
        tls_private_key_path.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    validate_sensitive_web_transport(&host, tls_paths.is_some())
        .map_err(|error| error.to_string())?;
    let static_dir = resolve_static_dir();

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
        event_broadcaster: Arc::new(WebEventBroadcaster::new()),
    });
    state
        .route_proxy
        .activity()
        .set_emitter(EventEmitter::Web(Arc::clone(&state.event_broadcaster)));

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
        axum_server::from_tcp_rustls(listener, rustls_config)
            .serve(router.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .map_err(|error| format!("HTTPS server error: {error}"))
    } else {
        println!(
            "AI Switch server listening on {}",
            bound_web_base_url("http", bound_address)
        );
        axum::serve(listener, router)
            .await
            .map_err(|error| format!("Server error: {error}"))
    }
}
