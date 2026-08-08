use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
use crate::error::{ApiError, AppError};
use crate::models::platform::{ApiDialect, CapabilityRule, PlatformId, PlatformOperation};
use crate::models::route_credential::{
    normalize_anthropic_api_key_field, ModelMapping, ANTHROPIC_API_KEY_FIELD,
    ANTHROPIC_AUTH_TOKEN_FIELD,
};
use crate::models::route_pool::RouteUsageBreakdown;
use crate::services::http_client::build_outbound_http_client;
use crate::services::official_agent_identity_service::{
    is_official_agent_identity_credential, resolve_agent_identity_headers,
    CODEX_AGENT_IDENTITY_BASE_URL,
};
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::response_failure_service::detect_response_failed;
use crate::services::route_config_service::generate_route_proxy_key;
use crate::services::route_protocol_bridge::{
    prepare_request as prepare_protocol_bridge_request,
    transform_response as transform_protocol_bridge_response, PreparedBridgeRequest,
    ProtocolBridgeKind,
};
use crate::services::route_model_capability::{
    advertised_model_ids, parse_model_capability, parse_model_capability_value,
    requested_model_from_body, supports_requested_model,
};
use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;

const BIND_HOST: &str = "127.0.0.1";
const DEFAULT_ROUTE_PROXY_PORT: u16 = 19527;
const ROUTE_PROXY_KEY_CACHE_TTL: Duration = Duration::from_secs(30);
/// Public xAI Grok CLI OAuth client ID (CLIProxyAPI / Grok CLI).
const XAI_OAUTH_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
// Keep in sync with CLIProxyAPI xai_executor (cli-chat-proxy identity headers).
const GROK_CLI_CLIENT_VERSION: &str = "0.2.93";
const GROK_CLI_TOKEN_AUTH_VALUE: &str = "xai-grok-cli";
const GROK_CLI_CHAT_PROXY_MARKER: &str = "cli-chat-proxy.grok.com";
/// Refresh a short time before wall-clock expiry to avoid edge 401s.
const OAUTH_REFRESH_LEAD: Duration = Duration::from_secs(5 * 60);
const CUSTOM_TOOL_INPUT_FIELD: &str = "input";
const CUSTOM_TOOL_INPUT_DESCRIPTION: &str =
    "Raw string input for the original custom tool. Preserve formatting exactly and follow the original tool definition embedded in the description.";
const CUSTOM_TOOL_PRESERVED_METADATA_HEADING: &str = "Original tool definition:";
const ROUTE_PROXY_PLATFORM_HEADER: &str = "x-ai-switch-platform";
pub const ROUTE_PROXY_TRACE_HEADER: &str = "x-ai-switch-test-trace-id";
const ROUTE_PROXY_CORS_ALLOW_METHODS: &str = "GET, POST, PUT, PATCH, DELETE, OPTIONS";
const ROUTE_PROXY_CORS_DEFAULT_ALLOW_HEADERS: &str =
    "Authorization, Content-Type, X-API-Key, API-Key, X-Google-API-Key, X-AI-Switch-Platform, X-AI-Switch-Test-Trace-Id, Accept";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteProxyStatus {
    pub running: bool,
    pub bind_host: String,
    pub port: Option<u16>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RouteProxyTransport {
    Http,
    Https {
        certificate_pem_path: std::path::PathBuf,
        private_key_pem_path: std::path::PathBuf,
    },
}

#[derive(Clone, Default)]
pub struct RouteProxyRuntimeState {
    inner: Arc<Mutex<RouteProxyInner>>,
}

#[derive(Default)]
struct RouteProxyInner {
    running: bool,
    port: Option<u16>,
    base_url: Option<String>,
    shutdown: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

#[derive(Clone)]
struct ProxyAppState {
    pool: SqlitePool,
    key_cache: Arc<Mutex<RouteProxyKeyCache>>,
}

#[derive(Default)]
struct RouteProxyKeyCache {
    loaded_at: Option<Instant>,
    // proxy_key -> platform
    by_key: HashMap<String, String>,
}

#[derive(Debug)]
pub(crate) struct BuiltUpstreamRequest {
    pub(crate) target_url: String,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
    pub(crate) bridge_kind: Option<ProtocolBridgeKind>,
}

impl RouteProxyKeyCache {
    fn get_if_fresh(&self, proxy_key: &str) -> Option<Option<String>> {
        let loaded_at = self.loaded_at?;
        if loaded_at.elapsed() > ROUTE_PROXY_KEY_CACHE_TTL {
            return None;
        }
        Some(self.by_key.get(proxy_key).cloned())
    }

    fn replace(&mut self, rows: Vec<(String, String)>) {
        self.by_key = rows.into_iter().collect();
        self.loaded_at = Some(Instant::now());
    }

    fn upsert(&mut self, proxy_key: String, platform: String) {
        // Keep cache coherent immediately after write_configs without waiting for TTL.
        if self.loaded_at.is_none() {
            self.loaded_at = Some(Instant::now());
        }
        self.by_key.insert(proxy_key, platform);
    }
}

pub struct RouteProxyService;

impl RouteProxyService {
    pub async fn get_or_create_platform_key(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<String, AppError> {
        let platform = PlatformId::parse(platform)?;
        if let Some(existing) =
            RouteProxyKeyRepository::get_by_platform(pool, platform.as_str()).await?
        {
            if !existing.starts_with("sk-ai-switch-test-") {
                return Ok(existing);
            }

            let replacement = generate_route_proxy_key();
            RouteProxyKeyRepository::replace_platform_key(pool, platform.as_str(), &replacement)
                .await?;
            return RouteProxyKeyRepository::get_by_platform(pool, platform.as_str())
                .await?
                .ok_or_else(|| AppError::Database {
                    code: "database.route_proxy_key_missing_after_rotate",
                    message: "Could not load the new route proxy key".to_string(),
                    details: None,
                    recoverable: true,
                });
        }

        RouteProxyKeyRepository::ensure_platform_key(
            pool,
            platform.as_str(),
            &generate_route_proxy_key(),
        )
        .await
    }

    pub async fn status(state: &RouteProxyRuntimeState) -> RouteProxyStatus {
        let inner = state.inner.lock().await;
        RouteProxyStatus {
            running: inner.running,
            bind_host: BIND_HOST.to_string(),
            port: inner.port,
            base_url: inner.base_url.clone(),
        }
    }

    pub async fn start(
        state: &RouteProxyRuntimeState,
        pool: SqlitePool,
        transport: RouteProxyTransport,
    ) -> Result<RouteProxyStatus, AppError> {
        let mut inner = state.inner.lock().await;
        if inner.running {
            return Ok(RouteProxyStatus {
                running: true,
                bind_host: BIND_HOST.to_string(),
                port: inner.port,
                base_url: inner.base_url.clone(),
            });
        }

        let listener = bind_route_proxy_listener().await?;
        let addr = listener.local_addr().map_err(|err| AppError::Filesystem {
            code: "filesystem.route_proxy_addr",
            message: "Could not resolve route proxy address".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        let port = addr.port();
        let scheme = match &transport {
            RouteProxyTransport::Http => "http",
            RouteProxyTransport::Https { .. } => "https",
        };
        let base_url = format!("{scheme}://{BIND_HOST}:{port}");

        let app_state = ProxyAppState {
            pool,
            key_cache: Arc::new(Mutex::new(RouteProxyKeyCache::default())),
        };
        let app = Router::new()
            .fallback(any(proxy_handler))
            .with_state(app_state);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let join_handle = match transport {
            RouteProxyTransport::Http => tokio::spawn(async move {
                let server = axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                });

                if let Err(err) = server.await {
                    eprintln!("route proxy server error: {err}");
                }
            }),
            RouteProxyTransport::Https {
                certificate_pem_path,
                private_key_pem_path,
            } => {
                let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
                    &certificate_pem_path,
                    &private_key_pem_path,
                )
                .await
                .map_err(|error| AppError::Validation {
                    code: "validation.route_proxy_https_certificate",
                    message: "Could not load local route proxy HTTPS certificate".to_string(),
                    details: Some(error.to_string()),
                    recoverable: true,
                })?;
                let std_listener = listener.into_std().map_err(|error| AppError::Filesystem {
                    code: "filesystem.route_proxy_tls_listener",
                    message: "Could not prepare local HTTPS listener".to_string(),
                    details: Some(error.to_string()),
                    recoverable: true,
                })?;
                let handle = axum_server::Handle::new();

                tokio::spawn(async move {
                    let server = axum_server::from_tcp_rustls(std_listener, rustls_config)
                        .handle(handle.clone())
                        .serve(app.into_make_service_with_connect_info::<SocketAddr>());
                    tokio::pin!(server);

                    tokio::select! {
                        result = &mut server => {
                            if let Err(error) = result {
                                eprintln!("route proxy HTTPS server error: {error}");
                            }
                        }
                        _ = shutdown_rx => {
                            handle.graceful_shutdown(Some(Duration::from_secs(5)));
                            if let Err(error) = server.await {
                                eprintln!("route proxy HTTPS shutdown error: {error}");
                            }
                        }
                    }
                })
            }
        };

        inner.running = true;
        inner.port = Some(port);
        inner.base_url = Some(base_url);
        inner.shutdown = Some(shutdown_tx);
        inner.join_handle = Some(join_handle);

        Ok(RouteProxyStatus {
            running: true,
            bind_host: BIND_HOST.to_string(),
            port: Some(port),
            base_url: inner.base_url.clone(),
        })
    }

    pub async fn stop(state: &RouteProxyRuntimeState) -> Result<RouteProxyStatus, AppError> {
        let mut inner = state.inner.lock().await;
        if let Some(shutdown) = inner.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(handle) = inner.join_handle.take() {
            let _ = handle.await;
        }
        inner.running = false;
        inner.port = None;
        inner.base_url = None;

        Ok(RouteProxyStatus {
            running: false,
            bind_host: BIND_HOST.to_string(),
            port: None,
            base_url: None,
        })
    }
}

async fn proxy_handler(
    AxumState(state): AxumState<ProxyAppState>,
    method: Method,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Body,
) -> Response {
    let origin = cors_request_origin(&headers);
    if method == Method::OPTIONS {
        return cors_preflight_response(&headers);
    }

    let mut response = match forward_request(&state, method, headers, uri, body).await {
        Ok(response) => response,
        Err(err) => json_error(route_proxy_error_status(&err), &err),
    };
    add_cors_headers(&mut response, origin.as_ref());
    response
}

fn cors_request_origin(headers: &HeaderMap) -> Option<HeaderValue> {
    let origin = headers.get("origin")?.to_str().ok()?.trim();
    if origin.is_empty() {
        return None;
    }
    HeaderValue::from_str(origin).ok()
}

fn add_cors_headers(response: &mut Response, origin: Option<&HeaderValue>) {
    let Some(origin) = origin else {
        return;
    };

    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("access-control-allow-origin"),
        origin.clone(),
    );
    headers.append(
        HeaderName::from_static("vary"),
        HeaderValue::from_static("Origin"),
    );
}

fn cors_preflight_response(headers: &HeaderMap) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    let Some(origin) = cors_request_origin(headers) else {
        return response;
    };

    let response_headers = response.headers_mut();
    response_headers.insert(
        HeaderName::from_static("access-control-allow-origin"),
        origin,
    );
    response_headers.insert(
        HeaderName::from_static("access-control-allow-methods"),
        HeaderValue::from_static(ROUTE_PROXY_CORS_ALLOW_METHODS),
    );
    response_headers.insert(
        HeaderName::from_static("access-control-allow-headers"),
        headers
            .get("access-control-request-headers")
            .cloned()
            .unwrap_or_else(|| HeaderValue::from_static(ROUTE_PROXY_CORS_DEFAULT_ALLOW_HEADERS)),
    );
    response_headers.insert(
        HeaderName::from_static("access-control-max-age"),
        HeaderValue::from_static("600"),
    );
    response_headers.insert(
        HeaderName::from_static("vary"),
        HeaderValue::from_static(
            "Origin, Access-Control-Request-Headers, Access-Control-Request-Method",
        ),
    );
    if headers.contains_key("access-control-request-private-network") {
        response_headers.insert(
            HeaderName::from_static("access-control-allow-private-network"),
            HeaderValue::from_static("true"),
        );
    }
    response
}

async fn forward_request(
    state: &ProxyAppState,
    method: Method,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Body,
) -> Result<Response, String> {
    let pool = &state.pool;
    let path = uri.path().to_string();
    let query = uri.query().map(|value| value.to_string());
    let trace_id = route_proxy_trace_id(&headers);
    let inbound_key = extract_inbound_api_key(&headers, query.as_deref());
    let platform_id = resolve_platform(state, &headers, inbound_key.as_deref())
        .await
        .map_err(format_app_error)?;
    let platform = platform_id.as_str().to_string();
    let routing_rule =
        PlatformCapabilityService::require(platform_id, PlatformOperation::GenericApiRouting)
            .map_err(format_app_error)?;

    // OpenAI-compatible model listing: aggregate/dedupe client-facing model ids
    // from every enabled pool credential mapping instead of forwarding upstream.
    if is_models_list_path(&path) {
        if method != Method::GET {
            return Ok((
                StatusCode::METHOD_NOT_ALLOWED,
                [("content-type", "application/json"), ("allow", "GET")],
                json!({
                    "error": {
                        "code": "route_proxy.method_not_allowed",
                        "message": "Method not allowed for models list",
                        "type": "route_proxy_error",
                    }
                })
                .to_string(),
            )
                .into_response());
        }
        let credentials = select_pool_credentials(pool, &platform)
            .await
            .map_err(|err| err.to_string())?;
        let credentials = filter_credentials_for_rule(credentials, &routing_rule);
        return Ok(json_models_list_response(&platform, &credentials));
    }

    let body_bytes = axum::body::to_bytes(body, 32 * 1024 * 1024)
        .await
        .map_err(|err| format!("Could not read proxy request body: {err}"))?;
    let requested_model = requested_model_from_body(&body_bytes);
    let credentials = select_pool_credentials(pool, &platform)
        .await
        .map_err(|err| err.to_string())?;
    let credentials = filter_credentials_for_rule(credentials, &routing_rule);
    if credentials.is_empty() {
        return Err("No enabled route credentials in pool".to_string());
    }
    let credentials = filter_credentials_for_model(credentials, requested_model.as_deref());
    if credentials.is_empty() {
        let model = requested_model.as_deref().unwrap_or("unknown");
        return Err(format!(
            "route_pool.model_unmatched: no enabled route credential supports model '{model}' on platform '{platform}'"
        ));
    }
    let cursor = RoutePoolRepository::next_cursor_index(pool, &platform)
        .await
        .map_err(|err| err.to_string())?;

    let mut outbound_headers = HeaderMap::new();
    for (name, value) in headers.iter() {
        if is_hop_by_hop_header(name) {
            continue;
        }
        outbound_headers.append(name.clone(), value.clone());
    }
    // The inbound key is local proxy authentication only. Never forward it upstream.
    strip_route_proxy_auth_headers(&mut outbound_headers);

    let custom_tool_names = collect_custom_tool_names(&body_bytes);
    let upstream_query = strip_route_proxy_auth_query(query.as_deref());
    let client = build_outbound_http_client(None)?;
    let request_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|err| format!("Unsupported method: {err}"))?;
    let retry_indexes = retry_credential_indexes(credentials.len(), cursor);
    let mut retry_errors = Vec::new();
    let request_start = Instant::now();

    for credential_index in retry_indexes {
        let selected = &credentials[credential_index];
        let credential = match maybe_refresh_official_credential(pool, selected).await {
            Ok(credential) => credential,
            Err(error) => {
                if matches!(
                    classify_proxy_failure(None, Some(&error)),
                    ProxyFailureKind::Transient
                ) {
                    record_route_credential_failure(pool, &selected.id, "refresh", &error, None).await;
                }
                let metadata = route_proxy_request_metadata(
                    &platform,
                    selected,
                    &path,
                    None,
                    None,
                    false,
                    trace_id.as_deref(),
                    request_start,
                    Some(&error),
                );
                let _ = insert_route_credential_request_event(
                    pool,
                    &selected.id,
                    &metadata,
                    &RouteUsageBreakdown::default(),
                )
                .await;
                retry_errors.push(format!("{}: {error}", selected.display_name));
                continue;
            }
        };
        let upstream_request = build_upstream_request_internal(
            &credential,
            &platform,
            &path,
            upstream_query.as_deref(),
            outbound_headers.clone(),
            &body_bytes,
        );
        let BuiltUpstreamRequest {
            target_url,
            headers: request_headers,
            body: outbound_body,
            bridge_kind,
        } = match upstream_request {
            Ok(request) => request,
            Err(error) => {
                record_route_credential_failure(pool, &credential.id, "request_build", &error, None)
                    .await;
                let metadata = route_proxy_request_metadata(
                    &platform,
                    &credential,
                    &path,
                    None,
                    None,
                    false,
                    trace_id.as_deref(),
                    request_start,
                    Some(&error),
                );
                let _ = insert_route_credential_request_event(
                    pool,
                    &credential.id,
                    &metadata,
                    &RouteUsageBreakdown::default(),
                )
                .await;
                retry_errors.push(format!("{}: {error}", credential.display_name));
                continue;
            }
        };
        let upstream = client
            .request(request_method.clone(), &target_url)
            .headers(map_to_reqwest_headers(&request_headers))
            .body(outbound_body)
            .send()
            .await;

        let upstream = match upstream {
            Ok(response) => response,
            Err(error) => {
                let error_message = format!(
                    "{}: upstream request failed: {error}",
                    credential.display_name
                );
                record_route_credential_failure(pool, &credential.id, "transport", &error_message, None)
                    .await;
                let metadata = route_proxy_request_metadata(
                    &platform,
                    &credential,
                    &path,
                    Some(&target_url),
                    None,
                    false,
                    trace_id.as_deref(),
                    request_start,
                    Some(&error_message),
                );
                let _ = insert_route_credential_request_event(
                    pool,
                    &credential.id,
                    &metadata,
                    &RouteUsageBreakdown::default(),
                )
                .await;
                retry_errors.push(error_message);
                continue;
            }
        };
        let status =
            StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let mut upstream_headers = upstream.headers().clone();
        let mut response_bytes = match upstream.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => {
                let error_message = format!(
                    "{}: could not read upstream response: {error}",
                    credential.display_name
                );
                record_route_credential_failure(pool, &credential.id, "transport", &error_message, None)
                    .await;
                let metadata = route_proxy_request_metadata(
                    &platform,
                    &credential,
                    &path,
                    Some(&target_url),
                    None,
                    false,
                    trace_id.as_deref(),
                    request_start,
                    Some(&error_message),
                );
                let _ = insert_route_credential_request_event(
                    pool,
                    &credential.id,
                    &metadata,
                    &RouteUsageBreakdown::default(),
                )
                .await;
                retry_errors.push(error_message);
                continue;
            }
        };
        if let Some(bridge_kind) = bridge_kind {
            let content_type = upstream_headers
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok());
            let transformed = match transform_protocol_bridge_response(
                bridge_kind,
                status.as_u16(),
                content_type,
                &response_bytes,
            ) {
                Ok(response) => response,
                Err(error) => {
                    let error_message = format!(
                        "{}: could not transform upstream response: {error}",
                        credential.display_name
                    );
                    record_route_credential_failure(
                        pool,
                        &credential.id,
                        "response_transform",
                        &error_message,
                        Some(&response_bytes),
                    )
                    .await;
                    let metadata = route_proxy_request_metadata(
                        &platform,
                        &credential,
                        &path,
                        Some(&target_url),
                        Some(status.as_u16()),
                        false,
                        trace_id.as_deref(),
                        request_start,
                        Some(&error_message),
                    );
                    let _ = insert_route_credential_request_event(
                        pool,
                        &credential.id,
                        &metadata,
                        &RouteUsageBreakdown::default(),
                    )
                    .await;
                    retry_errors.push(error_message);
                    continue;
                }
            };
            response_bytes = transformed.body.into();
            upstream_headers.remove(axum::http::header::CONTENT_LENGTH);
            if let Some(content_type) = transformed.content_type {
                if let Ok(value) = HeaderValue::from_str(&content_type) {
                    upstream_headers.insert(axum::http::header::CONTENT_TYPE, value);
                }
            }
        }
        if !custom_tool_names.is_empty() {
            response_bytes =
                restore_custom_tools_in_responses_payload(&response_bytes, &custom_tool_names)
                    .into();
        }
        // Capture official subscription/quota signals (e.g. Grok free-usage-exhausted).
        let quota_exhausted = if credential.kind == "official" {
            if let Ok(body_text) = std::str::from_utf8(&response_bytes) {
                maybe_persist_official_quota_from_response(pool, &credential, body_text)
                    .await
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };
        let response_text = std::str::from_utf8(&response_bytes).ok();
        let semantic_failure = detect_response_failed(&response_bytes);
        let failure_kind = classify_proxy_failure(Some(status), response_text);
        let should_retry = !matches!(failure_kind, ProxyFailureKind::None);
        let proxy_success =
            status.is_success() && !quota_exhausted && !should_retry && semantic_failure.is_none();
        let retry_error = if let Some(failure) = semantic_failure.as_ref() {
            Some(failure.message.clone())
        } else if quota_exhausted {
            Some("upstream quota exhausted".to_string())
        } else if should_retry {
            Some("upstream returned retryable status".to_string())
        } else {
            None
        };
        let metadata = route_proxy_request_metadata(
            &platform,
            &credential,
            &path,
            Some(&target_url),
            Some(status.as_u16()),
            proxy_success,
            trace_id.as_deref(),
            request_start,
            retry_error.as_deref(),
        );
        let usage = extract_usage_breakdown(&response_bytes);
        let _ =
            insert_route_credential_request_event(pool, &credential.id, &metadata, &usage).await;

        let next_index = (credential_index + 1) % credentials.len();
        let _ = RoutePoolRepository::save_cursor_index(pool, &platform, next_index as i64).await;

        if quota_exhausted {
            retry_errors.push(format!(
                "{}: upstream quota exhausted",
                credential.display_name
            ));
            continue;
        }
        if let Some(failure) = semantic_failure {
            let _ = RouteCredentialRepository::record_semantic_failure(
                pool,
                &credential.id,
                &failure.message,
                Some(&response_bytes),
            )
            .await;
            retry_errors.push(format!("{}: {}", credential.display_name, failure.message));
            continue;
        }
        if should_retry {
            let error_message = format!("upstream returned {}", status.as_u16());
            if matches!(failure_kind, ProxyFailureKind::Permanent) {
                mark_route_credential_revoked(pool, &credential.id).await;
            } else {
                record_route_credential_failure(
                    pool,
                    &credential.id,
                    "upstream_status",
                    &error_message,
                    Some(&response_bytes),
                )
                .await;
            }
            retry_errors.push(format!(
                "{}: upstream returned {}",
                credential.display_name,
                status.as_u16()
            ));
            continue;
        }

        let _ = RouteCredentialRepository::clear_transient_failure(pool, &credential.id).await;
        return proxy_upstream_response(status, upstream_headers, response_bytes.to_vec());
    }

    Err(format!(
        "All route credentials failed for {platform}: {}",
        retry_errors.join(" | ")
    ))
}

fn map_to_reqwest_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut mapped = reqwest::header::HeaderMap::new();
    for (name, value) in headers.iter() {
        if let (Ok(req_name), Ok(req_value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()),
            reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            mapped.append(req_name, req_value);
        }
    }
    mapped
}

fn strip_route_proxy_auth_query(query: Option<&str>) -> Option<String> {
    let query = query?.trim();
    if query.is_empty() {
        return None;
    }

    let remaining: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair.split_once('=').map(|(key, _)| key).unwrap_or(*pair);
            !matches!(key, "key" | "api_key" | "apiKey")
        })
        .collect();
    if remaining.is_empty() {
        None
    } else {
        Some(remaining.join("&"))
    }
}

fn strip_route_proxy_auth_headers(headers: &mut HeaderMap) {
    headers.remove(axum::http::header::AUTHORIZATION);
    headers.remove("x-api-key");
    headers.remove("api-key");
    headers.remove("x-goog-api-key");
    headers.remove(ROUTE_PROXY_PLATFORM_HEADER);
    headers.remove(ROUTE_PROXY_TRACE_HEADER);
}

fn route_proxy_trace_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get(ROUTE_PROXY_TRACE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn route_proxy_request_metadata(
    platform: &str,
    credential: &SelectedCredential,
    path: &str,
    target_url: Option<&str>,
    status: Option<u16>,
    success: bool,
    trace_id: Option<&str>,
    started_at: Instant,
    error_message: Option<&str>,
) -> String {
    serde_json::json!({
        "platform": platform,
        "route_credential_id": credential.id,
        "route_credential_name": credential.display_name,
        "entry_path": path,
        "path": path,
        "target_url": target_url,
        "status": status,
        "success": success,
        "duration_ms": elapsed_millis(started_at),
        "trace_id": trace_id,
        "error_message": error_message,
    })
    .to_string()
}

fn elapsed_millis(started_at: Instant) -> i64 {
    started_at.elapsed().as_millis().min(i64::MAX as u128) as i64
}

fn retry_credential_indexes(len: usize, cursor: i64) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let first = cursor.rem_euclid(len as i64) as usize;
    (0..len).map(|offset| (first + offset) % len).collect()
}

fn proxy_upstream_response(
    status: StatusCode,
    upstream_headers: HeaderMap,
    response_bytes: Vec<u8>,
) -> Result<Response, String> {
    let mut response = Response::builder().status(status);
    if let Some(header_map) = response.headers_mut() {
        for (name, value) in upstream_headers.iter() {
            if is_hop_by_hop_header(name) {
                continue;
            }
            header_map.append(name.clone(), value.clone());
        }
    }
    response
        .body(Body::from(response_bytes))
        .map_err(|error| format!("Could not build proxy response: {error}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyFailureKind {
    Transient,
    Permanent,
    None,
}

pub fn classify_proxy_failure(
    status: Option<StatusCode>,
    message: Option<&str>,
) -> ProxyFailureKind {
    let lower = message.unwrap_or_default().to_ascii_lowercase();
    if lower.contains("invalid_grant")
        || lower.contains("refresh token has been revoked")
        || lower.contains("token has been revoked")
        || lower.contains("官方 oauth 凭证已失效")
    {
        return ProxyFailureKind::Permanent;
    }
    if status.is_some_and(should_retry_proxy_failure) || status.is_none() {
        return ProxyFailureKind::Transient;
    }
    ProxyFailureKind::None
}

fn should_retry_proxy_failure(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::REQUEST_TIMEOUT
            | StatusCode::UNAUTHORIZED
            | StatusCode::FORBIDDEN
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

pub fn credential_is_retryable_now(
    next_retry_at: Option<&str>,
    cooldown_until: Option<&str>,
    now: DateTime<Utc>,
) -> bool {
    [next_retry_at, cooldown_until]
        .into_iter()
        .all(|timestamp| {
            timestamp
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc) <= now)
                .unwrap_or(true)
        })
}

async fn record_route_credential_failure(
    pool: &SqlitePool,
    credential_id: &str,
    kind: &str,
    message: &str,
    response_body: Option<&[u8]>,
) {
    let _ = RouteCredentialRepository::record_transient_failure(
        pool,
        credential_id,
        kind,
        message,
        response_body,
    )
    .await;
}

async fn mark_route_credential_revoked(pool: &SqlitePool, credential_id: &str) {
    let _ = RouteCredentialRepository::update_status(pool, credential_id, "revoked").await;
}

fn json_error(status: StatusCode, message: &str) -> Response {
    let platform_unresolved = message.contains("route_proxy.platform_unresolved");
    let code = if platform_unresolved {
        "route_proxy.auth_required"
    } else if message.contains("No enabled route credentials in pool") {
        "route_pool.empty"
    } else if message.contains("route_pool.model_unmatched") {
        "route_pool.model_unmatched"
    } else {
        "route_proxy.error"
    };
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message,
            "type": "route_proxy_error",
        }
    })
    .to_string();
    let mut response = (status, [("content-type", "application/json")], body).into_response();
    if platform_unresolved {
        response.headers_mut().insert(
            axum::http::header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer"),
        );
    }
    response
}

fn route_proxy_error_status(message: &str) -> StatusCode {
    if message.contains("route_proxy.platform_unresolved") {
        StatusCode::UNAUTHORIZED
    } else {
        StatusCode::BAD_GATEWAY
    }
}

async fn resolve_platform(
    state: &ProxyAppState,
    headers: &HeaderMap,
    inbound_key: Option<&str>,
) -> Result<PlatformId, AppError> {
    // Preferred: stable per-platform local proxy key written into CLI configs.
    // Keys are cached in memory and refreshed at most every 30s.
    if let Some(key) = inbound_key {
        if let Some(platform) = lookup_platform_by_proxy_key(state, key).await? {
            return PlatformId::parse(&platform);
        }
    }

    if let Some(value) = headers
        .get(ROUTE_PROXY_PLATFORM_HEADER)
        .and_then(|value| value.to_str().ok())
    {
        return PlatformId::parse(value);
    }

    Err(AppError::Validation {
        code: "route_proxy.platform_unresolved",
        message: "Route proxy platform could not be resolved; provide the local route proxy key with Authorization: Bearer, x-api-key, or x-ai-switch-platform".to_string(),
        details: None,
        recoverable: true,
    })
}

async fn lookup_platform_by_proxy_key(
    state: &ProxyAppState,
    proxy_key: &str,
) -> Result<Option<String>, AppError> {
    let key = proxy_key.trim();
    if key.is_empty() {
        return Ok(None);
    }

    let fresh_hit = {
        let cache = state.key_cache.lock().await;
        cache.get_if_fresh(key)
    };
    if let Some(Some(platform)) = fresh_hit {
        return Ok(Some(platform));
    }
    // Fresh negative cache hit: still re-check DB so newly written keys work before TTL.
    if matches!(fresh_hit, Some(None)) {
        if let Some(platform) =
            RouteProxyKeyRepository::get_platform_by_key(&state.pool, key).await?
        {
            let mut cache = state.key_cache.lock().await;
            cache.upsert(key.to_string(), platform.clone());
            return Ok(Some(platform));
        }
        return Ok(None);
    }

    let rows = RouteProxyKeyRepository::list_all(&state.pool).await?;
    let mut cache = state.key_cache.lock().await;
    cache.replace(rows);
    Ok(cache.by_key.get(key).cloned())
}

pub fn extract_inbound_api_key(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(value) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        let trimmed = value.trim();
        if let Some(token) = trimmed
            .strip_prefix("Bearer ")
            .or_else(|| trimmed.strip_prefix("bearer "))
        {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }

    for name in ["x-api-key", "api-key", "x-goog-api-key"] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    if let Some(query) = query {
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default();
            if matches!(key, "key" | "api_key" | "apiKey") {
                let decoded = urlencoding_decode(value);
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
        }
    }

    None
}

fn urlencoding_decode(value: &str) -> String {
    // Minimal percent-decoding for query api keys (hex digits only).
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedCredential {
    pub id: String,
    pub platform: String,
    pub kind: String,
    pub display_name: String,
    pub status: String,
    pub secret_payload_json: String,
    pub config_json: String,
}

pub async fn select_pool_credentials(
    pool: &SqlitePool,
    platform: &str,
) -> Result<Vec<SelectedCredential>, AppError> {
    let rows = sqlx::query(
        "SELECT c.id, c.platform, c.kind, c.display_name, c.status, c.secret_payload_json, c.config_json,
                c.next_retry_at, c.cooldown_until
         FROM route_pool_members rpm
         INNER JOIN route_credentials c ON c.id = rpm.route_credential_id
         WHERE rpm.platform = ?
           AND rpm.enabled = 1
           AND c.archived_at IS NULL
           AND c.status = 'ok'
           AND (c.primary_remain IS NULL OR c.primary_remain > 0)
           AND (c.weekly_remain IS NULL OR c.weekly_remain > 0)
         ORDER BY rpm.sort_order ASC, rpm.created_at ASC",
    )
    .bind(platform)
    .fetch_all(pool)
    .await
    .map_err(|err| AppError::Database {
        code: "database.route_proxy_credentials",
        message: "Could not load route credentials for proxy".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    let now = Utc::now();
    let mut eligible = Vec::new();
    let mut cooling = Vec::new();
    for row in rows {
        let next_retry_at: Option<String> = row.get("next_retry_at");
        let cooldown_until: Option<String> = row.get("cooldown_until");
        let credential = SelectedCredential {
            id: row.get("id"),
            platform: row.get("platform"),
            kind: row.get("kind"),
            display_name: row.get("display_name"),
            status: row.get("status"),
            secret_payload_json: row.get("secret_payload_json"),
            config_json: row.get("config_json"),
        };
        // Skip official accounts already known to have zero remaining quota.
        if !is_route_credential_quota_available(&credential.config_json) {
            continue;
        }
        if credential_is_retryable_now(next_retry_at.as_deref(), cooldown_until.as_deref(), now) {
            eligible.push(credential);
            continue;
        }

        let retry_at = [next_retry_at.as_deref(), cooldown_until.as_deref()]
            .into_iter()
            .filter_map(|timestamp| {
                DateTime::parse_from_rfc3339(timestamp?)
                    .ok()
                    .map(|value| value.with_timezone(&Utc))
            })
            .max();
        if let Some(retry_at) = retry_at {
            cooling.push((retry_at, cooling.len(), credential));
        }
    }

    if !eligible.is_empty() {
        return Ok(eligible);
    }

    cooling.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    Ok(cooling
        .into_iter()
        .take(1)
        .map(|(_, _, credential)| credential)
        .collect())
}

fn filter_credentials_for_rule(
    mut credentials: Vec<SelectedCredential>,
    rule: &CapabilityRule,
) -> Vec<SelectedCredential> {
    if !rule.credential_kinds.is_empty() {
        credentials.retain(|credential| {
            rule.credential_kinds
                .iter()
                .any(|kind| kind == &credential.kind)
        });
    }
    credentials
}

fn filter_credentials_for_model(
    mut credentials: Vec<SelectedCredential>,
    requested_model: Option<&str>,
) -> Vec<SelectedCredential> {
    let Some(requested_model) = requested_model else {
        return credentials;
    };

    credentials.retain(|credential| {
        let capability = parse_model_capability(&credential.config_json);
        supports_requested_model(&capability, Some(requested_model))
    });
    credentials
}

async fn bind_route_proxy_listener() -> Result<TcpListener, AppError> {
    bind_route_proxy_listener_from(DEFAULT_ROUTE_PROXY_PORT).await
}

async fn bind_route_proxy_listener_from(start_port: u16) -> Result<TcpListener, AppError> {
    let mut last_error = None;
    for port in start_port..=u16::MAX {
        match TcpListener::bind((BIND_HOST, port)).await {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(format!("{BIND_HOST}:{port}: {error}")),
        }
    }

    Err(AppError::Filesystem {
        code: "filesystem.route_proxy_bind",
        message: "Could not bind local route proxy".to_string(),
        details: last_error,
        recoverable: true,
    })
}

pub fn pick_credential(items: &[SelectedCredential], cursor: i64) -> Option<&SelectedCredential> {
    if items.is_empty() {
        return None;
    }
    let index = cursor.rem_euclid(items.len() as i64) as usize;
    items.get(index)
}

/// Third-party Responses gateways often reject Codex `tools[].type = "custom"`.
/// Keep model mapping independent so callers can compose both rewrites.
pub fn apply_responses_custom_tool_compat(body: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let _ = rewrite_custom_tools_in_responses_request(&mut value);
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

pub fn apply_model_mappings(body: &[u8], mappings: &[ModelMapping]) -> Vec<u8> {
    if mappings.is_empty() {
        return body.to_vec();
    }

    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    rewrite_model_value(&mut value, mappings);
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

fn rewrite_model_value(value: &mut Value, mappings: &[ModelMapping]) {
    match value {
        Value::Object(object) => {
            if let Some(model) = object
                .get("model")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
            {
                if let Some(mapping) = mappings
                    .iter()
                    .find(|mapping| model_mapping_matches(&mapping.from, &model))
                {
                    object.insert("model".to_string(), Value::String(mapping.to.clone()));
                }
            }
            for child in object.values_mut() {
                rewrite_model_value(child, mappings);
            }
        }
        Value::Array(items) => {
            for child in items {
                rewrite_model_value(child, mappings);
            }
        }
        _ => {}
    }
}

fn model_mapping_matches(mapping_from: &str, requested_model: &str) -> bool {
    let mapping_from = mapping_from.trim();
    let requested_model = requested_model.trim();
    if mapping_from == requested_model {
        return true;
    }

    match (
        claude_route_lookup_model(mapping_from),
        claude_route_lookup_model(requested_model),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn claude_route_lookup_model(model: &str) -> Option<&str> {
    let stripped = strip_one_m_suffix_for_route_lookup(model);
    if is_claude_route_model(stripped) {
        Some(stripped)
    } else {
        None
    }
}

fn strip_one_m_suffix_for_route_lookup(model: &str) -> &str {
    const ONE_M_CONTEXT_MARKER: &str = "[1m]";
    let trimmed = model.trim();
    let marker = ONE_M_CONTEXT_MARKER.as_bytes();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= marker.len()
        && bytes[bytes.len() - marker.len()..].eq_ignore_ascii_case(marker)
    {
        return trimmed[..trimmed.len() - marker.len()].trim_end();
    }
    trimmed
}

fn is_claude_route_model(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    lower.starts_with("claude-") || lower.starts_with("anthropic/claude-")
}

pub fn build_upstream_request(
    credential: &SelectedCredential,
    platform: &str,
    path: &str,
    query: Option<&str>,
    headers: HeaderMap,
    body: &[u8],
) -> Result<(String, HeaderMap, Vec<u8>), String> {
    let request =
        build_upstream_request_internal(credential, platform, path, query, headers, body)?;
    Ok((request.target_url, request.headers, request.body))
}

pub(crate) fn build_upstream_request_with_bridge(
    credential: &SelectedCredential,
    platform: &str,
    path: &str,
    query: Option<&str>,
    headers: HeaderMap,
    body: &[u8],
) -> Result<BuiltUpstreamRequest, String> {
    build_upstream_request_internal(credential, platform, path, query, headers, body)
}

fn build_upstream_request_internal(
    credential: &SelectedCredential,
    platform: &str,
    path: &str,
    query: Option<&str>,
    mut headers: HeaderMap,
    body: &[u8],
) -> Result<BuiltUpstreamRequest, String> {
    let secret = parse_json_object(&credential.secret_payload_json, "secret")?;
    let config = parse_json_object(&credential.config_json, "config")?;

    if credential.kind == "api" {
        build_api_upstream_request(
            credential,
            platform,
            path,
            query,
            &mut headers,
            body,
            &secret,
            &config,
        )
    } else {
        build_official_upstream_request(
            credential,
            platform,
            path,
            query,
            &mut headers,
            body,
            &secret,
            &config,
        )
    }
}

fn build_api_upstream_request(
    credential: &SelectedCredential,
    platform: &str,
    path: &str,
    query: Option<&str>,
    headers: &mut HeaderMap,
    body: &[u8],
    secret: &Value,
    config: &Value,
) -> Result<BuiltUpstreamRequest, String> {
    let platform = PlatformId::parse(platform).map_err(format_app_error)?;
    PlatformCapabilityService::require(platform, PlatformOperation::GenericApiRouting)
        .map_err(format_app_error)?;
    let dialect = match string_value(config, "interface_format") {
        Some(value) => ApiDialect::parse(value).map_err(format_app_error)?,
        None if matches!(
            platform,
            PlatformId::OpenCode | PlatformId::OpenClaw | PlatformId::Hermes
        ) =>
        {
            return Err("validation.api_dialect_required".to_string());
        }
        None => platform
            .default_api_credential_dialect()
            .ok_or_else(|| "validation.api_dialect_required".to_string())?,
    };
    let api_key = string_value(secret, "api_key").ok_or_else(|| {
        format!(
            "Route credential {} is missing api_key",
            credential.display_name
        )
    })?;
    let base_url = string_value(config, "base_url").ok_or_else(|| {
        format!(
            "Route credential {} is missing base_url",
            credential.display_name
        )
    })?;
    let interface_format = dialect.as_str();
    let mappings = parse_model_capability_value(config).mappings;
    let upstream_path = normalize_api_upstream_path(interface_format, path);
    let mut rewritten_body = apply_model_mappings(body, &mappings);
    // API relays (e.g. Xiaomi) commonly lack Codex custom-tool support on Responses.
    let bridge_requires_custom_tool_compat = platform == PlatformId::Codex
        && dialect == ApiDialect::OpenAi
        && is_responses_path(&upstream_path);
    if (responses_custom_tool_compat_enabled(config) || bridge_requires_custom_tool_compat)
        && should_rewrite_custom_tools_for_api(interface_format, &upstream_path)
    {
        rewritten_body = apply_responses_custom_tool_compat(&rewritten_body);
    }
    let PreparedBridgeRequest {
        kind: bridge_kind,
        upstream_path,
        upstream_query,
        body: rewritten_body,
        ..
    } = prepare_protocol_bridge_request(platform, dialect, &upstream_path, &rewritten_body)?;
    let merged_query = merge_query_parts(query, upstream_query.as_deref());
    let mut target_url = build_target_url(base_url, &upstream_path, merged_query.as_deref());

    match dialect {
        ApiDialect::Anthropic => {
            match normalize_anthropic_api_key_field(string_value(config, "api_key_field"))
                .map_err(|err| format!("Route credential {} {err}", credential.display_name))?
            {
                ANTHROPIC_AUTH_TOKEN_FIELD => {
                    headers.remove("x-api-key");
                    insert_header(headers, "authorization", &format!("Bearer {api_key}"))?;
                }
                ANTHROPIC_API_KEY_FIELD => {
                    headers.remove("authorization");
                    insert_header(headers, "x-api-key", api_key)?;
                }
                _ => unreachable!("normalize_anthropic_api_key_field returns known constants"),
            }
            headers
                .entry(HeaderName::from_static("anthropic-version"))
                .or_insert(HeaderValue::from_static("2023-06-01"));
        }
        ApiDialect::Gemini => {
            target_url = append_query_param(&target_url, "key", api_key);
        }
        ApiDialect::OpenAi | ApiDialect::OpenAiResponses => {
            insert_header(headers, "authorization", &format!("Bearer {api_key}"))?;
        }
    }

    apply_credential_user_agent(headers, config)?;
    Ok(BuiltUpstreamRequest {
        target_url,
        headers: headers.clone(),
        body: rewritten_body,
        bridge_kind,
    })
}

fn build_official_upstream_request(
    credential: &SelectedCredential,
    platform: &str,
    path: &str,
    query: Option<&str>,
    headers: &mut HeaderMap,
    body: &[u8],
    secret: &Value,
    config: &Value,
) -> Result<BuiltUpstreamRequest, String> {
    let platform = PlatformId::parse(platform).map_err(format_app_error)?;
    PlatformCapabilityService::require(platform, PlatformOperation::OfficialAccountRouting)
        .map_err(format_app_error)?;
    // Apply credential-provided headers first (CPA may ship extra headers).
    apply_config_headers(headers, config)?;

    if let Some(agent_identity) = resolve_agent_identity_headers(secret, config)? {
        insert_header(headers, "authorization", &agent_identity.authorization)?;
        insert_header(
            headers,
            "chatgpt-account-id",
            &agent_identity.chatgpt_account_id,
        )?;
        if agent_identity.is_fedramp_account {
            insert_header(headers, "x-openai-fedramp", "true")?;
        }
    } else {
        let access_token = resolve_official_access_token(credential, secret, config)?;
        insert_header(headers, "authorization", &format!("Bearer {access_token}"))?;
    }
    if platform == PlatformId::Claude {
        headers
            .entry(HeaderName::from_static("anthropic-version"))
            .or_insert(HeaderValue::from_static("2023-06-01"));
    }
    let base_url = if let Some(base_url) = string_value(config, "base_url") {
        base_url
    } else if platform == PlatformId::Codex && is_official_agent_identity_credential(secret, config)
    {
        CODEX_AGENT_IDENTITY_BASE_URL
    } else {
        default_official_base_url(platform)?
    };
    // cli-chat-proxy rejects unversioned clients with HTTP 426 (version = none).
    if platform == PlatformId::Grok && is_grok_cli_chat_proxy_base_url(base_url) {
        apply_official_grok_cli_headers(headers)?;
    }
    apply_credential_user_agent(headers, config)?;
    let target_url = build_target_url(base_url, path, query);
    Ok(BuiltUpstreamRequest {
        target_url,
        headers: headers.clone(),
        body: body.to_vec(),
        bridge_kind: None,
    })
}

fn is_grok_cli_chat_proxy_base_url(base_url: &str) -> bool {
    base_url
        .to_ascii_lowercase()
        .contains(GROK_CLI_CHAT_PROXY_MARKER)
}

fn apply_official_grok_cli_headers(headers: &mut HeaderMap) -> Result<(), String> {
    // Force-set so outdated CPA exports (User-Agent: grok-cli) cannot win.
    insert_header(headers, "x-xai-token-auth", GROK_CLI_TOKEN_AUTH_VALUE)?;
    insert_header(headers, "x-grok-client-version", GROK_CLI_CLIENT_VERSION)?;
    insert_header(
        headers,
        "user-agent",
        &format!("xai-grok-workspace/{GROK_CLI_CLIENT_VERSION}"),
    )?;
    headers.remove("x-client-name");
    Ok(())
}

fn apply_config_headers(headers: &mut HeaderMap, config: &Value) -> Result<(), String> {
    let Some(Value::Object(extra)) = config.get("headers") else {
        return Ok(());
    };
    for (name, value) in extra {
        let Some(value) = value
            .as_str()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        else {
            continue;
        };
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|err| format!("Invalid credential header name {name}: {err}"))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|err| format!("Invalid credential header value for {name}: {err}"))?;
        // Only fill missing headers so inbound request values still win when present.
        headers.entry(header_name).or_insert(header_value);
    }
    Ok(())
}

fn credential_user_agent(config: &Value) -> Option<&str> {
    let Some(Value::Object(extra)) = config.get("headers") else {
        return None;
    };
    for (name, value) in extra {
        if name.eq_ignore_ascii_case("user-agent") {
            return value
                .as_str()
                .map(str::trim)
                .filter(|item| !item.is_empty());
        }
    }
    None
}

fn apply_credential_user_agent(headers: &mut HeaderMap, config: &Value) -> Result<(), String> {
    let Some(user_agent) = credential_user_agent(config) else {
        return Ok(());
    };
    insert_header(headers, "user-agent", user_agent)
}

fn resolve_official_access_token(
    credential: &SelectedCredential,
    secret: &Value,
    _config: &Value,
) -> Result<String, String> {
    // Token refresh happens in maybe_refresh_official_credential before build.
    if let Some(access_token) = string_value(secret, "access_token") {
        return Ok(access_token.to_string());
    }

    if string_value(secret, "refresh_token").is_some() {
        return Err("route_credential.refresh_only_unsupported".to_string());
    }

    Err(format!(
        "Route credential {} is missing access_token",
        credential.display_name
    ))
}

fn access_token_is_expired(config: &Value) -> bool {
    access_token_is_expired_with_secret(config, None)
}

fn access_token_is_expired_with_secret(config: &Value, secret: Option<&Value>) -> bool {
    if let Some(raw) = config.get("expired") {
        match raw {
            Value::String(value) => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
                        return dt.with_timezone(&Utc)
                            <= Utc::now()
                                + chrono::Duration::from_std(OAUTH_REFRESH_LEAD)
                                    .unwrap_or_default();
                    }
                }
            }
            Value::Number(number) => {
                if let Some(ts) = number.as_i64() {
                    // Accept unix seconds.
                    return Utc::now().timestamp() + OAUTH_REFRESH_LEAD.as_secs() as i64 >= ts;
                }
            }
            Value::Bool(true) => return true,
            _ => {}
        }
    }

    // Fallback: parse access JWT `exp` when config.expired is missing/unusable.
    if let Some(secret) = secret {
        if let Some(access_token) = string_value(secret, "access_token") {
            if let Some(exp) = jwt_claim_i64(access_token, "exp") {
                return Utc::now().timestamp() + OAUTH_REFRESH_LEAD.as_secs() as i64 >= exp;
            }
        }
    }

    false
}

/// Refresh an official OAuth access token when missing/expired and a token_endpoint exists.
/// Returns updated secret/config JSON when refresh succeeds.
pub async fn maybe_refresh_official_credential(
    pool: &SqlitePool,
    credential: &SelectedCredential,
) -> Result<SelectedCredential, String> {
    if credential.kind != "official" {
        return Ok(credential.clone());
    }

    let secret = parse_json_object(&credential.secret_payload_json, "secret")?;
    let config = parse_json_object(&credential.config_json, "config")?;
    let has_access = string_value(&secret, "access_token").is_some()
        && !access_token_is_expired_with_secret(&config, Some(&secret));
    if has_access {
        return Ok(credential.clone());
    }

    let refresh_token = match string_value(&secret, "refresh_token") {
        Some(value) => value.to_string(),
        None => return Ok(credential.clone()),
    };
    let Some(token_endpoint) = string_value(&config, "token_endpoint").map(str::to_string) else {
        return Ok(credential.clone());
    };
    let client_id = resolve_oauth_client_id(&credential.platform, &config, &secret);

    let refreshed =
        match refresh_oauth_access_token(&token_endpoint, &refresh_token, client_id.as_deref())
            .await
        {
            Ok(value) => value,
            Err(err) => {
                if is_permanent_oauth_refresh_failure(&err) {
                    mark_route_credential_revoked(pool, &credential.id).await;
                }
                return Err(format_oauth_refresh_failure(&err));
            }
        };
    let mut secret_obj = secret
        .as_object()
        .cloned()
        .ok_or_else(|| "Route credential secret JSON must be an object".to_string())?;
    let mut config_obj = config
        .as_object()
        .cloned()
        .ok_or_else(|| "Route credential config JSON must be an object".to_string())?;

    secret_obj.insert(
        "access_token".to_string(),
        Value::String(refreshed.access_token.clone()),
    );
    if let Some(refresh) = refreshed.refresh_token {
        secret_obj.insert("refresh_token".to_string(), Value::String(refresh));
    }
    if let Some(id_token) = refreshed.id_token {
        secret_obj.insert("id_token".to_string(), Value::String(id_token));
    }
    if let Some(token_type) = refreshed.token_type {
        config_obj.insert("token_type".to_string(), Value::String(token_type));
    }
    if let Some(expires_in) = refreshed.expires_in {
        config_obj.insert("expires_in".to_string(), json!(expires_in));
        if let Some(expired_at) =
            Utc::now().checked_add_signed(chrono::Duration::seconds(expires_in))
        {
            config_obj.insert(
                "expired".to_string(),
                Value::String(expired_at.to_rfc3339()),
            );
        }
    } else if let Some(exp) = jwt_claim_i64(&refreshed.access_token, "exp") {
        if let Some(expired_at) = chrono::DateTime::<Utc>::from_timestamp(exp, 0) {
            config_obj.insert(
                "expired".to_string(),
                Value::String(expired_at.to_rfc3339()),
            );
        }
    }
    config_obj.insert(
        "last_refresh".to_string(),
        Value::String(Utc::now().to_rfc3339()),
    );

    let secret_payload_json = Value::Object(secret_obj).to_string();
    let config_json = Value::Object(config_obj).to_string();

    // Best-effort persistence; request can still proceed with in-memory tokens.
    let _ = RouteCredentialRepository::update_secret_and_config(
        pool,
        &credential.id,
        &secret_payload_json,
        &config_json,
    )
    .await;

    Ok(SelectedCredential {
        secret_payload_json,
        config_json,
        ..credential.clone()
    })
}

#[derive(Debug, Clone)]
struct OAuthRefreshResult {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<i64>,
}

async fn refresh_oauth_access_token(
    token_endpoint: &str,
    refresh_token: &str,
    client_id: Option<&str>,
) -> Result<OAuthRefreshResult, String> {
    let client = build_outbound_http_client(Some(Duration::from_secs(20)))?;
    let mut form = format!(
        "grant_type=refresh_token&refresh_token={}",
        urlencoding_encode(refresh_token)
    );
    if let Some(client_id) = client_id.map(str::trim).filter(|item| !item.is_empty()) {
        form.push_str("&client_id=");
        form.push_str(&urlencoding_encode(client_id));
    }
    let response = client
        .post(token_endpoint)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(form)
        .send()
        .await
        .map_err(|err| format!("OAuth refresh request failed: {err}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("OAuth refresh response read failed: {err}"))?;
    if !status.is_success() {
        return Err(format!(
            "OAuth refresh failed with status {}: {}",
            status.as_u16(),
            body.chars().take(240).collect::<String>()
        ));
    }

    let value = serde_json::from_str::<Value>(&body)
        .map_err(|err| format!("OAuth refresh JSON invalid: {err}"))?;
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .ok_or_else(|| "OAuth refresh response missing access_token".to_string())?
        .to_string();

    Ok(OAuthRefreshResult {
        access_token,
        refresh_token: value
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string),
        id_token: value
            .get("id_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string),
        token_type: value
            .get("token_type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string),
        expires_in: value.get("expires_in").and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_f64().map(|n| n as i64))
                .or_else(|| item.as_str().and_then(|s| s.parse::<i64>().ok()))
        }),
    })
}

fn is_permanent_oauth_refresh_failure(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("invalid_grant")
        || lower.contains("refresh token has been revoked")
        || lower.contains("token has been revoked")
        || lower.contains("invalid_client")
        || lower.contains("unauthorized_client")
}

fn format_oauth_refresh_failure(message: &str) -> String {
    if is_permanent_oauth_refresh_failure(message) {
        format!("官方 OAuth 凭证已失效（revoked），请重新导入 CPA 授权文件。原始错误：{message}")
    } else {
        message.to_string()
    }
}

fn resolve_oauth_client_id(platform: &str, config: &Value, secret: &Value) -> Option<String> {
    if let Some(value) = string_value(config, "client_id").map(str::to_string) {
        return Some(value);
    }
    if let Some(value) = string_value(secret, "client_id").map(str::to_string) {
        return Some(value);
    }
    if let Some(access_token) = string_value(secret, "access_token") {
        if let Some(value) = jwt_claim_string(access_token, "client_id") {
            return Some(value);
        }
        if let Some(value) = jwt_claim_string(access_token, "azp") {
            return Some(value);
        }
    }

    let platform = platform.trim().to_ascii_lowercase();
    let endpoint = string_value(config, "token_endpoint").unwrap_or("");
    let endpoint_lower = endpoint.to_ascii_lowercase();
    if platform == "grok"
        || platform == "xai"
        || endpoint_lower.contains("auth.x.ai")
        || endpoint_lower.contains("x.ai")
    {
        return Some(XAI_OAUTH_CLIENT_ID.to_string());
    }
    None
}

fn jwt_claim_string(token: &str, claim: &str) -> Option<String> {
    jwt_payload(token)?
        .get(claim)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

fn jwt_claim_i64(token: &str, claim: &str) -> Option<i64> {
    let payload = jwt_payload(token)?;
    let value = payload.get(claim)?;
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|n| n as i64))
        .or_else(|| value.as_str().and_then(|s| s.parse::<i64>().ok()))
}

fn jwt_payload(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    if payload.is_empty() {
        return None;
    }
    let decoded = decode_base64url_nopad(payload)?;
    serde_json::from_slice::<Value>(&decoded).ok()
}

fn decode_base64url_nopad(input: &str) -> Option<Vec<u8>> {
    fn decode_table(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }

    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4 + 2);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for &byte in bytes {
        if byte == b'=' {
            break;
        }
        let value = decode_table(byte)?;
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

fn urlencoding_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficialQuotaSnapshot {
    pub subscription_type: Option<String>,
    pub primary_remain: Option<i64>,
    pub weekly_remain: Option<i64>,
    pub reset_primary: Option<String>,
    pub reset_weekly: Option<String>,
    // Legacy detail fields retained in config_json only.
    pub quota_remaining: Option<i64>,
    pub quota_limit: Option<i64>,
    pub quota_used: Option<i64>,
}

fn config_remain_positive(config: &Value, keys: &[&str]) -> bool {
    for key in keys {
        match config.get(*key) {
            None | Some(Value::Null) => continue,
            Some(Value::Number(value)) => {
                return value
                    .as_i64()
                    .map(|remaining| remaining > 0)
                    .unwrap_or(true);
            }
            Some(Value::String(value)) => {
                return value
                    .trim()
                    .parse::<i64>()
                    .map(|remaining| remaining > 0)
                    .unwrap_or(true);
            }
            Some(_) => return true,
        }
    }
    true
}

pub fn is_route_credential_quota_available(config_json: &str) -> bool {
    // Unknown/missing remaining means "not known exhausted" — keep selectable.
    let Ok(config) = parse_json_object(config_json, "config") else {
        return true;
    };
    let primary_ok = config_remain_positive(&config, &["primary_remain", "quota_remaining"]);
    let weekly_ok = config_remain_positive(&config, &["weekly_remain"]);
    primary_ok && weekly_ok
}

pub fn parse_official_quota_snapshot(response_body: &str) -> Option<OfficialQuotaSnapshot> {
    let lower = response_body.to_ascii_lowercase();
    let exhausted = lower.contains("subscription:free-usage-exhausted")
        || lower.contains("free-usage-exhausted")
        || lower.contains("used all the included free usage");
    if !exhausted {
        return None;
    }

    let mut quota_used = None;
    let mut quota_limit = None;
    if let Some((used, limit)) = parse_tokens_actual_limit(response_body) {
        quota_used = Some(used);
        quota_limit = Some(limit);
    }

    Some(OfficialQuotaSnapshot {
        subscription_type: Some("free".to_string()),
        primary_remain: Some(0),
        weekly_remain: None,
        reset_primary: None,
        reset_weekly: None,
        quota_remaining: Some(0),
        quota_limit,
        quota_used,
    })
}

fn parse_tokens_actual_limit(text: &str) -> Option<(i64, i64)> {
    let marker = "tokens (actual/limit):";
    let lower = text.to_ascii_lowercase();
    let start = lower.find(marker)?;
    let tail = text[start + marker.len()..].trim_start();
    let mut digits = String::new();
    let mut slash_seen = false;
    let mut left = String::new();
    let mut right = String::new();
    for ch in tail.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        if ch == '/' && !slash_seen && !digits.is_empty() {
            left = std::mem::take(&mut digits);
            slash_seen = true;
            continue;
        }
        if !digits.is_empty() {
            if slash_seen {
                right = std::mem::take(&mut digits);
            } else {
                left = std::mem::take(&mut digits);
            }
            break;
        }
        if !left.is_empty() {
            break;
        }
    }
    if slash_seen && right.is_empty() && !digits.is_empty() {
        right = digits;
    }
    if left.is_empty() || right.is_empty() {
        return None;
    }
    let used = left.parse::<i64>().ok()?;
    let limit = right.parse::<i64>().ok()?;
    Some((used, limit))
}

pub fn apply_official_quota_snapshot(
    config_json: &str,
    snapshot: &OfficialQuotaSnapshot,
) -> Result<String, String> {
    let mut config = parse_json_object(config_json, "config")?;
    let Some(object) = config.as_object_mut() else {
        return Err("Route credential config JSON must be an object".to_string());
    };
    if let Some(subscription_type) = &snapshot.subscription_type {
        object.insert("subscription_type".to_string(), json!(subscription_type));
    }
    if let Some(primary_remain) = snapshot.primary_remain {
        object.insert("primary_remain".to_string(), json!(primary_remain));
        // Keep legacy key dual-written for older readers/filters.
        object.insert("quota_remaining".to_string(), json!(primary_remain));
    } else if let Some(quota_remaining) = snapshot.quota_remaining {
        object.insert("quota_remaining".to_string(), json!(quota_remaining));
        object.insert("primary_remain".to_string(), json!(quota_remaining));
    }
    if let Some(weekly_remain) = snapshot.weekly_remain {
        object.insert("weekly_remain".to_string(), json!(weekly_remain));
    }
    if let Some(reset_primary) = &snapshot.reset_primary {
        object.insert("reset_primary".to_string(), json!(reset_primary));
    }
    if let Some(reset_weekly) = &snapshot.reset_weekly {
        object.insert("reset_weekly".to_string(), json!(reset_weekly));
    }
    if let Some(quota_limit) = snapshot.quota_limit {
        object.insert("quota_limit".to_string(), json!(quota_limit));
    }
    if let Some(quota_used) = snapshot.quota_used {
        object.insert("quota_used".to_string(), json!(quota_used));
    }
    let now = Utc::now().to_rfc3339();
    object.insert("quota_updated_at".to_string(), json!(now.clone()));
    if snapshot.reset_primary.is_none() && snapshot.primary_remain.is_some() {
        // Without an exact vendor reset timestamp, stamp primary reset to update time.
        object
            .entry("reset_primary".to_string())
            .or_insert_with(|| json!(now));
    }
    Ok(config.to_string())
}

pub async fn maybe_persist_official_quota_from_response(
    pool: &SqlitePool,
    credential: &SelectedCredential,
    response_body: &str,
) -> Result<bool, AppError> {
    if credential.kind != "official" {
        return Ok(false);
    }
    let Some(snapshot) = parse_official_quota_snapshot(response_body) else {
        return Ok(false);
    };
    let next_config =
        apply_official_quota_snapshot(&credential.config_json, &snapshot).map_err(|message| {
            AppError::Validation {
                code: "validation.route_credential_quota",
                message,
                details: Some(credential.id.clone()),
                recoverable: true,
            }
        })?;
    if next_config == credential.config_json {
        return Ok(false);
    }
    RouteCredentialRepository::update_secret_and_config(
        pool,
        &credential.id,
        &credential.secret_payload_json,
        &next_config,
    )
    .await?;
    Ok(true)
}

fn parse_json_object(raw: &str, label: &str) -> Result<Value, String> {
    let value = serde_json::from_str::<Value>(raw)
        .map_err(|err| format!("Route credential {label} JSON is invalid: {err}"))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(format!("Route credential {label} JSON must be an object"))
    }
}

fn string_value<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
}

fn should_rewrite_custom_tools_for_api(interface_format: &str, path: &str) -> bool {
    interface_format == "openai-responses" || is_responses_path(path)
}

pub fn normalize_api_upstream_path(interface_format: &str, path: &str) -> String {
    let normalized = normalize_request_path(path);
    if !matches!(interface_format, "openai" | "openai-responses") {
        return normalized;
    }
    strip_leading_version_path_segments(&normalized)
}

fn normalize_request_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn responses_custom_tool_compat_enabled(config: &Value) -> bool {
    config
        .get("responses_custom_tool_compat")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn is_responses_path(path: &str) -> bool {
    let normalized = path.trim().trim_end_matches('/');
    normalized.ends_with("/responses") || normalized == "responses"
}

fn strip_leading_version_path_segments(path: &str) -> String {
    let mut remaining = path.trim_start_matches('/');
    while let Some(first) = remaining.split('/').next() {
        if !is_version_path_segment(first) {
            break;
        }
        remaining = remaining[first.len()..].trim_start_matches('/');
    }
    if remaining.is_empty() {
        String::new()
    } else {
        format!("/{remaining}")
    }
}

fn is_version_path_segment(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix('v') else {
        return false;
    };
    !rest.is_empty() && rest.chars().next().is_some_and(|ch| ch.is_ascii_digit())
}

fn collect_custom_tool_names(body: &[u8]) -> std::collections::HashSet<String> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return std::collections::HashSet::new();
    };
    let mut names = std::collections::HashSet::new();
    if let Some(tools) = value.get("tools").and_then(Value::as_array) {
        for tool in tools {
            if tool_type(tool) == Some("custom") {
                if let Some(name) = responses_tool_name(tool) {
                    names.insert(name);
                }
            }
        }
    }
    if let Some(input) = value.get("input").and_then(Value::as_array) {
        for item in input {
            if tool_type(item) == Some("custom_tool_call") {
                if let Some(name) = item
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    names.insert(name.to_string());
                }
            }
        }
    }
    names
}

fn rewrite_custom_tools_in_responses_request(
    value: &mut Value,
) -> std::collections::HashSet<String> {
    let mut custom_names = std::collections::HashSet::new();
    if let Some(tools) = value.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools.iter_mut() {
            if tool_type(tool) != Some("custom") {
                continue;
            }
            if let Some(name) = responses_tool_name(tool) {
                custom_names.insert(name.clone());
                *tool = custom_tool_to_function_tool(tool, &name);
            }
        }
    }

    if let Some(input) = value.get_mut("input").and_then(Value::as_array_mut) {
        for item in input.iter_mut() {
            match tool_type(item) {
                Some("custom_tool_call") => {
                    if let Some(name) = item
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                    {
                        custom_names.insert(name);
                    }
                    *item = custom_tool_call_to_function_call(item);
                }
                Some("custom_tool_call_output") => {
                    *item = custom_tool_call_output_to_function_call_output(item);
                }
                _ => {}
            }
        }
    }

    custom_names
}

fn restore_custom_tools_in_responses_payload(
    body: &[u8],
    custom_tool_names: &std::collections::HashSet<String>,
) -> Vec<u8> {
    if custom_tool_names.is_empty() {
        return body.to_vec();
    }

    if let Ok(mut value) = serde_json::from_slice::<Value>(body) {
        restore_custom_tools_in_value(&mut value, custom_tool_names);
        return serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec());
    }

    // SSE Responses streams are line-delimited `data: {...}` payloads.
    let Ok(text) = std::str::from_utf8(body) else {
        return body.to_vec();
    };
    if !text.contains("data:") {
        return body.to_vec();
    }

    let mut rewritten = String::with_capacity(text.len() + 64);
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let ends_with_crlf = line.ends_with("\r\n");
        let ends_with_lf = line.ends_with('\n');
        if let Some(payload) = trimmed.strip_prefix("data:") {
            let payload = payload.trim_start();
            if payload != "[DONE]" {
                if let Ok(mut value) = serde_json::from_str::<Value>(payload) {
                    restore_custom_tools_in_value(&mut value, custom_tool_names);
                    if let Ok(serialized) = serde_json::to_string(&value) {
                        rewritten.push_str("data: ");
                        rewritten.push_str(&serialized);
                        if ends_with_crlf {
                            rewritten.push_str("\r\n");
                        } else if ends_with_lf {
                            rewritten.push('\n');
                        }
                        continue;
                    }
                }
            }
        }
        rewritten.push_str(line);
    }
    rewritten.into_bytes()
}

fn restore_custom_tools_in_value(
    value: &mut Value,
    custom_tool_names: &std::collections::HashSet<String>,
) {
    if tool_type(value) == Some("function_call") {
        if let Some(name) = value
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
        {
            if custom_tool_names.contains(&name) {
                *value = function_call_to_custom_tool_call(value);
                return;
            }
        }
    }

    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                restore_custom_tools_in_value(child, custom_tool_names);
            }
        }
        Value::Array(items) => {
            for child in items {
                restore_custom_tools_in_value(child, custom_tool_names);
            }
        }
        _ => {}
    }
}

fn custom_tool_to_function_tool(tool: &Value, name: &str) -> Value {
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            format!(
                "{CUSTOM_TOOL_PRESERVED_METADATA_HEADING}\n```json\n{}\n```",
                tool
            )
        });

    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": {
            "type": "object",
            "properties": {
                CUSTOM_TOOL_INPUT_FIELD: {
                    "type": "string",
                    "description": CUSTOM_TOOL_INPUT_DESCRIPTION
                }
            },
            "required": [CUSTOM_TOOL_INPUT_FIELD],
            "additionalProperties": false
        }
    })
}

fn custom_tool_call_to_function_call(item: &Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("type".to_string(), json!("function_call"));
    if let Some(id) = item.get("id").cloned() {
        object.insert("id".to_string(), id);
    }
    if let Some(call_id) = item.get("call_id").cloned() {
        object.insert("call_id".to_string(), call_id);
    }
    if let Some(status) = item.get("status").cloned() {
        object.insert("status".to_string(), status);
    }
    if let Some(name) = item.get("name").cloned() {
        object.insert("name".to_string(), name);
    }
    let input = item.get("input").cloned().unwrap_or_else(|| json!(""));
    let arguments = serde_json::to_string(&json!({ CUSTOM_TOOL_INPUT_FIELD: input }))
        .unwrap_or_else(|_| format!(r#"{{"{CUSTOM_TOOL_INPUT_FIELD}":""}}"#));
    object.insert("arguments".to_string(), json!(arguments));
    Value::Object(object)
}

fn custom_tool_call_output_to_function_call_output(item: &Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("type".to_string(), json!("function_call_output"));
    if let Some(call_id) = item.get("call_id").cloned() {
        object.insert("call_id".to_string(), call_id);
    }
    if let Some(id) = item.get("id").cloned() {
        object.insert("id".to_string(), id);
    }
    if let Some(output) = item.get("output").cloned() {
        object.insert("output".to_string(), output);
    } else if let Some(input) = item.get("input").cloned() {
        // Some clients reuse `input` for custom tool outputs.
        object.insert("output".to_string(), input);
    }
    Value::Object(object)
}

fn function_call_to_custom_tool_call(item: &Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("type".to_string(), json!("custom_tool_call"));
    if let Some(id) = item.get("id").cloned() {
        object.insert("id".to_string(), id);
    }
    if let Some(call_id) = item.get("call_id").cloned() {
        object.insert("call_id".to_string(), call_id);
    }
    if let Some(status) = item.get("status").cloned() {
        object.insert("status".to_string(), status);
    }
    if let Some(name) = item.get("name").cloned() {
        object.insert("name".to_string(), name);
    }

    let input = item
        .get("arguments")
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|value| value.get(CUSTOM_TOOL_INPUT_FIELD).cloned())
        .or_else(|| item.get("arguments").cloned())
        .unwrap_or_else(|| json!(""));
    object.insert("input".to_string(), input);
    Value::Object(object)
}

fn tool_type(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str)
}

fn responses_tool_name(tool: &Value) -> Option<String> {
    tool.get("name")
        .or_else(|| {
            tool.get("function")
                .and_then(|function| function.get("name"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn is_models_list_path(path: &str) -> bool {
    matches!(path.trim().trim_end_matches('/'), "/models" | "/v1/models")
}

fn build_models_list_payload(platform: &str, credentials: &[SelectedCredential]) -> Value {
    let created = Utc::now().timestamp();
    let capabilities = credentials
        .iter()
        .map(|credential| parse_model_capability(&credential.config_json))
        .collect::<Vec<_>>();
    let data: Vec<Value> = advertised_model_ids(platform, &capabilities)
        .into_iter()
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "created": created,
                "owned_by": "ai-switch",
            })
        })
        .collect();

    json!({
        "object": "list",
        "data": data,
    })
}

fn json_models_list_response(platform: &str, credentials: &[SelectedCredential]) -> Response {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        build_models_list_payload(platform, credentials).to_string(),
    )
        .into_response()
}

fn insert_header(headers: &mut HeaderMap, name: &'static str, value: &str) -> Result<(), String> {
    let value =
        HeaderValue::from_str(value).map_err(|err| format!("Invalid header value: {err}"))?;
    headers.insert(HeaderName::from_static(name), value);
    Ok(())
}

fn default_official_base_url(platform: PlatformId) -> Result<&'static str, String> {
    match platform {
        PlatformId::Codex => Ok("https://api.openai.com"),
        PlatformId::Claude => Ok("https://api.anthropic.com"),
        // CLIProxyAPI xAI official API base for Grok.
        PlatformId::Grok => Ok("https://api.x.ai/v1"),
        PlatformId::Gemini => Ok("https://generativelanguage.googleapis.com"),
        PlatformId::OpenCode | PlatformId::OpenClaw | PlatformId::Hermes => {
            Err("capability.unavailable: official account routing is unavailable".to_string())
        }
    }
}

fn format_app_error(error: AppError) -> String {
    let error = ApiError::from(error);
    format!("{}: {}", error.code, error.message)
}

fn append_query_param(url: &str, key: &str, value: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{key}={value}")
}

fn merge_query_parts(original: Option<&str>, bridge: Option<&str>) -> Option<String> {
    match (
        original.map(str::trim).filter(|value| !value.is_empty()),
        bridge.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(left), Some(right)) => Some(format!("{left}&{right}")),
        (Some(left), None) => Some(left.to_string()),
        (None, Some(right)) => Some(right.to_string()),
        (None, None) => None,
    }
}

pub fn build_target_url(base_url: &str, path: &str, query: Option<&str>) -> String {
    let base = collapse_duplicate_terminal_version_segments(base_url.trim().trim_end_matches('/'));
    let normalized_path = if path.is_empty() {
        "".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let upstream_path = upstream_path_for_base(&base, &normalized_path);
    let mut url = format!("{base}{upstream_path}");
    if let Some(query) = query {
        if !query.is_empty() {
            url.push('?');
            url.push_str(query);
        }
    }
    url
}

fn collapse_duplicate_terminal_version_segments(base_url: &str) -> String {
    let mut base = base_url.to_string();
    loop {
        let trimmed_len = base.trim_end_matches('/').len();
        if trimmed_len != base.len() {
            base.truncate(trimmed_len);
        }
        let Some(last_slash_index) = base.rfind('/') else {
            return base;
        };
        let last_segment = &base[last_slash_index + 1..];
        if !is_version_path_segment(last_segment) {
            return base;
        }
        let prefix = &base[..last_slash_index];
        let Some(previous_segment) = base_last_path_segment(prefix) else {
            return base;
        };
        if !previous_segment.eq_ignore_ascii_case(last_segment) {
            return base;
        }
        base.truncate(last_slash_index);
    }
}

fn upstream_path_for_base(base_url: &str, path: &str) -> String {
    let first_segment = match first_path_segment(path) {
        Some(segment) => segment,
        None => return String::new(),
    };
    let base_last_segment = base_last_path_segment(base_url);
    let should_strip_duplicate_version =
        base_last_segment.is_some_and(|segment| segment.eq_ignore_ascii_case(first_segment));
    let should_strip_codex_proxy_version =
        first_segment.eq_ignore_ascii_case("v1") && is_codex_backend_base_url(base_url);

    if should_strip_duplicate_version {
        strip_leading_matching_path_segments(path, first_segment)
    } else if should_strip_codex_proxy_version {
        strip_first_path_segment(path)
    } else {
        path.to_string()
    }
}

fn first_path_segment(path: &str) -> Option<&str> {
    path.trim_start_matches('/')
        .split('/')
        .find(|segment| !segment.is_empty())
}

fn strip_first_path_segment(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    match trimmed.split_once('/') {
        Some((_, rest)) if !rest.is_empty() => format!("/{rest}"),
        _ => String::new(),
    }
}

fn strip_leading_matching_path_segments(path: &str, segment: &str) -> String {
    let mut remaining = path.trim_start_matches('/');
    while let Some(first) = remaining.split('/').next() {
        if !first.eq_ignore_ascii_case(segment) {
            break;
        }
        remaining = remaining[first.len()..].trim_start_matches('/');
    }
    if remaining.is_empty() {
        String::new()
    } else {
        format!("/{remaining}")
    }
}

fn base_last_path_segment(base_url: &str) -> Option<&str> {
    let after_scheme = base_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url);
    let path = after_scheme.split_once('/').map(|(_, path)| path)?;
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .next_back()
}

fn is_codex_backend_base_url(base_url: &str) -> bool {
    base_url.to_ascii_lowercase().contains("/backend-api/codex")
}

fn is_hop_by_hop_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

const PRICE_MICRO_SCALE: f64 = 1_000_000.0;

pub fn extract_usage_breakdown(body: &[u8]) -> RouteUsageBreakdown {
    let value: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return RouteUsageBreakdown::default(),
    };
    let usage = value.get("usage");
    let usage_metadata = value.get("usageMetadata");

    let input_tokens = first_non_negative_i64(&[
        usage.and_then(|item| item.get("input_tokens")),
        usage.and_then(|item| item.get("prompt_tokens")),
        usage_metadata.and_then(|item| item.get("promptTokenCount")),
    ]);
    let output_tokens = first_non_negative_i64(&[
        usage.and_then(|item| item.get("output_tokens")),
        usage.and_then(|item| item.get("completion_tokens")),
        usage_metadata.and_then(|item| item.get("candidatesTokenCount")),
    ]);
    let cache_tokens = first_non_negative_i64(&[
        usage.and_then(|item| item.pointer("/input_tokens_details/cached_tokens")),
        usage.and_then(|item| item.pointer("/prompt_tokens_details/cached_tokens")),
        usage.and_then(|item| item.get("prompt_cache_hit_tokens")),
    ])
    .or_else(|| {
        sum_non_negative_i64(
            usage.and_then(|item| item.get("cache_read_input_tokens")),
            usage.and_then(|item| item.get("cache_creation_input_tokens")),
        )
    })
    .or_else(|| {
        first_non_negative_i64(&[
            usage_metadata.and_then(|item| item.get("cachedContentTokenCount"))
        ])
    });

    let (price_usd_micros, price_cny_micros, price_currency) = extract_prices(&value, usage);

    RouteUsageBreakdown {
        input_tokens,
        output_tokens,
        cache_tokens,
        price_usd_micros,
        price_cny_micros,
        price_currency,
    }
}

pub fn extract_token_count(body: &[u8]) -> Option<i64> {
    let usage = extract_usage_breakdown(body);
    let total = match (usage.input_tokens, usage.output_tokens) {
        (Some(input), Some(output)) => Some(input.saturating_add(output)),
        (Some(input), None) => Some(input),
        (None, Some(output)) => Some(output),
        (None, None) => serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|value| value.pointer("/usage/total_tokens").and_then(json_i64)),
    };

    total.filter(|value| *value > 0)
}

pub fn extract_cost_micros(body: &[u8]) -> Option<i64> {
    extract_usage_breakdown(body).price_usd_micros
}

fn first_non_negative_i64(values: &[Option<&Value>]) -> Option<i64> {
    values.iter().find_map(|value| value.and_then(json_i64))
}

fn sum_non_negative_i64(first: Option<&Value>, second: Option<&Value>) -> Option<i64> {
    match (first.and_then(json_i64), second.and_then(json_i64)) {
        (Some(first), Some(second)) => Some(first.saturating_add(second)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn json_i64(value: &Value) -> Option<i64> {
    let parsed = value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
        .or_else(|| {
            value
                .as_str()
                .and_then(|text| text.trim().parse::<i64>().ok())
        })?;
    (parsed >= 0).then_some(parsed)
}

fn extract_prices(
    value: &Value,
    usage: Option<&Value>,
) -> (Option<i64>, Option<i64>, Option<String>) {
    let explicit_currency = first_present(&[
        usage.and_then(|item| item.get("price_currency")),
        usage.and_then(|item| item.get("currency")),
        usage.and_then(|item| item.get("unit")),
        value.get("price_currency"),
        value.get("currency"),
        value.get("unit"),
    ])
    .and_then(normalize_price_currency);

    let usd_micros = first_present(&[
        usage.and_then(|item| item.get("price_usd_micros")),
        usage.and_then(|item| item.get("cost_usd_micros")),
        usage.and_then(|item| item.get("cost_micros")),
        value.get("price_usd_micros"),
        value.get("cost_usd_micros"),
        value.get("cost_micros"),
    ])
    .and_then(|item| price_micros(item, true))
    .or_else(|| {
        first_present(&[
            usage.and_then(|item| item.get("price_usd")),
            usage.and_then(|item| item.get("cost_usd")),
            value.get("price_usd"),
            value.get("cost_usd"),
        ])
        .and_then(|item| price_micros(item, false))
    });
    let cny_micros = first_present(&[
        usage.and_then(|item| item.get("price_cny_micros")),
        usage.and_then(|item| item.get("cost_cny_micros")),
        value.get("price_cny_micros"),
        value.get("cost_cny_micros"),
    ])
    .and_then(|item| price_micros(item, true))
    .or_else(|| {
        first_present(&[
            usage.and_then(|item| item.get("price_cny")),
            usage.and_then(|item| item.get("cost_cny")),
            value.get("price_cny"),
            value.get("cost_cny"),
        ])
        .and_then(|item| price_micros(item, false))
    });

    let generic_price = first_present(&[
        usage.and_then(|item| item.get("price")),
        usage.and_then(|item| item.get("cost")),
        value.get("price"),
        value.get("cost"),
    ]);
    let generic_currency = generic_price
        .and_then(price_object_currency)
        .or(explicit_currency);
    let generic_unit = generic_price.and_then(price_object_unit).or_else(|| {
        first_present(&[
            usage.and_then(|item| item.get("price_unit")),
            usage.and_then(|item| item.get("unit")),
            value.get("price_unit"),
            value.get("unit"),
        ])
        .and_then(|item| item.as_str())
    });
    let generic_micros = generic_price.and_then(|item| {
        price_micros(
            item,
            generic_unit.is_some_and(|unit| unit.contains("micro")),
        )
    });

    let mut price_usd_micros = usd_micros;
    let mut price_cny_micros = cny_micros;
    if price_usd_micros.is_none() && generic_currency == Some("usd") {
        price_usd_micros = generic_micros;
    }
    if price_cny_micros.is_none() && generic_currency == Some("cny") {
        price_cny_micros = generic_micros;
    }

    let price_currency = explicit_currency
        .or(generic_currency)
        .or_else(
            || match (price_usd_micros.is_some(), price_cny_micros.is_some()) {
                (true, false) => Some("usd"),
                (false, true) => Some("cny"),
                _ => None,
            },
        )
        .map(str::to_string);

    (price_usd_micros, price_cny_micros, price_currency)
}

fn first_present<'a>(values: &[Option<&'a Value>]) -> Option<&'a Value> {
    values.iter().copied().flatten().next()
}

fn price_micros(value: &Value, already_micros: bool) -> Option<i64> {
    let raw = value
        .as_f64()
        .or_else(|| {
            value
                .as_str()
                .and_then(|text| text.trim().parse::<f64>().ok())
        })
        .or_else(|| {
            value.as_object().and_then(|object| {
                ["amount", "value", "number"]
                    .iter()
                    .find_map(|key| object.get(*key))
                    .and_then(|item| {
                        item.as_f64().or_else(|| {
                            item.as_str()
                                .and_then(|text| text.trim().parse::<f64>().ok())
                        })
                    })
            })
        })?;
    if !raw.is_finite() || raw < 0.0 {
        return None;
    }
    let scaled = if already_micros {
        raw
    } else {
        raw * PRICE_MICRO_SCALE
    };
    if !scaled.is_finite() || scaled > i64::MAX as f64 {
        return None;
    }
    Some(scaled.round() as i64)
}

fn price_object_currency(value: &Value) -> Option<&'static str> {
    value
        .as_object()
        .and_then(|object| object.get("currency").or_else(|| object.get("unit")))
        .and_then(normalize_price_currency)
}

fn price_object_unit(value: &Value) -> Option<&str> {
    value.as_object().and_then(|object| {
        object
            .get("unit")
            .and_then(Value::as_str)
            .or_else(|| object.get("currency").and_then(Value::as_str))
    })
}

fn normalize_price_currency(value: &Value) -> Option<&'static str> {
    let text = value.as_str()?.trim().to_ascii_lowercase();
    if text.contains("usd") || text.contains("dollar") || text == "$" {
        Some("usd")
    } else if text.contains("cny") || text.contains("rmb") || text.contains("yuan") || text == "¥"
    {
        Some("cny")
    } else {
        None
    }
}

async fn insert_route_credential_request_event(
    pool: &SqlitePool,
    route_credential_id: &str,
    metadata_json: &str,
    usage: &RouteUsageBreakdown,
) -> Result<(), AppError> {
    RoutePoolRepository::insert_request_event(
        pool,
        route_credential_id,
        "route_proxy",
        metadata_json,
        usage,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[derive(Debug)]
    struct CapturedChatRequest {
        method: String,
        path: String,
        authorization: Option<String>,
        body: Value,
    }

    #[derive(Clone)]
    struct ChatUpstreamState {
        requests: tokio::sync::mpsc::UnboundedSender<CapturedChatRequest>,
    }

    async fn recording_chat_upstream_handler(
        AxumState(state): AxumState<ChatUpstreamState>,
        method: Method,
        headers: HeaderMap,
        uri: axum::http::Uri,
        body: Body,
    ) -> Response {
        let body = axum::body::to_bytes(body, 32 * 1024 * 1024)
            .await
            .expect("upstream request body");
        let value = serde_json::from_slice::<Value>(&body).expect("upstream request json");
        let streaming = value.get("stream").and_then(Value::as_bool) == Some(true);
        let _ = state.requests.send(CapturedChatRequest {
            method: method.to_string(),
            path: uri.path().to_string(),
            authorization: headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string),
            body: value,
        });

        let (content_type, response_body) = if streaming {
            (
                "text/event-stream",
                concat!(
                    "data: {\"id\":\"chatcmpl-route\",\"model\":\"deepseek-chat\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
                    "data: {\"id\":\"chatcmpl-route\",\"model\":\"deepseek-chat\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
                    "data: {\"id\":\"chatcmpl-route\",\"model\":\"deepseek-chat\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
                    "data: [DONE]\n\n"
                ),
            )
        } else {
            (
                "application/json",
                r#"{"id":"chatcmpl-route","object":"chat.completion","model":"deepseek-chat","choices":[{"index":0,"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}"#,
            )
        };

        Response::builder()
            .status(StatusCode::OK)
            .header(axum::http::header::CONTENT_TYPE, content_type)
            .body(Body::from(response_body))
            .expect("upstream response")
    }

    async fn start_recording_chat_upstream() -> (
        String,
        tokio::sync::mpsc::UnboundedReceiver<CapturedChatRequest>,
    ) {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let app = Router::new()
            .fallback(recording_chat_upstream_handler)
            .with_state(ChatUpstreamState { requests: sender });
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind recording upstream");
        let address = listener.local_addr().expect("recording upstream address");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve recording upstream");
        });
        (format!("http://{address}/v1"), receiver)
    }

    async fn start_fixed_upstream(status: StatusCode, body: &'static str) -> String {
        let app = Router::new().fallback(move || async move { (status, body) });
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind upstream");
        let address = listener.local_addr().expect("upstream address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve upstream");
        });
        format!("http://{address}/v1")
    }

    async fn create_proxy_api_credential(pool: &SqlitePool, name: &str, base_url: &str) -> String {
        let credential = RouteCredentialRepository::create(
            pool,
            "codex",
            "api",
            name,
            None,
            "ok",
            None,
            r#"{"api_key":"sk-upstream"}"#,
            &json!({
                "base_url": base_url,
                "interface_format": "openai",
                "model_mappings": []
            })
            .to_string(),
            "{}",
        )
        .await
        .expect("create credential");
        credential.id
    }

    #[tokio::test]
    async fn route_proxy_listener_falls_forward_when_start_port_is_unavailable() {
        let occupied = TcpListener::bind((BIND_HOST, 0))
            .await
            .expect("bind occupied port");
        let occupied_port = occupied.local_addr().expect("occupied address").port();
        if occupied_port == u16::MAX {
            return;
        }

        let listener = bind_route_proxy_listener_from(occupied_port)
            .await
            .expect("fallback listener");
        let selected_port = listener.local_addr().expect("selected address").port();

        assert!(selected_port > occupied_port);
    }

    #[tokio::test]
    async fn start_uses_next_port_when_default_route_proxy_port_is_unavailable() {
        let default_port_guard = TcpListener::bind((BIND_HOST, DEFAULT_ROUTE_PROXY_PORT)).await;
        if default_port_guard.is_err() {
            return;
        }

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = RouteProxyRuntimeState::default();

        let status = RouteProxyService::start(&runtime, pool, RouteProxyTransport::Http)
            .await
            .expect("start proxy");
        let port = status.port.expect("port");

        assert!(port > DEFAULT_ROUTE_PROXY_PORT);
        assert_eq!(
            status.base_url.as_deref(),
            Some(format!("http://{BIND_HOST}:{port}").as_str())
        );

        RouteProxyService::stop(&runtime).await.expect("stop");
    }

    #[tokio::test]
    async fn https_transport_serves_the_existing_route_proxy_handler_and_rejects_plain_http() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};
        use crate::paths::AppPaths;
        use crate::services::route_proxy_https_service::RouteProxyHttpsService;

        let temp = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
        let material = RouteProxyHttpsService::ensure_material(&paths)
            .await
            .expect("material");
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let key = RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-test")
            .await
            .expect("proxy key");
        let runtime = RouteProxyRuntimeState::default();

        let status = RouteProxyService::start(
            &runtime,
            pool,
            RouteProxyTransport::Https {
                certificate_pem_path: material.server_certificate_pem.clone(),
                private_key_pem_path: material.server_private_key_pem.clone(),
            },
        )
        .await
        .expect("start tls");
        let root = reqwest::Certificate::from_pem(
            &tokio::fs::read(&material.root_certificate_pem)
                .await
                .expect("root pem"),
        )
        .expect("root certificate");
        let client = reqwest::Client::builder()
            .add_root_certificate(root)
            .build()
            .expect("client");
        let tls_response = client
            .get(format!(
                "{}/v1/models",
                status.base_url.as_deref().expect("base url")
            ))
            .bearer_auth(key)
            .send()
            .await
            .expect("tls request");

        assert_eq!(
            status
                .base_url
                .as_deref()
                .map(|value| value.starts_with("https://")),
            Some(true)
        );
        assert_eq!(tls_response.status(), reqwest::StatusCode::OK);
        let plain_error = reqwest::get(format!(
            "http://127.0.0.1:{}/v1/models",
            status.port.expect("port")
        ))
        .await
        .expect_err("plain HTTP must not be served by the TLS listener");
        assert!(plain_error.is_request() || plain_error.is_connect() || plain_error.is_decode());

        RouteProxyService::stop(&runtime).await.expect("stop");
    }

    #[tokio::test]
    async fn http_transport_retains_the_existing_http_base_url() {
        use crate::database::{create_memory_pool, run_migrations};

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = RouteProxyRuntimeState::default();

        let status = RouteProxyService::start(&runtime, pool, RouteProxyTransport::Http)
            .await
            .expect("start http");

        assert_eq!(
            status
                .base_url
                .as_deref()
                .map(|value| value.starts_with("http://")),
            Some(true)
        );
        RouteProxyService::stop(&runtime).await.expect("stop");
    }

    #[tokio::test]
    async fn route_proxy_answers_cors_preflight_without_platform_authentication() {
        use crate::database::{create_memory_pool, run_migrations};

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = RouteProxyRuntimeState::default();
        let status = RouteProxyService::start(&runtime, pool, RouteProxyTransport::Http)
            .await
            .expect("start proxy");

        let response = reqwest::Client::new()
            .request(
                reqwest::Method::OPTIONS,
                format!(
                    "{}/v1/responses",
                    status.base_url.as_deref().expect("base url")
                ),
            )
            .header("Origin", "https://fastview.lingyun.net")
            .header("Access-Control-Request-Method", "POST")
            .header(
                "Access-Control-Request-Headers",
                "authorization,content-type,x-ai-switch-platform",
            )
            .header("Access-Control-Request-Private-Network", "true")
            .send()
            .await
            .expect("preflight response");

        assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .and_then(|value| value.to_str().ok()),
            Some("https://fastview.lingyun.net")
        );
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-private-network")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn route_proxy_bridges_codex_responses_to_chat_json_and_sse() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;

        let (upstream_url, mut requests) = start_recording_chat_upstream().await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential(&pool, "chat-bridge", &upstream_url).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
            .await
            .expect("pool members");
        let route_key = RouteProxyKeyRepository::ensure_platform_key(
            &pool,
            "codex",
            "sk-ai-switch-test-bridge",
        )
        .await
        .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start(&runtime, pool, RouteProxyTransport::Http)
            .await
            .expect("start proxy");
        let client = reqwest::Client::new();
        let endpoint = format!(
            "{}/v1/responses",
            proxy.base_url.as_deref().expect("base url")
        );

        let json_response = client
            .post(&endpoint)
            .bearer_auth(&route_key)
            .json(&json!({"model":"gpt-5","input":"hello"}))
            .send()
            .await
            .expect("json proxy response");
        assert_eq!(json_response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            json_response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.split(';').next().unwrap_or(value)),
            Some("application/json")
        );
        let json_body: Value = json_response.json().await.expect("responses json");
        assert_eq!(json_body["object"], "response");
        assert_eq!(json_body["output_text"], "hello");
        assert_eq!(json_body["usage"]["input_tokens"], 3);
        assert_eq!(json_body["usage"]["output_tokens"], 1);

        let captured_json = requests.recv().await.expect("captured json request");
        assert_eq!(captured_json.method, "POST");
        assert_eq!(captured_json.path, "/v1/chat/completions");
        assert_eq!(
            captured_json.authorization.as_deref(),
            Some("Bearer sk-upstream")
        );
        assert_eq!(
            captured_json.body["messages"][0],
            json!({"role":"user","content":"hello"})
        );
        assert!(captured_json.body.get("input").is_none());

        let sse_response = client
            .post(&endpoint)
            .bearer_auth(&route_key)
            .json(&json!({"model":"gpt-5","input":"hello","stream":true}))
            .send()
            .await
            .expect("sse proxy response");
        assert_eq!(sse_response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            sse_response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.split(';').next().unwrap_or(value)),
            Some("text/event-stream")
        );
        let sse_body = sse_response.text().await.expect("responses sse");
        assert!(sse_body.contains("event: response.created"));
        assert!(sse_body.contains("response.output_text.delta"));
        assert!(sse_body.contains("\"delta\":\"hello\""));
        assert!(sse_body.contains("event: response.completed"));

        let captured_sse = requests.recv().await.expect("captured sse request");
        assert_eq!(captured_sse.path, "/v1/chat/completions");
        assert_eq!(captured_sse.body["stream"], true);
        assert!(captured_sse.body["stream_options"]["include_usage"] == true);

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn proxy_retries_next_pool_account_after_unauthorized_response() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let failed_upstream = start_fixed_upstream(StatusCode::UNAUTHORIZED, "expired").await;
        let healthy_upstream = start_fixed_upstream(
            StatusCode::OK,
            r#"{"usage":{"prompt_tokens":120,"completion_tokens":30,"prompt_cache_hit_tokens":80,"price_cny":7.1}}"#,
        )
        .await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let failed_id = create_proxy_api_credential(&pool, "failed", &failed_upstream).await;
        let healthy_id = create_proxy_api_credential(&pool, "healthy", &healthy_upstream).await;
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[failed_id.clone(), healthy_id.clone()],
        )
        .await
        .expect("pool members");
        let route_key =
            RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-test")
                .await
                .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start(&runtime, pool.clone(), RouteProxyTransport::Http)
            .await
            .expect("start proxy");

        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/chat/completions",
                proxy.base_url.as_deref().expect("base url")
            ))
            .bearer_auth(route_key)
            .json(&json!({"model":"gpt-5","messages":[]}))
            .send()
            .await
            .expect("proxy response");

        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            response.text().await.expect("body"),
            r#"{"usage":{"prompt_tokens":120,"completion_tokens":30,"prompt_cache_hit_tokens":80,"price_cny":7.1}}"#
        );
        let failed = RouteCredentialRepository::get(&pool, &failed_id)
            .await
            .expect("failed account");
        assert_eq!(failed.status, "ok");
        assert_eq!(failed.transient_failure_count, 1);
        assert!(failed.next_retry_at.is_some());
        assert_eq!(
            RouteCredentialRepository::get(&pool, &healthy_id)
                .await
                .expect("healthy account")
                .status,
            "ok"
        );
        assert_eq!(
            RoutePoolRepository::next_cursor_index(&pool, "codex")
                .await
                .expect("next cursor"),
            0
        );
        let usage_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM usage_events WHERE source_label = 'route_proxy'",
        )
        .fetch_one(&pool)
        .await
        .expect("usage count");
        assert_eq!(usage_count, 2);
        let stats = RoutePoolRepository::stats(&pool, "codex", None, 1, 20)
            .await
            .expect("usage stats");
        assert_eq!(stats.requests.len(), 2);
        let healthy_request = stats
            .requests
            .iter()
            .find(|request| request.account_id.as_deref() == Some(healthy_id.as_str()))
            .expect("healthy request row");
        assert_eq!(healthy_request.input_tokens, Some(120));
        assert_eq!(healthy_request.output_tokens, Some(30));
        assert_eq!(healthy_request.cache_tokens, Some(80));
        assert_eq!(healthy_request.price_cny_micros, Some(7_100_000));
        assert_eq!(healthy_request.price_currency.as_deref(), Some("cny"));

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn pool_selection_probes_cooling_account_until_cleared() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id =
            create_proxy_api_credential(&pool, "cooling", "http://127.0.0.1:1/v1").await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
            .await
            .expect("pool members");

        for _ in 0..3 {
            RouteCredentialRepository::record_transient_failure(
                &pool,
                &credential_id,
                "transport",
                "temporary",
                None,
            )
            .await
            .expect("record failure");
        }
        assert_eq!(
            select_pool_credentials(&pool, "codex")
                .await
                .expect("cooldown pool")
                .into_iter()
                .map(|credential| credential.id)
                .collect::<Vec<_>>(),
            vec![credential_id.clone()]
        );

        RouteCredentialRepository::clear_transient_failure(&pool, &credential_id)
            .await
            .expect("clear retry state");
        assert_eq!(
            select_pool_credentials(&pool, "codex")
                .await
                .expect("eligible pool")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn pool_selection_excludes_non_ok_members_until_they_recover() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let recovering_id =
            create_proxy_api_credential(&pool, "recovering", "http://127.0.0.1:1/v1").await;
        let healthy_id =
            create_proxy_api_credential(&pool, "healthy", "http://127.0.0.1:1/v1").await;
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[recovering_id.clone(), healthy_id.clone()],
        )
        .await
        .expect("pool members");
        RouteCredentialRepository::update_status(&pool, &recovering_id, "error")
            .await
            .expect("error status");

        let selected = select_pool_credentials(&pool, "codex")
            .await
            .expect("healthy selection");
        assert_eq!(
            selected.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![healthy_id.clone()]
        );

        RouteCredentialRepository::update_status(&pool, &recovering_id, "ok")
            .await
            .expect("recovered status");
        let selected = select_pool_credentials(&pool, "codex")
            .await
            .expect("recovered selection");
        assert_eq!(
            selected.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![recovering_id, healthy_id]
        );
    }

    #[tokio::test]
    async fn pool_selection_uses_earliest_cooling_account_and_pool_order_tie_breaker() {
        use crate::database::{create_memory_pool, run_migrations};

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let first_id = create_proxy_api_credential(&pool, "first", "http://127.0.0.1:1/v1").await;
        let second_id = create_proxy_api_credential(&pool, "second", "http://127.0.0.1:1/v1").await;
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[first_id.clone(), second_id.clone()],
        )
        .await
        .expect("pool members");
        sqlx::query(
            "UPDATE route_credentials SET next_retry_at = ?, cooldown_until = ? WHERE id = ?",
        )
        .bind("2999-01-01T00:00:00Z")
        .bind("2999-01-01T00:00:00Z")
        .bind(&first_id)
        .execute(&pool)
        .await
        .expect("first cooldown");
        sqlx::query(
            "UPDATE route_credentials SET next_retry_at = ?, cooldown_until = ? WHERE id = ?",
        )
        .bind("2999-01-01T00:00:00Z")
        .bind("2999-01-01T00:00:00Z")
        .bind(&second_id)
        .execute(&pool)
        .await
        .expect("second cooldown");

        let selected = select_pool_credentials(&pool, "codex")
            .await
            .expect("selection");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, first_id);
    }

    #[tokio::test]
    async fn pool_selection_prefers_eligible_accounts_over_cooling_fallback() {
        use crate::database::{create_memory_pool, run_migrations};

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let cooling_id =
            create_proxy_api_credential(&pool, "cooling", "http://127.0.0.1:1/v1").await;
        let ready_id = create_proxy_api_credential(&pool, "ready", "http://127.0.0.1:1/v1").await;
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[cooling_id.clone(), ready_id.clone()],
        )
        .await
        .expect("pool members");
        sqlx::query(
            "UPDATE route_credentials SET next_retry_at = ?, cooldown_until = ? WHERE id = ?",
        )
        .bind("2999-01-01T00:00:00Z")
        .bind("2999-01-01T00:00:00Z")
        .bind(&cooling_id)
        .execute(&pool)
        .await
        .expect("cooldown");

        let selected = select_pool_credentials(&pool, "codex")
            .await
            .expect("selection");
        assert_eq!(
            selected.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![ready_id]
        );
    }

    fn api_credential(name: &str, interface_format: &str) -> SelectedCredential {
        SelectedCredential {
            id: name.to_string(),
            platform: "codex".to_string(),
            kind: "api".to_string(),
            display_name: name.to_string(),
            status: "ok".to_string(),
            secret_payload_json: r#"{"api_key":"sk-test"}"#.to_string(),
            config_json: serde_json::json!({
                "base_url": "https://api.example.com/v1",
                "interface_format": interface_format,
                "model_mappings": [{"from":"gpt-5","to":"up-gpt"}]
            })
            .to_string(),
        }
    }

    fn api_credential_with_config(name: &str, config_json: &str) -> SelectedCredential {
        let mut credential = api_credential(name, "openai");
        credential.config_json = config_json.to_string();
        credential
    }

    #[test]
    fn partial_platform_api_requires_explicit_dialect() {
        let credential = SelectedCredential {
            id: "hermes-api".to_string(),
            platform: "hermes".to_string(),
            kind: "api".to_string(),
            display_name: "Hermes API".to_string(),
            status: "ok".to_string(),
            secret_payload_json: r#"{"api_key":"sk-test"}"#.to_string(),
            config_json: json!({
                "base_url": "https://api.example.com/v1",
                "model_mappings": []
            })
            .to_string(),
        };

        let error = build_upstream_request(
            &credential,
            "hermes",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5"}"#,
        )
        .expect_err("partial platforms require an explicit API dialect");

        assert!(error.contains("validation.api_dialect_required"));
    }

    #[test]
    fn gemini_bridge_query_merges_with_original_query_and_key() {
        let credential = api_credential("gemini-upstream", "gemini");
        let (url, _, _) = build_upstream_request(
            &credential,
            "codex",
            "/v1/responses",
            Some("trace=1"),
            HeaderMap::new(),
            br#"{"model":"gemini-2.5-flash","stream":true,"input":"hello"}"#,
        )
        .unwrap();

        assert!(url.contains("/v1beta/models/gemini-2.5-flash:streamGenerateContent?"));
        assert!(url.contains("trace=1"));
        assert!(url.contains("alt=sse"));
        assert!(url.contains("key="));
    }

    #[test]
    fn partial_platform_official_routing_is_unavailable() {
        let credential = SelectedCredential {
            id: "hermes-official".to_string(),
            platform: "hermes".to_string(),
            kind: "official".to_string(),
            display_name: "Hermes Official".to_string(),
            status: "ok".to_string(),
            secret_payload_json: r#"{"access_token":"at"}"#.to_string(),
            config_json: "{}".to_string(),
        };

        let error = build_upstream_request(
            &credential,
            "hermes",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5"}"#,
        )
        .expect_err("partial platforms do not support official routing");

        assert!(error.contains("capability.unavailable"));
    }

    #[test]
    fn build_target_url_joins_base_path_and_query() {
        assert_eq!(
            build_target_url(
                "https://api.example.com/v1/",
                "/chat/completions",
                Some("beta=1")
            ),
            "https://api.example.com/v1/chat/completions?beta=1"
        );
    }

    #[test]
    fn build_target_url_avoids_duplicate_local_v1_prefix() {
        assert_eq!(
            build_target_url("https://api.example.com/v1", "/v1/responses", None),
            "https://api.example.com/v1/responses"
        );
        assert_eq!(
            build_target_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "/v1beta/models/gemini:generateContent",
                None
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini:generateContent"
        );
    }

    #[test]
    fn build_target_url_collapses_repeated_base_version_segments() {
        assert_eq!(
            build_target_url("https://vsllm.com/v1/v1/", "/v1/chat/completions", None),
            "https://vsllm.com/v1/chat/completions"
        );
        assert_eq!(
            build_target_url("https://vsllm.com/v1", "/v1/v1/chat/completions", None),
            "https://vsllm.com/v1/chat/completions"
        );
        assert_eq!(
            build_target_url(
                "https://generativelanguage.googleapis.com/v1beta/v1beta",
                "/v1beta/models/gemini:generateContent",
                None
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini:generateContent"
        );
    }

    #[test]
    fn build_target_url_keeps_v1_prefix_when_upstream_base_is_unversioned() {
        assert_eq!(
            build_target_url(
                "https://api.example.com",
                "/v1/responses",
                Some("stream=true")
            ),
            "https://api.example.com/v1/responses?stream=true"
        );
    }

    #[test]
    fn pick_credential_selects_by_cursor_round_robin() {
        let credentials = vec![
            api_credential("first", "openai"),
            api_credential("second", "openai"),
        ];
        assert_eq!(
            pick_credential(&credentials, 0).map(|item| item.id.as_str()),
            Some("first")
        );
        assert_eq!(
            pick_credential(&credentials, 3).map(|item| item.id.as_str()),
            Some("second")
        );
    }

    #[test]
    fn retry_credential_indexes_wrap_once_from_the_route_cursor() {
        assert_eq!(retry_credential_indexes(3, 0), vec![0, 1, 2]);
        assert_eq!(retry_credential_indexes(3, 2), vec![2, 0, 1]);
        assert_eq!(retry_credential_indexes(3, -1), vec![2, 0, 1]);
        assert!(retry_credential_indexes(0, 0).is_empty());
    }

    #[test]
    fn retry_policy_only_retries_credentials_that_are_known_unusable() {
        assert!(should_retry_proxy_failure(StatusCode::UNAUTHORIZED));
        assert!(should_retry_proxy_failure(StatusCode::FORBIDDEN));
        assert!(should_retry_proxy_failure(StatusCode::BAD_GATEWAY));
        assert!(should_retry_proxy_failure(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!should_retry_proxy_failure(StatusCode::TOO_MANY_REQUESTS));
        assert!(!should_retry_proxy_failure(
            StatusCode::INTERNAL_SERVER_ERROR
        ));
    }

    #[test]
    fn proxy_failure_classification_separates_transient_and_permanent_errors() {
        assert_eq!(
            classify_proxy_failure(Some(StatusCode::BAD_GATEWAY), None),
            ProxyFailureKind::Transient
        );
        assert_eq!(
            classify_proxy_failure(None, Some("connection reset by peer")),
            ProxyFailureKind::Transient
        );
        assert_eq!(
            classify_proxy_failure(
                Some(StatusCode::UNAUTHORIZED),
                Some("invalid_grant: refresh token has been revoked")
            ),
            ProxyFailureKind::Permanent
        );
        assert_eq!(
            classify_proxy_failure(Some(StatusCode::BAD_REQUEST), Some("invalid model")),
            ProxyFailureKind::None
        );
    }

    #[test]
    fn retry_eligibility_accepts_missing_or_invalid_timestamps() {
        let now = Utc::now();
        assert!(credential_is_retryable_now(None, None, now));
        assert!(!credential_is_retryable_now(
            Some("2999-01-01T00:00:00Z"),
            None,
            now
        ));
        assert!(credential_is_retryable_now(
            Some("not-a-timestamp"),
            None,
            now
        ));
        assert!(credential_is_retryable_now(
            Some("2000-01-01T00:00:00Z"),
            Some("2000-01-01T00:00:00Z"),
            now
        ));
    }

    #[test]
    fn apply_model_mappings_rewrites_nested_model_fields() {
        let body = br#"{"model":"gpt-5","nested":{"model":"gpt-5"},"keep":"same"}"#;
        let mapped = apply_model_mappings(
            body,
            &[ModelMapping {
                from: "gpt-5".to_string(),
                to: "up-gpt".to_string(),
                label: None,
                supports_1m: None,
            }],
        );
        let value: Value = serde_json::from_slice(&mapped).expect("json");

        assert_eq!(
            value.pointer("/model").and_then(Value::as_str),
            Some("up-gpt")
        );
        assert_eq!(
            value.pointer("/nested/model").and_then(Value::as_str),
            Some("up-gpt")
        );
    }

    #[test]
    fn apply_model_mappings_strips_claude_one_m_suffix_for_lookup() {
        let mapped = apply_model_mappings(
            br#"{"model":"claude-sonnet-5 [1M]","nested":{"model":"claude-opus-4-8[1m]"}}"#,
            &[
                ModelMapping {
                    from: "claude-sonnet-5".to_string(),
                    to: "provider-sonnet".to_string(),
                    label: Some("Sonnet".to_string()),
                    supports_1m: Some(true),
                },
                ModelMapping {
                    from: "claude-opus-4-8".to_string(),
                    to: "provider-opus".to_string(),
                    label: Some("Opus".to_string()),
                    supports_1m: Some(true),
                },
            ],
        );
        let value: Value = serde_json::from_slice(&mapped).expect("json");

        assert_eq!(
            value.pointer("/model").and_then(Value::as_str),
            Some("provider-sonnet")
        );
        assert_eq!(
            value.pointer("/nested/model").and_then(Value::as_str),
            Some("provider-opus")
        );
    }

    #[test]
    fn apply_model_mappings_does_not_strip_one_m_suffix_from_non_claude_models() {
        let mapped = apply_model_mappings(
            br#"{"model":"gpt-5[1M]"}"#,
            &[ModelMapping {
                from: "gpt-5".to_string(),
                to: "up-gpt".to_string(),
                label: None,
                supports_1m: None,
            }],
        );
        let value: Value = serde_json::from_slice(&mapped).expect("json");

        assert_eq!(
            value.pointer("/model").and_then(Value::as_str),
            Some("gpt-5[1M]")
        );
    }

    #[test]
    fn build_upstream_request_ignores_placeholder_model_mapping() {
        let mut credential = api_credential("placeholder", "openai");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.example.com/v1",
            "interface_format": "openai",
            "model_mappings": [{"from":"gpt-5","to":"upstream-model"}]
        })
        .to_string();

        let (_, _, body) = build_upstream_request(
            &credential,
            "codex",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5"}"#,
        )
        .expect("openai request");
        let value: Value = serde_json::from_slice(&body).expect("json");

        assert_eq!(
            value.pointer("/model").and_then(Value::as_str),
            Some("gpt-5")
        );
    }

    #[test]
    fn build_upstream_request_sets_auth_by_interface_format() {
        let openai = api_credential("openai", "openai");
        let (_, headers, body) = build_upstream_request(
            &openai,
            "codex",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5"}"#,
        )
        .expect("openai request");
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer sk-test")
        );
        assert!(String::from_utf8(body).expect("body").contains("up-gpt"));

        let anthropic = api_credential("anthropic", "anthropic");
        let (_, headers, _) = build_upstream_request(
            &anthropic,
            "claude",
            "/v1/messages",
            None,
            HeaderMap::new(),
            br#"{}"#,
        )
        .expect("anthropic request");
        assert_eq!(
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("sk-test")
        );
        assert!(headers.get("authorization").is_none());

        let mut anthropic_bearer = api_credential("anthropic-bearer", "anthropic");
        anthropic_bearer.config_json = serde_json::json!({
            "base_url": "https://api.example.com/v1",
            "interface_format": "anthropic",
            "api_key_field": "ANTHROPIC_AUTH_TOKEN",
            "model_mappings": []
        })
        .to_string();
        let (_, headers, _) = build_upstream_request(
            &anthropic_bearer,
            "claude",
            "/v1/messages",
            None,
            HeaderMap::new(),
            br#"{}"#,
        )
        .expect("anthropic bearer request");
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer sk-test")
        );
        assert!(headers.get("x-api-key").is_none());

        let gemini = api_credential("gemini", "gemini");
        let (url, _, _) = build_upstream_request(
            &gemini,
            "gemini",
            "/v1beta/models/gemini:generateContent",
            None,
            HeaderMap::new(),
            br#"{}"#,
        )
        .expect("gemini request");
        assert!(url.contains("key=sk-test"));
    }

    #[test]
    fn build_upstream_request_appends_chat_completions_to_base_url() {
        let mut credential = api_credential("root-openai", "openai");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.example.com",
            "interface_format": "openai",
            "model_mappings": []
        })
        .to_string();

        let (url, _, _) = build_upstream_request(
            &credential,
            "codex",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5.5"}"#,
        )
        .expect("request");

        assert_eq!(url, "https://api.example.com/chat/completions");
    }

    #[test]
    fn build_upstream_request_appends_responses_to_base_url() {
        let mut credential = api_credential("root-responses", "openai-responses");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.example.com",
            "interface_format": "openai-responses",
            "model_mappings": []
        })
        .to_string();

        let (url, _, _) = build_upstream_request(
            &credential,
            "codex",
            "/responses",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5.5"}"#,
        )
        .expect("request");

        assert_eq!(url, "https://api.example.com/responses");
    }

    #[test]
    fn build_upstream_request_bridges_codex_responses_to_chat_upstream() {
        let credential = api_credential("chat-upstream", "openai");
        let (url, _, body) = build_upstream_request(
            &credential,
            "codex",
            "/v1/responses",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5","input":"hello","max_output_tokens":32}"#,
        )
        .expect("bridged request");
        let body: Value = serde_json::from_slice(&body).expect("chat json");

        assert_eq!(url, "https://api.example.com/v1/chat/completions");
        assert_eq!(body["model"], "up-gpt");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["max_tokens"], 32);
        assert!(body.get("input").is_none());
    }

    #[test]
    fn build_upstream_request_keeps_codex_responses_for_responses_upstream() {
        let credential = api_credential("responses-upstream", "openai-responses");
        let (url, _, body) = build_upstream_request(
            &credential,
            "codex",
            "/v1/responses",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5","input":"hello"}"#,
        )
        .expect("responses request");
        let body: Value = serde_json::from_slice(&body).expect("responses json");

        assert_eq!(url, "https://api.example.com/v1/responses");
        assert_eq!(body["model"], "up-gpt");
        assert_eq!(body["input"], "hello");
        assert!(body.get("messages").is_none());
    }

    #[test]
    fn build_upstream_request_bridges_custom_responses_tools_to_chat_functions() {
        let credential = api_credential("chat-custom-tool", "openai");
        let body = br#"{
            "model":"gpt-5",
            "input":[{
                "type":"custom_tool_call",
                "call_id":"call_1",
                "name":"apply_patch",
                "input":"*** Begin Patch"
            }],
            "tools":[{
                "type":"custom",
                "name":"apply_patch",
                "description":"Apply a patch"
            }]
        }"#;
        let (_, _, rewritten) = build_upstream_request(
            &credential,
            "codex",
            "/v1/responses",
            None,
            HeaderMap::new(),
            body,
        )
        .expect("bridged custom request");
        let value: Value = serde_json::from_slice(&rewritten).expect("chat json");

        assert_eq!(value.pointer("/tools/0/type"), Some(&json!("function")));
        assert_eq!(
            value.pointer("/tools/0/function/name"),
            Some(&json!("apply_patch"))
        );
        assert_eq!(
            value.pointer("/messages/0/tool_calls/0/function/name"),
            Some(&json!("apply_patch"))
        );
        assert_eq!(
            value.pointer("/messages/0/tool_calls/0/function/arguments"),
            Some(&json!(r#"{"input":"*** Begin Patch"}"#))
        );
    }

    #[test]
    fn build_upstream_request_appends_endpoint_to_openai_base_url_path() {
        let mut credential = api_credential("path-responses", "openai-responses");
        credential.config_json = serde_json::json!({
            "base_url": "https://new.sharedchat.cc/codex",
            "interface_format": "openai-responses",
            "model_mappings": []
        })
        .to_string();

        let (url, _, _) = build_upstream_request(
            &credential,
            "codex",
            "/v1/responses",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5.5"}"#,
        )
        .expect("request");

        assert_eq!(url, "https://new.sharedchat.cc/codex/responses");
    }

    #[test]
    fn extract_token_count_supports_openai_and_anthropic_shapes() {
        let openai = br#"{"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
        let anthropic = br#"{"usage":{"input_tokens":11,"output_tokens":7}}"#;
        assert_eq!(extract_token_count(openai), Some(15));
        assert_eq!(extract_token_count(anthropic), Some(18));
    }

    #[test]
    fn extract_usage_breakdown_supports_deepseek_usage_and_cny_price() {
        let body = br#"{
            "usage": {
                "prompt_tokens": 120,
                "completion_tokens": 30,
                "prompt_cache_hit_tokens": 80,
                "price_cny": 7.1
            }
        }"#;

        let usage = extract_usage_breakdown(body);

        assert_eq!(usage.input_tokens, Some(120));
        assert_eq!(usage.output_tokens, Some(30));
        assert_eq!(usage.cache_tokens, Some(80));
        assert_eq!(usage.price_cny_micros, Some(7_100_000));
        assert_eq!(usage.price_currency.as_deref(), Some("cny"));
    }

    #[test]
    fn extract_usage_breakdown_supports_openai_responses_and_usd_price() {
        let body = br#"{
            "usage": {
                "input_tokens": 100,
                "output_tokens": 25,
                "input_tokens_details": {"cached_tokens": 60},
                "cost_usd": 0.0042
            }
        }"#;

        let usage = extract_usage_breakdown(body);

        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.output_tokens, Some(25));
        assert_eq!(usage.cache_tokens, Some(60));
        assert_eq!(usage.price_usd_micros, Some(4_200));
        assert_eq!(usage.price_currency.as_deref(), Some("usd"));
    }

    #[test]
    fn extract_usage_breakdown_supports_generic_object_price() {
        let usage = extract_usage_breakdown(
            br#"{"usage":{"input_tokens":1,"output_tokens":2,"price":{"amount":"0.12","currency":"CNY"}}}"#,
        );

        assert_eq!(usage.price_cny_micros, Some(120_000));
        assert_eq!(usage.price_currency.as_deref(), Some("cny"));
    }

    #[test]
    fn extract_usage_breakdown_supports_anthropic_and_gemini_cache_shapes() {
        let anthropic = extract_usage_breakdown(
            br#"{"usage":{"input_tokens":11,"output_tokens":7,"cache_read_input_tokens":5,"cache_creation_input_tokens":2}}"#,
        );
        let gemini = extract_usage_breakdown(
            br#"{"usageMetadata":{"promptTokenCount":13,"candidatesTokenCount":9,"cachedContentTokenCount":4}}"#,
        );

        assert_eq!(anthropic.input_tokens, Some(11));
        assert_eq!(anthropic.output_tokens, Some(7));
        assert_eq!(anthropic.cache_tokens, Some(7));
        assert_eq!(gemini.input_tokens, Some(13));
        assert_eq!(gemini.output_tokens, Some(9));
        assert_eq!(gemini.cache_tokens, Some(4));
    }

    #[test]
    fn extract_usage_breakdown_leaves_price_empty_when_upstream_omits_price() {
        let usage =
            extract_usage_breakdown(br#"{"usage":{"prompt_tokens":10,"completion_tokens":2}}"#);

        assert_eq!(usage.price_usd_micros, None);
        assert_eq!(usage.price_cny_micros, None);
        assert_eq!(usage.price_currency, None);
    }

    #[test]
    fn extract_inbound_api_key_from_bearer_x_api_key_and_query() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-ai-switch-bearer"),
        );
        assert_eq!(
            extract_inbound_api_key(&headers, None).as_deref(),
            Some("sk-ai-switch-bearer")
        );

        let mut key_headers = HeaderMap::new();
        key_headers.insert("x-api-key", HeaderValue::from_static("sk-ai-switch-xkey"));
        assert_eq!(
            extract_inbound_api_key(&key_headers, None).as_deref(),
            Some("sk-ai-switch-xkey")
        );

        assert_eq!(
            extract_inbound_api_key(&HeaderMap::new(), Some("key=sk-ai-switch-query&x=1"))
                .as_deref(),
            Some("sk-ai-switch-query")
        );
    }

    #[test]
    fn strip_route_proxy_auth_query_keeps_unrelated_parameters() {
        assert_eq!(
            strip_route_proxy_auth_query(Some("key=sk-ai-switch-local&alt=sse&api_key=ignored")),
            Some("alt=sse".to_string())
        );
        assert_eq!(strip_route_proxy_auth_query(Some("apiKey=local")), None);
        assert_eq!(
            strip_route_proxy_auth_query(Some("alt=sse")),
            Some("alt=sse".to_string())
        );
    }

    #[test]
    fn strip_route_proxy_auth_headers_removes_only_local_credential_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-ai-switch-local"),
        );
        headers.insert("x-api-key", HeaderValue::from_static("sk-ai-switch-local"));
        headers.insert(
            "x-goog-api-key",
            HeaderValue::from_static("sk-ai-switch-local"),
        );
        headers.insert("accept", HeaderValue::from_static("application/json"));

        strip_route_proxy_auth_headers(&mut headers);

        assert!(headers.get("authorization").is_none());
        assert!(headers.get("x-api-key").is_none());
        assert!(headers.get("x-goog-api-key").is_none());
        assert_eq!(
            headers.get("accept"),
            Some(&HeaderValue::from_static("application/json"))
        );
    }

    #[test]
    fn cors_preflight_echoes_origin_requested_headers_and_private_network_access() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "origin",
            HeaderValue::from_static("https://fastview.lingyun.net"),
        );
        headers.insert(
            "access-control-request-method",
            HeaderValue::from_static("POST"),
        );
        headers.insert(
            "access-control-request-headers",
            HeaderValue::from_static("authorization,content-type,x-ai-switch-platform"),
        );
        headers.insert(
            "access-control-request-private-network",
            HeaderValue::from_static("true"),
        );

        let response = cors_preflight_response(&headers);

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .and_then(|value| value.to_str().ok()),
            Some("https://fastview.lingyun.net")
        );
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-methods")
                .and_then(|value| value.to_str().ok()),
            Some("GET, POST, PUT, PATCH, DELETE, OPTIONS")
        );
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-headers")
                .and_then(|value| value.to_str().ok()),
            Some("authorization,content-type,x-ai-switch-platform")
        );
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-private-network")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );

        let mut actual = StatusCode::OK.into_response();
        add_cors_headers(&mut actual, cors_request_origin(&headers).as_ref());
        assert_eq!(
            actual
                .headers()
                .get("access-control-allow-origin")
                .and_then(|value| value.to_str().ok()),
            Some("https://fastview.lingyun.net")
        );
    }

    #[tokio::test]
    async fn resolve_platform_preserves_hermes_proxy_key_identity() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "hermes", "sk-ai-switch-hermes")
            .await
            .expect("store key");

        let state = ProxyAppState {
            pool,
            key_cache: Arc::new(Mutex::new(RouteProxyKeyCache::default())),
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-ai-switch-hermes"),
        );
        let platform = resolve_platform(&state, &headers, Some("sk-ai-switch-hermes"))
            .await
            .expect("resolve");
        assert_eq!(platform, PlatformId::Hermes);

        // Second lookup should hit the in-memory cache (still within 30s TTL).
        let cached = resolve_platform(&state, &headers, Some("sk-ai-switch-hermes"))
            .await
            .expect("cached resolve");
        assert_eq!(cached, PlatformId::Hermes);
    }

    #[tokio::test]
    async fn resolve_platform_without_key_or_header_fails_closed() {
        use crate::database::{create_memory_pool, run_migrations};

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let state = ProxyAppState {
            pool,
            key_cache: Arc::new(Mutex::new(RouteProxyKeyCache::default())),
        };

        let error = resolve_platform(&state, &HeaderMap::new(), None)
            .await
            .expect_err("platform identity must be explicit");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "route_proxy.platform_unresolved",
                ..
            }
        ));
    }

    #[test]
    fn build_upstream_request_uses_official_cpa_base_url_and_headers() {
        let credential = SelectedCredential {
            id: "official-grok".to_string(),
            platform: "grok".to_string(),
            kind: "official".to_string(),
            display_name: "Grok OAuth".to_string(),
            status: "ok".to_string(),
            secret_payload_json: r#"{"access_token":"at-xai","refresh_token":"rt-xai"}"#
                .to_string(),
            config_json: serde_json::json!({
                "base_url": "https://cli-chat-proxy.grok.com/v1",
                "token_endpoint": "https://auth.x.ai/oauth2/token",
                "headers": {
                    "X-Client-Name": "grok-cli"
                }
            })
            .to_string(),
        };

        let (url, headers, _) = build_upstream_request(
            &credential,
            "grok",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"grok-3"}"#,
        )
        .expect("official grok request");

        assert_eq!(url, "https://cli-chat-proxy.grok.com/v1/chat/completions");
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer at-xai")
        );
        // Outdated CPA User-Agent/X-Client-Name must be upgraded to CLIProxyAPI identity.
        assert_eq!(
            headers
                .get("user-agent")
                .and_then(|value| value.to_str().ok()),
            Some("xai-grok-workspace/0.2.93")
        );
        assert_eq!(
            headers
                .get("x-grok-client-version")
                .and_then(|value| value.to_str().ok()),
            Some("0.2.93")
        );
        assert_eq!(
            headers
                .get("x-xai-token-auth")
                .and_then(|value| value.to_str().ok()),
            Some("xai-grok-cli")
        );
        assert!(headers.get("x-client-name").is_none());
    }

    #[test]
    fn build_upstream_request_custom_user_agent_overrides_grok_forced_ua() {
        let credential = SelectedCredential {
            id: "official-grok-custom-ua".to_string(),
            platform: "grok".to_string(),
            kind: "official".to_string(),
            display_name: "Grok Custom UA".to_string(),
            status: "ok".to_string(),
            secret_payload_json: r#"{"access_token":"at-xai"}"#.to_string(),
            config_json: serde_json::json!({
                "base_url": "https://cli-chat-proxy.grok.com/v1",
                "headers": {
                    "User-Agent": "MyGrokClient/9.9.9",
                    "X-Client-Name": "grok-cli"
                }
            })
            .to_string(),
        };

        let (_, headers, _) = build_upstream_request(
            &credential,
            "grok",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"grok-4.5"}"#,
        )
        .expect("request");

        assert_eq!(
            headers
                .get("user-agent")
                .and_then(|value| value.to_str().ok()),
            Some("MyGrokClient/9.9.9")
        );
        assert_eq!(
            headers
                .get("x-grok-client-version")
                .and_then(|value| value.to_str().ok()),
            Some("0.2.93")
        );
        assert_eq!(
            headers
                .get("x-xai-token-auth")
                .and_then(|value| value.to_str().ok()),
            Some("xai-grok-cli")
        );
    }

    #[test]
    fn build_upstream_request_empty_user_agent_keeps_grok_forced_ua() {
        let credential = SelectedCredential {
            id: "official-grok-empty-ua".to_string(),
            platform: "grok".to_string(),
            kind: "official".to_string(),
            display_name: "Grok Empty UA".to_string(),
            status: "ok".to_string(),
            secret_payload_json: r#"{"access_token":"at-xai"}"#.to_string(),
            config_json: serde_json::json!({
                "base_url": "https://cli-chat-proxy.grok.com/v1",
                "headers": {
                    "User-Agent": "   "
                }
            })
            .to_string(),
        };

        let (_, headers, _) = build_upstream_request(
            &credential,
            "grok",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"grok-4.5"}"#,
        )
        .expect("request");

        assert_eq!(
            headers
                .get("user-agent")
                .and_then(|value| value.to_str().ok()),
            Some("xai-grok-workspace/0.2.93")
        );
    }

    #[test]
    fn build_upstream_request_custom_user_agent_applies_on_api_accounts() {
        let mut credential = api_credential("relay-ua", "openai");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.example.com/v1",
            "interface_format": "openai",
            "model_mappings": [],
            "headers": {
                "user-agent": "RelayBot/1.0"
            }
        })
        .to_string();

        let (_, headers, _) = build_upstream_request(
            &credential,
            "codex",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5.5"}"#,
        )
        .expect("request");

        assert_eq!(
            headers
                .get("user-agent")
                .and_then(|value| value.to_str().ok()),
            Some("RelayBot/1.0")
        );
    }

    #[test]
    fn build_upstream_request_skips_cli_headers_for_official_xai_api() {
        let credential = SelectedCredential {
            id: "official-grok-api".to_string(),
            platform: "grok".to_string(),
            kind: "official".to_string(),
            display_name: "Grok API".to_string(),
            status: "ok".to_string(),
            secret_payload_json: r#"{"access_token":"at-xai"}"#.to_string(),
            config_json: serde_json::json!({
                "base_url": "https://api.x.ai/v1"
            })
            .to_string(),
        };

        let (_, headers, _) = build_upstream_request(
            &credential,
            "grok",
            "/chat/completions",
            None,
            HeaderMap::new(),
            br#"{"model":"grok-3"}"#,
        )
        .expect("official api.x.ai request");

        assert!(headers.get("x-grok-client-version").is_none());
        assert!(headers.get("x-xai-token-auth").is_none());
    }

    #[test]
    fn build_upstream_request_uses_agent_identity_for_sub2api_codex() {
        use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
        use base64::Engine as _;
        use ed25519_dalek::pkcs8::EncodePrivateKey;
        use ed25519_dalek::SigningKey;

        let signing_key = SigningKey::from_bytes(&[9u8; 32]);
        let private_key = signing_key.to_pkcs8_der().expect("encode key");
        let credential = SelectedCredential {
            id: "official-k12".to_string(),
            platform: "codex".to_string(),
            kind: "official".to_string(),
            display_name: "K12 Agent".to_string(),
            status: "ok".to_string(),
            secret_payload_json: serde_json::json!({
                "agent_runtime_id": "agent-runtime-1",
                "agent_private_key": BASE64_STANDARD.encode(private_key.as_bytes()),
                "task_id": "task-run-1",
                "account_id": "account-1"
            })
            .to_string(),
            config_json: serde_json::json!({
                "auth_mode": "agentIdentity",
                "chatgpt_account_is_fedramp": true
            })
            .to_string(),
        };

        let (url, headers, _) = build_upstream_request(
            &credential,
            "codex",
            "/v1/responses",
            None,
            HeaderMap::new(),
            br#"{"model":"gpt-5"}"#,
        )
        .expect("agent identity request");

        assert_eq!(url, "https://chatgpt.com/backend-api/codex/responses");
        assert!(headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("AgentAssertion ")));
        assert_eq!(
            headers
                .get("chatgpt-account-id")
                .and_then(|value| value.to_str().ok()),
            Some("account-1")
        );
        assert_eq!(
            headers
                .get("x-openai-fedramp")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
    }

    #[test]
    fn is_route_credential_quota_available_filters_zero_remaining() {
        assert!(is_route_credential_quota_available("{}"));
        assert!(is_route_credential_quota_available(
            r#"{"quota_remaining":12}"#
        ));
        assert!(is_route_credential_quota_available(
            r#"{"primary_remain":12}"#
        ));
        assert!(!is_route_credential_quota_available(
            r#"{"quota_remaining":0}"#
        ));
        assert!(!is_route_credential_quota_available(
            r#"{"primary_remain":0}"#
        ));
        assert!(!is_route_credential_quota_available(
            r#"{"primary_remain":5,"weekly_remain":0}"#
        ));
        assert!(!is_route_credential_quota_available(
            r#"{"quota_remaining":-1}"#
        ));
        assert!(!is_route_credential_quota_available(
            r#"{"quota_remaining":"0"}"#
        ));
        assert!(is_route_credential_quota_available(
            r#"{"quota_remaining":"5"}"#
        ));
    }

    #[test]
    fn parse_official_quota_snapshot_from_free_usage_exhausted() {
        let body = r#"{
  "code": "subscription:free-usage-exhausted",
  "error": "You've used all the included free usage for model grok-4.5-build-free for now. Usage resets over a rolling 24-hour window — tokens (actual/limit): 1177205/1000000. Upgrade to a Grok subscription for higher limits: https://grok.com/supergrok"
}"#;
        let snapshot = parse_official_quota_snapshot(body).expect("snapshot");
        assert_eq!(snapshot.subscription_type.as_deref(), Some("free"));
        assert_eq!(snapshot.primary_remain, Some(0));
        assert_eq!(snapshot.quota_remaining, Some(0));
        assert_eq!(snapshot.quota_used, Some(1_177_205));
        assert_eq!(snapshot.quota_limit, Some(1_000_000));

        let next = apply_official_quota_snapshot("{}", &snapshot).expect("config");
        assert!(next.contains("\"subscription_type\":\"free\""));
        assert!(next.contains("\"primary_remain\":0"));
        assert!(next.contains("\"quota_remaining\":0"));
        assert!(next.contains("\"quota_used\":1177205"));
        assert!(next.contains("\"quota_limit\":1000000"));
        assert!(next.contains("quota_updated_at"));
        assert!(next.contains("reset_primary"));
    }

    #[test]
    fn access_token_is_expired_parses_rfc3339() {
        let future = serde_json::json!({
            "expired": (Utc::now() + chrono::Duration::hours(1)).to_rfc3339()
        });
        let past = serde_json::json!({
            "expired": (Utc::now() - chrono::Duration::hours(1)).to_rfc3339()
        });
        assert!(!access_token_is_expired(&future));
        assert!(access_token_is_expired(&past));
    }

    #[test]
    fn is_permanent_oauth_refresh_failure_detects_revoked_refresh_token() {
        assert!(is_permanent_oauth_refresh_failure(
            r#"OAuth refresh failed with status 400: {"error":"invalid_grant","error_description":"Refresh token has been revoked"}"#
        ));
        assert!(!is_permanent_oauth_refresh_failure(
            "OAuth refresh request failed: error sending request"
        ));
        assert!(format_oauth_refresh_failure("invalid_grant").contains("重新导入"));
    }

    #[test]
    fn resolve_oauth_client_id_uses_xai_public_client_for_grok() {
        let config = serde_json::json!({
            "token_endpoint": "https://auth.x.ai/oauth2/token"
        });
        let secret = serde_json::json!({});
        assert_eq!(
            resolve_oauth_client_id("grok", &config, &secret).as_deref(),
            Some(XAI_OAUTH_CLIENT_ID)
        );
        assert_eq!(
            resolve_oauth_client_id("xai", &config, &secret).as_deref(),
            Some(XAI_OAUTH_CLIENT_ID)
        );
    }

    #[test]
    fn resolve_oauth_client_id_prefers_config_value() {
        let config = serde_json::json!({
            "client_id": "custom-client",
            "token_endpoint": "https://auth.x.ai/oauth2/token"
        });
        let secret = serde_json::json!({});
        assert_eq!(
            resolve_oauth_client_id("grok", &config, &secret).as_deref(),
            Some("custom-client")
        );
    }

    #[test]
    fn jwt_claim_helpers_parse_payload() {
        // {"alg":"none"}.{"exp":1893456000,"client_id":"cid-from-jwt"}.sig
        let token =
            "eyJhbGciOiJub25lIn0.eyJleHAiOjE4OTM0NTYwMDAsImNsaWVudF9pZCI6ImNpZC1mcm9tLWp3dCJ9.sig";
        assert_eq!(jwt_claim_i64(token, "exp"), Some(1893456000));
        assert_eq!(
            jwt_claim_string(token, "client_id").as_deref(),
            Some("cid-from-jwt")
        );
    }

    #[test]
    fn apply_responses_custom_tool_compat_rewrites_custom_tools_and_calls() {
        let body = br#"{
            "model":"gpt-5",
            "tools":[
                {"type":"function","name":"shell","parameters":{"type":"object"}},
                {"type":"custom","name":"apply_patch","description":"patch files"}
            ],
            "input":[
                {
                    "type":"custom_tool_call",
                    "id":"call_1",
                    "call_id":"call_1",
                    "name":"apply_patch",
                    "input":"*** Begin Patch"
                }
            ]
        }"#;
        let rewritten = apply_responses_custom_tool_compat(body);
        let value: Value = serde_json::from_slice(&rewritten).expect("json");

        assert_eq!(
            value.pointer("/tools/1/type").and_then(Value::as_str),
            Some("function")
        );
        assert_eq!(
            value.pointer("/tools/1/name").and_then(Value::as_str),
            Some("apply_patch")
        );
        assert_eq!(
            value
                .pointer("/tools/1/parameters/properties/input/type")
                .and_then(Value::as_str),
            Some("string")
        );
        assert_eq!(
            value.pointer("/input/0/type").and_then(Value::as_str),
            Some("function_call")
        );
        let args = value
            .pointer("/input/0/arguments")
            .and_then(Value::as_str)
            .expect("arguments");
        let args_value: Value = serde_json::from_str(args).expect("args json");
        assert_eq!(
            args_value.pointer("/input").and_then(Value::as_str),
            Some("*** Begin Patch")
        );
    }

    #[test]
    fn build_upstream_request_rewrites_custom_tools_for_responses_api_relays() {
        let mut credential = api_credential("xiaomi", "openai-responses");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.xiaomi.example/v1",
            "interface_format": "openai-responses",
            "model_mappings": [{"from":"gpt-5","to":"mi-model"}],
            "responses_custom_tool_compat": true
        })
        .to_string();

        let body = br#"{
            "model":"gpt-5",
            "tools":[{"type":"custom","name":"apply_patch","description":"patch files"}]
        }"#;
        let (url, _, rewritten) = build_upstream_request(
            &credential,
            "codex",
            "/v1/responses",
            None,
            HeaderMap::new(),
            body,
        )
        .expect("request");
        assert_eq!(url, "https://api.xiaomi.example/v1/responses");
        let value: Value = serde_json::from_slice(&rewritten).expect("json");

        assert_eq!(
            value.pointer("/model").and_then(Value::as_str),
            Some("mi-model")
        );
        assert_eq!(
            value.pointer("/tools/0/type").and_then(Value::as_str),
            Some("function")
        );
        assert_eq!(
            value.pointer("/tools/0/name").and_then(Value::as_str),
            Some("apply_patch")
        );
    }

    #[test]
    fn build_upstream_request_skips_custom_tool_rewrite_when_switch_off() {
        let mut credential = api_credential("xiaomi", "openai-responses");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.xiaomi.example/v1",
            "interface_format": "openai-responses",
            "model_mappings": [{"from":"gpt-5","to":"mi-model"}],
            "responses_custom_tool_compat": false
        })
        .to_string();

        let body = br#"{
            "model":"gpt-5",
            "tools":[{"type":"custom","name":"apply_patch","description":"patch files"}]
        }"#;
        let (_, _, rewritten) = build_upstream_request(
            &credential,
            "codex",
            "/responses",
            None,
            HeaderMap::new(),
            body,
        )
        .expect("request");
        let value: Value = serde_json::from_slice(&rewritten).expect("json");

        assert_eq!(
            value.pointer("/model").and_then(Value::as_str),
            Some("mi-model")
        );
        assert_eq!(
            value.pointer("/tools/0/type").and_then(Value::as_str),
            Some("custom")
        );
    }

    #[test]
    fn build_upstream_request_rewrites_custom_tools_when_switch_on() {
        let mut credential = api_credential("xiaomi", "openai-responses");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.xiaomi.example/v1",
            "interface_format": "openai-responses",
            "model_mappings": [{"from":"gpt-5","to":"mi-model"}],
            "responses_custom_tool_compat": true
        })
        .to_string();

        let body = br#"{
            "model":"gpt-5",
            "tools":[{"type":"custom","name":"apply_patch","description":"patch files"}]
        }"#;
        let (_, _, rewritten) = build_upstream_request(
            &credential,
            "codex",
            "/responses",
            None,
            HeaderMap::new(),
            body,
        )
        .expect("request");
        let value: Value = serde_json::from_slice(&rewritten).expect("json");

        assert_eq!(
            value.pointer("/tools/0/type").and_then(Value::as_str),
            Some("function")
        );
    }

    #[test]
    fn build_upstream_request_skips_custom_tool_rewrite_when_switch_missing() {
        let mut credential = api_credential("xiaomi", "openai-responses");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.xiaomi.example/v1",
            "interface_format": "openai-responses",
            "model_mappings": []
        })
        .to_string();

        let body = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#;
        let (_, _, rewritten) = build_upstream_request(
            &credential,
            "codex",
            "/responses",
            None,
            HeaderMap::new(),
            body,
        )
        .expect("request");
        let value: Value = serde_json::from_slice(&rewritten).expect("json");
        assert_eq!(
            value.pointer("/tools/0/type").and_then(Value::as_str),
            Some("custom")
        );
    }

    #[test]
    fn build_upstream_request_skips_custom_tool_rewrite_on_non_responses_path_even_when_on() {
        let mut credential = api_credential("xiaomi", "openai");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.xiaomi.example/v1",
            "interface_format": "openai",
            "model_mappings": [],
            "responses_custom_tool_compat": true
        })
        .to_string();

        let body = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#;
        let (_, _, rewritten) = build_upstream_request(
            &credential,
            "codex",
            "/chat/completions",
            None,
            HeaderMap::new(),
            body,
        )
        .expect("request");
        let value: Value = serde_json::from_slice(&rewritten).expect("json");
        assert_eq!(
            value.pointer("/tools/0/type").and_then(Value::as_str),
            Some("custom")
        );
    }

    #[test]
    fn restore_custom_tools_in_sse_payload_rewrites_function_calls() {
        let names = std::collections::HashSet::from(["apply_patch".to_string()]);
        let body = concat!(
            r#"data: {"type":"response.output_item.done","item":{"type":"function_call","name":"apply_patch","arguments":"{\"input\":\"patch\"}","call_id":"c1"}}"#,
            "\n",
            "data: [DONE]\n"
        );
        let restored = restore_custom_tools_in_responses_payload(body.as_bytes(), &names);
        let text = String::from_utf8(restored).expect("utf8");
        assert!(text.contains("custom_tool_call"), "restored={text}");
        assert!(
            text.contains(r#""input":"patch""#) || text.contains(r#""input": "patch""#),
            "restored={text}"
        );
        assert!(text.contains("data: [DONE]"), "restored={text}");
        assert!(!text.contains("function_call"), "restored={text}");
    }

    #[test]
    fn is_models_list_path_matches_openai_style_paths() {
        assert!(is_models_list_path("/v1/models"));
        assert!(is_models_list_path("/models"));
        assert!(is_models_list_path("/v1/models/"));
        assert!(!is_models_list_path("/v1/chat/completions"));
        assert!(!is_models_list_path(
            "/v1beta/models/gemini:generateContent"
        ));
    }

    #[test]
    fn filter_credentials_for_model_keeps_wildcard_and_matching_mappings_only() {
        let wildcard = api_credential_with_config("wildcard", r#"{"model_mappings":[]}"#);
        let sol = api_credential_with_config(
            "sol",
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"sol-upstream"}]}"#,
        );
        let luna = api_credential_with_config(
            "luna",
            r#"{"model_mappings":[{"from":"gpt-5.6-luna","to":"luna-upstream"}]}"#,
        );

        let selected = filter_credentials_for_model(
            vec![wildcard.clone(), sol.clone(), luna],
            Some("gpt-5.6-sol"),
        );

        assert_eq!(
            selected
                .iter()
                .map(|item| item.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["wildcard", "sol"]
        );
    }

    #[tokio::test]
    async fn model_unmatched_error_uses_stable_error_code() {
        let response = json_error(
            StatusCode::BAD_GATEWAY,
            "route_pool.model_unmatched: no enabled route credential supports model 'gpt-5.6-luna' on platform 'codex'",
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("error body");
        let value: Value = serde_json::from_slice(&body).expect("error json");

        assert_eq!(
            value.pointer("/error/code").and_then(Value::as_str),
            Some("route_pool.model_unmatched")
        );
    }

    #[test]
    fn models_list_includes_codex_baseline_when_pool_has_wildcard_account() {
        let wildcard = api_credential_with_config("wildcard", r#"{"model_mappings":[]}"#);
        let mut mapped = api_credential("mapped", "openai");
        mapped.config_json = serde_json::json!({
            "model_mappings": [{"from":"gpt-5.6-sol","to":"sol-upstream"}]
        })
        .to_string();

        let payload = build_models_list_payload("codex", &[wildcard, mapped]);
        let models = payload
            .get("data")
            .and_then(Value::as_array)
            .expect("models data")
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert_eq!(
            models,
            vec![
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
                "gpt-5.5"
            ]
        );
    }

    #[test]
    fn advertised_models_aggregate_and_dedupe_mappings() {
        let mut first = api_credential("first", "openai");
        first.config_json = serde_json::json!({
            "base_url": "https://api.example.com/v1",
            "interface_format": "openai",
            "model_mappings": [
                {"from":"gpt-5.5","to":"up-a"},
                {"from":"gpt-5","to":"up-b"},
                {"from":"upstream-model","to":"ignored"}
            ]
        })
        .to_string();

        let mut second = api_credential("second", "openai");
        second.config_json = serde_json::json!({
            "base_url": "https://api.example.com/v1",
            "interface_format": "openai",
            "model_mappings": [
                {"from":"GPT-5","to":"up-c"},
                {"from":"gpt-4.1","to":"up-d"}
            ]
        })
        .to_string();

        let payload = build_models_list_payload("codex", &[first, second]);
        let models = payload
            .get("data")
            .and_then(Value::as_array)
            .expect("models data")
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(models, vec!["gpt-5.5", "gpt-5", "gpt-4.1"]);
    }

    #[test]
    fn advertised_models_expose_claude_1m_variants() {
        let mut credential = api_credential("claude-a", "anthropic");
        credential.platform = "claude".to_string();
        credential.config_json = serde_json::json!({
            "base_url": "https://api.anthropic.com",
            "interface_format": "anthropic",
            "model_mappings": [
                {
                    "from": "claude-sonnet-5",
                    "to": "provider-sonnet",
                    "supports_1m": true
                },
                {
                    "from": "claude-opus-4-8",
                    "to": "provider-opus",
                    "supports_1m": false
                }
            ]
        })
        .to_string();

        let payload = build_models_list_payload("claude", &[credential]);
        let models = payload
            .get("data")
            .and_then(Value::as_array)
            .expect("models data")
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(
            models,
            vec!["claude-sonnet-5", "claude-sonnet-5[1m]", "claude-opus-4-8"]
        );
    }

    #[test]
    fn json_models_list_response_returns_openai_compatible_list() {
        let mut credential = api_credential("first", "openai");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.example.com/v1",
            "interface_format": "openai",
            "model_mappings": [
                {"from":"gpt-5.5","to":"up-a"},
                {"from":"gpt-5","to":"up-b"}
            ]
        })
        .to_string();

        let payload = build_models_list_payload("codex", &[credential]);
        assert_eq!(payload.get("object").and_then(Value::as_str), Some("list"));
        let data = payload.get("data").and_then(Value::as_array).expect("data");
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].get("id").and_then(Value::as_str), Some("gpt-5.5"));
        assert_eq!(data[0].get("object").and_then(Value::as_str), Some("model"));
        assert_eq!(
            data[0].get("owned_by").and_then(Value::as_str),
            Some("ai-switch")
        );
        assert_eq!(data[1].get("id").and_then(Value::as_str), Some("gpt-5"));

        let response = json_models_list_response("codex", &[]);
        assert_eq!(response.status(), StatusCode::OK);
    }
}
