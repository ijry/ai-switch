use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, RawPathParams, Request, State};
use axum::http::{header, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tower_http::cors::{Any, CorsLayer};

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::services::mobile_pairing::MobileTokenRegistry;
use crate::services::web_service::WebService;
use crate::web::auth::{authorize_api_request, ApiAuthState};
use crate::web::handlers::{dispatch_command, is_sensitive_command};
use crate::web::static_assets::resolve_static_file;
use crate::web::ws::events_socket;
use crate::web::terminal_ws::terminal_socket;

#[derive(Clone)]
pub struct WebServerContext {
    pub state: Arc<AppState>,
    pub token: Arc<String>,
    pub mobile_tokens: MobileTokenRegistry,
    pub static_dir: PathBuf,
    pub sensitive_command_gate: Arc<AtomicBool>,
}

pub const SENSITIVE_COMMAND_BODY_LIMIT: usize = 12 * 1024 * 1024;

fn h5_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}

pub fn build_router(state: Arc<AppState>, token: String, static_dir: PathBuf) -> Router {
    build_router_with_sensitive_commands(state, token, static_dir, true)
}

pub(crate) fn build_router_with_sensitive_commands(
    state: Arc<AppState>,
    token: String,
    static_dir: PathBuf,
    sensitive_commands_enabled: bool,
) -> Router {
    build_router_with_sensitive_command_gate(
        state,
        token,
        static_dir,
        Arc::new(AtomicBool::new(sensitive_commands_enabled)),
    )
}

pub(crate) fn build_router_with_sensitive_command_gate(
    state: Arc<AppState>,
    token: String,
    static_dir: PathBuf,
    sensitive_command_gate: Arc<AtomicBool>,
) -> Router {
    let context = WebServerContext {
        mobile_tokens: WebService::mobile_token_registry(&state.web_service),
        state,
        token: Arc::new(token),
        static_dir,
        sensitive_command_gate,
    };
    let api_router = Router::new()
        .route("/:command", post(api_command))
        .layer(DefaultBodyLimit::max(SENSITIVE_COMMAND_BODY_LIMIT))
        .layer(middleware::from_fn_with_state(
            context.clone(),
            gate_sensitive_commands,
        ))
        .layer(middleware::from_fn_with_state(
            Arc::new(ApiAuthState {
                primary_token: Arc::clone(&context.token),
                mobile_tokens: Arc::clone(&context.mobile_tokens),
            }),
            authorize_api_request,
        ))
        .layer(middleware::from_fn(disable_api_caching));

    Router::new()
        .route("/health", get(health))
        .route(
            "/pairing/redeem",
            post(redeem_mobile_pairing).layer(middleware::from_fn(disable_api_caching)),
        )
        .route("/ws/events", get(events_socket))
        .route("/ws/terminal/:session_id", get(terminal_socket))
        .nest("/api", api_router)
        .fallback(static_fallback)
        .with_state(context)
        .layer(h5_cors_layer())
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RedeemMobilePairingRequest {
    code: String,
}

async fn redeem_mobile_pairing(
    State(context): State<WebServerContext>,
    Json(input): Json<RedeemMobilePairingRequest>,
) -> Response {
    match WebService::redeem_mobile_pairing(&context.state, input.code).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => api_error_response(StatusCode::BAD_REQUEST, error.into()),
    }
}

async fn api_command(
    State(context): State<WebServerContext>,
    Path(command): Path<String>,
    Json(args): Json<Value>,
) -> Response {
    match dispatch_command(context.state, &command, args).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => api_error_response(StatusCode::BAD_REQUEST, error),
    }
}

async fn gate_sensitive_commands(
    State(context): State<WebServerContext>,
    path_params: RawPathParams,
    request: Request,
    next: Next,
) -> Response {
    let sensitive = path_params
        .iter()
        .find_map(|(key, value)| (key == "command").then_some(value))
        .is_some_and(is_sensitive_command);
    if sensitive && !context.sensitive_command_gate.load(Ordering::Acquire) {
        return error_response(StatusCode::NOT_FOUND, "Web command is not available");
    }
    next.run(request).await
}

async fn disable_api_caching(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        header::PRAGMA,
        axum::http::HeaderValue::from_static("no-cache"),
    );
    response
}

async fn static_fallback(State(context): State<WebServerContext>, uri: Uri) -> Response {
    let Some(file_path) = resolve_static_file(&context.static_dir, uri.path()) else {
        return error_response(StatusCode::NOT_FOUND, "AI Switch web assets not found");
    };

    match tokio::fs::read(&file_path).await {
        Ok(bytes) => {
            let content_type = content_type_for(&file_path);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(bytes))
                .unwrap_or_else(|_| {
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Could not build response",
                    )
                })
        }
        Err(_) => error_response(StatusCode::NOT_FOUND, "AI Switch web assets not found"),
    }
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "map" => "application/json; charset=utf-8",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn error_response(status: StatusCode, message: &str) -> Response {
    let code = if status == StatusCode::UNAUTHORIZED {
        "web.unauthorized"
    } else {
        "web.error"
    };
    (
        status,
        Json(json!({
            "code": code,
            "message": message,
            "details": null,
            "recoverable": status != StatusCode::UNAUTHORIZED,
            "operation_id": null
        })),
    )
        .into_response()
}

fn api_error_response(status: StatusCode, error: ApiError) -> Response {
    (status, Json(error)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::services::config_write_service::ConfigWriteRuntimeState;
    use crate::services::deeplink_protocol_service::DeepLinkProtocolRuntime;
    use crate::services::route_proxy_service::RouteProxyRuntimeState;
    use crate::services::tailscale_service::TailscaleRuntimeState;
    use crate::services::web_service::{WebService, WebServiceConfig, WebServiceRuntimeState};
    use crate::terminal_manager::TerminalManager;
    use crate::web::event_bridge::WebEventBroadcaster;
    use axum::body::to_bytes;
    use std::net::SocketAddr;
    use std::time::{Duration, SystemTime};
    use tempfile::{tempdir, TempDir};

    async fn spawn_test_router(
        sensitive_commands_enabled: bool,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        spawn_test_router_with_token(sensitive_commands_enabled, "secret").await
    }

    async fn spawn_test_router_with_token(
        sensitive_commands_enabled: bool,
        token: &str,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        spawn_test_router_with_gate(Arc::new(AtomicBool::new(sensitive_commands_enabled)), token)
            .await
    }

    async fn spawn_test_router_with_gate(
        sensitive_command_gate: Arc<AtomicBool>,
        token: &str,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
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
            terminal_hub: Arc::new(crate::web::terminal_hub::TerminalHub::default()),
            event_broadcaster: Arc::new(WebEventBroadcaster::default()),
        });
        let router = build_router_with_sensitive_command_gate(
            state,
            token.to_string(),
            temp.path().to_path_buf(),
            sensitive_command_gate,
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (address, handle)
    }

    async fn spawn_test_router_with_state(
        sensitive_command_gate: Arc<AtomicBool>,
        token: &str,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>, Arc<AppState>, TempDir) {
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
            terminal_hub: Arc::new(crate::web::terminal_hub::TerminalHub::default()),
            event_broadcaster: Arc::new(WebEventBroadcaster::default()),
        });
        let router = build_router_with_sensitive_command_gate(
            Arc::clone(&state),
            token.to_string(),
            temp.path().to_path_buf(),
            sensitive_command_gate,
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (address, handle, state, temp)
    }

    fn assert_sensitive_cache_headers(response: &reqwest::Response) {
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            response
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
    }

    fn assert_h5_cors_origin(response: &reqwest::Response) {
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|value| value.to_str().ok()),
            Some("*")
        );
        assert!(response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none());
    }

    fn assert_h5_preflight_headers(response: &reqwest::Response) {
        assert_h5_cors_origin(response);
        let allowed_methods = response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_METHODS)
            .and_then(|value| value.to_str().ok())
            .unwrap();
        for expected in ["GET", "POST", "OPTIONS"] {
            assert!(
                allowed_methods
                    .split(',')
                    .map(str::trim)
                    .any(|value| value == expected),
                "missing allowed method {expected}: {allowed_methods}"
            );
        }

        let allowed_headers = response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .and_then(|value| value.to_str().ok())
            .unwrap()
            .to_ascii_lowercase();
        for expected in ["authorization", "content-type"] {
            assert!(
                allowed_headers
                    .split(',')
                    .map(str::trim)
                    .any(|value| value == expected),
                "missing allowed header {expected}: {allowed_headers}"
            );
        }
    }

    #[tokio::test]
    async fn api_error_response_serializes_structured_error_directly() {
        let response = api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError {
                code: "capability.unavailable".to_string(),
                message: "Not supported".to_string(),
                details: Some("hermes:config_write".to_string()),
                recoverable: true,
                operation_id: Some("operation-1".to_string()),
            },
        );
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(value["code"], "capability.unavailable");
        assert_eq!(value["details"], "hermes:config_write");
        assert_eq!(value["operation_id"], "operation-1");
    }

    #[tokio::test]
    async fn unauthorized_response_uses_stable_code() {
        let response = error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(value["code"], "web.unauthorized");
        assert_eq!(value["recoverable"], false);
    }

    #[tokio::test]
    async fn h5_preflight_bypasses_api_auth_and_advertises_supported_request_shape() {
        let (address, server) = spawn_test_router(true).await;
        let response = reqwest::Client::new()
            .request(
                reqwest::Method::OPTIONS,
                format!("http://{address}/api/list_platform_capabilities"),
            )
            .header(header::ORIGIN, "https://h5.example.test")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .header(
                header::ACCESS_CONTROL_REQUEST_HEADERS,
                "authorization, content-type",
            )
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_h5_preflight_headers(&response);
        server.abort();
    }

    #[tokio::test]
    async fn h5_origin_does_not_bypass_api_bearer_authorization() {
        let (address, server) = spawn_test_router(true).await;
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/list_platform_capabilities"))
            .header(header::ORIGIN, "https://h5.example.test")
            .json(&json!({}))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_h5_cors_origin(&response);
        assert_sensitive_cache_headers(&response);
        server.abort();
    }

    #[tokio::test]
    async fn authenticated_h5_api_request_returns_cors_origin_header() {
        let (address, server) = spawn_test_router(true).await;
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/list_platform_capabilities"))
            .bearer_auth("secret")
            .header(header::ORIGIN, "https://h5.example.test")
            .json(&json!({}))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_h5_cors_origin(&response);
        server.abort();
    }

    #[tokio::test]
    async fn authorization_rejects_before_the_json_extractor() {
        let (address, server) = spawn_test_router(true).await;
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/export_route_credentials"))
            .header(header::CONTENT_TYPE, "application/json")
            .body("not-json")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_sensitive_cache_headers(&response);
        server.abort();
    }

    #[tokio::test]
    async fn export_requires_a_configured_bearer_token() {
        let (address, server) = spawn_test_router_with_token(true, "").await;
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/export_route_credentials"))
            .json(&json!({
                "input": {
                    "selection_context": {"platform": "claude", "pool_scope": "in_pool"},
                    "credential_ids": []
                }
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_sensitive_cache_headers(&response);
        server.abort();
    }

    #[tokio::test]
    async fn percent_encoded_export_command_cannot_bypass_sensitive_auth() {
        let (address, server) = spawn_test_router_with_token(true, "").await;
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/%65xport_route_credentials"))
            .json(&json!({
                "input": {
                    "selection_context": {"platform": "claude", "pool_scope": "in_pool"},
                    "credential_ids": []
                }
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_sensitive_cache_headers(&response);
        server.abort();
    }

    #[tokio::test]
    async fn export_success_and_errors_are_not_cacheable() {
        let (address, server) = spawn_test_router(true).await;
        let client = reqwest::Client::new();
        let success = client
            .post(format!("http://{address}/api/export_route_credentials"))
            .bearer_auth("secret")
            .json(&json!({
                "input": {
                    "selection_context": {"platform": "claude", "pool_scope": "in_pool"},
                    "credential_ids": []
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(success.status(), StatusCode::OK);
        assert_sensitive_cache_headers(&success);

        let error = client
            .post(format!("http://{address}/api/export_route_credentials"))
            .bearer_auth("secret")
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        assert_sensitive_cache_headers(&error);
        server.abort();
    }

    #[tokio::test]
    async fn export_route_enforces_the_sensitive_body_limit() {
        let (address, server) = spawn_test_router(true).await;
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/export_route_credentials"))
            .bearer_auth("secret")
            .header(header::CONTENT_TYPE, "application/json")
            .body(vec![b' '; SENSITIVE_COMMAND_BODY_LIMIT + 1])
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_sensitive_cache_headers(&response);
        server.abort();
    }

    #[tokio::test]
    async fn import_route_enforces_sensitive_auth_body_limit_and_cache_headers() {
        let (address, server) = spawn_test_router(true).await;
        let response = reqwest::Client::new()
            .post(format!(
                "http://{address}/api/preview_route_credential_import"
            ))
            .bearer_auth("secret")
            .header(header::CONTENT_TYPE, "application/json")
            .body(vec![b' '; SENSITIVE_COMMAND_BODY_LIMIT + 1])
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_sensitive_cache_headers(&response);
        server.abort();
    }

    #[tokio::test]
    async fn unauthorized_oversized_import_is_rejected_before_body_limit() {
        let (address, server) = spawn_test_router(true).await;
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/import_route_credentials"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::CONTENT_LENGTH,
                (SENSITIVE_COMMAND_BODY_LIMIT + 1).to_string(),
            )
            .body("")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_sensitive_cache_headers(&response);
        server.abort();
    }

    #[tokio::test]
    async fn import_success_and_errors_are_not_cacheable() {
        let (address, server) = spawn_test_router(true).await;
        let client = reqwest::Client::new();
        for (command, body, status) in [
            (
                "preview_route_credential_import",
                json!({
                    "input": {"text": "[]", "ambiguous_platform_choices": []}
                }),
                StatusCode::OK,
            ),
            (
                "import_route_credentials",
                json!({
                    "input": {
                        "text": "[]",
                        "ambiguous_platform_choices": [],
                        "restore_pool_membership": false
                    }
                }),
                StatusCode::OK,
            ),
            (
                "preview_route_credential_import",
                json!({}),
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let response = client
                .post(format!("http://{address}/api/{command}"))
                .bearer_auth("secret")
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), status, "{command}");
            assert_sensitive_cache_headers(&response);
        }
        server.abort();
    }

    #[tokio::test]
    async fn insecure_local_transport_does_not_expose_transfer_commands() {
        let (address, server) = spawn_test_router(false).await;
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/export_route_credentials"))
            .bearer_auth("secret")
            .json(&json!({}))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_sensitive_cache_headers(&response);
        server.abort();
    }

    #[tokio::test]
    async fn sensitive_command_gate_updates_without_rebuilding_the_router() {
        let gate = Arc::new(AtomicBool::new(true));
        let (address, server) = spawn_test_router_with_gate(Arc::clone(&gate), "secret").await;
        let client = reqwest::Client::new();
        let enabled = client
            .post(format!("http://{address}/api/export_route_credentials"))
            .bearer_auth("secret")
            .json(&json!({
                "input": {
                    "selection_context": {"platform": "claude", "pool_scope": "in_pool"},
                    "credential_ids": []
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(enabled.status(), StatusCode::OK);

        gate.store(false, Ordering::Release);
        let disabled = client
            .post(format!("http://{address}/api/export_route_credentials"))
            .bearer_auth("secret")
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(disabled.status(), StatusCode::NOT_FOUND);
        assert_sensitive_cache_headers(&disabled);
        server.abort();
    }

    #[tokio::test]
    async fn pairing_route_redeems_once_without_leaking_primary_token_and_sets_no_store_headers() {
        let gate = Arc::new(AtomicBool::new(true));
        let (address, server, state, _temp) =
            spawn_test_router_with_state(Arc::clone(&gate), "primary-secret").await;
        WebService::save_config(
            &state.paths,
            &WebServiceConfig {
                token: Some("primary-secret".to_string()),
                ..WebServiceConfig::default()
            },
        )
        .await
        .unwrap();
        let pairing_store = WebService::mobile_pairing_store_for_test(&state.web_service);
        let payload = pairing_store
            .create(
                Some("https://public.example".to_string()),
                None,
                SystemTime::now(),
                Duration::from_secs(300),
            )
            .await
            .unwrap();
        let client = reqwest::Client::new();

        let first = client
            .post(format!("http://{address}/pairing/redeem"))
            .json(&json!({ "code": payload.pairing_code }))
            .send()
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(
            first
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            first
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
        let first_body = first.text().await.unwrap();
        assert!(!first_body.contains("primary-secret"));
        let first_value: Value = serde_json::from_str(&first_body).unwrap();
        let mobile_token = first_value["token"].as_str().unwrap().to_string();
        assert!(mobile_token.starts_with("ms_"));

        let second = client
            .post(format!("http://{address}/pairing/redeem"))
            .json(&json!({ "code": payload.pairing_code }))
            .send()
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            second
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            second
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );

        let ordinary = client
            .post(format!("http://{address}/api/list_platform_capabilities"))
            .bearer_auth(&mobile_token)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(ordinary.status(), StatusCode::OK);

        for (command, body) in [
            ("get_web_service_config", json!({})),
            ("save_web_service_config", json!({})),
            ("get_web_server_status", json!({})),
            ("start_web_server", json!({})),
            ("stop_web_server", json!({})),
            ("get_tailscale_status", json!({})),
            ("start_tailscale_login", json!({})),
            (
                "start_tailscale_with_auth_key",
                json!({ "authKey": "tskey-auth-test" }),
            ),
            ("disconnect_tailscale", json!({})),
            ("create_mobile_pairing", json!({})),
            (
                "create_terminal_session",
                json!({
                    "input": {
                        "kind": "shell",
                        "cwd": "C:\\"
                    }
                }),
            ),
        ] {
            let response = client
                .post(format!("http://{address}/api/{command}"))
                .bearer_auth(&mobile_token)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{command}");
            let response_body = response.text().await.unwrap();
            assert!(!response_body.contains("primary-secret"), "{command}");
        }
        server.abort();
    }

    #[tokio::test]
    async fn mobile_pairing_token_cannot_open_the_event_stream() {
        let gate = Arc::new(AtomicBool::new(true));
        let (address, server, state, _temp) =
            spawn_test_router_with_state(gate, "primary-secret").await;
        let pairing_store = WebService::mobile_pairing_store_for_test(&state.web_service);
        let payload = pairing_store
            .create(
                Some("https://public.example".to_string()),
                None,
                SystemTime::now(),
                Duration::from_secs(300),
            )
            .await
            .unwrap();
        let redeemed = pairing_store
            .redeem(&payload.pairing_code, SystemTime::now())
            .await
            .unwrap();
        let response = reqwest::Client::new()
            .get(format!("http://{address}/ws/events"))
            .bearer_auth(redeemed.token)
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, "websocket")
            .header(header::SEC_WEBSOCKET_VERSION, "13")
            .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        server.abort();
    }

    #[tokio::test]
    async fn mobile_pairing_token_can_open_the_terminal_stream() {
        let gate = Arc::new(AtomicBool::new(true));
        let (address, server, state, _temp) =
            spawn_test_router_with_state(gate, "primary-secret").await;
        let pairing_store = WebService::mobile_pairing_store_for_test(&state.web_service);
        let payload = pairing_store
            .create(
                Some("https://public.example".to_string()),
                None,
                SystemTime::now(),
                Duration::from_secs(300),
            )
            .await
            .unwrap();
        let redeemed = pairing_store
            .redeem(&payload.pairing_code, SystemTime::now())
            .await
            .unwrap();
        let response = reqwest::Client::new()
            .get(format!("http://{address}/ws/terminal/session-1"))
            .bearer_auth(redeemed.token)
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, "websocket")
            .header(header::SEC_WEBSOCKET_VERSION, "13")
            .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        server.abort();
    }

    #[tokio::test]
    async fn unknown_token_cannot_open_the_terminal_stream() {
        let gate = Arc::new(AtomicBool::new(true));
        let (address, server) = spawn_test_router_with_gate(gate, "primary-secret").await;
        let response = reqwest::Client::new()
            .get(format!("http://{address}/ws/terminal/session-1"))
            .bearer_auth("not-a-token")
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, "websocket")
            .header(header::SEC_WEBSOCKET_VERSION, "13")
            .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        server.abort();
    }

    #[tokio::test]
    async fn terminal_stream_accepts_query_token_auth() {
        let gate = Arc::new(AtomicBool::new(true));
        let (address, server) = spawn_test_router_with_gate(gate, "primary-secret").await;
        let response = reqwest::Client::new()
            .get(format!(
                "http://{address}/ws/terminal/session-1?token=primary-secret&since=7"
            ))
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, "websocket")
            .header(header::SEC_WEBSOCKET_VERSION, "13")
            .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        server.abort();
    }

    #[tokio::test]
    async fn primary_token_can_open_the_event_stream_with_header_or_query_auth() {
        let gate = Arc::new(AtomicBool::new(true));
        let (address, server, _state, _temp) =
            spawn_test_router_with_state(gate, "primary-secret").await;
        let client = reqwest::Client::new();
        for request in [
            client
                .get(format!("http://{address}/ws/events"))
                .bearer_auth("primary-secret")
                .header(header::CONNECTION, "Upgrade")
                .header(header::UPGRADE, "websocket")
                .header(header::SEC_WEBSOCKET_VERSION, "13")
                .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ=="),
            client
                .get(format!("http://{address}/ws/events?token=primary-secret"))
                .header(header::CONNECTION, "Upgrade")
                .header(header::UPGRADE, "websocket")
                .header(header::SEC_WEBSOCKET_VERSION, "13")
                .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ=="),
        ] {
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        }
        server.abort();
    }

    #[tokio::test]
    async fn malformed_pairing_json_is_not_cacheable() {
        let gate = Arc::new(AtomicBool::new(true));
        let (address, server, _state, _temp) =
            spawn_test_router_with_state(gate, "primary-secret").await;
        let response = reqwest::Client::new()
            .post(format!("http://{address}/pairing/redeem"))
            .header(header::CONTENT_TYPE, "application/json")
            .body("{")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_sensitive_cache_headers(&response);
        server.abort();
    }
}
