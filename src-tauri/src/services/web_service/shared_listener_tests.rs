use super::*;
use crate::database::{create_memory_pool, run_migrations};
use crate::models::route_proxy_https::RouteProxyHttpsConfig;
use crate::services::route_proxy_https_service::{
    restore_auto_started_proxy, RouteProxyHttpsService,
};
use crate::services::route_proxy_service::{
    RouteProxyRuntimeState, RouteProxyService, RouteProxyStatus, RouteProxyTransport,
};
use serde_json::json;
use tempfile::{tempdir, TempDir};

async fn fixture() -> (TempDir, Arc<AppState>, WebServiceConfig) {
    let temp = tempdir().unwrap();
    let pool = create_memory_pool().await.unwrap();
    run_migrations(&pool).await.unwrap();
    let state = Arc::new(AppState {
        paths: AppPaths::from_data_dir(temp.path().join("app-data")),
        pool,
        saas: Default::default(),
        config_writes: Default::default(),
        deeplink_protocols: Default::default(),
        close_to_tray: Default::default(),
        route_proxy: RouteProxyRuntimeState::default(),
        web_service: Default::default(),
        tailscale: Default::default(),
        terminals: Default::default(),
        terminal_hub: Default::default(),
        event_broadcaster: Default::default(),
    });
    let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = WebServiceConfig {
        port: reservation.local_addr().unwrap().port(),
        route_access_enabled: true,
        ..Default::default()
    };
    WebService::save_config(&state.paths, &config)
        .await
        .unwrap();
    (temp, state, config)
}

#[tokio::test]
async fn desktop_web_and_model_apis_share_a_listener_without_saas() {
    let (_temp, state, config) = fixture().await;
    let started = WebService::start(Arc::clone(&state)).await.unwrap();
    let base_url = started.base_url.as_ref().unwrap();
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert_eq!(
        client
            .get(format!("{base_url}/health"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .post(format!("{base_url}/api/get_settings"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let models = client
        .get(format!("{base_url}/v1/models"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        models.status(),
        401,
        "model requests must reach proxy auth, never HTML fallback"
    );
    assert!(models.text().await.unwrap().contains("route_proxy"));
    let key = RouteProxyService::get_or_create_platform_key(&state.pool, "codex")
        .await
        .unwrap();
    assert_eq!(
        client
            .get(format!("{base_url}/v1/models"))
            .bearer_auth(&key)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .post(format!("{base_url}/api/get_settings"))
            .bearer_auth(key)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let proxy = RouteProxyService::status(&state.route_proxy).await;
    assert_eq!(proxy.port, started.port);
    assert_eq!(proxy.base_url, started.base_url);
    assert!(proxy.running);
    let again = RouteProxyHttpsService::start_proxy(&state).await.unwrap();
    assert_eq!(
        again, proxy,
        "starting the pool must reuse the shared listener"
    );
    WebService::stop(&state).await;
    assert!(
        !WebService::status(&state.web_service, &config)
            .await
            .running
    );
    assert!(!RouteProxyService::status(&state.route_proxy).await.running);
    assert!(tokio::net::TcpListener::bind(("127.0.0.1", config.port))
        .await
        .is_ok());
}

#[tokio::test]
async fn desktop_web_takes_over_an_existing_independent_pool() {
    let (_temp, state, config) = fixture().await;
    let old = RouteProxyService::start(
        &state.route_proxy,
        state.pool.clone(),
        RouteProxyTransport::HttpOnly,
    )
    .await
    .unwrap();
    let started = WebService::start(Arc::clone(&state)).await.unwrap();
    assert_eq!(
        RouteProxyService::status(&state.route_proxy).await.base_url,
        started.base_url
    );
    assert_ne!(old.port, started.port);
    assert!(
        tokio::net::TcpListener::bind(("127.0.0.1", old.port.unwrap()))
            .await
            .is_ok(),
        "old pool port must be released"
    );
    WebService::stop(&state).await;
    assert!(!RouteProxyService::status(&state.route_proxy).await.running);
    assert!(tokio::net::TcpListener::bind(("127.0.0.1", config.port))
        .await
        .is_ok());
}

#[tokio::test]
async fn desktop_web_reuses_an_existing_pool_socket_on_the_configured_port() {
    let (_temp, state, mut config) = fixture().await;
    let old = RouteProxyService::start(
        &state.route_proxy,
        state.pool.clone(),
        RouteProxyTransport::HttpOnly,
    )
    .await
    .unwrap();
    config.port = old.port.unwrap();
    WebService::save_config(&state.paths, &config)
        .await
        .unwrap();
    let started = WebService::start(Arc::clone(&state))
        .await
        .expect("our own pool socket is not an external port conflict");
    assert_eq!(started.port, old.port);
    let health = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("{}/health", started.base_url.unwrap()))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), 200);
    assert_eq!(
        health.json::<serde_json::Value>().await.unwrap()["ok"],
        true
    );
    WebService::stop(&state).await;
    assert!(!RouteProxyService::status(&state.route_proxy).await.running);
}

#[tokio::test]
async fn a_failed_web_bind_preserves_the_independent_pool() {
    let (_temp, state, mut config) = fixture().await;
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    config.port = occupied.local_addr().unwrap().port();
    WebService::save_config(&state.paths, &config)
        .await
        .unwrap();
    let old = RouteProxyService::start(
        &state.route_proxy,
        state.pool.clone(),
        RouteProxyTransport::HttpOnly,
    )
    .await
    .unwrap();
    assert_eq!(
        WebService::start(Arc::clone(&state))
            .await
            .unwrap_err()
            .code(),
        "web_service.bind"
    );
    let expected = RouteProxyStatus {
        route_access_enabled: true,
        ..old
    };
    assert_eq!(
        RouteProxyService::status(&state.route_proxy).await,
        expected
    );
    assert!(
        !WebService::status(&state.web_service, &config)
            .await
            .running
    );
    RouteProxyService::stop(&state.route_proxy).await.unwrap();
}

#[tokio::test]
async fn stopping_an_unstarted_web_service_does_not_clear_the_standalone_listener() {
    let (_temp, state, _config) = fixture().await;
    RouteProxyService::mark_shared_listener(
        &state.route_proxy,
        "127.0.0.1",
        32123,
        "http://127.0.0.1:32123",
    )
    .await;
    WebService::stop(&state).await;
    assert!(RouteProxyService::status(&state.route_proxy).await.running);
    RouteProxyService::clear_shared_listener(&state.route_proxy).await;
}

#[cfg(feature = "desktop")]
#[tokio::test]
async fn starting_the_desktop_pool_first_uses_the_configured_web_port() {
    let (_temp, state, config) = fixture().await;
    let proxy = RouteProxyHttpsService::start_proxy(&state).await.unwrap();
    assert_eq!(proxy.port, Some(config.port));
    assert!(
        WebService::status(&state.web_service, &config)
            .await
            .running
    );
    let stopped = RouteProxyHttpsService::stop_proxy(&state).await.unwrap();
    assert!(stopped.running);
    assert!(!stopped.route_access_enabled);
    assert!(
        WebService::status(&state.web_service, &config)
            .await
            .running
    );
}

#[cfg(feature = "desktop")]
#[tokio::test]
async fn legacy_pool_auto_start_restores_the_shared_web_listener() {
    let (_temp, state, config) = fixture().await;
    RouteProxyHttpsService::save_config(
        &state.paths,
        &RouteProxyHttpsConfig {
            enabled: false,
            auto_start: true,
        },
    )
    .await
    .unwrap();
    restore_auto_started_proxy(&state).await;
    assert_eq!(
        RouteProxyService::status(&state.route_proxy).await.port,
        Some(config.port)
    );
    assert!(
        WebService::status(&state.web_service, &config)
            .await
            .running
    );
    WebService::stop(&state).await;
    assert!(
        !RouteProxyHttpsService::load_config(&state.paths)
            .await
            .unwrap()
            .auto_start,
        "stopping the shared service must not leave legacy pool auto-start on"
    );
}

#[tokio::test]
async fn shared_web_model_routes_do_not_trust_unauthenticated_platform_headers() {
    let (_temp, state, config) = fixture().await;
    let started = WebService::start(Arc::clone(&state)).await.unwrap();
    let url = format!("{}/v1/models", started.base_url.unwrap());
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for key in [None, Some("invalid-key")] {
        let mut request = client.get(&url).header("x-ai-switch-platform", "codex");
        if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        assert_eq!(request.send().await.unwrap().status(), 401);
    }
    // The administrator's diagnostic requests may still select a platform.
    assert_eq!(
        client
            .get(&url)
            .header("x-ai-switch-platform", "codex")
            .bearer_auth(config.token.unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    WebService::stop(&state).await;
}

#[tokio::test]
async fn desktop_all_interface_http_serves_sensitive_commands_without_tls() {
    let (_temp, state, mut config) = fixture().await;
    config.host = "0.0.0.0".to_string();
    WebService::save_config(&state.paths, &config)
        .await
        .unwrap();

    let started = WebService::start(Arc::clone(&state)).await.unwrap();
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!(
            "{}/api/get_route_proxy_key",
            started.base_url.as_deref().unwrap()
        ))
        .bearer_auth(config.token.as_deref().unwrap())
        .json(&json!({"platform": "codex"}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert!(response.text().await.unwrap().contains("sk-ai-switch-"));
    WebService::stop(&state).await;
}

#[tokio::test]
async fn shared_web_https_reports_the_same_tls_endpoint_for_the_pool() {
    let (temp, state, mut config) = fixture().await;
    let rcgen::CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_path = temp.path().join("server-cert.pem");
    let key_path = temp.path().join("server-key.pem");
    tokio::fs::write(&cert_path, cert.pem()).await.unwrap();
    tokio::fs::write(&key_path, key_pair.serialize_pem())
        .await
        .unwrap();
    config.tls_enabled = true;
    config.tls_cert_path = Some(cert_path.display().to_string());
    config.tls_key_path = Some(key_path.display().to_string());
    WebService::save_config(&state.paths, &config)
        .await
        .unwrap();
    let web = WebService::start(Arc::clone(&state)).await.unwrap();
    let proxy = RouteProxyService::status(&state.route_proxy).await;
    assert_eq!(proxy.port, web.port);
    assert_eq!(proxy.https_port, web.port);
    assert_eq!(proxy.base_url, web.base_url);
    assert_eq!(proxy.https_base_url, web.base_url);
    assert_eq!(
        serde_json::to_value(&proxy).unwrap()["shared_listener"],
        true
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    assert_eq!(
        client
            .get(format!("{}/v1/models", web.base_url.unwrap()))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    WebService::stop(&state).await;
    assert!(!RouteProxyService::status(&state.route_proxy).await.running);
}

#[tokio::test]
async fn local_https_controls_cannot_mutate_a_shared_web_listener() {
    let (_temp, state, _config) = fixture().await;
    WebService::start(Arc::clone(&state)).await.unwrap();
    let before = RouteProxyService::status(&state.route_proxy).await;
    assert_eq!(
        RouteProxyHttpsService::disable(&state)
            .await
            .unwrap_err()
            .code(),
        "validation.route_proxy_shared_listener"
    );
    assert_eq!(RouteProxyService::status(&state.route_proxy).await, before);
    WebService::stop(&state).await;
}

#[test]
fn desktop_dev_uses_10086_while_release_keeps_the_server_default() {
    assert_eq!(super::default_service_port(true), 10086);
    assert_eq!(
        super::default_service_port(false),
        crate::server::standalone_default_port()
    );
    // Unit tests compile the release-shaped default; `tauri dev` supplies the
    // `dev` cfg and therefore gets 10086 without changing this contract.
    assert_eq!(
        WebServiceConfig::default().port,
        crate::server::standalone_default_port()
    );
}

#[tokio::test]
async fn historical_web_ports_migrate_once_to_the_shared_service_port() {
    let temp = tempfile::tempdir().unwrap();
    let paths = crate::paths::AppPaths::from_data_dir(temp.path().join("app-data"));
    paths.ensure().await.unwrap();
    let legacy = serde_json::json!({
        "host": "127.0.0.1",
        "port": 3090,
        "token": "legacy-token-123456",
        "autoStart": false,
        "tailscaleEnabled": false,
        "tailscaleExposureMode": "private",
        "tlsEnabled": false
    });
    tokio::fs::write(&paths.web_service_file, legacy.to_string())
        .await
        .unwrap();

    let migrated = WebService::load_config(&paths).await.unwrap();
    assert_eq!(migrated.port, 19527);
    assert!(migrated.shared_port_migrated);

    let mut custom = migrated.clone();
    custom.port = 3100;
    WebService::save_config(&paths, &custom).await.unwrap();
    assert_eq!(WebService::load_config(&paths).await.unwrap().port, 3100);
}

#[tokio::test]
async fn new_service_configs_mark_the_shared_port_migration_without_reset() {
    let temp = tempfile::tempdir().unwrap();
    let paths = crate::paths::AppPaths::from_data_dir(temp.path().join("app-data"));
    paths.ensure().await.unwrap();

    let created = WebService::load_config(&paths).await.unwrap();
    assert_eq!(created.port, 19527);
    assert!(created.shared_port_migrated);
    let mut custom = created.clone();
    custom.port = 3100;
    WebService::save_config(&paths, &custom).await.unwrap();
    assert_eq!(WebService::load_config(&paths).await.unwrap().port, 3100);
}
