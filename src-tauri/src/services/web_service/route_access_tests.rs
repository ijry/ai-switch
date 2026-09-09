use super::*;
use crate::database::{create_memory_pool, run_migrations};
use crate::services::route_proxy_https_service::RouteProxyHttpsService;
use crate::services::route_proxy_service::{RouteProxyRuntimeState, RouteProxyService};
use tempfile::tempdir;

async fn fixture() -> (tempfile::TempDir, Arc<AppState>, WebServiceConfig) {
    let directory = tempdir().unwrap();
    let pool = create_memory_pool().await.unwrap();
    run_migrations(&pool).await.unwrap();
    let state = Arc::new(AppState {
        paths: AppPaths::from_data_dir(directory.path().to_path_buf()),
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
        ..WebServiceConfig::default()
    };
    drop(reservation);
    (directory, state, config)
}

#[tokio::test]
async fn legacy_auto_start_migrates_to_route_access_preference() {
    let (_directory, state, _config) = fixture().await;
    let legacy = serde_json::json!({
        "host": "127.0.0.1",
        "port": 32123,
        "token": "0123456789abcdef",
        "autoStart": true,
        "tailscaleEnabled": false,
        "tlsEnabled": false
    });
    tokio::fs::write(
        &state.paths.web_service_file,
        serde_json::to_vec(&legacy).unwrap(),
    )
    .await
    .unwrap();

    let migrated = WebService::load_config(&state.paths).await.unwrap();
    assert!(migrated.route_access_enabled);
    let persisted: serde_json::Value = serde_json::from_slice(
        &tokio::fs::read(&state.paths.web_service_file)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(persisted["routeAccessEnabled"], true);
    assert!(persisted.get("autoStart").is_none());
}

#[tokio::test]
async fn route_access_toggle_is_independent_of_the_shared_listener() {
    let (_directory, state, config) = fixture().await;
    WebService::save_config(&state.paths, &config)
        .await
        .unwrap();

    RouteProxyService::set_route_access_enabled(&state.route_proxy, false).await;
    let disabled = RouteProxyService::status(&state.route_proxy).await;
    assert!(!disabled.route_access_enabled);
    assert!(!disabled.running);

    RouteProxyService::set_route_access_enabled(&state.route_proxy, true).await;
    let enabled = RouteProxyService::status(&state.route_proxy).await;
    assert!(enabled.route_access_enabled);
    assert!(!enabled.running);
}

#[tokio::test]
async fn enabling_route_access_starts_the_shared_listener_when_needed() {
    let (_directory, state, mut config) = fixture().await;
    config.route_access_enabled = false;
    config.token = Some("0123456789abcdef".to_string());
    let saved = WebService::save_config(&state.paths, &config)
        .await
        .unwrap();
    WebService::set_route_access(&state, true).await.unwrap();

    let status = RouteProxyService::status(&state.route_proxy).await;
    assert!(status.running);
    assert!(status.route_access_enabled);
    assert!(WebService::status(&state.web_service, &saved).await.running);
    assert!(
        WebService::load_config(&state.paths)
            .await
            .unwrap()
            .route_access_enabled
    );
}

#[tokio::test]
async fn stopping_the_shared_listener_preserves_route_access_preference() {
    let (_directory, state, mut config) = fixture().await;
    config.route_access_enabled = true;
    config.token = Some("0123456789abcdef".to_string());
    WebService::save_config(&state.paths, &config)
        .await
        .unwrap();
    WebService::set_route_access(&state, true).await.unwrap();
    WebService::stop(&state).await;

    let status = RouteProxyService::status(&state.route_proxy).await;
    assert!(!status.running);
    assert!(status.route_access_enabled);
    assert!(
        WebService::load_config(&state.paths)
            .await
            .unwrap()
            .route_access_enabled
    );
    RouteProxyHttpsService::clear_auto_start(&state.paths)
        .await
        .unwrap();
}
