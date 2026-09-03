use crate::database::repositories::route_credential_model_repository::RouteCredentialModelRepository;
use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
use crate::error::{ApiError, AppError};
use crate::models::platform::{ApiDialect, CapabilityRule, PlatformId, PlatformOperation};
use crate::models::route_credential::{
    is_synthetic_route_alias, normalize_anthropic_api_key_field, ModelMapping,
    RouteCredentialFailurePolicy, ANTHROPIC_API_KEY_FIELD, ANTHROPIC_AUTH_TOKEN_FIELD,
    CLAUDE_ONE_M_SUFFIX,
};
use crate::models::route_credential_model::{
    FailureScope, RouteCredentialModelState, MODEL_STATUS_OK,
};
use crate::models::route_pool::RouteUsageBreakdown;
use crate::services::client_identity;
use crate::services::codex_reasoning_cache::CodexReasoningCache;
use crate::services::http_client::{
    build_outbound_http_client, build_outbound_http_client_with_timeouts, OutboundTimeouts,
};
use crate::services::official_agent_identity_service::{
    is_official_agent_identity_credential, resolve_agent_identity_headers,
    CODEX_AGENT_IDENTITY_BASE_URL,
};
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::response_failure_service::{
    detect_response_failed, is_quota_exhaustion_failure, stream_disconnected_before_completion,
    STREAM_DISCONNECTED_FAILURE_MESSAGE,
};
use crate::services::route_config_service::generate_route_proxy_key;
use crate::services::route_credential_activity::{
    RouteCredentialActivityLease, RouteCredentialActivityRegistry,
};
use crate::services::route_failure_scope::is_account_scoped_failure;
use crate::services::route_model_capability::{
    advertised_model_catalog_entries, codex_effective_context_window, codex_reasoning_metadata,
    known_upstream_models, model_state_key, parse_model_capability, parse_model_capability_value,
    requested_model_from_body, resolve_mapping_target, supports_requested_model,
};
use crate::services::route_protocol_bridge::{
    is_anthropic_count_tokens_path, prepare_request as prepare_protocol_bridge_request,
    transform_response_with_tool_namespaces as transform_protocol_bridge_response, turn_reminder,
    PreparedBridgeRequest, ProtocolBridgeKind,
};
use crate::services::route_proxy_live_log::{
    stage_preview, RouteProxyLiveLog, RouteProxyLiveLogEntry, LIVE_LOG_STAGE_LIMIT,
};
use crate::services::route_proxy_stream::StreamObserver;
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
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;

const BIND_HOST: &str = "127.0.0.1";
const DEFAULT_ROUTE_PROXY_PORT: u16 = 19527;
const ROUTE_PROXY_KEY_CACHE_TTL: Duration = Duration::from_secs(30);
/// Upstream connect-phase ceiling. If the handshake does not complete there is
/// nothing to wait for — fail the attempt so the retry queue moves on.
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Maximum gap between upstream reads, reset after each successful read.
///
/// Deliberately not a total deadline. A buffered response is only complete once
/// the whole body has arrived, and a streamed one stays open for as long as the
/// upstream keeps generating, so on either path a long answer legitimately takes
/// minutes and a total budget would kill valid generations. What this catches is
/// bytes stopping altogether — an upstream that accepted the connection and then
/// went silent. Without it such a stall never returns an error, so the failover
/// loop never runs and the CLI hangs forever.
const UPSTREAM_READ_TIMEOUT: Duration = Duration::from_secs(180);
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
const ROUTE_PROXY_RESPONSE_BODY_LIMIT: usize = 16 * 1024;
/// Successful responses keep only a short preview so callers can confirm the
/// upstream returned real AI content (vs. a fake/empty 200) without bloating
/// `usage_events`, which is never pruned.
const ROUTE_PROXY_SUCCESS_BODY_LIMIT: usize = 2 * 1024;

async fn wait_for_credential_retry(policy: RouteCredentialFailurePolicy) {
    if policy.retry_interval_ms > 0 {
        tokio::time::sleep(Duration::from_millis(policy.retry_interval_ms.into())).await;
    }
}

/// Describe an upstream transport failure, naming the deadline that fired.
///
/// reqwest renders a timeout as a bare "operation timed out", which reads the
/// same as any other transport error in the request log. Since a stalled
/// upstream and a refused one call for different fixes, spell out which limit
/// was hit and what it was set to.
fn describe_upstream_transport_error(
    display_name: &str,
    context: &str,
    error: &reqwest::Error,
    timeouts: OutboundTimeouts,
) -> String {
    let mut message = format!("{display_name}: {context}: {error}");
    if error.is_timeout() {
        if let Some(read) = timeouts.read {
            message.push_str(&format!(
                " (no data from upstream for {}s; treated as a stalled connection and retried/failed over)",
                read.as_secs()
            ));
        }
    } else if error.is_connect() {
        if let Some(connect) = timeouts.connect {
            message.push_str(&format!(
                " (could not connect within {}s; check network/proxy reachability)",
                connect.as_secs()
            ));
        }
    }
    message
}

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
    activity: RouteCredentialActivityRegistry,
    live_log: RouteProxyLiveLog,
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
    activity: RouteCredentialActivityRegistry,
    live_log: RouteProxyLiveLog,
    codex_history: CodexReasoningCache,
    /// Deadlines for upstream requests. A field rather than a constant read at
    /// the call site so tests can drive a stalled upstream in milliseconds
    /// instead of waiting out the production ceiling.
    upstream_timeouts: OutboundTimeouts,
}

impl ProxyAppState {
    fn default_upstream_timeouts() -> OutboundTimeouts {
        OutboundTimeouts {
            connect: Some(UPSTREAM_CONNECT_TIMEOUT),
            read: Some(UPSTREAM_READ_TIMEOUT),
            ..OutboundTimeouts::default()
        }
    }
}

#[derive(Default)]
struct RouteProxyKeyCache {
    loaded_at: Option<Instant>,
    // proxy_key -> platform
    by_key: HashMap<String, String>,
}

impl RouteProxyRuntimeState {
    pub fn activity(&self) -> RouteCredentialActivityRegistry {
        self.activity.clone()
    }

    pub fn live_log(&self) -> RouteProxyLiveLog {
        self.live_log.clone()
    }
}

#[derive(Debug)]
pub(crate) struct BuiltUpstreamRequest {
    pub(crate) target_url: String,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
    pub(crate) bridge_kind: Option<ProtocolBridgeKind>,
    pub(crate) tool_namespaces: BTreeMap<String, String>,
    pub(crate) streaming_request: bool,
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
        Self::start_with_upstream_timeouts(
            state,
            pool,
            transport,
            ProxyAppState::default_upstream_timeouts(),
        )
        .await
    }

    /// Same as [`RouteProxyService::start`] but with explicit upstream deadlines,
    /// so a test can exercise the stall path without a multi-minute wait.
    #[cfg(test)]
    pub(crate) async fn start_with_test_upstream_timeouts(
        state: &RouteProxyRuntimeState,
        pool: SqlitePool,
        transport: RouteProxyTransport,
        upstream_timeouts: OutboundTimeouts,
    ) -> Result<RouteProxyStatus, AppError> {
        Self::start_with_upstream_timeouts(state, pool, transport, upstream_timeouts).await
    }

    async fn start_with_upstream_timeouts(
        state: &RouteProxyRuntimeState,
        pool: SqlitePool,
        transport: RouteProxyTransport,
        upstream_timeouts: OutboundTimeouts,
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
            activity: state.activity.clone(),
            live_log: state.live_log.clone(),
            codex_history: CodexReasoningCache::default(),
            upstream_timeouts,
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
        let candidates = load_pool_candidates(pool, &platform)
            .await
            .map_err(|err| err.to_string())?;
        let candidates = filter_candidates_for_rule(candidates, &routing_rule);
        let credentials = partition_by_cooldown(candidates, &HashMap::new(), Utc::now());
        return Ok(json_models_list_response(
            &platform,
            &credentials,
            query.as_deref(),
        ));
    }

    let body_bytes = axum::body::to_bytes(body, 32 * 1024 * 1024)
        .await
        .map_err(|err| format!("Could not read proxy request body: {err}"))?;

    // Anthropic's token-counting endpoint, answered locally. No bridge can
    // convert it, and most third-party relays do not implement it — forwarding
    // it earns a 404 that the retry loop would charge against every credential
    // in the pool, cooling down accounts that are perfectly healthy.
    if is_anthropic_count_tokens_path(&path) {
        if method != Method::POST {
            return Ok((
                StatusCode::METHOD_NOT_ALLOWED,
                [("content-type", "application/json"), ("allow", "POST")],
                json!({
                    "type": "error",
                    "error": {
                        "type": "invalid_request_error",
                        "message": "Method not allowed for count_tokens",
                    }
                })
                .to_string(),
            )
                .into_response());
        }
        return Ok(json_count_tokens_response(&body_bytes));
    }

    let requested_model = requested_model_from_body(&body_bytes);
    let candidates = load_pool_candidates(pool, &platform)
        .await
        .map_err(|err| err.to_string())?;
    let candidates = filter_candidates_for_rule(candidates, &routing_rule);
    if candidates.is_empty() {
        return Err("No enabled route credentials in pool".to_string());
    }
    // Model filtering must run before cooldown partitioning: the model a request
    // asks for decides which cooldown applies, so an account cannot be judged
    // before its model key is known. It also means the all-cooling probe below
    // is now scoped to accounts that can actually serve this model.
    let candidates = filter_candidates_for_model(&platform, candidates, requested_model.as_deref());
    if candidates.is_empty() {
        let model = requested_model.as_deref().unwrap_or("unknown");
        return Err(format!(
            "route_pool.model_unmatched: no enabled route credential supports model '{model}' on platform '{platform}'"
        ));
    }
    // Keyed by account id: two accounts may map the same requested model to
    // different upstream names, so the pair is the key, never the model alone.
    let model_keys: HashMap<String, String> = candidates
        .iter()
        .filter_map(|candidate| {
            Some((
                candidate.credential.id.clone(),
                candidate.model_key.clone()?,
            ))
        })
        .collect();
    let model_states = load_candidate_model_states(pool, &candidates)
        .await
        .map_err(|err| err.to_string())?;
    let credentials = partition_by_cooldown(candidates, &model_states, Utc::now());
    if credentials.is_empty() {
        let model = requested_model.as_deref().unwrap_or("unknown");
        return Err(format!(
            "route_pool.model_unavailable: every route credential for model '{model}' on platform '{platform}' is paused or marked unhealthy"
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
    // Our reqwest client is built without any decompression features, so we must
    // never let the upstream compress the response — otherwise we relay/inspect
    // raw gzip/br/zstd bytes and everything downstream sees garbage. Force
    // identity encoding, matching what the real Claude Code / Codex CLIs send.
    force_identity_accept_encoding(&mut outbound_headers);

    let custom_tool_names = collect_custom_tool_names(&body_bytes);
    let upstream_query = strip_route_proxy_auth_query(query.as_deref());
    let client = build_outbound_http_client_with_timeouts(state.upstream_timeouts)?;
    // Fallback for upstreams that fetch remote image URLs and reject non-image
    // Content-Types (e.g. OSS objects served as text/plain): when a routed
    // credential opts in, inline remote images as base64 data URLs up front.
    let body_bytes = if credentials.iter().any(selected_credential_inlines_images) {
        axum::body::Bytes::from(inline_remote_image_urls_in_body(&body_bytes, &client).await)
    } else {
        body_bytes
    };
    let request_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|err| format!("Unsupported method: {err}"))?;
    let retry_indexes = credential_indexes_by_priority(&credentials, cursor);
    let mut retry_queue = retry_indexes
        .into_iter()
        .map(|credential_index| (credential_index, 0usize))
        .collect::<VecDeque<_>>();
    let mut retry_errors = Vec::new();
    let request_start = Instant::now();
    let mut acquired_any = false;
    let mut attempt = 0usize;

    while let Some((credential_index, credential_retry_count)) = retry_queue.pop_front() {
        attempt += 1;
        let selected = &credentials[credential_index];
        // The upstream model this account was matched on. Every failure recorded
        // below is charged against it unless the failure is account-scoped.
        let selected_model_key = model_keys.get(&selected.id).cloned();
        let Some(activity_lease) = state
            .activity
            .try_acquire(&platform, &selected.id, selected.max_concurrency)
            .await
        else {
            continue;
        };
        acquired_any = true;
        let credential =
            match maybe_refresh_official_credential(pool, selected, Some(&state.activity)).await {
                Ok(credential) => credential,
                Err(error) => {
                    if matches!(
                        classify_proxy_failure(None, Some(&error)),
                        ProxyFailureKind::Transient
                    ) {
                        record_route_credential_failure(
                            &state.activity,
                            &platform,
                            pool,
                            selected,
                            selected_model_key.as_deref(),
                            "refresh",
                            &error,
                            None,
                        )
                        .await;
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
                        requested_model.as_deref(),
                        None,
                        None,
                    );
                    let _ = insert_route_credential_request_event(
                        pool,
                        &selected.id,
                        &metadata,
                        &RouteUsageBreakdown::default(),
                        None,
                    )
                    .await;
                    emit_live_log(
                        &state.live_log,
                        &platform,
                        selected,
                        attempt,
                        &path,
                        None,
                        // Failed before an upstream request was built.
                        None,
                        None,
                        false,
                        trace_id.as_deref(),
                        request_start,
                        Some(&error),
                        requested_model.as_deref(),
                        None,
                        None,
                        Some(&body_bytes),
                        None,
                        None,
                        None,
                    );
                    retry_errors.push(format!("{}: {error}", selected.display_name));
                    continue;
                }
            };
        let failure_policy =
            RouteCredentialFailurePolicy::from_config_json(&credential.config_json);
        let upstream_request = build_upstream_request_internal(
            &credential,
            &platform,
            &path,
            upstream_query.as_deref(),
            outbound_headers.clone(),
            &body_bytes,
            Some(&state.codex_history),
            TurnReminderMode::Apply,
        );
        let BuiltUpstreamRequest {
            target_url,
            headers: request_headers,
            body: outbound_body,
            bridge_kind,
            tool_namespaces,
            streaming_request,
        } = match upstream_request {
            Ok(request) => request,
            Err(error) => {
                record_route_credential_failure(
                    &state.activity,
                    &platform,
                    pool,
                    &credential,
                    selected_model_key.as_deref(),
                    "request_build",
                    &error,
                    None,
                )
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
                    requested_model.as_deref(),
                    None,
                    None,
                );
                let _ = insert_route_credential_request_event(
                    pool,
                    &credential.id,
                    &metadata,
                    &RouteUsageBreakdown::default(),
                    None,
                )
                .await;
                emit_live_log(
                    &state.live_log,
                    &platform,
                    &credential,
                    attempt,
                    &path,
                    None,
                    // The upstream request could not be built, so there are no
                    // outbound headers to report.
                    None,
                    None,
                    false,
                    trace_id.as_deref(),
                    request_start,
                    Some(&error),
                    requested_model.as_deref(),
                    None,
                    None,
                    Some(&body_bytes),
                    None,
                    None,
                    None,
                );
                retry_errors.push(format!("{}: {error}", credential.display_name));
                continue;
            }
        };
        let bridge_name = bridge_kind.map(|kind| format!("{kind:?}"));
        let upstream_request_bytes = outbound_body.clone();
        let upstream_model = requested_model_from_body(&outbound_body);
        let upstream = client
            .request(request_method.clone(), &target_url)
            .headers(map_to_reqwest_headers(&request_headers))
            .body(outbound_body)
            .send()
            .await;

        let upstream = match upstream {
            Ok(response) => response,
            Err(error) => {
                let error_message = describe_upstream_transport_error(
                    &credential.display_name,
                    "upstream request failed",
                    &error,
                    state.upstream_timeouts,
                );
                let should_retry_same_credential =
                    credential_retry_count < failure_policy.retry_count as usize;
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
                    requested_model.as_deref(),
                    upstream_model.as_deref(),
                    None,
                );
                let _ = insert_route_credential_request_event(
                    pool,
                    &credential.id,
                    &metadata,
                    &RouteUsageBreakdown::default(),
                    None,
                )
                .await;
                emit_live_log(
                    &state.live_log,
                    &platform,
                    &credential,
                    attempt,
                    &path,
                    Some(&target_url),
                    Some(&request_headers),
                    None,
                    false,
                    trace_id.as_deref(),
                    request_start,
                    Some(&error_message),
                    requested_model.as_deref(),
                    upstream_model.as_deref(),
                    bridge_name.as_deref(),
                    Some(&body_bytes),
                    Some(&upstream_request_bytes),
                    None,
                    None,
                );
                if should_retry_same_credential {
                    wait_for_credential_retry(failure_policy).await;
                    retry_queue.push_front((credential_index, credential_retry_count + 1));
                } else {
                    record_route_credential_failure(
                        &state.activity,
                        &platform,
                        pool,
                        &credential,
                        selected_model_key.as_deref(),
                        "transport",
                        &error_message,
                        None,
                    )
                    .await;
                    retry_errors.push(error_message);
                }
                continue;
            }
        };
        let status =
            StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let mut upstream_headers = upstream.headers().clone();

        // Stream the response through instead of buffering it, when nothing
        // downstream needs the complete body.
        //
        // The gate is deliberately narrow. Buffering is what makes the retry
        // loop able to switch accounts on a failure that only shows up late in
        // the body, so it stays the default; streaming is opt-in per request
        // and only where every buffered-path consumer has an incremental
        // equivalent. See `route_proxy_stream` for those.
        if should_stream_upstream_response(
            bridge_kind,
            streaming_request,
            status,
            &custom_tool_names,
            &credential,
        ) {
            let mut stream = Box::pin(upstream.bytes_stream());
            // Wait for the first chunk while still inside the retry loop: until
            // a byte reaches the client this attempt can still be abandoned for
            // the next account, which is the whole reason to prime rather than
            // return immediately.
            let first_chunk = match futures_util::StreamExt::next(&mut stream).await {
                Some(Ok(chunk)) => chunk,
                Some(Err(error)) => {
                    let error_message = describe_upstream_transport_error(
                        &credential.display_name,
                        "could not read upstream response",
                        &error,
                        state.upstream_timeouts,
                    );
                    handle_stream_prime_failure(
                        &state,
                        pool,
                        &platform,
                        &credential,
                        &error_message,
                        StreamPrimeContext {
                            attempt,
                            path: &path,
                            target_url: &target_url,
                            status,
                            trace_id: trace_id.as_deref(),
                            request_start,
                            requested_model: requested_model.as_deref(),
                            upstream_model: upstream_model.as_deref(),
                            model_key: selected_model_key.as_deref(),
                            bridge_name: bridge_name.as_deref(),
                            client_request: &body_bytes,
                            upstream_request: &upstream_request_bytes,
                            upstream_headers: &request_headers,
                        },
                        credential_retry_count < failure_policy.retry_count as usize,
                        failure_policy,
                        &mut retry_queue,
                        credential_index,
                        credential_retry_count,
                        &mut retry_errors,
                    )
                    .await;
                    continue;
                }
                None => {
                    let error_message = format!(
                        "{}: upstream closed the stream before sending any data",
                        credential.display_name
                    );
                    handle_stream_prime_failure(
                        &state,
                        pool,
                        &platform,
                        &credential,
                        &error_message,
                        StreamPrimeContext {
                            attempt,
                            path: &path,
                            target_url: &target_url,
                            status,
                            trace_id: trace_id.as_deref(),
                            request_start,
                            requested_model: requested_model.as_deref(),
                            upstream_model: upstream_model.as_deref(),
                            model_key: selected_model_key.as_deref(),
                            bridge_name: bridge_name.as_deref(),
                            client_request: &body_bytes,
                            upstream_request: &upstream_request_bytes,
                            upstream_headers: &request_headers,
                        },
                        credential_retry_count < failure_policy.retry_count as usize,
                        failure_policy,
                        &mut retry_queue,
                        credential_index,
                        credential_retry_count,
                        &mut retry_errors,
                    )
                    .await;
                    continue;
                }
            };

            // A gateway that reports failure in a 200 body usually does it in
            // the opening frame. Catch that here, where failover still works.
            if let Some(failure) = detect_response_failed(&first_chunk) {
                let error_message = format!("{}: {}", credential.display_name, failure.message);
                handle_stream_prime_failure(
                    &state,
                    pool,
                    &platform,
                    &credential,
                    &error_message,
                    StreamPrimeContext {
                        attempt,
                        path: &path,
                        target_url: &target_url,
                        status,
                        trace_id: trace_id.as_deref(),
                        request_start,
                        requested_model: requested_model.as_deref(),
                        upstream_model: upstream_model.as_deref(),
                        model_key: selected_model_key.as_deref(),
                        bridge_name: bridge_name.as_deref(),
                        client_request: &body_bytes,
                        upstream_request: &upstream_request_bytes,
                        upstream_headers: &request_headers,
                    },
                    !matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
                        && credential_retry_count < failure_policy.retry_count as usize,
                    failure_policy,
                    &mut retry_queue,
                    credential_index,
                    credential_retry_count,
                    &mut retry_errors,
                )
                .await;
                continue;
            }

            let next_index = (credential_index + 1) % credentials.len();
            let _ =
                RoutePoolRepository::save_cursor_index(pool, &platform, next_index as i64).await;

            // Health, usage and the request log are all settled by the
            // completion handle when the stream ends — success here only means
            // "the first chunk looked fine", not that the response is whole.
            let completion = StreamCompletion {
                state: state.clone(),
                credential: credential.clone(),
                platform: platform.clone(),
                attempt,
                path: path.clone(),
                target_url: target_url.clone(),
                status,
                trace_id: trace_id.clone(),
                request_start,
                requested_model: requested_model.clone(),
                upstream_model: upstream_model.clone(),
                model_key: selected_model_key.clone(),
                bridge_name: bridge_name.clone(),
                client_request: body_bytes.clone(),
                upstream_request: upstream_request_bytes.clone(),
                upstream_headers: request_headers.clone(),
                observer: StreamObserver::new(LIVE_LOG_STAGE_LIMIT, streaming_request),
                // Held until the stream ends so the account's concurrency slot
                // is not handed out while this response is still in flight.
                _activity_lease: activity_lease,
            };

            let body = Body::from_stream(observed_upstream_stream(first_chunk, stream, completion));
            upstream_headers.remove(axum::http::header::CONTENT_LENGTH);
            return proxy_upstream_stream_response(status, upstream_headers, body);
        }

        let mut response_bytes = match upstream.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => {
                let error_message = describe_upstream_transport_error(
                    &credential.display_name,
                    "could not read upstream response",
                    &error,
                    state.upstream_timeouts,
                );
                let should_retry_same_credential =
                    credential_retry_count < failure_policy.retry_count as usize;
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
                    requested_model.as_deref(),
                    upstream_model.as_deref(),
                    None,
                );
                let _ = insert_route_credential_request_event(
                    pool,
                    &credential.id,
                    &metadata,
                    &RouteUsageBreakdown::default(),
                    None,
                )
                .await;
                emit_live_log(
                    &state.live_log,
                    &platform,
                    &credential,
                    attempt,
                    &path,
                    Some(&target_url),
                    Some(&request_headers),
                    None,
                    false,
                    trace_id.as_deref(),
                    request_start,
                    Some(&error_message),
                    requested_model.as_deref(),
                    upstream_model.as_deref(),
                    bridge_name.as_deref(),
                    Some(&body_bytes),
                    Some(&upstream_request_bytes),
                    None,
                    None,
                );
                if should_retry_same_credential {
                    wait_for_credential_retry(failure_policy).await;
                    retry_queue.push_front((credential_index, credential_retry_count + 1));
                } else {
                    record_route_credential_failure(
                        &state.activity,
                        &platform,
                        pool,
                        &credential,
                        selected_model_key.as_deref(),
                        "transport",
                        &error_message,
                        None,
                    )
                    .await;
                    retry_errors.push(error_message);
                }
                continue;
            }
        };
        // Snapshot the raw upstream body before any bridge transform mutates it
        // in place, so the live log can show stage 3 (raw) and stage 4 (final)
        // separately. `Bytes` clone is a cheap ref-count bump.
        let raw_upstream_bytes = response_bytes.clone();
        // Capture the upstream's plaintext reasoning + tool calls so the next
        // Codex turn can restore them (Responses→Chat only). See enrich above.
        if bridge_kind == Some(ProtocolBridgeKind::ResponsesToChat) && status.is_success() {
            state
                .codex_history
                .record_from_chat_response(&raw_upstream_bytes);
        }
        let upstream_content_type = upstream_headers
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        let stream_disconnected = stream_disconnected_before_completion(
            &raw_upstream_bytes,
            upstream_content_type,
            streaming_request,
        );
        if let Some(bridge_kind) = bridge_kind {
            let content_type = upstream_headers
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok());
            let transformed = match transform_protocol_bridge_response(
                bridge_kind,
                status.as_u16(),
                content_type,
                &response_bytes,
                &tool_namespaces,
            ) {
                Ok(response) => response,
                Err(error) => {
                    let error_message = format!(
                        "{}: could not transform upstream response: {error}",
                        credential.display_name
                    );
                    record_route_credential_failure(
                        &state.activity,
                        &platform,
                        pool,
                        &credential,
                        selected_model_key.as_deref(),
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
                        requested_model.as_deref(),
                        upstream_model.as_deref(),
                        Some(&response_bytes),
                    );
                    let _ = insert_route_credential_request_event(
                        pool,
                        &credential.id,
                        &metadata,
                        &RouteUsageBreakdown::default(),
                        crate::services::upstream_response_id::extract_upstream_response_id(
                            &response_bytes,
                        )
                        .as_deref(),
                    )
                    .await;
                    emit_live_log(
                        &state.live_log,
                        &platform,
                        &credential,
                        attempt,
                        &path,
                        Some(&target_url),
                        Some(&request_headers),
                        Some(status.as_u16()),
                        false,
                        trace_id.as_deref(),
                        request_start,
                        Some(&error_message),
                        requested_model.as_deref(),
                        upstream_model.as_deref(),
                        bridge_name.as_deref(),
                        Some(&body_bytes),
                        Some(&upstream_request_bytes),
                        Some(&raw_upstream_bytes),
                        None,
                    );
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
        let semantic_failure = detect_response_failed(&response_bytes).or_else(|| {
            stream_disconnected.then(|| {
                crate::services::response_failure_service::SemanticResponseFailure {
                    code: None,
                    error_type: None,
                    message: STREAM_DISCONNECTED_FAILURE_MESSAGE.to_string(),
                }
            })
        });
        let quota_failure = semantic_failure
            .as_ref()
            .is_some_and(is_quota_exhaustion_failure);
        let failure_kind = classify_proxy_failure(Some(status), response_text);
        let should_retry = !status.is_success() || !matches!(failure_kind, ProxyFailureKind::None);
        let proxy_success = status.is_success()
            && !quota_exhausted
            && !should_retry
            && !quota_failure
            && semantic_failure.is_none();
        let retry_error = if let Some(failure) = semantic_failure.as_ref() {
            Some(failure.message.clone())
        } else if quota_failure || quota_exhausted {
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
            requested_model.as_deref(),
            upstream_model.as_deref(),
            // Keep a body preview for every request (short on success, full on
            // failure) so the stats view can confirm real AI output.
            Some(response_bytes.as_ref()),
        );
        let usage = extract_usage_breakdown(&response_bytes);
        let mut usage = usage;
        // Prefer the model the upstream itself reported (a gateway may serve a
        // different one than was asked for), then the mapped upstream model, then
        // whatever the client requested.
        let priced_model = extract_response_model(&response_bytes)
            .or_else(|| upstream_model.clone())
            .or_else(|| requested_model.clone());
        apply_estimated_price(&mut usage, priced_model.as_deref());
        let _ = insert_route_credential_request_event(
            pool,
            &credential.id,
            &metadata,
            &usage,
            crate::services::upstream_response_id::extract_upstream_response_id(&response_bytes)
                .as_deref(),
        )
        .await;
        emit_live_log(
            &state.live_log,
            &platform,
            &credential,
            attempt,
            &path,
            Some(&target_url),
            Some(&request_headers),
            Some(status.as_u16()),
            proxy_success,
            trace_id.as_deref(),
            request_start,
            retry_error.as_deref(),
            requested_model.as_deref(),
            upstream_model.as_deref(),
            bridge_name.as_deref(),
            Some(&body_bytes),
            Some(&upstream_request_bytes),
            Some(&raw_upstream_bytes),
            Some(response_bytes.as_ref()),
        );

        let next_index = (credential_index + 1) % credentials.len();
        let _ = RoutePoolRepository::save_cursor_index(pool, &platform, next_index as i64).await;

        if quota_failure || quota_exhausted {
            if let Some(failure) = semantic_failure.as_ref() {
                let _ = RouteCredentialRepository::record_semantic_failure_with_status(
                    pool,
                    &credential.id,
                    Some(status.as_u16()),
                    1,
                    &failure.message,
                    Some(&response_bytes),
                )
                .await;
                state
                    .activity
                    .notify_status_change(&platform, &credential.id);
            } else {
                mark_route_credential_error(&state.activity, &platform, pool, &credential.id).await;
            }
            retry_errors.push(format!(
                "{}: upstream quota exhausted",
                credential.display_name
            ));
            continue;
        }
        if matches!(failure_kind, ProxyFailureKind::Permanent) {
            mark_route_credential_revoked(&state.activity, &platform, pool, &credential.id).await;
            let error_message = semantic_failure
                .as_ref()
                .map(|failure| failure.message.as_str())
                .unwrap_or("upstream credentials are invalid");
            retry_errors.push(format!("{}: {error_message}", credential.display_name));
            continue;
        }
        if let Some(failure) = semantic_failure.as_ref() {
            let can_retry_same_credential =
                !matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN);
            if can_retry_same_credential
                && credential_retry_count < failure_policy.retry_count as usize
            {
                wait_for_credential_retry(failure_policy).await;
                retry_queue.push_front((credential_index, credential_retry_count + 1));
            } else {
                record_route_credential_failure(
                    &state.activity,
                    &platform,
                    pool,
                    &credential,
                    selected_model_key.as_deref(),
                    "semantic_response_transient",
                    &failure.message,
                    Some(&response_bytes),
                )
                .await;
                retry_errors.push(format!("{}: {}", credential.display_name, failure.message));
            }
            continue;
        }
        if should_retry_same_credential_status(status) {
            let error_message = format!("upstream returned {}", status.as_u16());
            if credential_retry_count < failure_policy.retry_count as usize {
                wait_for_credential_retry(failure_policy).await;
                retry_queue.push_front((credential_index, credential_retry_count + 1));
            } else {
                record_route_credential_failure_with_status(
                    &state.activity,
                    &platform,
                    pool,
                    &credential,
                    selected_model_key.as_deref(),
                    "upstream_status",
                    Some(status.as_u16()),
                    &error_message,
                    Some(&response_bytes),
                )
                .await;
                retry_errors.push(format!("{}: {error_message}", credential.display_name));
            }
            continue;
        }
        if should_retry {
            let error_message = format!("upstream returned {}", status.as_u16());
            record_route_credential_failure_with_status(
                &state.activity,
                &platform,
                pool,
                &credential,
                selected_model_key.as_deref(),
                "upstream_status",
                Some(status.as_u16()),
                &error_message,
                Some(&response_bytes),
            )
            .await;
            retry_errors.push(format!(
                "{}: upstream returned {}",
                credential.display_name,
                status.as_u16()
            ));
            continue;
        }

        if RouteCredentialRepository::clear_transient_failure(
            pool,
            &credential.id,
            selected_model_key.as_deref(),
        )
        .await
        .is_ok()
        {
            state
                .activity
                .notify_status_change(&platform, &credential.id);
        }
        return proxy_upstream_response(status, upstream_headers, response_bytes.to_vec());
    }

    if !acquired_any {
        return Err(format!(
            "route_pool.concurrency_exhausted: all route credentials are at their concurrency limit on platform '{platform}'"
        ));
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

/// Force `accept-encoding: identity` so upstreams never compress the response.
/// The outbound reqwest client has no gzip/br/zstd decompression support, so a
/// compressed body would be relayed and inspected as raw bytes.
fn force_identity_accept_encoding(headers: &mut HeaderMap) {
    headers.insert(
        HeaderName::from_static("accept-encoding"),
        HeaderValue::from_static("identity"),
    );
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
    requested_model: Option<&str>,
    upstream_model: Option<&str>,
    response_body: Option<&[u8]>,
) -> String {
    let mut metadata = serde_json::json!({
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
    });
    if let Some(object) = metadata.as_object_mut() {
        if let Some(model) = requested_model
            .map(str::trim)
            .filter(|model| !model.is_empty())
        {
            object.insert("requested_model".to_string(), json!(model));
        }
        if let Some(model) = upstream_model
            .map(str::trim)
            .filter(|model| !model.is_empty())
        {
            object.insert("upstream_model".to_string(), json!(model));
        }
        if let Some(response_body) = route_proxy_response_body_metadata(response_body, success) {
            object.insert("response_body".to_string(), json!(response_body));
        }
    }
    metadata.to_string()
}

fn route_proxy_response_body_metadata(
    response_body: Option<&[u8]>,
    success: bool,
) -> Option<String> {
    let body = response_body?;
    if body.is_empty() {
        return None;
    }
    let limit = if success {
        ROUTE_PROXY_SUCCESS_BODY_LIMIT
    } else {
        ROUTE_PROXY_RESPONSE_BODY_LIMIT
    };
    let truncated_body = &body[..body.len().min(limit)];
    let text = String::from_utf8_lossy(truncated_body).to_string();
    (!text.trim().is_empty()).then_some(text)
}

/// Record one live-log entry for a proxy attempt, carrying whichever of the four
/// stages are available at the call site (missing stages pass `None`).
#[allow(clippy::too_many_arguments)]
fn emit_live_log(
    live_log: &RouteProxyLiveLog,
    platform: &str,
    credential: &SelectedCredential,
    attempt: usize,
    path: &str,
    target_url: Option<&str>,
    upstream_headers: Option<&HeaderMap>,
    status: Option<u16>,
    success: bool,
    trace_id: Option<&str>,
    started_at: Instant,
    error_message: Option<&str>,
    requested_model: Option<&str>,
    upstream_model: Option<&str>,
    bridge: Option<&str>,
    client_request: Option<&[u8]>,
    upstream_request: Option<&[u8]>,
    upstream_response: Option<&[u8]>,
    final_response: Option<&[u8]>,
) {
    let client_request = redact_verbose_request_fields(client_request);
    let upstream_request = redact_verbose_request_fields(upstream_request);
    let (client_request, t1) = stage_preview(client_request.as_deref());
    let (upstream_request, t2) = stage_preview(upstream_request.as_deref());
    let (upstream_response, t3) = stage_preview(upstream_response);
    let notes = diagnostic_notes(success, bridge, client_request.as_deref(), final_response);
    let (final_response, t4) = stage_preview(final_response);
    live_log.record(RouteProxyLiveLogEntry {
        id: uuid::Uuid::new_v4().to_string(),
        trace_id: trace_id.map(str::to_string),
        platform: platform.to_string(),
        credential_id: credential.id.clone(),
        credential_name: credential.display_name.clone(),
        attempt,
        path: path.to_string(),
        target_url: target_url.map(redact_sensitive_url),
        upstream_headers: upstream_headers.map(format_upstream_headers),
        requested_model: requested_model
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string),
        upstream_model: upstream_model
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string),
        status,
        success,
        error_message: error_message.map(str::to_string),
        duration_ms: elapsed_millis(started_at),
        bridge: bridge.map(str::to_string),
        client_request,
        upstream_request,
        upstream_response,
        final_response,
        notes,
        truncated: t1 || t2 || t3 || t4,
        created_at: Utc::now().to_rfc3339(),
    });
}

/// Derive non-error, troubleshooting-only hints from a completed proxied
/// request. Currently flags the "agent turn ended without a tool call" and
/// "empty upstream output" cases that make bridged clients (Codex, Claude Code)
/// look like they stopped responding. Heuristic text scan by design — this only
/// annotates the live log, it never drives control flow.
fn diagnostic_notes(
    success: bool,
    bridge: Option<&str>,
    client_request: Option<&str>,
    final_response: Option<&[u8]>,
) -> Vec<String> {
    if !success || bridge.is_none() {
        return Vec::new();
    }
    let Some(final_text) = final_response.and_then(|bytes| std::str::from_utf8(bytes).ok()) else {
        return Vec::new();
    };
    let completed = final_text.contains("\"type\":\"response.completed\"")
        || final_text.contains("\"status\":\"completed\"")
        || final_text.contains("\"stop_reason\"");
    if !completed {
        return Vec::new();
    }
    let mut notes = Vec::new();
    if final_text.contains("\"output\":[]") {
        notes.push("上游返回空输出（无文本/推理/工具调用）".to_string());
        return notes;
    }
    let has_tool_call = final_text.contains("\"type\":\"function_call\"")
        || final_text.contains("\"type\":\"tool_use\"")
        || final_text.contains("response.function_call_arguments");
    let request_offered_tools = client_request.is_some_and(|request| request.contains("\"tools\""));
    if request_offered_tools && !has_tool_call {
        notes.push("上游未发起工具调用（纯文本回合，agent 可能就此停止）".to_string());
    }
    notes
}

/// Above this many bytes, a system-prompt-shaped field is replaced with a short
/// `<field omitted: N chars>` marker in the live log preview.
const VERBOSE_REQUEST_FIELD_LIMIT: usize = 200;

/// The live log keeps a preview of every request stage, but agent clients ship a
/// huge system prompt on every call — Responses `instructions`, the Chat system
/// message it converts into, or Anthropic's `system`. That blob dwarfs the parts
/// worth reading (messages, tools) and eats the per-stage byte budget, so strip
/// it to a marker before storing. Non-JSON bodies (SSE, etc.) pass through
/// untouched.
fn redact_verbose_request_fields(body: Option<&[u8]>) -> Option<Vec<u8>> {
    let body = body?;
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return Some(body.to_vec());
    };
    let Some(object) = value.as_object_mut() else {
        return Some(body.to_vec());
    };
    let mut changed = false;
    // Responses `instructions` and Anthropic `system` are top-level strings.
    for key in ["instructions", "system"] {
        if let Some(field) = object.get_mut(key) {
            changed |= redact_long_string(field, key);
        }
    }
    // Chat Completions carries the system prompt as a system/developer message.
    if let Some(messages) = object.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            let is_system = message
                .get("role")
                .and_then(Value::as_str)
                .is_some_and(|role| matches!(role, "system" | "developer"));
            if is_system {
                if let Some(content) = message.get_mut("content") {
                    changed |= redact_long_string(content, "system");
                }
            }
        }
    }
    if !changed {
        return Some(body.to_vec());
    }
    serde_json::to_vec(&value)
        .ok()
        .or_else(|| Some(body.to_vec()))
}

/// Replace `value` with a `<label omitted: N chars>` marker when it is a string
/// longer than [`VERBOSE_REQUEST_FIELD_LIMIT`]. Returns whether it changed.
fn redact_long_string(value: &mut Value, label: &str) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    if text.len() <= VERBOSE_REQUEST_FIELD_LIMIT {
        return false;
    }
    let chars = text.chars().count();
    *value = Value::String(format!("<{label} omitted: {chars} chars>"));
    true
}

fn elapsed_millis(started_at: Instant) -> i64 {
    started_at.elapsed().as_millis().min(i64::MAX as u128) as i64
}

/// Header names whose values carry a credential and must never reach the live log.
///
/// Everything else is shown in full — the identity headers the proxy injects to
/// look like an official CLI (`user-agent`, `anthropic-beta`, `x-app`,
/// `x-stainless-*`, `originator`) are exactly what a gateway fingerprints on, so
/// masking them would defeat the purpose of logging headers at all.
const SENSITIVE_HEADER_NAMES: [&str; 5] = [
    "authorization",
    "x-api-key",
    "api-key",
    "x-goog-api-key",
    "x-xai-token-auth",
];

/// Query keys that carry a credential in the URL rather than a header.
const SENSITIVE_QUERY_KEYS: [&str; 2] = ["key", "api_key"];

/// Keep the shape of a secret without revealing it: a short value is fully
/// masked, a long one keeps its last four characters so two credentials can be
/// told apart in a log.
fn mask_secret(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let chars = trimmed.chars().count();
    if chars <= 8 {
        return "***".to_string();
    }
    let tail: String = trimmed.chars().skip(chars - 4).collect();
    format!("***{tail}")
}

/// Mask credential-bearing query values in a URL.
///
/// Gemini puts the API key in `?key=` (see `append_query_param` at the Gemini
/// dialect arm), so the target URL is itself a secret-bearing string and cannot
/// be logged verbatim.
fn redact_sensitive_url(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let redacted = query
        .split('&')
        .map(|part| match part.split_once('=') {
            Some((key, value))
                if SENSITIVE_QUERY_KEYS
                    .iter()
                    .any(|sensitive| key.eq_ignore_ascii_case(sensitive)) =>
            {
                format!("{key}={}", mask_secret(value))
            }
            _ => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{redacted}")
}

/// Render outbound headers as sorted `name: value` lines for the live log.
///
/// Sorted so two requests can be diffed by eye, which is the whole point when a
/// gateway accepts one request and rejects another with `unauthorized client
/// detected`.
fn format_upstream_headers(headers: &HeaderMap) -> String {
    let mut lines = headers
        .iter()
        .map(|(name, value)| {
            let name = name.as_str();
            let value = value.to_str().unwrap_or("<non-utf8>");
            if SENSITIVE_HEADER_NAMES
                .iter()
                .any(|sensitive| name.eq_ignore_ascii_case(sensitive))
            {
                format!("{name}: {}", mask_secret(value))
            } else {
                format!("{name}: {value}")
            }
        })
        .collect::<Vec<_>>();
    lines.sort();
    lines.join("\n")
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

/// Build the client response around a streaming body.
///
/// Same header handling as [`proxy_upstream_response`] — `is_hop_by_hop_header`
/// already drops `content-length` and `transfer-encoding`, so hyper is free to
/// frame the response as chunked.
fn proxy_upstream_stream_response(
    status: StatusCode,
    upstream_headers: HeaderMap,
    body: Body,
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
        .body(body)
        .map_err(|error| format!("Could not build proxy response: {error}"))
}

/// Whether this response can be streamed to the client rather than buffered.
///
/// Every condition here exists because some consumer of the buffered path needs
/// the complete body:
///
/// - a protocol bridge rewrites the body wholesale, and five of the seven do it
///   by aggregating the entire stream before re-emitting it;
/// - a non-streaming reply has no frames to inspect incrementally, and nothing
///   to gain — the client waits for one JSON document either way;
/// - a non-2xx body decides retry classification, which must happen before
///   anything reaches the client;
/// - custom tool restoration rewrites frames on the way out;
/// - official credentials parse the body for subscription/quota signals.
fn should_stream_upstream_response(
    bridge_kind: Option<ProtocolBridgeKind>,
    streaming_request: bool,
    status: StatusCode,
    custom_tool_names: &std::collections::HashSet<String>,
    credential: &SelectedCredential,
) -> bool {
    bridge_kind.is_none()
        && streaming_request
        && status.is_success()
        && custom_tool_names.is_empty()
        && credential.kind != "official"
}

/// The per-request values a streamed response still has to report once it ends.
struct StreamPrimeContext<'a> {
    attempt: usize,
    path: &'a str,
    target_url: &'a str,
    status: StatusCode,
    trace_id: Option<&'a str>,
    request_start: Instant,
    requested_model: Option<&'a str>,
    upstream_model: Option<&'a str>,
    /// Upstream model key this attempt is charged under, `None` when the request
    /// carried no model.
    model_key: Option<&'a str>,
    bridge_name: Option<&'a str>,
    client_request: &'a [u8],
    upstream_request: &'a [u8],
    upstream_headers: &'a HeaderMap,
}

/// Record a failure that happened before any byte reached the client.
///
/// Mirrors the buffered path's transport-failure arm: log the attempt, then
/// either queue the same account again or record the failure and let the caller
/// fall through to the next candidate.
#[allow(clippy::too_many_arguments)]
async fn handle_stream_prime_failure(
    state: &ProxyAppState,
    pool: &SqlitePool,
    platform: &str,
    credential: &SelectedCredential,
    error_message: &str,
    context: StreamPrimeContext<'_>,
    retry_same_credential: bool,
    failure_policy: RouteCredentialFailurePolicy,
    retry_queue: &mut VecDeque<(usize, usize)>,
    credential_index: usize,
    credential_retry_count: usize,
    retry_errors: &mut Vec<String>,
) {
    let metadata = route_proxy_request_metadata(
        platform,
        credential,
        context.path,
        Some(context.target_url),
        Some(context.status.as_u16()),
        false,
        context.trace_id,
        context.request_start,
        Some(error_message),
        context.requested_model,
        context.upstream_model,
        None,
    );
    let _ = insert_route_credential_request_event(
        pool,
        &credential.id,
        &metadata,
        &RouteUsageBreakdown::default(),
        None,
    )
    .await;
    emit_live_log(
        &state.live_log,
        platform,
        credential,
        context.attempt,
        context.path,
        Some(context.target_url),
        Some(context.upstream_headers),
        Some(context.status.as_u16()),
        false,
        context.trace_id,
        context.request_start,
        Some(error_message),
        context.requested_model,
        context.upstream_model,
        context.bridge_name,
        Some(context.client_request),
        Some(context.upstream_request),
        None,
        None,
    );
    if retry_same_credential {
        wait_for_credential_retry(failure_policy).await;
        retry_queue.push_front((credential_index, credential_retry_count + 1));
    } else {
        record_route_credential_failure(
            &state.activity,
            platform,
            pool,
            credential,
            context.model_key,
            "transport",
            error_message,
            None,
        )
        .await;
        retry_errors.push(error_message.to_string());
    }
}

/// Everything needed to close the books on a streamed response.
///
/// Carries the request's identity plus the observer accumulating what passed
/// through, and settles usage, the request log and account health when the
/// stream ends.
struct StreamCompletion {
    state: ProxyAppState,
    credential: SelectedCredential,
    platform: String,
    attempt: usize,
    path: String,
    target_url: String,
    status: StatusCode,
    trace_id: Option<String>,
    request_start: Instant,
    requested_model: Option<String>,
    upstream_model: Option<String>,
    /// Upstream model key this response is charged under, `None` when the request
    /// carried no model.
    model_key: Option<String>,
    bridge_name: Option<String>,
    client_request: axum::body::Bytes,
    upstream_request: Vec<u8>,
    upstream_headers: HeaderMap,
    observer: StreamObserver,
    _activity_lease: RouteCredentialActivityLease,
}

impl StreamCompletion {
    /// Persist usage, log the request, and update account health.
    ///
    /// Runs whether the stream ended on its own or the client hung up, so a
    /// half-read response is still accounted for rather than silently dropped.
    async fn finish(self) {
        let StreamCompletion {
            state,
            credential,
            platform,
            attempt,
            path,
            target_url,
            status,
            trace_id,
            request_start,
            requested_model,
            upstream_model,
            model_key,
            bridge_name,
            client_request,
            upstream_request,
            upstream_headers,
            observer,
            _activity_lease,
        } = self;

        let preview = observer.preview().to_vec();
        let outcome = observer.finish();
        let truncated = outcome.disconnected_before_completion;
        let error_message = truncated.then(|| STREAM_DISCONNECTED_FAILURE_MESSAGE.to_string());

        let mut usage = outcome.usage;
        let priced_model = outcome
            .response_model
            .or_else(|| upstream_model.clone())
            .or_else(|| requested_model.clone());
        apply_estimated_price(&mut usage, priced_model.as_deref());

        let metadata = route_proxy_request_metadata(
            &platform,
            &credential,
            &path,
            Some(&target_url),
            Some(status.as_u16()),
            !truncated,
            trace_id.as_deref(),
            request_start,
            error_message.as_deref(),
            requested_model.as_deref(),
            upstream_model.as_deref(),
            Some(&preview),
        );
        let _ = insert_route_credential_request_event(
            &state.pool,
            &credential.id,
            &metadata,
            &usage,
            crate::services::upstream_response_id::extract_upstream_response_id(&preview)
                .as_deref(),
        )
        .await;
        emit_live_log(
            &state.live_log,
            &platform,
            &credential,
            attempt,
            &path,
            Some(&target_url),
            Some(&upstream_headers),
            Some(status.as_u16()),
            !truncated,
            trace_id.as_deref(),
            request_start,
            error_message.as_deref(),
            requested_model.as_deref(),
            upstream_model.as_deref(),
            bridge_name.as_deref(),
            Some(&client_request),
            Some(&upstream_request),
            Some(&preview),
            Some(&preview),
        );

        if truncated {
            // The bytes are already with the client, so this cannot become a
            // retry. Recording it still matters: the account's backoff and
            // health state are what stop the pool from picking a chronically
            // truncating upstream first next time.
            record_route_credential_failure(
                &state.activity,
                &platform,
                &state.pool,
                &credential,
                model_key.as_deref(),
                "semantic_response_transient",
                STREAM_DISCONNECTED_FAILURE_MESSAGE,
                Some(&preview),
            )
            .await;
            return;
        }

        if RouteCredentialRepository::clear_transient_failure(
            &state.pool,
            &credential.id,
            model_key.as_deref(),
        )
        .await
        .is_ok()
        {
            state
                .activity
                .notify_status_change(&platform, &credential.id);
        }
    }
}

/// Wrap the upstream stream so every chunk is observed on its way to the client.
///
/// The completion handle is moved into the stream, so it is dropped — and the
/// books closed — when the stream ends or the client disconnects.
fn observed_upstream_stream(
    first_chunk: axum::body::Bytes,
    rest: impl futures_util::Stream<Item = reqwest::Result<axum::body::Bytes>> + Send + 'static,
    completion: StreamCompletion,
) -> impl futures_util::Stream<Item = Result<axum::body::Bytes, std::io::Error>> + Send + 'static {
    let replayed = futures_util::StreamExt::chain(
        futures_util::stream::once(async move { Ok(first_chunk) }),
        rest,
    );
    let guard = StreamCompletionGuard {
        completion: Some(completion),
    };
    futures_util::stream::unfold(
        (Box::pin(replayed), guard),
        |(mut stream, mut guard)| async move {
            match futures_util::StreamExt::next(&mut stream).await {
                Some(Ok(chunk)) => {
                    if let Some(completion) = guard.completion.as_mut() {
                        completion.observer.observe(&chunk);
                    }
                    Some((Ok(chunk), (stream, guard)))
                }
                Some(Err(error)) => {
                    // Upstream died mid-body. The client already has the earlier
                    // bytes, so this can only end the stream — but the partial
                    // response still gets accounted for.
                    if let Some(completion) = guard.completion.take() {
                        completion.finish().await;
                    }
                    Some((
                        Err(std::io::Error::other(format!(
                            "upstream stream failed: {error}"
                        ))),
                        (stream, guard),
                    ))
                }
                None => {
                    if let Some(completion) = guard.completion.take() {
                        completion.finish().await;
                    }
                    None
                }
            }
        },
    )
}

/// Ensures a streamed response is always accounted for.
///
/// The stream is dropped without being polled to completion whenever the client
/// hangs up early, which would otherwise lose the request from usage stats and
/// leave the account's last-failure state stale. `Drop` cannot await, so the
/// remaining work is handed to a task.
struct StreamCompletionGuard {
    completion: Option<StreamCompletion>,
}

impl Drop for StreamCompletionGuard {
    fn drop(&mut self) {
        let Some(completion) = self.completion.take() else {
            return;
        };
        // Only reachable when the stream never finished, i.e. the client went
        // away. Spawning needs a runtime; during shutdown there is none, and
        // dropping the handle is the right outcome then.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { completion.finish().await });
        }
    }
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
            | StatusCode::TOO_MANY_REQUESTS
    ) || status.is_server_error()
}

pub(crate) fn should_retry_same_credential_status(status: StatusCode) -> bool {
    !status.is_success() && !matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
}

/// Account-level retry eligibility. `partition_by_cooldown` supersedes this for
/// selection — it has to, because it also weighs the requested model — so this
/// now only pins down the account-level rule the partition inherits.
#[cfg_attr(not(test), allow(dead_code))]
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

/// Record a failure whose scope can be decided from `kind` alone.
///
/// The account's mapped models are only parsed when the failure could be
/// model-scoped, so the common account-wide case stays as cheap as before.
async fn record_route_credential_failure(
    activity: &RouteCredentialActivityRegistry,
    platform: &str,
    pool: &SqlitePool,
    credential: &SelectedCredential,
    model_key: Option<&str>,
    kind: &str,
    message: &str,
    response_body: Option<&[u8]>,
) {
    record_route_credential_failure_with_status(
        activity,
        platform,
        pool,
        credential,
        model_key,
        kind,
        None,
        message,
        response_body,
    )
    .await;
}

/// Same, for failures whose scope also depends on the upstream status code
/// (`upstream_status` / `model_test_status`: 401/403 condemn the key, anything
/// else is a verdict on one model).
#[allow(clippy::too_many_arguments)]
async fn record_route_credential_failure_with_status(
    activity: &RouteCredentialActivityRegistry,
    platform: &str,
    pool: &SqlitePool,
    credential: &SelectedCredential,
    model_key: Option<&str>,
    kind: &str,
    status: Option<u16>,
    message: &str,
    response_body: Option<&[u8]>,
) {
    // Without a model name there is nothing to charge but the account — Gemini
    // keeps its model in the path, and some routes carry none at all.
    let siblings = (!is_account_scoped_failure(kind, status))
        .then(|| {
            model_key.map(|_| {
                let capability = parse_model_capability(&credential.config_json);
                known_upstream_models(platform, &capability, &credential.kind)
            })
        })
        .flatten();
    let scope = match (model_key, siblings.as_deref()) {
        (Some(key), Some(siblings)) => FailureScope::Model { key, siblings },
        _ => FailureScope::Account,
    };
    if RouteCredentialRepository::record_transient_failure(
        pool,
        &credential.id,
        kind,
        message,
        response_body,
        scope,
    )
    .await
    .is_ok()
    {
        activity.notify_status_change(platform, &credential.id);
    }
}

async fn mark_route_credential_revoked(
    activity: &RouteCredentialActivityRegistry,
    platform: &str,
    pool: &SqlitePool,
    credential_id: &str,
) {
    if RouteCredentialRepository::update_status(pool, credential_id, "revoked")
        .await
        .is_ok()
    {
        activity.notify_status_change(platform, credential_id);
    }
}

async fn mark_route_credential_error(
    activity: &RouteCredentialActivityRegistry,
    platform: &str,
    pool: &SqlitePool,
    credential_id: &str,
) {
    if RouteCredentialRepository::update_status(pool, credential_id, "error")
        .await
        .is_ok()
    {
        activity.notify_status_change(platform, credential_id);
    }
}

fn json_error(status: StatusCode, message: &str) -> Response {
    let key_invalid = message.contains("route_proxy.key_invalid");
    let platform_unresolved = message.contains("route_proxy.platform_unresolved");
    let code = if key_invalid {
        "route_proxy.key_invalid"
    } else if platform_unresolved {
        "route_proxy.auth_required"
    } else if message.contains("No enabled route credentials in pool") {
        "route_pool.empty"
    } else if message.contains("route_pool.model_unmatched") {
        "route_pool.model_unmatched"
    } else if message.contains("route_pool.model_unavailable") {
        // Every account that could serve this model is paused, marked unhealthy,
        // or still cooling. Distinct from `model_unmatched`: the mapping exists,
        // it is just unusable right now.
        "route_pool.model_unavailable"
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
    if key_invalid || platform_unresolved {
        response.headers_mut().insert(
            axum::http::header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer"),
        );
    }
    response
}

fn route_proxy_error_status(message: &str) -> StatusCode {
    if message.contains("route_proxy.platform_unresolved")
        || message.contains("route_proxy.key_invalid")
    {
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

    if inbound_key.is_some() {
        return Err(AppError::Validation {
            code: "route_proxy.key_invalid",
            message: "route_proxy.key_invalid: Local route proxy key is invalid, expired, or belongs to another ai-switch instance; provide a valid local route proxy key with Authorization: Bearer, x-api-key, or x-ai-switch-platform".to_string(),
            details: None,
            recoverable: true,
        });
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
    pub route_priority: i64,
    pub max_concurrency: i64,
    pub secret_payload_json: String,
    pub config_json: String,
}

/// A pool row plus the state needed to decide whether it may serve *this*
/// request. `model_key` is filled in by `filter_candidates_for_model` and stays
/// `None` when the request carries no model (Gemini puts it in the path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolCandidate {
    pub credential: SelectedCredential,
    pub cooldown_until: Option<String>,
    pub model_key: Option<String>,
}

/// Load the pool rows a request could use: SQL-level filters and quota only.
/// Cooldown partitioning deliberately happens later — which rows count as
/// cooling depends on the requested model, which is not known here.
pub async fn load_pool_candidates(
    pool: &SqlitePool,
    platform: &str,
) -> Result<Vec<PoolCandidate>, AppError> {
    let rows = sqlx::query(
        "SELECT c.id, c.platform, c.kind, c.display_name, c.status,
                c.route_priority, c.max_concurrency,
                c.secret_payload_json, c.config_json,
                c.next_retry_at, c.cooldown_until
         FROM route_pool_members rpm
         INNER JOIN route_credentials c ON c.id = rpm.route_credential_id
         WHERE rpm.platform = ?
           AND rpm.enabled = 1
           AND c.archived_at IS NULL
           AND c.status = 'ok'
           AND (c.primary_remain IS NULL OR c.primary_remain > 0)
           AND (c.weekly_remain IS NULL OR c.weekly_remain > 0)
         ORDER BY c.route_priority ASC, rpm.sort_order ASC, rpm.created_at ASC",
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

    let mut candidates = Vec::with_capacity(rows.len());
    for row in rows {
        let next_retry_at: Option<String> = row.get("next_retry_at");
        let cooldown_until: Option<String> = row.get("cooldown_until");
        let credential = SelectedCredential {
            id: row.get("id"),
            platform: row.get("platform"),
            kind: row.get("kind"),
            display_name: row.get("display_name"),
            status: row.get("status"),
            route_priority: row.get("route_priority"),
            max_concurrency: row.get("max_concurrency"),
            secret_payload_json: row.get("secret_payload_json"),
            config_json: row.get("config_json"),
        };
        // Skip official accounts already known to have zero remaining quota.
        if !is_route_credential_quota_available(&credential.config_json) {
            continue;
        }
        candidates.push(PoolCandidate {
            credential,
            // The two account columns are always written the same value, so the
            // later of them is the single deadline that matters.
            cooldown_until: latest_deadline(next_retry_at.as_deref(), cooldown_until.as_deref()),
            model_key: None,
        });
    }
    Ok(candidates)
}

fn latest_deadline(left: Option<&str>, right: Option<&str>) -> Option<String> {
    [left, right]
        .into_iter()
        .flatten()
        .filter_map(|value| {
            DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|parsed| (parsed.with_timezone(&Utc), value.to_string()))
        })
        .max_by_key(|(parsed, _)| *parsed)
        .map(|(_, value)| value)
}

/// Batch-read the per-model state for candidates that carry a model key.
///
/// One query for the whole pool rather than one per account: the forward path
/// runs this on every request, so the round trip count has to stay flat.
pub(crate) async fn load_candidate_model_states(
    pool: &SqlitePool,
    candidates: &[PoolCandidate],
) -> Result<HashMap<(String, String), RouteCredentialModelState>, AppError> {
    let keys: Vec<(String, String)> = candidates
        .iter()
        .filter_map(|candidate| {
            Some((
                candidate.credential.id.clone(),
                candidate.model_key.clone()?,
            ))
        })
        .collect();
    RouteCredentialModelRepository::load_states(pool, &keys).await
}

/// Split candidates into "may serve now" and "still cooling", then hand back the
/// usable set.
///
/// Order matters: `paused`/`error` models are dropped outright, because those are
/// verdicts rather than waits and must never be reached by the all-cooling probe
/// below. Only time-based cooldowns get that second chance — otherwise a pool
/// where everything is briefly cooling would fail requests it could still serve.
pub fn partition_by_cooldown(
    candidates: Vec<PoolCandidate>,
    model_states: &HashMap<(String, String), RouteCredentialModelState>,
    now: DateTime<Utc>,
) -> Vec<SelectedCredential> {
    let mut eligible = Vec::new();
    let mut cooling: Vec<(DateTime<Utc>, usize, SelectedCredential)> = Vec::new();

    for candidate in candidates {
        let state = candidate.model_key.as_ref().and_then(|model_key| {
            model_states.get(&(candidate.credential.id.clone(), model_key.clone()))
        });
        if state.is_some_and(|state| state.status != MODEL_STATUS_OK) {
            continue;
        }
        let model_cooldown = state.and_then(|state| state.cooldown_until.clone());
        let deadline = latest_deadline(
            candidate.cooldown_until.as_deref(),
            model_cooldown.as_deref(),
        )
        .and_then(|value| {
            DateTime::parse_from_rfc3339(&value)
                .ok()
                .map(|parsed| parsed.with_timezone(&Utc))
        });

        match deadline {
            Some(deadline) if deadline > now => {
                cooling.push((deadline, cooling.len(), candidate.credential));
            }
            _ => eligible.push(candidate.credential),
        }
    }

    if !eligible.is_empty() {
        return eligible;
    }

    cooling.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    cooling
        .into_iter()
        .take(1)
        .map(|(_, _, credential)| credential)
        .collect()
}

pub async fn select_pool_credentials(
    pool: &SqlitePool,
    platform: &str,
) -> Result<Vec<SelectedCredential>, AppError> {
    let candidates = load_pool_candidates(pool, platform).await?;
    Ok(partition_by_cooldown(
        candidates,
        &HashMap::new(),
        Utc::now(),
    ))
}

fn filter_candidates_for_rule(
    mut candidates: Vec<PoolCandidate>,
    rule: &CapabilityRule,
) -> Vec<PoolCandidate> {
    if !rule.credential_kinds.is_empty() {
        candidates.retain(|candidate| {
            rule.credential_kinds
                .iter()
                .any(|kind| kind == &candidate.credential.kind)
        });
    }
    candidates
}

/// Drop candidates that cannot serve the requested model, and record the model
/// key the survivors will be charged under. Both happen in one pass because both
/// need the same parsed capability.
fn filter_candidates_for_model(
    platform: &str,
    candidates: Vec<PoolCandidate>,
    requested_model: Option<&str>,
) -> Vec<PoolCandidate> {
    let Some(requested_model) = requested_model else {
        return candidates;
    };

    candidates
        .into_iter()
        .filter_map(|mut candidate| {
            let mut capability = parse_model_capability(&candidate.credential.config_json);
            if candidate.credential.kind == "official" {
                // build_official_upstream_request never applies model mappings, so a
                // synthetic alias would reach the vendor verbatim and 404. Ignoring
                // those entries here keeps official accounts on exactly their
                // pre-feature semantics (an alias-only config collapses to the
                // baseline-only wildcard).
                capability
                    .mappings
                    .retain(|mapping| !is_synthetic_route_alias(&mapping.from));
            }
            if !supports_requested_model(platform, &capability, Some(requested_model)) {
                return None;
            }
            candidate.model_key = Some(model_state_key(
                platform,
                &capability,
                &candidate.credential.kind,
                requested_model,
            ));
            Some(candidate)
        })
        .collect()
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

pub fn credential_indexes_by_priority(
    credentials: &[SelectedCredential],
    cursor: i64,
) -> Vec<usize> {
    let mut groups = BTreeMap::<i64, Vec<usize>>::new();
    for (index, credential) in credentials.iter().enumerate() {
        groups
            .entry(credential.route_priority)
            .or_default()
            .push(index);
    }

    groups
        .into_values()
        .flat_map(|indexes| {
            retry_credential_indexes(indexes.len(), cursor)
                .into_iter()
                .map(move |index| indexes[index])
        })
        .collect()
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
                if let Some(target) = resolve_mapping_target(mappings, &model) {
                    object.insert("model".to_string(), Value::String(target.to_string()));
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

/// Max remote images inlined per request, and max bytes per image.
const INLINE_IMAGE_MAX_COUNT: usize = 16;
const INLINE_IMAGE_MAX_BYTES: usize = 16 * 1024 * 1024;

/// Whether this credential opts into inlining remote image URLs (fallback for
/// upstreams that fetch `image_url` links and reject non-`image/*` responses,
/// e.g. OSS objects served as `text/plain`).
fn selected_credential_inlines_images(credential: &SelectedCredential) -> bool {
    parse_json_object(&credential.config_json, "config")
        .ok()
        .is_some_and(|config| inline_remote_images_enabled(&config))
}

fn inline_remote_images_enabled(config: &Value) -> bool {
    config
        .get("inline_remote_images")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn is_remote_http_url(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("http://") || value.starts_with("https://")
}

/// Fetch every remote `image_url` in the request body and replace it with a
/// base64 `data:` URL carrying the sniffed image MIME. Best-effort: images that
/// cannot be fetched or identified are left untouched.
async fn inline_remote_image_urls_in_body(body: &[u8], client: &reqwest::Client) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let mut urls: Vec<String> = Vec::new();
    collect_remote_image_urls(&value, &mut urls);
    urls.sort();
    urls.dedup();
    urls.truncate(INLINE_IMAGE_MAX_COUNT);
    if urls.is_empty() {
        return body.to_vec();
    }
    let mut replacements: HashMap<String, String> = HashMap::new();
    for url in urls {
        if let Some(data_url) = fetch_image_as_data_url(client, &url).await {
            replacements.insert(url, data_url);
        }
    }
    if replacements.is_empty() {
        return body.to_vec();
    }
    replace_remote_image_urls(&mut value, &replacements);
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

fn collect_remote_image_urls(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            match map.get("image_url") {
                Some(Value::String(url)) if is_remote_http_url(url) => {
                    out.push(url.trim().to_string());
                }
                Some(Value::Object(inner)) => {
                    if let Some(Value::String(url)) = inner.get("url") {
                        if is_remote_http_url(url) {
                            out.push(url.trim().to_string());
                        }
                    }
                }
                _ => {}
            }
            for child in map.values() {
                collect_remote_image_urls(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_remote_image_urls(child, out);
            }
        }
        _ => {}
    }
}

fn replace_remote_image_urls(value: &mut Value, replacements: &HashMap<String, String>) {
    if let Value::Object(map) = value {
        match map.get_mut("image_url") {
            Some(Value::String(url)) => {
                if let Some(data_url) = replacements.get(url.trim()) {
                    *url = data_url.clone();
                }
            }
            Some(Value::Object(inner)) => {
                if let Some(Value::String(url)) = inner.get_mut("url") {
                    if let Some(data_url) = replacements.get(url.trim()) {
                        *url = data_url.clone();
                    }
                }
            }
            _ => {}
        }
        for child in map.values_mut() {
            replace_remote_image_urls(child, replacements);
        }
    } else if let Value::Array(items) = value {
        for child in items {
            replace_remote_image_urls(child, replacements);
        }
    }
}

async fn fetch_image_as_data_url(client: &reqwest::Client, url: &str) -> Option<String> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    if let Some(length) = response.content_length() {
        if length as usize > INLINE_IMAGE_MAX_BYTES {
            return None;
        }
    }
    let header_mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or(value)
                .trim()
                .to_ascii_lowercase()
        });
    let bytes = response.bytes().await.ok()?;
    if bytes.is_empty() || bytes.len() > INLINE_IMAGE_MAX_BYTES {
        return None;
    }
    let mime = sniff_image_mime(&bytes)
        .or_else(|| header_mime.filter(|mime| mime.starts_with("image/")))
        .or_else(|| image_mime_from_url(url))?;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine as _;
    Some(format!(
        "data:{mime};base64,{}",
        BASE64_STANDARD.encode(&bytes)
    ))
}

fn sniff_image_mime(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Some("image/png".to_string());
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg".to_string());
    }
    if bytes.starts_with(b"GIF8") {
        return Some("image/gif".to_string());
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp".to_string());
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp".to_string());
    }
    None
}

fn image_mime_from_url(url: &str) -> Option<String> {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    let extension = path.rsplit('.').next()?;
    let mime = match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    Some(mime.to_string())
}

pub fn build_upstream_request(
    credential: &SelectedCredential,
    platform: &str,
    path: &str,
    query: Option<&str>,
    headers: HeaderMap,
    body: &[u8],
) -> Result<(String, HeaderMap, Vec<u8>), String> {
    let request = build_upstream_request_internal(
        credential,
        platform,
        path,
        query,
        headers,
        body,
        None,
        TurnReminderMode::Apply,
    )?;
    Ok((request.target_url, request.headers, request.body))
}

/// Whether a request carries the account's per-turn reminder.
///
/// The connectivity probe must opt out. It asks the model to reply with exactly
/// `ai-switch-ok`, which a reminder like "answer in Chinese" contradicts head-on:
/// the model may follow the reminder and stop emitting the token, so every probe
/// against a reminder-enabled account would fail permanently. Spelled as an enum
/// rather than a bool so the intent is legible at each call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TurnReminderMode {
    Apply,
    Skip,
}

pub(crate) fn build_upstream_request_with_bridge(
    credential: &SelectedCredential,
    platform: &str,
    path: &str,
    query: Option<&str>,
    headers: HeaderMap,
    body: &[u8],
    turn_reminder: TurnReminderMode,
) -> Result<BuiltUpstreamRequest, String> {
    build_upstream_request_internal(
        credential,
        platform,
        path,
        query,
        headers,
        body,
        None,
        turn_reminder,
    )
}

fn build_upstream_request_internal(
    credential: &SelectedCredential,
    platform: &str,
    path: &str,
    query: Option<&str>,
    mut headers: HeaderMap,
    body: &[u8],
    codex_history: Option<&CodexReasoningCache>,
    turn_reminder: TurnReminderMode,
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
            codex_history,
            turn_reminder,
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
    codex_history: Option<&CodexReasoningCache>,
    turn_reminder: TurnReminderMode,
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
        // Third-party Responses gateways (Xiaomi MiMo, …) reject OpenAI-hosted
        // tool types with `responses_feature_not_supported`; drop them so the
        // rest of the request still goes through.
        rewritten_body = strip_unsupported_hosted_tools(&rewritten_body);
    }
    // Restore the real reasoning_content (and, if the client dropped it, the
    // whole function_call) onto tool-call turns before the Responses→Chat
    // conversion. Chat reasoning providers (DeepSeek/MiMo) otherwise lose the
    // model's plan across tool calls and stall. See [`CodexReasoningCache`].
    if bridge_requires_custom_tool_compat {
        if let Some(cache) = codex_history {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&rewritten_body) {
                if cache.enrich_responses_request(&mut value) > 0 {
                    if let Ok(bytes) = serde_json::to_vec(&value) {
                        rewritten_body = bytes;
                    }
                }
            }
        }
    }
    let PreparedBridgeRequest {
        kind: bridge_kind,
        upstream_path,
        upstream_query,
        body: rewritten_body,
        tool_namespaces,
        ..
    } = prepare_protocol_bridge_request(platform, dialect, &upstream_path, &rewritten_body)?;
    // Only now is the body in its final upstream schema — bridging may have
    // converted a Responses request into `messages` or `contents`, so a reminder
    // written any earlier would land in the wrong shape. Everything below this
    // point touches headers and the URL only.
    let mut rewritten_body = rewritten_body;
    if turn_reminder == TurnReminderMode::Apply {
        if let Some(reminder) = turn_reminder_text(config) {
            rewritten_body =
                turn_reminder::append_turn_reminder(dialect, &rewritten_body, &reminder);
        }
    }
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
            // Impersonate Claude Code so client-fingerprinting gateways
            // (e.g. agentrouter.org) don't reject us as an unknown client.
            apply_claude_code_identity(headers);
            // Read the 1M intent from the *client's* model value: the `[1M]`
            // suffix is stripped by the mapping lookup before the upstream body
            // is built, so by this point only the original request still carries
            // it. Without the beta marker the gateway answers "please enable 1m
            // context and retry" no matter what the model name said.
            if client_requested_one_m_context(body) {
                apply_one_m_context_beta(headers);
            }
            if is_messages_path(&upstream_path) {
                target_url = ensure_query_flag(&target_url, "beta", "true");
            }
        }
        ApiDialect::Gemini => {
            target_url = append_query_param(&target_url, "key", api_key);
        }
        ApiDialect::OpenAi | ApiDialect::OpenAiResponses => {
            insert_header(headers, "authorization", &format!("Bearer {api_key}"))?;
            // Impersonate the Codex CLI for gateways that fingerprint clients.
            apply_codex_cli_identity(headers);
        }
    }

    apply_credential_user_agent(headers, config)?;
    let streaming_request = request_body_requests_stream(&rewritten_body);
    Ok(BuiltUpstreamRequest {
        target_url,
        headers: headers.clone(),
        body: rewritten_body,
        bridge_kind,
        tool_namespaces,
        streaming_request,
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
        tool_namespaces: BTreeMap::new(),
        streaming_request: request_body_requests_stream(body),
    })
}

fn is_grok_cli_chat_proxy_base_url(base_url: &str) -> bool {
    base_url
        .to_ascii_lowercase()
        .contains(GROK_CLI_CHAT_PROXY_MARKER)
}

fn request_body_requests_stream(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
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

pub(crate) fn credential_user_agent(config: &Value) -> Option<&str> {
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
    activity: Option<&RouteCredentialActivityRegistry>,
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
                    if let Some(activity) = activity {
                        mark_route_credential_revoked(
                            activity,
                            &credential.platform,
                            pool,
                            &credential.id,
                        )
                        .await;
                    } else {
                        let _ = RouteCredentialRepository::update_status(
                            pool,
                            &credential.id,
                            "revoked",
                        )
                        .await;
                    }
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

/// The reminder this account appends to every turn, or `None` when off.
///
/// Two keys: `turn_reminder` switches it on, `turn_reminder_text` optionally
/// overrides the wording. A blank or absent text falls back to the default, so
/// ticking the box alone is enough to get a working reminder.
fn turn_reminder_text(config: &Value) -> Option<String> {
    if !config
        .get("turn_reminder")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let text = config
        .get("turn_reminder_text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or(turn_reminder::DEFAULT_TURN_REMINDER);
    Some(text.to_string())
}

/// Tool `type`s only OpenAI's own Responses backend can execute. Third-party
/// Responses-compatible gateways reject them (e.g. Xiaomi MiMo returns
/// `responses_feature_not_supported: tool type 'web_search' is not supported`),
/// so they are stripped from the passthrough request.
const UNSUPPORTED_HOSTED_TOOL_TYPES: &[&str] = &[
    "web_search",
    "web_search_preview",
    "file_search",
    "computer_use_preview",
    "code_interpreter",
    "image_generation",
    "container_file_citation",
];

/// Remove OpenAI-hosted tools a limited Responses gateway can't run, recursing
/// into `namespace` tool groups. Non-JSON / no-tools bodies pass through. When a
/// pinned `tool_choice` targets a removed tool, it is relaxed to `"auto"`.
fn strip_unsupported_hosted_tools(body: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(object) = value.as_object_mut() else {
        return body.to_vec();
    };
    let changed = match object.get_mut("tools").and_then(Value::as_array_mut) {
        Some(tools) => filter_hosted_tools(tools),
        None => false,
    };
    if !changed {
        return body.to_vec();
    }
    if object
        .get("tool_choice")
        .is_some_and(tool_choice_targets_hosted)
    {
        object.insert("tool_choice".to_string(), Value::String("auto".to_string()));
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

fn filter_hosted_tools(tools: &mut Vec<Value>) -> bool {
    let before = tools.len();
    tools.retain(|tool| !is_unsupported_hosted_tool(tool));
    let mut changed = tools.len() != before;
    for tool in tools.iter_mut() {
        if tool.get("type").and_then(Value::as_str) == Some("namespace") {
            if let Some(nested) = tool.get_mut("tools").and_then(Value::as_array_mut) {
                changed |= filter_hosted_tools(nested);
            }
        }
    }
    changed
}

fn is_unsupported_hosted_tool(tool: &Value) -> bool {
    tool.get("type")
        .and_then(Value::as_str)
        .is_some_and(|tool_type| UNSUPPORTED_HOSTED_TOOL_TYPES.contains(&tool_type))
}

fn tool_choice_targets_hosted(tool_choice: &Value) -> bool {
    tool_choice
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|tool_type| UNSUPPORTED_HOSTED_TOOL_TYPES.contains(&tool_type))
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
    let data: Vec<Value> = advertised_model_catalog_entries(platform, &capabilities)
        .into_iter()
        .map(|entry| {
            let mut model = json!({
                "id": entry.id,
                "object": "model",
                "created": created,
                "owned_by": "ai-switch",
            });
            if platform.eq_ignore_ascii_case("codex") {
                let (supported_reasoning_levels, default_reasoning_level) =
                    codex_reasoning_metadata(&entry.id, entry.reasoning_levels.as_deref());
                if let Some(object) = model.as_object_mut() {
                    object.insert(
                        "supported_reasoning_levels".to_string(),
                        Value::Array(supported_reasoning_levels),
                    );
                    object.insert(
                        "default_reasoning_level".to_string(),
                        Value::String(default_reasoning_level),
                    );
                    // Always stated, declared or not: the default depends on the
                    // upstream model this alias points at, which the client has
                    // no way to work out on its own.
                    object.insert(
                        "context_window".to_string(),
                        json!(codex_effective_context_window(
                            entry.context_window,
                            &entry.upstream_model
                        )),
                    );
                }
            }
            model
        })
        .collect();

    json!({
        "object": "list",
        "data": data,
    })
}

fn build_route_models_list_payload(platform: &str, credentials: &[SelectedCredential]) -> Value {
    build_models_list_payload(platform, credentials)
}

fn json_models_list_response(
    platform: &str,
    credentials: &[SelectedCredential],
    _query: Option<&str>,
) -> Response {
    let payload = build_route_models_list_payload(platform, credentials);
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        payload.to_string(),
    )
        .into_response()
}

fn json_count_tokens_response(body: &[u8]) -> Response {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        json!({ "input_tokens": estimate_anthropic_input_tokens(body) }).to_string(),
    )
        .into_response()
}

/// Rough token estimate over the text content of an Anthropic request: counts
/// `system`, every message's text, and tool schemas at ~4 characters per token.
fn estimate_anthropic_input_tokens(body: &[u8]) -> i64 {
    const CHARS_PER_TOKEN: i64 = 4;

    fn text_len(value: &Value) -> i64 {
        match value {
            Value::String(text) => text.chars().count() as i64,
            Value::Array(items) => items.iter().map(text_len).sum(),
            Value::Object(fields) => fields
                .iter()
                .map(|(key, nested)| match key.as_str() {
                    // Base64 payloads are not text; skip them rather than
                    // inflating the estimate by megabytes of encoded bytes.
                    "data" if nested.is_string() => 0,
                    _ => text_len(nested),
                })
                .sum(),
            _ => 0,
        }
    }

    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return 0;
    };
    let chars: i64 = ["system", "messages", "tools", "tool_choice"]
        .iter()
        .filter_map(|key| value.get(*key))
        .map(text_len)
        .sum();
    // Always report at least 1 token for a non-empty request so clients do not
    // read the estimate as "no context".
    (chars / CHARS_PER_TOKEN).max(i64::from(chars > 0))
}

fn insert_header(headers: &mut HeaderMap, name: &'static str, value: &str) -> Result<(), String> {
    let value =
        HeaderValue::from_str(value).map_err(|err| format!("Invalid header value: {err}"))?;
    headers.insert(HeaderName::from_static(name), value);
    Ok(())
}

/// Insert a header only when the client did not already provide it, so a real
/// CLI's own values still win when routing through the proxy.
fn fill_header_if_absent(headers: &mut HeaderMap, name: &'static str, value: &str) {
    let Ok(value) = HeaderValue::from_str(value) else {
        return;
    };
    headers
        .entry(HeaderName::from_static(name))
        .or_insert(value);
}

/// True when the request already carries an Anthropic SDK identity.
///
/// Anthropic's SDK is Stainless-generated and tags every request with
/// `x-stainless-*`. Their presence means the caller is a genuine SDK client —
/// Claude Code among them — already carrying a complete, self-consistent identity.
fn has_stainless_sdk_identity(headers: &HeaderMap) -> bool {
    headers
        .keys()
        .any(|name| name.as_str().starts_with("x-stainless-"))
}

/// Make an Anthropic-dialect upstream request look like Claude Code: guarantee
/// the `claude-code-*` beta marker, and own the CLI identity headers whenever the
/// caller does not already have an SDK identity of its own.
///
/// Identity is applied all-or-nothing. Filling only the gaps used to splice our
/// hardcoded versions into a real client's set — a request would claim
/// `x-stainless-package-version: 0.70.0` from us alongside the caller's newer
/// runtime and `user-agent`, a combination no real client emits. A gateway that
/// fingerprints clients (agentrouter.org rejects with `unauthorized client
/// detected`) sees that as more suspicious than either identity alone, which is
/// why routing through the pool failed where a direct per-account test — starting
/// from an empty header map, so every value came from us — succeeded.
fn apply_claude_code_identity(headers: &mut HeaderMap) {
    // Merged either way: the beta marker is a capability signal rather than part
    // of the identity set, and gateways gate on it. A plain SDK caller that omits
    // it still ends up in Claude Code's shape, since Claude Code is itself an SDK
    // client carrying this marker.
    let existing_beta = headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok());
    if let Some(merged) = client_identity::merge_claude_code_beta(existing_beta) {
        if let Ok(value) = HeaderValue::from_str(&merged) {
            headers.insert(HeaderName::from_static("anthropic-beta"), value);
        }
    }

    // The caller brought its own coherent identity; adding ours would only
    // contradict it.
    if has_stainless_sdk_identity(headers) {
        return;
    }

    for (name, value) in client_identity::claude_code_identity_headers() {
        let Ok(value) = HeaderValue::from_str(value) else {
            continue;
        };
        headers.insert(HeaderName::from_static(name), value);
    }
}

/// True when the client asked for the 1M context window.
///
/// Claude Code signals this by appending `[1M]` to the model value it sends. The
/// mapping lookup strips that suffix (so `claude-opus-alias[1m]` still resolves
/// to the account's upstream model), which means the intent has to be read from
/// the inbound body before any rewriting.
fn client_requested_one_m_context(body: &[u8]) -> bool {
    requested_model_from_body(body).is_some_and(|model| {
        model
            .trim_end()
            .to_ascii_lowercase()
            .ends_with(&CLAUDE_ONE_M_SUFFIX.to_ascii_lowercase())
    })
}

/// Merge the 1M-context beta marker into `anthropic-beta`, preserving whatever
/// the client already sent.
fn apply_one_m_context_beta(headers: &mut HeaderMap) {
    let existing = headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok());
    if let Some(merged) = client_identity::merge_one_m_context_beta(existing) {
        if let Ok(value) = HeaderValue::from_str(&merged) {
            headers.insert(HeaderName::from_static("anthropic-beta"), value);
        }
    }
}

/// Make an OpenAI/Responses-dialect upstream request look like the Codex CLI.
fn apply_codex_cli_identity(headers: &mut HeaderMap) {
    for (name, value) in client_identity::codex_cli_identity_headers() {
        fill_header_if_absent(headers, name, &value);
    }
}

/// Append `key=value` to the URL query only when the key is not already present.
fn ensure_query_flag(url: &str, key: &str, value: &str) -> String {
    let already_present = url.split(['?', '&']).skip(1).any(|part| {
        let part_key = part.split_once('=').map(|(k, _)| k).unwrap_or(part);
        part_key == key
    });
    if already_present {
        url.to_string()
    } else {
        append_query_param(url, key, value)
    }
}

/// Anthropic messages endpoint detection (`/v1/messages`, `/messages`).
fn is_messages_path(path: &str) -> bool {
    let normalized = path.trim().trim_end_matches('/');
    normalized.ends_with("/messages") || normalized == "messages"
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
    match serde_json::from_slice::<Value>(body) {
        Ok(value) => usage_breakdown_from_value(&value),
        // A streaming response body is SSE text, not a single JSON document, so
        // whole-body parsing always fails for it. Claude Code and Codex stream by
        // default, so without this fallback the vast majority of real requests
        // persist NULL tokens and NULL price.
        Err(_) => usage_breakdown_from_sse(body),
    }
}

/// Fill in a locally estimated price when the upstream returned none.
///
/// Anthropic, OpenAI, and Gemini all report token counts but no price, so
/// without this the recorded amount is always zero — the "cost is always 0"
/// symptom. The estimate is tagged `price_source = "estimated"` so it is never
/// confused with a real upstream charge, and an unknown model is left unpriced
/// rather than being silently treated as free.
pub fn apply_estimated_price(usage: &mut RouteUsageBreakdown, model: Option<&str>) {
    use crate::services::model_pricing::{self, PriceSource, TokenUsage};

    if usage.price_usd_micros.is_some() || usage.price_cny_micros.is_some() {
        usage.price_source = Some(PriceSource::Upstream.as_str().to_string());
        return;
    }

    let Some(model) = model else {
        return;
    };
    // Cache tokens are reported as a single total by most upstreams, without
    // splitting writes from reads. Charging them at the cheaper read rate keeps
    // the estimate a lower bound instead of inflating it.
    let token_usage = TokenUsage {
        input_tokens: usage.input_tokens.unwrap_or(0),
        output_tokens: usage.output_tokens.unwrap_or(0),
        cache_write_tokens: 0,
        cache_read_tokens: usage.cache_tokens.unwrap_or(0),
    };
    if token_usage.input_tokens <= 0
        && token_usage.output_tokens <= 0
        && token_usage.cache_read_tokens <= 0
    {
        return;
    }

    if let Some(cost) = model_pricing::estimate_cost_micros(model, token_usage) {
        usage.price_usd_micros = Some(cost);
        usage.price_currency = Some("usd".to_string());
        usage.price_source = Some(PriceSource::Estimated.as_str().to_string());
    }
}

/// The model name a response reports, checked across the JSON body and, for a
/// streaming reply, its SSE frames.
///
/// Used to price a request when the caller has no model in scope. The upstream's
/// own value is also the right one to bill against, since a gateway may serve a
/// different model than the one requested.
pub fn extract_response_model(body: &[u8]) -> Option<String> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        return response_model_from_value(&value);
    }
    crate::services::route_protocol_bridge::sse::parse_sse_data_records_lossy(body)
        .iter()
        .find_map(response_model_from_value)
}

pub(crate) fn response_model_from_value(value: &Value) -> Option<String> {
    [
        value.get("model"),
        value.pointer("/message/model"),
        value.pointer("/response/model"),
        value.pointer("/modelVersion"),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_str)
    .map(str::trim)
    .filter(|model| !model.is_empty())
    .map(str::to_string)
}

/// Merge per-frame usage from an SSE body.
///
/// Providers spread usage across frames differently: OpenAI reports it only in
/// the final chunk, while Anthropic splits it between `message_start` (input and
/// cache tokens) and `message_delta` (output tokens). Taking the last non-empty
/// value per field — rather than summing — covers both without double counting
/// the cumulative totals that Anthropic and Gemini resend on every frame.
fn usage_breakdown_from_sse(body: &[u8]) -> RouteUsageBreakdown {
    let mut merged = RouteUsageBreakdown::default();
    for frame in crate::services::route_protocol_bridge::sse::parse_sse_data_records_lossy(body) {
        merged.merge_from(usage_breakdown_from_value(&frame));
    }
    merged
}

pub(crate) fn usage_breakdown_from_value(value: &Value) -> RouteUsageBreakdown {
    // Anthropic's `message_start` nests usage under `message`, and the OpenAI
    // Responses API nests it under `response`, so accept either alongside the
    // top-level field used by non-streaming replies.
    let usage = value
        .get("usage")
        .or_else(|| value.pointer("/message/usage"))
        .or_else(|| value.pointer("/response/usage"));
    let usage_metadata = value
        .get("usageMetadata")
        .or_else(|| value.pointer("/response/usageMetadata"));

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

    let (price_usd_micros, price_cny_micros, price_currency) = extract_prices(value, usage);

    RouteUsageBreakdown {
        input_tokens,
        output_tokens,
        cache_tokens,
        price_usd_micros,
        price_cny_micros,
        price_currency,
        // Set later by `apply_estimated_price`, which is the only place that can
        // tell an upstream price from a locally computed one.
        price_source: None,
    }
}

pub fn extract_token_count(body: &[u8]) -> Option<i64> {
    let usage = extract_usage_breakdown(body);
    let total = match (usage.input_tokens, usage.output_tokens) {
        (Some(input), Some(output)) => Some(input.saturating_add(output)),
        (Some(input), None) => Some(input),
        (None, Some(output)) => Some(output),
        (None, None) => total_tokens_fallback(body),
    };

    total.filter(|value| *value > 0)
}

/// Last-resort total: some gateways report only `usage.total_tokens`.
/// Checks the whole body first, then each SSE frame for streaming replies.
fn total_tokens_fallback(body: &[u8]) -> Option<i64> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        return total_tokens_from_value(&value);
    }
    crate::services::route_protocol_bridge::sse::parse_sse_data_records_lossy(body)
        .iter()
        .rev()
        .find_map(total_tokens_from_value)
}

fn total_tokens_from_value(value: &Value) -> Option<i64> {
    first_non_negative_i64(&[
        value.pointer("/usage/total_tokens"),
        value.pointer("/message/usage/total_tokens"),
        value.pointer("/response/usage/total_tokens"),
    ])
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
    upstream_response_id: Option<&str>,
) -> Result<(), AppError> {
    RoutePoolRepository::insert_request_event(
        pool,
        route_credential_id,
        "route_proxy",
        metadata_json,
        usage,
        upstream_response_id,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::route_credential::{
        DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT, FALLBACK_MODEL_ALIAS,
    };
    use crate::models::route_credential_model::{
        RouteCredentialModelState, MODEL_STATUS_ERROR, MODEL_STATUS_OK, MODEL_STATUS_PAUSED,
    };
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn flags_bridged_turn_without_tool_call() {
        let request = br#"{"model":"x","tools":[{"type":"function"}],"input":"hi"}"#;
        let response =
            br#"data: {"type":"response.completed","response":{"output":[{"type":"message"}]}}"#;
        let notes = diagnostic_notes(
            true,
            Some("codex"),
            Some(std::str::from_utf8(request).unwrap()),
            Some(response),
        );
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("未发起工具调用"));
    }

    #[test]
    fn flags_bridged_empty_output() {
        let response = br#"data: {"type":"response.completed","response":{"output":[]}}"#;
        let notes = diagnostic_notes(true, Some("codex"), Some("{\"tools\":[]}"), Some(response));
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("空输出"));
    }

    #[test]
    fn redacts_long_instructions_but_keeps_tools_and_messages() {
        let long = "x".repeat(VERBOSE_REQUEST_FIELD_LIMIT + 50);
        let body = serde_json::to_vec(&json!({
            "model": "mimo-v2.5-pro",
            "instructions": long,
            "tools": [{"type": "function", "name": "lookup"}],
            "input": "hi"
        }))
        .unwrap();
        let redacted = redact_verbose_request_fields(Some(&body)).expect("redacted");
        let value: Value = serde_json::from_slice(&redacted).unwrap();

        assert!(value["instructions"]
            .as_str()
            .is_some_and(|text| text.starts_with("<instructions omitted:")));
        assert_eq!(value["tools"][0]["name"], "lookup");
        assert_eq!(value["input"], "hi");
    }

    #[test]
    fn redacts_long_system_message_and_short_instructions_kept() {
        let long = "y".repeat(VERBOSE_REQUEST_FIELD_LIMIT + 1);
        let body = serde_json::to_vec(&json!({
            "instructions": "short",
            "messages": [
                {"role": "system", "content": long},
                {"role": "user", "content": "hello"}
            ]
        }))
        .unwrap();
        let redacted = redact_verbose_request_fields(Some(&body)).expect("redacted");
        let value: Value = serde_json::from_slice(&redacted).unwrap();

        assert_eq!(value["instructions"], "short");
        assert!(value["messages"][0]["content"]
            .as_str()
            .is_some_and(|text| text.starts_with("<system omitted:")));
        assert_eq!(value["messages"][1]["content"], "hello");
    }

    #[test]
    fn leaves_non_json_request_untouched() {
        let body = b"data: {not json";
        let out = redact_verbose_request_fields(Some(body)).expect("passthrough");
        assert_eq!(out, body);
    }

    #[test]
    fn stays_quiet_when_tool_call_present_or_not_bridged() {
        let request = "{\"tools\":[{}]}";
        let with_tool = br#"data: {"type":"response.completed","response":{"output":[{"type":"function_call"}]}}"#;
        assert!(diagnostic_notes(true, Some("codex"), Some(request), Some(with_tool)).is_empty());
        // No bridge applied -> no diagnostics.
        let text_only =
            br#"data: {"type":"response.completed","response":{"output":[{"type":"message"}]}}"#;
        assert!(diagnostic_notes(true, None, Some(request), Some(text_only)).is_empty());
        // Request offered no tools -> a plain text answer is expected, not flagged.
        assert!(diagnostic_notes(
            true,
            Some("codex"),
            Some("{\"input\":\"hi\"}"),
            Some(text_only)
        )
        .is_empty());
    }

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

    /// An SSE upstream that emits chunks on demand, with a deliberate split
    /// inside one frame.
    ///
    /// The single-`Body::from` fixtures above hand the whole payload over at
    /// once, so they never exercise a chunk boundary. Real upstreams split
    /// wherever the network does, including mid-frame, which is exactly what the
    /// framer has to survive.
    ///
    /// `gap` is awaited after the first chunk, so a test can prove the client
    /// received the opening bytes before the upstream finished.
    async fn start_chunked_sse_upstream(
        chunks: Vec<&'static str>,
        gap: Duration,
    ) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handler_calls = Arc::clone(&calls);
        let app = Router::new().fallback(move || {
            let chunks = chunks.clone();
            let calls = Arc::clone(&handler_calls);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let stream = futures_util::stream::unfold(
                    chunks.into_iter().enumerate(),
                    move |mut iter| async move {
                        let (index, chunk) = iter.next()?;
                        if index > 0 {
                            tokio::time::sleep(gap).await;
                        }
                        Some((
                            Ok::<_, std::io::Error>(axum::body::Bytes::from_static(
                                chunk.as_bytes(),
                            )),
                            iter,
                        ))
                    },
                );
                Response::builder()
                    .status(StatusCode::OK)
                    .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
                    .body(Body::from_stream(stream))
                    .expect("chunked sse response")
            }
        });
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind chunked sse upstream");
        let address = listener.local_addr().expect("chunked sse address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve chunked sse");
        });
        (format!("http://{address}/v1"), calls)
    }

    /// An upstream that returns 200 with SSE headers and then closes without
    /// sending a single byte of body.
    async fn start_empty_stream_upstream() -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handler_calls = Arc::clone(&calls);
        let app = Router::new().fallback(move || {
            let calls = Arc::clone(&handler_calls);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Response::builder()
                    .status(StatusCode::OK)
                    .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
                    .body(Body::empty())
                    .expect("empty stream response")
            }
        });
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind empty stream upstream");
        let address = listener.local_addr().expect("empty stream address");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve empty stream");
        });
        (format!("http://{address}/v1"), calls)
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

    /// An upstream that fails one model and serves another, i.e. exactly the
    /// relay behaviour that makes account-wide cooldown wrong.
    async fn start_per_model_upstream(failing_model: &'static str) -> String {
        let app = Router::new().fallback(move |body: axum::body::Bytes| async move {
            let model = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            if model == failing_model {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    r#"{"error":{"message":"rate limited"}}"#,
                )
            } else {
                (
                    StatusCode::OK,
                    r#"{"choices":[{"message":{"content":"ok"}}]}"#,
                )
            }
        });
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind per-model upstream");
        let address = listener.local_addr().expect("per-model address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve per-model");
        });
        format!("http://{address}/v1")
    }

    /// An upstream that accepts the connection and then goes silent forever.
    ///
    /// This is the failure the proxy could not see before it had deadlines: no
    /// RST, no EOF, no status line — just a live socket with nothing on it. It
    /// holds every accepted stream so the peer never observes a close, and
    /// returns the number of connections it has swallowed.
    async fn start_stalled_upstream() -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind stalled upstream");
        let address = listener.local_addr().expect("stalled upstream address");
        let accepted = Arc::clone(&connections);
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        accepted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        // Never read or write; keep the socket open so the
                        // client waits on bytes that will never come.
                        held.push(stream);
                    }
                    Err(_) => break,
                }
            }
        });
        (format!("http://{address}/v1"), connections)
    }

    #[derive(Clone)]
    struct SequenceUpstreamState {
        calls: Arc<std::sync::atomic::AtomicUsize>,
        overload_attempts: usize,
        success_body: &'static str,
    }

    async fn sequence_upstream_handler(
        AxumState(state): AxumState<SequenceUpstreamState>,
    ) -> Response {
        let attempt = state
            .calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let body = if attempt <= state.overload_attempts {
            r#"{"type":"response.failed","response":{"status":"failed","error":{"message":"Our servers are currently overloaded. Please try again later."}}}"#
        } else {
            state.success_body
        };
        Response::builder()
            .status(StatusCode::OK)
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .expect("sequence response")
    }

    async fn start_sequence_upstream(
        overload_attempts: usize,
        success_body: &'static str,
    ) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = Router::new()
            .fallback(sequence_upstream_handler)
            .with_state(SequenceUpstreamState {
                calls: Arc::clone(&calls),
                overload_attempts,
                success_body,
            });
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind sequence upstream");
        let address = listener.local_addr().expect("sequence upstream address");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve sequence upstream");
        });
        (format!("http://{address}/v1"), calls)
    }

    #[derive(Clone)]
    struct StatusSequenceUpstreamState {
        calls: Arc<AtomicUsize>,
        failed_attempts: usize,
        failure_status: StatusCode,
        failure_body: &'static str,
        success_body: &'static str,
    }

    async fn status_sequence_upstream_handler(
        AxumState(state): AxumState<StatusSequenceUpstreamState>,
    ) -> Response {
        let attempt = state.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let (status, body) = if attempt <= state.failed_attempts {
            (state.failure_status, state.failure_body)
        } else {
            (StatusCode::OK, state.success_body)
        };
        Response::builder()
            .status(status)
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .expect("status sequence response")
    }

    async fn start_status_sequence_upstream(
        failed_attempts: usize,
        failure_status: StatusCode,
        failure_body: &'static str,
        success_body: &'static str,
    ) -> (String, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .fallback(status_sequence_upstream_handler)
            .with_state(StatusSequenceUpstreamState {
                calls: Arc::clone(&calls),
                failed_attempts,
                failure_status,
                failure_body,
                success_body,
            });
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind status sequence upstream");
        let address = listener
            .local_addr()
            .expect("status sequence upstream address");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve status sequence upstream");
        });
        (format!("http://{address}/v1"), calls)
    }

    async fn start_flaky_body_upstream(
        failed_attempts: usize,
        success_body: &'static str,
    ) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind flaky body upstream");
        let address = listener.local_addr().expect("flaky upstream address");
        let calls = Arc::new(AtomicUsize::new(0));
        let server_calls = Arc::clone(&calls);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let attempt = server_calls.fetch_add(1, Ordering::SeqCst) + 1;
                let body = if attempt <= failed_attempts {
                    r#"{"choices":"#
                } else {
                    success_body
                };
                let content_length = if attempt <= failed_attempts {
                    body.len() + 64
                } else {
                    body.len()
                };
                let mut request_buffer = [0u8; 1024];
                let _ = socket.read(&mut request_buffer).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n{body}"
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        (format!("http://{address}/v1"), calls)
    }

    async fn create_proxy_api_credential(pool: &SqlitePool, name: &str, base_url: &str) -> String {
        create_proxy_api_credential_with_config(pool, name, base_url, json!({})).await
    }

    async fn create_proxy_api_credential_with_config(
        pool: &SqlitePool,
        name: &str,
        base_url: &str,
        extra_config: Value,
    ) -> String {
        let mut config = json!({
            "base_url": base_url,
            "interface_format": "openai",
            "model_mappings": []
        });
        if let (Some(config), Some(extra_config)) =
            (config.as_object_mut(), extra_config.as_object())
        {
            config.extend(extra_config.clone());
        }
        let credential = RouteCredentialRepository::create(
            pool,
            "codex",
            "api",
            name,
            None,
            "ok",
            None,
            r#"{"api_key":"sk-upstream"}"#,
            &config.to_string(),
            "{}",
        )
        .await
        .expect("create credential");
        credential.id
    }

    async fn create_proxy_api_credential_with_mappings(
        pool: &SqlitePool,
        name: &str,
        base_url: &str,
        model_mappings: Value,
    ) -> String {
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
                "model_mappings": model_mappings
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
    async fn route_proxy_falls_back_to_next_priority_when_first_account_is_at_limit() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let first_upstream =
            start_fixed_upstream(StatusCode::OK, r#"{"route":"priority-one"}"#).await;
        let second_upstream =
            start_fixed_upstream(StatusCode::OK, r#"{"route":"priority-two"}"#).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let first_id = create_proxy_api_credential(&pool, "priority-one", &first_upstream).await;
        let second_id = create_proxy_api_credential(&pool, "priority-two", &second_upstream).await;
        sqlx::query(
            "UPDATE route_credentials
             SET route_priority = CASE id WHEN ? THEN 1 ELSE 2 END,
                 max_concurrency = 1
             WHERE id IN (?, ?)",
        )
        .bind(&first_id)
        .bind(&first_id)
        .bind(&second_id)
        .execute(&pool)
        .await
        .expect("routing settings");
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[first_id.clone(), second_id.clone()],
        )
        .await
        .expect("pool members");
        let route_key = RouteProxyKeyRepository::ensure_platform_key(
            &pool,
            "codex",
            "sk-ai-switch-test-priority",
        )
        .await
        .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let held_lease = runtime
            .activity()
            .try_acquire("codex", &first_id, 1)
            .await
            .expect("hold first account concurrency slot");
        let proxy = RouteProxyService::start(&runtime, pool, RouteProxyTransport::Http)
            .await
            .expect("start proxy");
        let client = reqwest::Client::new();
        let endpoint = format!(
            "{}/v1/chat/completions",
            proxy.base_url.as_deref().expect("base url")
        );

        let fallback_response = client
            .post(&endpoint)
            .bearer_auth(&route_key)
            .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("fallback response");
        assert_eq!(fallback_response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            fallback_response.text().await.expect("fallback body"),
            r#"{"route":"priority-two"}"#
        );

        drop(held_lease);
        let primary_response = client
            .post(&endpoint)
            .bearer_auth(&route_key)
            .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("primary response");
        assert_eq!(primary_response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            primary_response.text().await.expect("primary body"),
            r#"{"route":"priority-one"}"#
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
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
        let credential_id = create_proxy_api_credential_with_mappings(
            &pool,
            "chat-bridge",
            &upstream_url,
            json!([{"from":"gpt-5","to":"deepseek-chat"}]),
        )
        .await;
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
        let proxy = RouteProxyService::start(&runtime, pool.clone(), RouteProxyTransport::Http)
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
        assert_eq!(captured_json.body["model"], "deepseek-chat");
        assert!(captured_json.body.get("input").is_none());
        let stats = RoutePoolRepository::stats(&pool, "codex", None, 1, 20)
            .await
            .expect("route stats");
        assert_eq!(stats.requests.len(), 1);
        let metadata: Value =
            serde_json::from_str(&stats.requests[0].metadata_json).expect("request metadata");
        assert_eq!(
            metadata.pointer("/requested_model").and_then(Value::as_str),
            Some("gpt-5")
        );
        assert_eq!(
            metadata.pointer("/upstream_model").and_then(Value::as_str),
            Some("deepseek-chat")
        );

        // Live log must capture all four stages, and the protocol conversion
        // must make the inbound request (stage 1) differ from the upstream
        // request (stage 2).
        let live_entries = runtime.live_log().subscribe("codex");
        assert_eq!(live_entries.len(), 1);
        let live_entry = &live_entries[0];
        assert!(live_entry.success);
        assert!(
            live_entry.bridge.is_some(),
            "protocol conversion should record a bridge kind"
        );
        let client_request = live_entry.client_request.as_deref().unwrap_or_default();
        let upstream_request = live_entry.upstream_request.as_deref().unwrap_or_default();
        assert!(client_request.contains("\"input\""));
        assert!(upstream_request.contains("\"messages\""));
        assert!(upstream_request.contains("deepseek-chat"));
        assert_ne!(
            client_request, upstream_request,
            "stage 1 and stage 2 must differ after protocol conversion"
        );
        assert!(live_entry.upstream_response.is_some());
        assert!(live_entry.final_response.is_some());
        runtime.live_log().unsubscribe();

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

        let failed_response_body = r#"{"error":{"message":"expired"}}"#;
        let failed_upstream =
            start_fixed_upstream(StatusCode::UNAUTHORIZED, failed_response_body).await;
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
            .json(&json!({"model":"gpt-5.5","messages":[]}))
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
        // Cooldown is opt-in, so the failure counts but schedules no backoff.
        assert!(failed.next_retry_at.is_none());
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
        let failed_request = stats
            .requests
            .iter()
            .find(|request| request.account_id.as_deref() == Some(failed_id.as_str()))
            .expect("failed request row");
        let healthy_metadata: Value =
            serde_json::from_str(&healthy_request.metadata_json).expect("healthy metadata");
        let failed_metadata: Value =
            serde_json::from_str(&failed_request.metadata_json).expect("failed metadata");
        assert_eq!(
            failed_metadata
                .pointer("/response_body")
                .and_then(Value::as_str),
            Some(failed_response_body)
        );
        assert_eq!(
            healthy_metadata
                .pointer("/response_body")
                .and_then(Value::as_str),
            Some(
                r#"{"usage":{"prompt_tokens":120,"completion_tokens":30,"prompt_cache_hit_tokens":80,"price_cny":7.1}}"#
            )
        );
        assert_eq!(healthy_request.input_tokens, Some(120));
        assert_eq!(healthy_request.output_tokens, Some(30));
        assert_eq!(healthy_request.cache_tokens, Some(80));
        assert_eq!(healthy_request.price_cny_micros, Some(7_100_000));
        assert_eq!(healthy_request.price_currency.as_deref(), Some("cny"));

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn upstream_timeout_fails_over_to_next_pool_account() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (stalled_upstream, stalled_connections) = start_stalled_upstream().await;
        let healthy_upstream = start_fixed_upstream(StatusCode::OK, r#"{"ok":true}"#).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        // retry_count 0 keeps the assertion about failover (not same-account
        // retries) unambiguous; the retry budget is covered by its own test.
        let stalled_id = create_proxy_api_credential_with_config(
            &pool,
            "stalled",
            &stalled_upstream,
            json!({"failure_policy": {"retry_count": 0}}),
        )
        .await;
        let healthy_id = create_proxy_api_credential(&pool, "healthy", &healthy_upstream).await;
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[stalled_id.clone(), healthy_id.clone()],
        )
        .await
        .expect("pool members");
        let route_key =
            RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-test")
                .await
                .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start_with_test_upstream_timeouts(
            &runtime,
            pool.clone(),
            RouteProxyTransport::Http,
            OutboundTimeouts {
                connect: Some(Duration::from_millis(500)),
                read: Some(Duration::from_millis(300)),
                ..OutboundTimeouts::default()
            },
        )
        .await
        .expect("start proxy");

        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/chat/completions",
                proxy.base_url.as_deref().expect("base url")
            ))
            .bearer_auth(route_key)
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");

        // Without a read deadline this request never returns at all.
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(response.text().await.expect("body"), r#"{"ok":true}"#);
        assert_eq!(
            stalled_connections.load(std::sync::atomic::Ordering::SeqCst),
            1
        );

        let stalled = RouteCredentialRepository::get(&pool, &stalled_id)
            .await
            .expect("stalled account");
        assert_eq!(stalled.transient_failure_count, 1);
        assert_eq!(stalled.last_failure_kind.as_deref(), Some("transport"));
        // Cooldown is opt-in, so the failure counts but schedules no backoff.
        assert!(stalled.next_retry_at.is_none());
        assert!(stalled
            .last_failure_message
            .as_deref()
            .is_some_and(|message| message.contains("stalled connection")));
        assert_eq!(
            RouteCredentialRepository::get(&pool, &healthy_id)
                .await
                .expect("healthy account")
                .status,
            "ok"
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn upstream_timeout_exhausts_same_account_retries_before_failing_over() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (stalled_upstream, stalled_connections) = start_stalled_upstream().await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let stalled_id = create_proxy_api_credential_with_config(
            &pool,
            "stalled",
            &stalled_upstream,
            json!({"failure_policy": {"retry_count": 2, "retry_interval_ms": 0}}),
        )
        .await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&stalled_id))
            .await
            .expect("pool members");
        let route_key =
            RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-test")
                .await
                .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start_with_test_upstream_timeouts(
            &runtime,
            pool.clone(),
            RouteProxyTransport::Http,
            OutboundTimeouts {
                connect: Some(Duration::from_millis(500)),
                read: Some(Duration::from_millis(200)),
                ..OutboundTimeouts::default()
            },
        )
        .await
        .expect("start proxy");

        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/chat/completions",
                proxy.base_url.as_deref().expect("base url")
            ))
            .bearer_auth(route_key)
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");

        // Only candidate in the pool, so the request ends in an error rather
        // than hanging — but not before the retry budget is spent.
        assert_ne!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            stalled_connections.load(std::sync::atomic::Ordering::SeqCst),
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize + 1
        );
        assert_eq!(
            RouteCredentialRepository::get(&pool, &stalled_id)
                .await
                .expect("stalled account")
                .transient_failure_count,
            1
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    /// The payload for the streaming tests, split so that chunk 2 ends in the
    /// middle of a JSON payload and chunk 3 completes it.
    const CHUNKED_SSE_PARTS: [&str; 4] = [
        "data: {\"id\":\"chatcmpl-1\",\"model\":\"deepseek-chat\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}],\"usage\":{\"prompt_to",
        "kens\":11,\"completion_tokens\":4}}\n\n",
        "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
    ];

    /// The core promise of streaming: the client sees the opening bytes while the
    /// upstream is still generating.
    ///
    /// The upstream pauses between chunks, so if the proxy were buffering, the
    /// first byte could not arrive before the whole generation finished.
    #[tokio::test]
    async fn streams_first_bytes_to_the_client_before_the_upstream_finishes() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let gap = Duration::from_millis(250);
        let (upstream, _calls) = start_chunked_sse_upstream(CHUNKED_SSE_PARTS.to_vec(), gap).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential(&pool, "streaming", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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

        let started = Instant::now();
        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/chat/completions",
                proxy.base_url.as_deref().expect("base url")
            ))
            .bearer_auth(route_key)
            .json(&json!({"model":"gpt-5.5","messages":[],"stream":true}))
            .send()
            .await
            .expect("proxy response");
        assert_eq!(response.status(), StatusCode::OK);

        let mut stream = Box::pin(response.bytes_stream());
        let first = futures_util::StreamExt::next(&mut stream)
            .await
            .expect("first chunk")
            .expect("first chunk bytes");
        let first_byte_at = started.elapsed();
        assert!(!first.is_empty());

        // Three gaps remain after the first chunk, so a buffering proxy could not
        // have answered before 3 * gap.
        let total_upstream_time = gap * (CHUNKED_SSE_PARTS.len() as u32 - 1);
        assert!(
            first_byte_at < total_upstream_time,
            "first byte took {first_byte_at:?}, which is not sooner than the \
             upstream's own {total_upstream_time:?} — the response was buffered"
        );

        let mut body = first.to_vec();
        while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
            body.extend_from_slice(&chunk.expect("chunk bytes"));
        }
        assert_eq!(body, CHUNKED_SSE_PARTS.concat().as_bytes());

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    /// Usage has to survive the switch to streaming, including when the frame
    /// carrying it was split across chunks.
    #[tokio::test]
    async fn streamed_response_still_records_usage_and_clears_failures() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (upstream, _calls) =
            start_chunked_sse_upstream(CHUNKED_SSE_PARTS.to_vec(), Duration::from_millis(0)).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential(&pool, "streaming", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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
            .json(&json!({"model":"gpt-5.5","messages":[],"stream":true}))
            .send()
            .await
            .expect("proxy response");
        assert_eq!(response.status(), StatusCode::OK);
        // Drain fully so the completion hook runs.
        let _ = response.bytes().await.expect("proxy body");

        let request = wait_for_single_request_event(&pool).await;
        assert_eq!(request.input_tokens, Some(11));
        assert_eq!(request.output_tokens, Some(4));
        assert_eq!(
            RouteCredentialRepository::get(&pool, &credential_id)
                .await
                .expect("credential")
                .status,
            "ok"
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    /// A stream that carried data but never terminated is still charged against
    /// the account, even though the bytes are already gone and cannot be retried.
    #[tokio::test]
    async fn truncated_stream_is_recorded_without_retrying() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        // Data frames, but no `finish_reason`, no `[DONE]`, no `message_stop`.
        let (upstream, calls) = start_chunked_sse_upstream(
            vec![
                "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}]}\n\n",
            ],
            Duration::from_millis(0),
        )
        .await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential(&pool, "truncating", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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
            .json(&json!({"model":"gpt-5.5","messages":[],"stream":true}))
            .send()
            .await
            .expect("proxy response");
        // The client keeps its 200 and its bytes: they were already delivered.
        assert_eq!(response.status(), StatusCode::OK);
        let _ = response.bytes().await.expect("proxy body");

        // Exactly one upstream call — a truncated stream must not be retried.
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let credential = wait_for_transient_failure(&pool, &credential_id).await;
        assert!(
            credential
                .last_failure_message
                .as_deref()
                .is_some_and(|message| message.contains("stream disconnected")),
            "expected a stream-disconnect failure, got {:?}",
            credential.last_failure_message
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    /// A stream that dies before its first byte has touched nothing on the
    /// client, so it must still fail over to the next account.
    #[tokio::test]
    async fn stream_failing_before_first_byte_fails_over_to_next_account() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (empty_upstream, empty_calls) = start_empty_stream_upstream().await;
        let (healthy_upstream, healthy_calls) =
            start_chunked_sse_upstream(CHUNKED_SSE_PARTS.to_vec(), Duration::from_millis(0)).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        // retry_count 0 so the empty upstream is abandoned immediately rather
        // than retried on the same account.
        let empty_id = create_proxy_api_credential_with_config(
            &pool,
            "empty",
            &empty_upstream,
            json!({"failure_policy": {"retry_count": 0}}),
        )
        .await;
        let healthy_id = create_proxy_api_credential(&pool, "healthy", &healthy_upstream).await;
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[empty_id.clone(), healthy_id.clone()],
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
            .json(&json!({"model":"gpt-5.5","messages":[],"stream":true}))
            .send()
            .await
            .expect("proxy response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.bytes().await.expect("proxy body");
        assert_eq!(body, CHUNKED_SSE_PARTS.concat().as_bytes());

        assert_eq!(empty_calls.load(Ordering::SeqCst), 1);
        assert_eq!(healthy_calls.load(Ordering::SeqCst), 1);

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    /// The buffered path stays in charge whenever a bridge has to rewrite the
    /// body: the gate must not stream a request that needs conversion.
    #[tokio::test]
    async fn bridged_streaming_request_still_uses_the_buffered_path() {
        let claude_to_chat = should_stream_upstream_response(
            Some(ProtocolBridgeKind::ClaudeToChat),
            true,
            StatusCode::OK,
            &std::collections::HashSet::new(),
            &streaming_gate_credential("api"),
        );
        assert!(!claude_to_chat, "a bridged response must stay buffered");

        let passthrough = should_stream_upstream_response(
            None,
            true,
            StatusCode::OK,
            &std::collections::HashSet::new(),
            &streaming_gate_credential("api"),
        );
        assert!(passthrough, "an unbridged streaming 2xx should stream");
    }

    /// Each remaining gate condition, so a future change cannot quietly widen
    /// the streaming path past what has an incremental equivalent.
    #[tokio::test]
    async fn streaming_gate_rejects_every_case_needing_the_whole_body() {
        let empty = std::collections::HashSet::new();
        let api = streaming_gate_credential("api");

        assert!(
            !should_stream_upstream_response(None, false, StatusCode::OK, &empty, &api),
            "a non-streaming reply has nothing to stream"
        );
        assert!(
            !should_stream_upstream_response(
                None,
                true,
                StatusCode::TOO_MANY_REQUESTS,
                &empty,
                &api
            ),
            "a non-2xx body decides retry classification and must be buffered"
        );
        assert!(
            !should_stream_upstream_response(
                None,
                true,
                StatusCode::OK,
                &std::collections::HashSet::from(["my_tool".to_string()]),
                &api
            ),
            "custom tool restoration rewrites frames on the way out"
        );
        assert!(
            !should_stream_upstream_response(
                None,
                true,
                StatusCode::OK,
                &empty,
                &streaming_gate_credential("official")
            ),
            "official credentials parse the body for quota signals"
        );
    }

    fn streaming_gate_credential(kind: &str) -> SelectedCredential {
        SelectedCredential {
            id: "credential".to_string(),
            platform: "codex".to_string(),
            kind: kind.to_string(),
            display_name: "Credential".to_string(),
            status: "ok".to_string(),
            route_priority: 3,
            max_concurrency: 1,
            secret_payload_json: r#"{"api_key":"sk-upstream"}"#.to_string(),
            config_json: "{}".to_string(),
        }
    }

    /// A streamed response is booked by the completion hook once the stream
    /// ends, which can land just after the client's last chunk. Poll rather than
    /// assume the row is already there.
    async fn wait_for_single_request_event(
        pool: &SqlitePool,
    ) -> crate::models::route_pool::RoutePoolUsageLog {
        for _ in 0..100 {
            let stats = RoutePoolRepository::stats(pool, "codex", None, 1, 20)
                .await
                .expect("usage stats");
            if let Some(request) = stats.requests.into_iter().next() {
                return request;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no route_proxy request event was recorded");
    }

    async fn wait_for_transient_failure(
        pool: &SqlitePool,
        credential_id: &str,
    ) -> crate::models::route_credential::RouteCredential {
        for _ in 0..100 {
            let credential = RouteCredentialRepository::get(pool, credential_id)
                .await
                .expect("credential");
            if credential.transient_failure_count > 0 {
                return credential;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("credential never recorded a transient failure");
    }

    #[tokio::test]
    async fn retries_overloaded_account_until_success() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (upstream, calls) = start_sequence_upstream(2, r#"{"ok":true}"#).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential(&pool, "overloaded", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json::<Value>().await.expect("proxy body")["ok"],
            true
        );
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize + 1
        );
        assert_eq!(
            RouteCredentialRepository::get(&pool, &credential_id)
                .await
                .expect("credential")
                .status,
            "ok"
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn proxy_retries_transient_body_read_errors_on_same_account_before_marking_failure() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (upstream, calls) = start_flaky_body_upstream(
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize,
            r#"{"ok":true}"#,
        )
        .await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential(&pool, "flaky", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.expect("proxy body"), r#"{"ok":true}"#);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize + 1
        );
        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "ok");
        assert_eq!(credential.transient_failure_count, 0);
        assert!(credential.next_retry_at.is_none());

        let usage_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM usage_events WHERE source_label = 'route_proxy'",
        )
        .fetch_one(&pool)
        .await
        .expect("usage count");
        assert_eq!(
            usage_count,
            i64::from(DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT + 1)
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn proxy_retries_retryable_http_statuses_on_the_same_account() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (upstream, calls) = start_status_sequence_upstream(
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize,
            StatusCode::SERVICE_UNAVAILABLE,
            r#"{"error":{"message":"temporarily unavailable"}}"#,
            r#"{"ok":true}"#,
        )
        .await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential(&pool, "retryable status", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.expect("proxy body"), r#"{"ok":true}"#);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize + 1
        );
        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "ok");
        assert_eq!(credential.transient_failure_count, 0);
        assert!(credential.next_retry_at.is_none());

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn proxy_retries_retryable_http_statuses_before_recording_semantic_failures() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (upstream, calls) = start_status_sequence_upstream(
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize,
            StatusCode::SERVICE_UNAVAILABLE,
            r#"{"type":"response.failed","response":{"status":"failed","error":{"message":"temporarily unavailable"}}}"#,
            r#"{"ok":true}"#,
        )
        .await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id =
            create_proxy_api_credential(&pool, "semantic retryable status", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.expect("proxy body"), r#"{"ok":true}"#);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize + 1
        );
        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "ok");
        assert_eq!(credential.transient_failure_count, 0);
        assert!(credential.next_retry_at.is_none());

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn exhausted_retryable_semantic_status_records_one_transient_failure() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (upstream, calls) = start_status_sequence_upstream(
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize + 1,
            StatusCode::SERVICE_UNAVAILABLE,
            r#"{"type":"response.failed","response":{"status":"failed","error":{"message":"temporarily unavailable"}}}"#,
            r#"{"ok":true}"#,
        )
        .await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id =
            create_proxy_api_credential(&pool, "semantic retry exhausted", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize + 1
        );
        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "ok");
        assert_eq!(credential.transient_failure_count, 1);
        assert_eq!(
            credential.last_failure_kind.as_deref(),
            Some("semantic_response_transient")
        );
        let streak_count: i64 = sqlx::query_scalar(
            "SELECT semantic_failure_streak_count FROM route_credentials WHERE id = ?",
        )
        .bind(&credential_id)
        .fetch_one(&pool)
        .await
        .expect("semantic streak");
        assert_eq!(streak_count, 0);

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn overloaded_response_does_not_mark_account_error() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let overloaded_body = r#"{"type":"response.failed","response":{"status":"failed","error":{"message":"Our servers are currently overloaded. Please try again later."}}}"#;
        let upstream = start_fixed_upstream(StatusCode::OK, overloaded_body).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential(&pool, "overloaded", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");
        let _ = response.bytes().await.expect("proxy body");

        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "ok");
        assert_eq!(credential.transient_failure_count, 1);

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn new_api_insufficient_balance_response_marks_account_error() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        // new-api gateways answer an exhausted balance with 403 and their own
        // error envelope, which carries no `code` — only `type`.
        let insufficient_balance_body = r#"{"error":{"type":"new_api_error","message":"用户额度不足, 剩余额度: ＄-0.398052 (request id: 202609020218166141364498268d9d6A3V7Qkt0)"},"type":"error"}"#;
        let upstream = start_fixed_upstream(StatusCode::FORBIDDEN, insufficient_balance_body).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id =
            create_proxy_api_credential(&pool, "insufficient balance", &upstream).await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
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
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");
        let _ = response.bytes().await.expect("proxy body");

        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "error");
        assert_eq!(
            credential.last_failure_kind.as_deref(),
            Some("semantic_response_failed")
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn switches_accounts_after_overload_retries_are_exhausted() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (first_upstream, first_calls) = start_sequence_upstream(3, r#"{"ok":"first"}"#).await;
        let (second_upstream, second_calls) =
            start_sequence_upstream(0, r#"{"ok":"second"}"#).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let first_id = create_proxy_api_credential(&pool, "first", &first_upstream).await;
        let second_id = create_proxy_api_credential(&pool, "second", &second_upstream).await;
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[first_id.clone(), second_id.clone()],
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
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json::<Value>().await.expect("proxy body")["ok"],
            "second"
        );
        assert_eq!(
            first_calls.load(std::sync::atomic::Ordering::SeqCst),
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize + 1
        );
        assert_eq!(second_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            RouteCredentialRepository::get(&pool, &first_id)
                .await
                .expect("first credential")
                .status,
            "ok"
        );
        assert_eq!(
            RouteCredentialRepository::get(&pool, &second_id)
                .await
                .expect("second credential")
                .status,
            "ok"
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn proxy_uses_account_specific_retry_count_for_overload_responses() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let (upstream, calls) = start_sequence_upstream(4, r#"{"ok":true}"#).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential_with_config(
            &pool,
            "custom retries",
            &upstream,
            json!({
                "failure_policy": {
                    "retry_count": 4,
                    "retry_interval_ms": 0,
                    "semantic_error_threshold": 10
                }
            }),
        )
        .await;
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential_id))
            .await
            .expect("pool members");
        let route_key =
            RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-test")
                .await
                .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start(&runtime, pool, RouteProxyTransport::Http)
            .await
            .expect("start proxy");

        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/chat/completions",
                proxy.base_url.as_deref().expect("base url")
            ))
            .bearer_auth(route_key)
            .json(&json!({"model":"gpt-5.5","messages":[]}))
            .send()
            .await
            .expect("proxy response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(calls.load(Ordering::SeqCst), 5);
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
                FailureScope::Account,
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

        RouteCredentialRepository::clear_transient_failure(&pool, &credential_id, None)
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
            route_priority: 3,
            max_concurrency: 1,
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
    fn credential_priority_order_prefers_lower_priority_and_round_robins_within_group() {
        let mut first = api_credential("priority-one-first", "openai");
        first.route_priority = 1;
        let mut second = api_credential("priority-one-second", "openai");
        second.route_priority = 1;
        let mut fallback = api_credential("priority-two", "openai");
        fallback.route_priority = 2;
        let credentials = vec![first, second, fallback];

        assert_eq!(
            credential_indexes_by_priority(&credentials, 0),
            vec![0, 1, 2]
        );
        assert_eq!(
            credential_indexes_by_priority(&credentials, 1),
            vec![1, 0, 2]
        );
        assert_eq!(
            credential_indexes_by_priority(&credentials, 2),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn partial_platform_api_requires_explicit_dialect() {
        let credential = SelectedCredential {
            id: "hermes-api".to_string(),
            platform: "hermes".to_string(),
            kind: "api".to_string(),
            display_name: "Hermes API".to_string(),
            status: "ok".to_string(),
            route_priority: 3,
            max_concurrency: 1,
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
            route_priority: 3,
            max_concurrency: 1,
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
    fn is_messages_path_matches_anthropic_messages_endpoint() {
        assert!(is_messages_path("/v1/messages"));
        assert!(is_messages_path("/messages/"));
        assert!(is_messages_path("messages"));
        assert!(!is_messages_path("/v1/chat/completions"));
        assert!(!is_messages_path("/v1/models"));
    }

    #[test]
    fn redacted_url_masks_the_gemini_key_but_keeps_other_query_parts() {
        // Gemini carries its credential in the URL, so the target URL is itself
        // secret-bearing and cannot be logged verbatim.
        assert_eq!(
            redact_sensitive_url(
                "https://generativelanguage.googleapis.com/v1beta/models/x:generateContent?alt=sse&key=AIzaSyVerySecret123"
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/x:generateContent?alt=sse&key=***t123"
        );
        // `?beta=true` must survive: it is the proxy-only query flag we need to
        // see when comparing an accepted request against a rejected one.
        assert_eq!(
            redact_sensitive_url("https://api.example.com/v1/messages?beta=true"),
            "https://api.example.com/v1/messages?beta=true"
        );
        assert_eq!(
            redact_sensitive_url("https://api.example.com/v1/messages"),
            "https://api.example.com/v1/messages"
        );
    }

    #[test]
    fn formatted_headers_mask_credentials_and_keep_client_identity() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer sk-ant-super-secret-value"),
        );
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("sk-short"),
        );
        headers.insert(
            HeaderName::from_static("user-agent"),
            HeaderValue::from_static("claude-cli/2.1.2 (external, cli)"),
        );
        headers.insert(
            HeaderName::from_static("anthropic-beta"),
            HeaderValue::from_static("claude-code-20250219"),
        );
        headers.insert(
            HeaderName::from_static("x-stainless-os"),
            HeaderValue::from_static("Windows"),
        );

        let rendered = format_upstream_headers(&headers);

        // Credentials never reach the log.
        assert!(!rendered.contains("super-secret-value"));
        assert!(!rendered.contains("sk-short"));
        assert!(rendered.contains("authorization: ***alue"));
        // A short secret is masked whole rather than leaking half of itself.
        assert!(rendered.contains("x-api-key: ***"));
        // The identity headers a gateway fingerprints on must be fully visible —
        // masking them would defeat the purpose of logging headers.
        assert!(rendered.contains("user-agent: claude-cli/2.1.2 (external, cli)"));
        assert!(rendered.contains("anthropic-beta: claude-code-20250219"));
        assert!(rendered.contains("x-stainless-os: Windows"));
        // Sorted so two requests can be diffed by eye.
        let names: Vec<&str> = rendered
            .lines()
            .filter_map(|line| line.split(':').next())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn ensure_query_flag_appends_only_when_absent() {
        assert_eq!(
            ensure_query_flag("https://api.example.com/v1/messages", "beta", "true"),
            "https://api.example.com/v1/messages?beta=true"
        );
        assert_eq!(
            ensure_query_flag(
                "https://api.example.com/v1/messages?stream=true",
                "beta",
                "true"
            ),
            "https://api.example.com/v1/messages?stream=true&beta=true"
        );
        assert_eq!(
            ensure_query_flag(
                "https://api.example.com/v1/messages?beta=true",
                "beta",
                "true"
            ),
            "https://api.example.com/v1/messages?beta=true"
        );
    }

    #[test]
    fn identity_is_left_alone_when_the_caller_is_already_an_sdk_client() {
        // Claude Code's own request: a complete Stainless SDK identity. Splicing
        // our hardcoded versions into it produced a combination no real client
        // emits, which is what agentrouter.org rejected as `unauthorized client
        // detected`.
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("anthropic-beta"),
            HeaderValue::from_static("interleaved-thinking-2025-05-14"),
        );
        headers.insert(
            HeaderName::from_static("user-agent"),
            HeaderValue::from_static("claude-cli/2.5.0 (external, cli)"),
        );
        headers.insert(
            HeaderName::from_static("x-stainless-package-version"),
            HeaderValue::from_static("0.99.0"),
        );
        headers.insert(
            HeaderName::from_static("x-stainless-runtime-version"),
            HeaderValue::from_static("v24.0.0"),
        );
        apply_claude_code_identity(&mut headers);

        // Every value the caller sent survives untouched.
        assert_eq!(
            headers.get("user-agent").and_then(|v| v.to_str().ok()),
            Some("claude-cli/2.5.0 (external, cli)")
        );
        assert_eq!(
            headers
                .get("x-stainless-package-version")
                .and_then(|v| v.to_str().ok()),
            Some("0.99.0")
        );
        assert_eq!(
            headers
                .get("x-stainless-runtime-version")
                .and_then(|v| v.to_str().ok()),
            Some("v24.0.0")
        );
        // We do not graft on the headers it chose to omit.
        assert!(!headers.contains_key("x-app"));
        assert!(!headers.contains_key("x-stainless-os"));
        // The beta marker is a capability signal, not identity, so it still merges.
        assert_eq!(
            headers.get("anthropic-beta").and_then(|v| v.to_str().ok()),
            Some("claude-code-20250219,interleaved-thinking-2025-05-14")
        );
    }

    #[test]
    fn identity_is_applied_whole_when_the_caller_has_none() {
        // A plain client (curl, a script, a non-SDK bridge) gets the full
        // impersonation so it can pass a fingerprinting gateway at all.
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("user-agent"),
            HeaderValue::from_static("curl/8.18.0"),
        );
        apply_claude_code_identity(&mut headers);

        // Our set wins outright — a half-curl half-CLI identity is exactly the
        // contradiction that gets rejected.
        assert_eq!(
            headers.get("user-agent").and_then(|v| v.to_str().ok()),
            Some(client_identity::CLAUDE_CODE_USER_AGENT)
        );
        assert_eq!(
            headers.get("x-app").and_then(|v| v.to_str().ok()),
            Some("cli")
        );
        assert!(headers.contains_key("x-stainless-package-version"));
        assert!(headers.contains_key("x-stainless-os"));
    }

    #[test]
    fn a_single_stainless_header_is_enough_to_count_as_an_sdk_identity() {
        let mut headers = HeaderMap::new();
        assert!(!has_stainless_sdk_identity(&headers));
        headers.insert(
            HeaderName::from_static("x-stainless-lang"),
            HeaderValue::from_static("js"),
        );
        assert!(has_stainless_sdk_identity(&headers));
    }

    #[test]
    fn apply_claude_code_identity_defaults_when_headers_absent() {
        let mut headers = HeaderMap::new();
        apply_claude_code_identity(&mut headers);
        assert_eq!(
            headers.get("anthropic-beta").and_then(|v| v.to_str().ok()),
            Some(client_identity::CLAUDE_CODE_DEFAULT_BETA)
        );
        assert_eq!(
            headers.get("user-agent").and_then(|v| v.to_str().ok()),
            Some(client_identity::CLAUDE_CODE_USER_AGENT)
        );
    }

    #[test]
    fn apply_codex_cli_identity_fills_originator_and_user_agent() {
        let mut headers = HeaderMap::new();
        apply_codex_cli_identity(&mut headers);
        assert_eq!(
            headers.get("originator").and_then(|v| v.to_str().ok()),
            Some(client_identity::CODEX_CLI_ORIGINATOR)
        );
        assert!(headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ua| ua.starts_with("codex_cli_rs/")));
    }

    #[test]
    fn response_body_metadata_uses_short_preview_on_success() {
        let body = vec![b'a'; ROUTE_PROXY_RESPONSE_BODY_LIMIT + 1024];
        let success_preview =
            route_proxy_response_body_metadata(Some(&body), true).expect("success body");
        assert_eq!(success_preview.len(), ROUTE_PROXY_SUCCESS_BODY_LIMIT);
        let failure_preview =
            route_proxy_response_body_metadata(Some(&body), false).expect("failure body");
        assert_eq!(failure_preview.len(), ROUTE_PROXY_RESPONSE_BODY_LIMIT);
        assert!(route_proxy_response_body_metadata(Some(b""), true).is_none());
        assert!(route_proxy_response_body_metadata(None, true).is_none());
    }

    #[test]
    fn force_identity_accept_encoding_overrides_client_value() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("accept-encoding"),
            HeaderValue::from_static("gzip, br, zstd"),
        );
        force_identity_accept_encoding(&mut headers);
        let values: Vec<_> = headers
            .get_all("accept-encoding")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        assert_eq!(values, vec!["identity"]);
    }

    #[test]
    fn collects_and_replaces_remote_image_urls() {
        let body = serde_json::json!({
            "messages": [
                {"role":"user","content":[
                    {"type":"text","text":"hi"},
                    {"type":"image_url","image_url":{"url":"https://example.com/a.png"}},
                    {"type":"input_image","image_url":"https://example.com/b.jpg"},
                    {"type":"image_url","image_url":{"url":"data:image/png;base64,AAAA"}}
                ]}
            ]
        });
        let mut urls = Vec::new();
        collect_remote_image_urls(&body, &mut urls);
        urls.sort();
        assert_eq!(
            urls,
            vec![
                "https://example.com/a.png".to_string(),
                "https://example.com/b.jpg".to_string()
            ]
        );

        let mut value = body.clone();
        let mut replacements = std::collections::HashMap::new();
        replacements.insert(
            "https://example.com/a.png".to_string(),
            "data:image/png;base64,ZZZ".to_string(),
        );
        replacements.insert(
            "https://example.com/b.jpg".to_string(),
            "data:image/jpeg;base64,YYY".to_string(),
        );
        replace_remote_image_urls(&mut value, &replacements);
        assert_eq!(
            value["messages"][0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,ZZZ"
        );
        assert_eq!(
            value["messages"][0]["content"][2]["image_url"],
            "data:image/jpeg;base64,YYY"
        );
        assert_eq!(
            value["messages"][0]["content"][3]["image_url"]["url"],
            "data:image/png;base64,AAAA"
        );
    }

    #[test]
    fn sniffs_image_mime_and_reads_flag() {
        assert_eq!(
            sniff_image_mime(&[0x89, 0x50, 0x4E, 0x47, 0, 0]).as_deref(),
            Some("image/png")
        );
        assert_eq!(
            sniff_image_mime(&[0xFF, 0xD8, 0xFF, 0]).as_deref(),
            Some("image/jpeg")
        );
        assert_eq!(sniff_image_mime(b"GIF89a").as_deref(), Some("image/gif"));
        assert!(sniff_image_mime(b"plain text bytes").is_none());
        assert_eq!(
            image_mime_from_url("https://x/y/a.PNG?v=1").as_deref(),
            Some("image/png")
        );
        assert!(!inline_remote_images_enabled(&serde_json::json!({})));
        assert!(inline_remote_images_enabled(
            &serde_json::json!({"inline_remote_images": true})
        ));
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
    fn retry_policy_marks_retryable_transport_and_upstream_statuses_as_transient() {
        assert!(should_retry_proxy_failure(StatusCode::UNAUTHORIZED));
        assert!(should_retry_proxy_failure(StatusCode::FORBIDDEN));
        assert!(should_retry_proxy_failure(StatusCode::TOO_MANY_REQUESTS));
        assert!(should_retry_proxy_failure(
            StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(should_retry_proxy_failure(StatusCode::BAD_GATEWAY));
        assert!(should_retry_proxy_failure(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!should_retry_proxy_failure(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn same_account_retry_excludes_authentication_failures() {
        assert!(should_retry_same_credential_status(
            StatusCode::REQUEST_TIMEOUT
        ));
        assert!(should_retry_same_credential_status(
            StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(should_retry_same_credential_status(
            StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(should_retry_same_credential_status(StatusCode::BAD_GATEWAY));
        assert!(should_retry_same_credential_status(
            StatusCode::GATEWAY_TIMEOUT
        ));
        assert!(!should_retry_same_credential_status(
            StatusCode::UNAUTHORIZED
        ));
        assert!(!should_retry_same_credential_status(StatusCode::FORBIDDEN));
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
                ..Default::default()
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
            br#"{"model":"claude-sonnet-alias [1M]","nested":{"model":"claude-opus-alias[1m]"}}"#,
            &[
                ModelMapping {
                    from: "claude-sonnet-alias".to_string(),
                    to: "provider-sonnet".to_string(),
                    label: Some("Sonnet".to_string()),
                    supports_1m: Some(true),
                    ..Default::default()
                },
                ModelMapping {
                    from: "claude-opus-alias".to_string(),
                    to: "provider-opus".to_string(),
                    label: Some("Opus".to_string()),
                    supports_1m: Some(true),
                    ..Default::default()
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
                ..Default::default()
            }],
        );
        let value: Value = serde_json::from_slice(&mapped).expect("json");

        assert_eq!(
            value.pointer("/model").and_then(Value::as_str),
            Some("gpt-5[1M]")
        );
    }

    #[test]
    fn apply_model_mappings_uses_fallback_only_when_no_specific_entry_matches() {
        // Fallback sits FIRST on purpose: a single-pass `.find()` would let it
        // swallow the specific claude-sonnet-alias entry that follows.
        let mapped = apply_model_mappings(
            br#"{"model":"claude-sonnet-alias","nested":{"model":"claude-opus-alias"}}"#,
            &[
                ModelMapping {
                    from: "claude-model".to_string(),
                    to: "fallback-upstream".to_string(),
                    label: None,
                    supports_1m: None,
                    ..Default::default()
                },
                ModelMapping {
                    from: "claude-sonnet-alias".to_string(),
                    to: "sonnet-upstream".to_string(),
                    label: None,
                    supports_1m: None,
                    ..Default::default()
                },
            ],
        );
        let value: Value = serde_json::from_slice(&mapped).expect("json");

        assert_eq!(
            value.pointer("/model").and_then(Value::as_str),
            Some("sonnet-upstream")
        );
        assert_eq!(
            value.pointer("/nested/model").and_then(Value::as_str),
            Some("fallback-upstream")
        );
    }

    #[test]
    fn apply_model_mappings_rewrites_the_subagent_alias() {
        let mapped = apply_model_mappings(
            br#"{"model":"claude-subagent"}"#,
            &[ModelMapping {
                from: "claude-subagent".to_string(),
                to: "provider-haiku".to_string(),
                label: None,
                supports_1m: None,
                ..Default::default()
            }],
        );
        let value: Value = serde_json::from_slice(&mapped).expect("json");

        assert_eq!(
            value.pointer("/model").and_then(Value::as_str),
            Some("provider-haiku")
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
        let assistant = value["messages"]
            .as_array()
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message["role"] == "assistant")
            })
            .expect("assistant message");
        assert_eq!(
            assistant.pointer("/tool_calls/0/function/name"),
            Some(&json!("apply_patch"))
        );
        assert_eq!(
            assistant.pointer("/tool_calls/0/function/arguments"),
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
    fn extract_usage_breakdown_merges_anthropic_streaming_frames() {
        // Anthropic splits usage across frames: input and cache tokens arrive in
        // `message_start` (nested under `message`), output tokens in `message_delta`.
        let body = b"event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":120,\"output_tokens\":1,\"cache_read_input_tokens\":40,\"cache_creation_input_tokens\":10}}}\n\
\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\
\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":88}}\n\
\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\n";

        let usage = extract_usage_breakdown(body);

        assert_eq!(usage.input_tokens, Some(120));
        assert_eq!(usage.output_tokens, Some(88));
        assert_eq!(usage.cache_tokens, Some(50));
        assert_eq!(extract_token_count(body), Some(208));
    }

    #[test]
    fn extract_usage_breakdown_reads_openai_final_streaming_chunk() {
        // OpenAI reports usage only in the final chunk; earlier chunks carry `"usage":null`.
        let body = b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}],\"usage\":null}\n\
\n\
data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":64,\"completion_tokens\":12,\"prompt_tokens_details\":{\"cached_tokens\":32}}}\n\
\n\
data: [DONE]\n\n";

        let usage = extract_usage_breakdown(body);

        assert_eq!(usage.input_tokens, Some(64));
        assert_eq!(usage.output_tokens, Some(12));
        assert_eq!(usage.cache_tokens, Some(32));
    }

    #[test]
    fn extract_usage_breakdown_reads_gemini_streaming_frames() {
        let body = b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hi\"}]}}]}\n\
\n\
data: {\"usageMetadata\":{\"promptTokenCount\":31,\"candidatesTokenCount\":9,\"cachedContentTokenCount\":7}}\n\n";

        let usage = extract_usage_breakdown(body);

        assert_eq!(usage.input_tokens, Some(31));
        assert_eq!(usage.output_tokens, Some(9));
        assert_eq!(usage.cache_tokens, Some(7));
    }

    #[test]
    fn extract_usage_breakdown_keeps_usage_from_truncated_stream() {
        // A client that disconnects mid-response leaves a partial trailing frame.
        // The usage already received must survive that unparsable tail.
        let body = b"event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":77,\"output_tokens\":2}}}\n\
\n\
data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"partia";

        let usage = extract_usage_breakdown(body);

        assert_eq!(usage.input_tokens, Some(77));
        assert_eq!(usage.output_tokens, Some(2));
    }

    #[test]
    fn extract_usage_breakdown_reads_streaming_price_and_total_tokens() {
        // Gateways that price requests themselves put the price in the tail frame.
        let priced = extract_usage_breakdown(
            b"data: {\"usage\":{\"input_tokens\":5,\"output_tokens\":6,\"price_cny\":7.1}}\n\n",
        );
        assert_eq!(priced.price_cny_micros, Some(7_100_000));
        assert_eq!(priced.price_currency.as_deref(), Some("cny"));

        // total_tokens-only shapes must still resolve through the SSE path.
        assert_eq!(
            extract_token_count(b"data: {\"usage\":{\"total_tokens\":404}}\n\n"),
            Some(404)
        );
    }

    #[test]
    fn extract_usage_breakdown_stays_empty_for_non_sse_garbage() {
        let usage = extract_usage_breakdown(b"<html><body>502 Bad Gateway</body></html>");

        assert_eq!(
            usage,
            crate::models::route_pool::RouteUsageBreakdown::default()
        );
        assert_eq!(extract_token_count(b"<html>502</html>"), None);
    }

    #[test]
    fn estimated_price_fills_in_cost_when_upstream_omits_it() {
        // The case behind "cost is always 0": Anthropic reports tokens, no price.
        let body =
            br#"{"model":"claude-opus-5","usage":{"input_tokens":1000000,"output_tokens":0}}"#;
        let mut usage = extract_usage_breakdown(body);
        assert_eq!(usage.price_usd_micros, None, "upstream sends no price");

        apply_estimated_price(&mut usage, extract_response_model(body).as_deref());

        // 1M input tokens at $5/MTok.
        assert_eq!(usage.price_usd_micros, Some(5_000_000));
        assert_eq!(usage.price_currency.as_deref(), Some("usd"));
        assert_eq!(usage.price_source.as_deref(), Some("estimated"));
    }

    #[test]
    fn upstream_price_is_never_overwritten_by_an_estimate() {
        let mut usage = extract_usage_breakdown(
            br#"{"model":"claude-opus-5","usage":{"input_tokens":1000000,"output_tokens":0,"cost_usd":0.25}}"#,
        );

        apply_estimated_price(&mut usage, Some("claude-opus-5"));

        // The real charge stands, tagged as such rather than replaced by $5.
        assert_eq!(usage.price_usd_micros, Some(250_000));
        assert_eq!(usage.price_source.as_deref(), Some("upstream"));
    }

    #[test]
    fn unknown_model_is_left_unpriced_rather_than_free() {
        let mut usage = extract_usage_breakdown(
            br#"{"model":"some-unreleased-model","usage":{"input_tokens":500,"output_tokens":5}}"#,
        );

        apply_estimated_price(&mut usage, Some("some-unreleased-model"));

        assert_eq!(usage.price_usd_micros, None);
        assert_eq!(usage.price_source, None, "unpriced must be distinguishable");
    }

    #[test]
    fn zero_token_response_is_not_priced() {
        // A failed or empty response should not gain a price row.
        let mut usage = crate::models::route_pool::RouteUsageBreakdown::default();
        apply_estimated_price(&mut usage, Some("claude-opus-5"));

        assert_eq!(usage.price_usd_micros, None);
        assert_eq!(usage.price_source, None);
    }

    #[test]
    fn response_model_is_read_from_json_and_streaming_shapes() {
        assert_eq!(
            extract_response_model(br#"{"model":"claude-opus-5"}"#).as_deref(),
            Some("claude-opus-5")
        );
        // Anthropic nests the model under `message` in `message_start`.
        assert_eq!(
            extract_response_model(
                b"data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-opus-alias\"}}\n\n"
            )
            .as_deref(),
            Some("claude-opus-alias")
        );
        // Gemini reports `modelVersion`.
        assert_eq!(
            extract_response_model(br#"{"modelVersion":"gemini-2.5-flash"}"#).as_deref(),
            Some("gemini-2.5-flash")
        );
        assert_eq!(extract_response_model(b"<html>502</html>"), None);
    }

    #[test]
    fn streaming_anthropic_response_is_priced_end_to_end() {
        // The full regression: a streaming Anthropic reply used to persist NULL
        // tokens and NULL cost. It must now yield both.
        let body = b"event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-opus-5\",\"usage\":{\"input_tokens\":1000000,\"output_tokens\":1,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n\
\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":1000000}}\n\
\n\
data: [DONE]\n\n";

        let mut usage = extract_usage_breakdown(body);
        apply_estimated_price(&mut usage, extract_response_model(body).as_deref());

        assert_eq!(usage.input_tokens, Some(1_000_000));
        assert_eq!(usage.output_tokens, Some(1_000_000));
        // $5 input + $25 output.
        assert_eq!(usage.price_usd_micros, Some(30_000_000));
        assert_eq!(usage.price_source.as_deref(), Some("estimated"));
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
            activity: RouteCredentialActivityRegistry::default(),
            live_log: RouteProxyLiveLog::default(),
            codex_history: CodexReasoningCache::default(),
            upstream_timeouts: ProxyAppState::default_upstream_timeouts(),
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
            activity: RouteCredentialActivityRegistry::default(),
            live_log: RouteProxyLiveLog::default(),
            codex_history: CodexReasoningCache::default(),
            upstream_timeouts: ProxyAppState::default_upstream_timeouts(),
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

    #[tokio::test]
    async fn resolve_platform_rejects_unknown_proxy_key() {
        use crate::database::{create_memory_pool, run_migrations};

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let state = ProxyAppState {
            pool,
            key_cache: Arc::new(Mutex::new(RouteProxyKeyCache::default())),
            activity: RouteCredentialActivityRegistry::default(),
            live_log: RouteProxyLiveLog::default(),
            codex_history: CodexReasoningCache::default(),
            upstream_timeouts: ProxyAppState::default_upstream_timeouts(),
        };

        let key = "sk-invalid";
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-invalid"),
        );
        let error = resolve_platform(&state, &headers, Some(key))
            .await
            .expect_err("unknown proxy key must be rejected");

        let message = match error {
            AppError::Validation { code, message, .. } => {
                assert_eq!(code, "route_proxy.key_invalid");
                message
            }
            other => panic!("unexpected error: {other:?}"),
        };
        assert!(!message.contains(key));
    }

    #[test]
    fn build_upstream_request_uses_official_cpa_base_url_and_headers() {
        let credential = SelectedCredential {
            id: "official-grok".to_string(),
            platform: "grok".to_string(),
            kind: "official".to_string(),
            display_name: "Grok OAuth".to_string(),
            status: "ok".to_string(),
            route_priority: 3,
            max_concurrency: 1,
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
            route_priority: 3,
            max_concurrency: 1,
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
            route_priority: 3,
            max_concurrency: 1,
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
            route_priority: 3,
            max_concurrency: 1,
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

    fn anthropic_api_credential() -> SelectedCredential {
        SelectedCredential {
            id: "claude-api-1".to_string(),
            platform: "claude".to_string(),
            kind: "api".to_string(),
            display_name: "Claude API".to_string(),
            status: "ok".to_string(),
            route_priority: 1,
            max_concurrency: 1,
            secret_payload_json: serde_json::json!({"api_key": "sk-test"}).to_string(),
            config_json: serde_json::json!({
                "base_url": "https://api.example.com",
                "interface_format": "anthropic",
                "model_mappings": [
                    {"from": "claude-opus-alias", "to": "provider-opus", "supports_1m": true}
                ]
            })
            .to_string(),
        }
    }

    #[test]
    fn the_one_m_suffix_adds_the_beta_marker_that_actually_enables_it() {
        // Claude Code signals 1M with a `[1M]` model suffix, which the mapping
        // lookup strips — so the suffix alone reaches the gateway as an ordinary
        // request and it replies "please enable 1m context and retry".
        let (_, headers, body) = build_upstream_request(
            &anthropic_api_credential(),
            "claude",
            "/v1/messages",
            None,
            HeaderMap::new(),
            br#"{"model":"claude-opus-alias[1M]","max_tokens":16}"#,
        )
        .expect("1M request");

        let beta = headers
            .get("anthropic-beta")
            .and_then(|value| value.to_str().ok())
            .expect("anthropic-beta header");
        assert!(
            beta.contains(client_identity::ANTHROPIC_ONE_M_CONTEXT_BETA),
            "beta header must carry the 1M marker: {beta}"
        );
        // The Claude Code identity marker still has to survive the merge.
        assert!(beta.contains(client_identity::CLAUDE_CODE_BETA_MARKER));
        // The suffix itself is stripped for the upstream body — that is exactly
        // why the header has to carry the intent.
        let sent: Value = serde_json::from_slice(&body).expect("body json");
        assert_eq!(sent["model"], "provider-opus");
    }

    #[test]
    fn a_plain_model_gets_no_one_m_marker() {
        let (_, headers, _) = build_upstream_request(
            &anthropic_api_credential(),
            "claude",
            "/v1/messages",
            None,
            HeaderMap::new(),
            br#"{"model":"claude-opus-alias","max_tokens":16}"#,
        )
        .expect("plain request");

        let beta = headers
            .get("anthropic-beta")
            .and_then(|value| value.to_str().ok())
            .expect("anthropic-beta header");
        assert!(
            !beta.contains(client_identity::ANTHROPIC_ONE_M_CONTEXT_BETA),
            "a request that did not ask for 1M must not claim it: {beta}"
        );
    }

    /// A credential of `platform` whose upstream speaks `dialect`, with the
    /// per-turn reminder switched on.
    fn reminder_credential(
        platform: &str,
        dialect: &str,
        text: Option<&str>,
    ) -> SelectedCredential {
        let mut config = serde_json::json!({
            "base_url": "https://api.example.com",
            "interface_format": dialect,
            "model_mappings": [],
            "turn_reminder": true,
        });
        if let Some(text) = text {
            config["turn_reminder_text"] = serde_json::json!(text);
        }
        SelectedCredential {
            id: format!("{platform}-{dialect}"),
            platform: platform.to_string(),
            kind: "api".to_string(),
            display_name: format!("{platform} via {dialect}"),
            status: "ok".to_string(),
            route_priority: 1,
            max_concurrency: 1,
            secret_payload_json: serde_json::json!({"api_key": "sk-test"}).to_string(),
            config_json: config.to_string(),
        }
    }

    #[test]
    fn the_turn_reminder_lands_in_each_upstream_dialects_own_shape() {
        // Injection has to happen after protocol bridging: a Codex client speaks
        // Responses, so against an Anthropic upstream the body it produces is
        // `messages`, and against Gemini it is `contents`. Each row below asserts
        // the pointer where that dialect actually carries user text.
        for (platform, dialect, path, request, pointer) in [
            (
                "claude",
                "anthropic",
                "/v1/messages",
                r#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#,
                "/messages/0/content/1/text",
            ),
            (
                "codex",
                "openai",
                "/v1/responses",
                r#"{"model":"m","input":"hi"}"#,
                // No tools in this request, so no system message is prepended and
                // the single converted user turn sits at index 0.
                "/messages/0/content",
            ),
            (
                "codex",
                "openai-responses",
                "/v1/responses",
                r#"{"model":"m","input":"hi"}"#,
                "/input",
            ),
            (
                "codex",
                "gemini",
                "/v1/responses",
                r#"{"model":"m","input":"hi"}"#,
                "/contents/0/parts/1/text",
            ),
        ] {
            let (_, _, body) = build_upstream_request(
                &reminder_credential(platform, dialect, None),
                platform,
                path,
                None,
                HeaderMap::new(),
                request.as_bytes(),
            )
            .unwrap_or_else(|error| panic!("{platform}/{dialect}: {error}"));

            let sent: Value = serde_json::from_slice(&body).expect("body json");
            let carried = sent
                .pointer(pointer)
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{platform}/{dialect}: nothing at {pointer}"));
            assert!(
                carried.contains(turn_reminder::DEFAULT_TURN_REMINDER),
                "{platform}/{dialect}: {pointer} = {carried}"
            );
        }
    }

    #[test]
    fn a_custom_reminder_text_replaces_the_default() {
        let (_, _, body) = build_upstream_request(
            &reminder_credential("claude", "anthropic", Some("Answer in Japanese.")),
            "claude",
            "/v1/messages",
            None,
            HeaderMap::new(),
            br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#,
        )
        .expect("request");

        let sent: Value = serde_json::from_slice(&body).expect("body json");
        let text = sent
            .pointer("/messages/0/content/1/text")
            .and_then(Value::as_str)
            .expect("reminder block");
        assert_eq!(text, "Answer in Japanese.");
        assert!(!text.contains(turn_reminder::DEFAULT_TURN_REMINDER));
    }

    #[test]
    fn an_account_without_the_reminder_sends_a_byte_identical_body() {
        let request = br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#;
        let mut off = reminder_credential("claude", "anthropic", None);
        off.config_json = serde_json::json!({
            "base_url": "https://api.example.com",
            "interface_format": "anthropic",
            "model_mappings": [],
        })
        .to_string();

        let (_, _, body) = build_upstream_request(
            &off,
            "claude",
            "/v1/messages",
            None,
            HeaderMap::new(),
            request,
        )
        .expect("request");

        // Anthropic→Anthropic is a passthrough, so an untouched body is literally
        // the input bytes. Anything else means the feature leaked when off.
        assert_eq!(body, request.to_vec());
    }

    #[test]
    fn the_connectivity_probe_opts_out_of_the_reminder() {
        // The probe asks for exactly `ai-switch-ok`. A reminder that says "answer
        // in Chinese" contradicts that, so applying it here would make the probe
        // fail forever on any account that enables the reminder — the account
        // would look broken when only the probe was.
        let credential = reminder_credential("claude", "anthropic", None);
        let request = br#"{"model":"m","messages":[{"role":"user","content":"Reply with exactly: ai-switch-ok"}]}"#;

        let probe = build_upstream_request_with_bridge(
            &credential,
            "claude",
            "/v1/messages",
            None,
            HeaderMap::new(),
            request,
            TurnReminderMode::Skip,
        )
        .expect("probe request");
        assert_eq!(probe.body, request.to_vec());

        // Same credential on the proxy path still gets it — proving the exemption
        // is the probe's, not a dead config reader.
        let forwarded = build_upstream_request_with_bridge(
            &credential,
            "claude",
            "/v1/messages",
            None,
            HeaderMap::new(),
            request,
            TurnReminderMode::Apply,
        )
        .expect("forwarded request");
        assert_ne!(forwarded.body, request.to_vec());
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
            route_priority: 3,
            max_concurrency: 1,
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
    fn strip_unsupported_hosted_tools_drops_web_search_keeps_functions() {
        let body = br#"{
            "model":"gpt-5.5",
            "tools":[
                {"type":"web_search"},
                {"type":"function","name":"exec_command","parameters":{"type":"object"}},
                {"type":"custom","name":"apply_patch"},
                {"type":"file_search"}
            ],
            "tool_choice":{"type":"web_search"},
            "input":"hi"
        }"#;
        let out = strip_unsupported_hosted_tools(body);
        let value: Value = serde_json::from_slice(&out).unwrap();
        let tools = value["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools
            .iter()
            .all(|tool| matches!(tool["type"].as_str(), Some("function") | Some("custom"))));
        // A tool_choice pinned to a removed hosted tool is relaxed to auto.
        assert_eq!(value["tool_choice"], "auto");
    }

    #[test]
    fn strip_unsupported_hosted_tools_noop_without_hosted_tools() {
        let body = br#"{"tools":[{"type":"function","name":"exec_command"}],"input":"hi"}"#;
        let out = strip_unsupported_hosted_tools(body);
        assert_eq!(out, body.to_vec());
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
    fn filter_credentials_for_model_keeps_baseline_and_matching_mappings_only() {
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
            "codex",
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

        let selected = filter_credentials_for_model(
            "codex",
            vec![wildcard, sol],
            Some("deepseek-v4-flash-0731"),
        );
        assert!(selected.is_empty());
    }

    #[test]
    fn filter_credentials_for_model_keeps_fallback_accounts_for_unmatched_models() {
        let wildcard = api_credential_with_config("wildcard", r#"{"model_mappings":[]}"#);
        let sol = api_credential_with_config(
            "sol",
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"sol-upstream"}]}"#,
        );
        let fallback = api_credential_with_config(
            "fallback",
            r#"{"model_mappings":[{"from":"claude-model","to":"catch-all-upstream"}]}"#,
        );

        // An unknown model used to empty the candidate list and hard-fail the
        // whole request with route_pool.model_unmatched.
        let selected = filter_credentials_for_model(
            "codex",
            vec![wildcard.clone(), sol.clone(), fallback.clone()],
            Some("deepseek-v4-flash-0731"),
        );
        assert_eq!(
            selected
                .iter()
                .map(|item| item.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["fallback"]
        );

        // A baseline model still keeps every eligible account.
        let selected = filter_credentials_for_model(
            "codex",
            vec![wildcard, sol, fallback],
            Some("gpt-5.6-sol"),
        );
        assert_eq!(
            selected
                .iter()
                .map(|item| item.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["wildcard", "sol", "fallback"]
        );
    }

    #[test]
    fn official_credentials_are_not_selected_for_synthetic_aliases() {
        // Official upstreams get the body unrewritten, so a synthetic alias
        // would reach the vendor verbatim and 404.
        let mut official = api_credential_with_config(
            "official",
            r#"{"model_mappings":[{"from":"claude-subagent","to":"provider-haiku"}]}"#,
        );
        official.kind = "official".to_string();
        official.platform = "claude".to_string();
        let api = api_credential_with_config(
            "api",
            r#"{"model_mappings":[{"from":"claude-subagent","to":"provider-haiku"}]}"#,
        );

        let selected =
            filter_credentials_for_model("claude", vec![official, api], Some("claude-subagent"));

        assert_eq!(
            selected
                .iter()
                .map(|item| item.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["api"]
        );
    }

    #[test]
    fn official_credentials_with_a_fallback_keep_baseline_only_semantics() {
        let mut official = api_credential_with_config(
            "official",
            r#"{"model_mappings":[{"from":"claude-model","to":"catch-all"}]}"#,
        );
        official.kind = "official".to_string();
        official.platform = "claude".to_string();

        // Stripping its only (synthetic) mapping collapses it to the
        // baseline-only wildcard — exactly its pre-feature behavior.
        let unmatched = filter_credentials_for_model(
            "claude",
            vec![official.clone()],
            Some("deepseek-v4-flash-0731"),
        );
        assert!(unmatched.is_empty());

        let baseline =
            filter_credentials_for_model("claude", vec![official], Some("claude-sonnet-alias"));
        assert_eq!(baseline.len(), 1);
    }

    fn candidate(id: &str, cooldown_until: Option<&str>, model_key: Option<&str>) -> PoolCandidate {
        PoolCandidate {
            credential: SelectedCredential {
                id: id.to_string(),
                platform: "codex".to_string(),
                kind: "api".to_string(),
                display_name: id.to_string(),
                status: "ok".to_string(),
                route_priority: 3,
                max_concurrency: 5,
                secret_payload_json: r#"{"api_key":"sk"}"#.to_string(),
                config_json: r#"{"base_url":"https://example.com","model_mappings":[]}"#
                    .to_string(),
            },
            cooldown_until: cooldown_until.map(str::to_string),
            model_key: model_key.map(str::to_string),
        }
    }

    fn model_state(
        credential_id: &str,
        model_key: &str,
        status: &str,
        cooldown_until: Option<&str>,
    ) -> ((String, String), RouteCredentialModelState) {
        (
            (credential_id.to_string(), model_key.to_string()),
            RouteCredentialModelState {
                route_credential_id: credential_id.to_string(),
                model_key: model_key.to_string(),
                status: status.to_string(),
                transient_failure_count: 1,
                cooldown_until: cooldown_until.map(str::to_string),
                semantic_failure_streak_count: 0,
                semantic_failure_streak_fingerprint: None,
                last_failure_kind: None,
                last_failure_message: None,
                last_failure_response_json: None,
                aliases: Vec::new(),
                created_at: "2026-09-02T00:00:00Z".to_string(),
                updated_at: "2026-09-02T00:00:00Z".to_string(),
            },
        )
    }

    fn now_for_partition() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-02T12:00:00Z")
            .expect("fixed now")
            .with_timezone(&Utc)
    }

    /// Model filtering now works on candidates, but the pre-existing assertions
    /// are about which accounts survive — wrap so they keep reading that way.
    fn filter_credentials_for_model(
        platform: &str,
        credentials: Vec<SelectedCredential>,
        requested_model: Option<&str>,
    ) -> Vec<SelectedCredential> {
        filter_candidates_for_model(
            platform,
            credentials
                .into_iter()
                .map(|credential| PoolCandidate {
                    credential,
                    cooldown_until: None,
                    model_key: None,
                })
                .collect(),
            requested_model,
        )
        .into_iter()
        .map(|candidate| candidate.credential)
        .collect()
    }

    #[test]
    fn a_cooling_model_does_not_park_its_siblings_on_the_same_account() {
        let now = now_for_partition();
        let states = HashMap::from([model_state(
            "cred-1",
            "upstream-sol",
            MODEL_STATUS_OK,
            Some("2026-09-02T12:00:30Z"),
        )]);

        // The request asks for the healthy sibling, so the account is eligible.
        let healthy = partition_by_cooldown(
            vec![candidate("cred-1", None, Some("upstream-glm"))],
            &states,
            now,
        );
        assert_eq!(
            healthy.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["cred-1"]
        );

        // The same account for the cooling model: only reachable as the
        // all-cooling probe, never as a normal pick.
        let cooling = partition_by_cooldown(
            vec![
                candidate("cred-1", None, Some("upstream-sol")),
                candidate("cred-2", None, Some("upstream-sol")),
            ],
            &states,
            now,
        );
        assert_eq!(
            cooling.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["cred-2"]
        );
    }

    #[test]
    fn account_level_cooldown_still_parks_every_model() {
        let now = now_for_partition();
        let selected = partition_by_cooldown(
            vec![
                candidate(
                    "cooling",
                    Some("2026-09-02T12:05:00Z"),
                    Some("upstream-glm"),
                ),
                candidate("ready", None, Some("upstream-glm")),
            ],
            &HashMap::new(),
            now,
        );
        assert_eq!(
            selected.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["ready"]
        );
    }

    #[test]
    fn paused_and_error_models_are_hard_excluded_even_when_nothing_else_is_left() {
        let now = now_for_partition();
        let states = HashMap::from([
            model_state("paused-acc", "upstream-sol", MODEL_STATUS_PAUSED, None),
            model_state("error-acc", "upstream-sol", MODEL_STATUS_ERROR, None),
        ]);
        let selected = partition_by_cooldown(
            vec![
                candidate("paused-acc", None, Some("upstream-sol")),
                candidate("error-acc", None, Some("upstream-sol")),
            ],
            &states,
            now,
        );
        // No probe fallback: unlike a cooldown these are verdicts, not waits.
        assert!(selected.is_empty());
    }

    #[test]
    fn all_cooling_falls_back_to_the_earliest_recovering_candidate() {
        let now = now_for_partition();
        let states = HashMap::from([
            model_state(
                "late",
                "upstream-sol",
                MODEL_STATUS_OK,
                Some("2026-09-02T12:10:00Z"),
            ),
            model_state(
                "soon",
                "upstream-sol",
                MODEL_STATUS_OK,
                Some("2026-09-02T12:01:00Z"),
            ),
        ]);
        let selected = partition_by_cooldown(
            vec![
                candidate("late", None, Some("upstream-sol")),
                candidate("soon", None, Some("upstream-sol")),
            ],
            &states,
            now,
        );
        assert_eq!(
            selected.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["soon"]
        );
    }

    #[test]
    fn a_candidate_without_a_model_key_only_consults_account_level_cooldown() {
        let now = now_for_partition();
        // A Gemini-style request carries its model in the path, so there is no
        // key to look up; the model table must not park it.
        let states = HashMap::from([model_state(
            "cred-1",
            "upstream-sol",
            MODEL_STATUS_PAUSED,
            None,
        )]);
        let selected = partition_by_cooldown(vec![candidate("cred-1", None, None)], &states, now);
        assert_eq!(
            selected.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["cred-1"]
        );
    }

    #[test]
    fn filter_candidates_for_model_records_the_upstream_key() {
        let mut item = candidate("cred-1", None, None);
        item.credential.config_json =
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#.to_string();
        let filtered = filter_candidates_for_model("codex", vec![item], Some("gpt-5.6-sol"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].model_key.as_deref(), Some("upstream-sol"));
    }

    #[test]
    fn filter_candidates_for_model_leaves_the_key_empty_without_a_requested_model() {
        let filtered =
            filter_candidates_for_model("codex", vec![candidate("cred-1", None, None)], None);
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].model_key.is_none());
    }

    #[tokio::test]
    async fn a_failing_model_does_not_take_out_its_sibling_on_the_same_account() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let upstream = start_per_model_upstream("upstream-sol").await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Dual Model",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-upstream"}"#,
            &json!({
                "base_url": upstream,
                "interface_format": "openai",
                "model_mappings": [
                    {"from": "gpt-5.6-sol", "to": "upstream-sol"},
                    {"from": "glm-5.3", "to": "upstream-glm"}
                ],
                "failure_policy": {"cooldown_enabled": true, "cooldown_seconds": 600, "retry_count": 0}
            })
            .to_string(),
            "{}",
        )
        .await
        .expect("create credential");
        RoutePoolRepository::replace_members(&pool, "codex", &[credential.id.clone()])
            .await
            .expect("pool members");
        let route_key =
            RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-per-model")
                .await
                .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start(&runtime, pool.clone(), RouteProxyTransport::Http)
            .await
            .expect("start proxy");
        let endpoint = format!(
            "{}/v1/chat/completions",
            proxy.base_url.as_deref().expect("base url")
        );
        let client = reqwest::Client::new();
        let post = |model: &'static str| {
            let client = client.clone();
            let endpoint = endpoint.clone();
            let route_key = route_key.clone();
            async move {
                client
                    .post(&endpoint)
                    .bearer_auth(&route_key)
                    .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
                    .json(&json!({"model": model, "messages": []}))
                    .send()
                    .await
                    .expect("proxy response")
            }
        };

        // 1. The failing model parks itself.
        assert_eq!(
            post("gpt-5.6-sol").await.status(),
            reqwest::StatusCode::BAD_GATEWAY
        );
        let states =
            RouteCredentialModelRepository::list_for_credentials(&pool, &[credential.id.clone()])
                .await
                .expect("model states");
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "upstream-sol");
        assert!(states[0].cooldown_until.is_some());

        // 2. The sibling still works — this is the regression this feature exists for.
        assert_eq!(post("glm-5.3").await.status(), reqwest::StatusCode::OK);

        // 3. The account itself was never parked, so the cooling model can still
        //    be probed as the last resort.
        let stored = RouteCredentialRepository::get(&pool, &credential.id)
            .await
            .expect("account row");
        assert!(stored.cooldown_until.is_none());
        assert_eq!(
            post("gpt-5.6-sol").await.status(),
            reqwest::StatusCode::BAD_GATEWAY
        );

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn a_healthy_account_wins_over_one_whose_model_is_cooling() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let failing = start_per_model_upstream("upstream-sol").await;
        let healthy = start_fixed_upstream(StatusCode::OK, r#"{"route":"healthy"}"#).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let cooling_id = create_proxy_api_credential_with_mappings(
            &pool,
            "cooling",
            &failing,
            json!([{"from": "gpt-5.6-sol", "to": "upstream-sol"}]),
        )
        .await;
        sqlx::query("UPDATE route_credentials SET config_json = json_set(config_json, '$.failure_policy', json('{\"cooldown_enabled\":true,\"cooldown_seconds\":600,\"retry_count\":0}')) WHERE id = ?")
            .bind(&cooling_id)
            .execute(&pool)
            .await
            .expect("enable cooldown");
        let healthy_id = create_proxy_api_credential_with_mappings(
            &pool,
            "healthy",
            &healthy,
            json!([{"from": "gpt-5.6-sol", "to": "upstream-sol"}]),
        )
        .await;
        RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[cooling_id.clone(), healthy_id.clone()],
        )
        .await
        .expect("pool members");
        let route_key = RouteProxyKeyRepository::ensure_platform_key(
            &pool,
            "codex",
            "sk-ai-switch-model-failover",
        )
        .await
        .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start(&runtime, pool.clone(), RouteProxyTransport::Http)
            .await
            .expect("start proxy");
        let endpoint = format!(
            "{}/v1/chat/completions",
            proxy.base_url.as_deref().expect("base url")
        );
        let client = reqwest::Client::new();

        // First request fails over from the cooling account to the healthy one.
        let first = client
            .post(&endpoint)
            .bearer_auth(&route_key)
            .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
            .json(&json!({"model": "gpt-5.6-sol", "messages": []}))
            .send()
            .await
            .expect("first response");
        assert_eq!(first.status(), reqwest::StatusCode::OK);

        // Second request skips the parked model outright.
        let second = client
            .post(&endpoint)
            .bearer_auth(&route_key)
            .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
            .json(&json!({"model": "gpt-5.6-sol", "messages": []}))
            .send()
            .await
            .expect("second response");
        assert_eq!(second.status(), reqwest::StatusCode::OK);
        assert_eq!(second.text().await.expect("body"), r#"{"route":"healthy"}"#);

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
    }

    #[tokio::test]
    async fn every_model_cooling_escalates_to_an_account_level_cooldown() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        // This upstream fails everything, so both mapped models get parked.
        let upstream = start_fixed_upstream(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"rate limited"}}"#,
        )
        .await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let credential_id = create_proxy_api_credential_with_mappings(
            &pool,
            "all-models",
            &upstream,
            json!([
                {"from": "gpt-5.6-sol", "to": "upstream-sol"},
                {"from": "glm-5.3", "to": "upstream-glm"}
            ]),
        )
        .await;
        sqlx::query("UPDATE route_credentials SET config_json = json_set(config_json, '$.failure_policy', json('{\"cooldown_enabled\":true,\"cooldown_seconds\":600,\"retry_count\":0}')) WHERE id = ?")
            .bind(&credential_id)
            .execute(&pool)
            .await
            .expect("enable cooldown");
        RoutePoolRepository::replace_members(&pool, "codex", &[credential_id.clone()])
            .await
            .expect("pool members");
        let route_key =
            RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-escalate")
                .await
                .expect("route key");
        let runtime = RouteProxyRuntimeState::default();
        let proxy = RouteProxyService::start(&runtime, pool.clone(), RouteProxyTransport::Http)
            .await
            .expect("start proxy");
        let endpoint = format!(
            "{}/v1/chat/completions",
            proxy.base_url.as_deref().expect("base url")
        );
        let client = reqwest::Client::new();
        for model in ["gpt-5.6-sol", "glm-5.3"] {
            let _ = client
                .post(&endpoint)
                .bearer_auth(&route_key)
                .header(ROUTE_PROXY_PLATFORM_HEADER, "codex")
                .json(&json!({"model": model, "messages": []}))
                .send()
                .await
                .expect("response");
        }

        let stored = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("account row");
        // With nothing left to serve, the account itself backs off — otherwise a
        // fully-down relay would be re-probed once per model forever.
        assert!(stored.cooldown_until.is_some());

        RouteProxyService::stop(&runtime).await.expect("stop proxy");
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

    #[tokio::test]
    async fn route_proxy_auth_errors_use_stable_codes_and_challenge() {
        let cases = [
            (
                "route_proxy.platform_unresolved: provide a local route proxy key",
                "route_proxy.auth_required",
            ),
            (
                "route_proxy.key_invalid: local route proxy key is invalid",
                "route_proxy.key_invalid",
            ),
        ];

        for (message, expected_code) in cases {
            assert_eq!(route_proxy_error_status(message), StatusCode::UNAUTHORIZED);
            let response = json_error(StatusCode::UNAUTHORIZED, message);
            assert_eq!(
                response
                    .headers()
                    .get(axum::http::header::WWW_AUTHENTICATE)
                    .and_then(|value| value.to_str().ok()),
                Some("Bearer")
            );
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("error body");
            let value: Value = serde_json::from_slice(&body).expect("error json");
            assert_eq!(
                value.pointer("/error/code").and_then(Value::as_str),
                Some(expected_code)
            );
        }
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
            vec!["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5"]
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
    fn models_list_excludes_the_fallback_sentinel() {
        let mut credential = api_credential("claude-fallback", "anthropic");
        credential.platform = "claude".to_string();
        credential.config_json = serde_json::json!({
            "base_url": "https://api.example.com",
            "interface_format": "anthropic",
            "model_mappings": [
                {"from":"claude-sonnet-alias","to":"provider-sonnet"},
                {"from":"claude-model","to":"catch-all-upstream"}
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

        // The catch-all is a routing sentinel, not a model id — it must never be
        // advertised.
        assert!(!models.contains(&FALLBACK_MODEL_ALIAS), "models={models:?}");
        assert!(models.contains(&"claude-sonnet-alias"), "models={models:?}");
        // A fallback account accepts anything, so it advertises the baseline.
        assert!(models.contains(&"claude-haiku-alias"), "models={models:?}");
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
                    "from": "claude-sonnet-alias",
                    "to": "provider-sonnet",
                    "supports_1m": true
                },
                {
                    "from": "claude-opus-alias",
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
            vec![
                "claude-sonnet-alias",
                "claude-sonnet-alias[1m]",
                "claude-opus-alias"
            ]
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

        let payload = build_route_models_list_payload("codex", &[credential]);
        assert_eq!(payload.get("object").and_then(Value::as_str), Some("list"));
        let data = payload.get("data").and_then(Value::as_array).expect("data");
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].get("id").and_then(Value::as_str), Some("gpt-5.5"));
        assert_eq!(data[0].get("object").and_then(Value::as_str), Some("model"));
        assert_eq!(
            data[0].get("owned_by").and_then(Value::as_str),
            Some("ai-switch")
        );
        assert_eq!(
            data[0]["supported_reasoning_levels"]
                .as_array()
                .expect("reasoning levels")
                .iter()
                .filter_map(|level| level.get("effort").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["low", "medium", "high", "xhigh"]
        );
        assert_eq!(data[0]["default_reasoning_level"].as_str(), Some("medium"));
        assert_eq!(data[1].get("id").and_then(Value::as_str), Some("gpt-5"));
        // Nothing was declared, so the upstream's own default is stated. The
        // client cannot derive it — it never sees the mapped-to name.
        assert_eq!(data[0]["context_window"].as_u64(), Some(256_000));
        assert!(payload.get("models").is_none());

        let response = json_models_list_response("codex", &[], None);
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn codex_models_endpoint_states_the_one_m_default_for_a_one_m_upstream() {
        let mut credential = api_credential("first", "openai");
        credential.config_json = serde_json::json!({
            "model_mappings": [
                {"from": "gpt-5.6-sol", "to": "deepseek-v4-flash-0731"},
                {"from": "gpt-5.5", "to": "gpt-5.5"}
            ]
        })
        .to_string();

        let payload = build_route_models_list_payload("codex", &[credential]);
        let data = payload.get("data").and_then(Value::as_array).expect("data");

        assert_eq!(data[0]["context_window"].as_u64(), Some(1_000_000));
        assert_eq!(data[1]["context_window"].as_u64(), Some(256_000));
    }

    #[test]
    fn codex_models_endpoint_advertises_per_mapping_catalog_overrides() {
        let mut credential = api_credential("first", "openai");
        credential.config_json = serde_json::json!({
            "base_url": "https://api.example.com/v1",
            "interface_format": "openai",
            "model_mappings": [{
                "from": "gpt-5.5",
                "to": "up-a",
                "context_window": 400000,
                "reasoning_levels": ["xhigh", "max"]
            }]
        })
        .to_string();

        let payload = build_route_models_list_payload("codex", &[credential]);
        let data = payload.get("data").and_then(Value::as_array).expect("data");

        assert_eq!(data[0]["context_window"].as_u64(), Some(400_000));
        assert_eq!(
            data[0]["supported_reasoning_levels"]
                .as_array()
                .expect("reasoning levels")
                .iter()
                .filter_map(|level| level.get("effort").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["xhigh", "max"]
        );
        // gpt-5.5 defaults to medium, which the custom list drops.
        assert_eq!(data[0]["default_reasoning_level"].as_str(), Some("xhigh"));
    }

    #[test]
    fn non_codex_models_endpoint_carries_no_catalog_fields() {
        let mut credential = api_credential("first", "anthropic");
        credential.config_json = serde_json::json!({
            "model_mappings": [{
                "from": "claude-sonnet-alias",
                "to": "up-sonnet",
                "context_window": 400000,
                "reasoning_levels": ["max"]
            }]
        })
        .to_string();

        let payload = build_route_models_list_payload("claude", &[credential]);
        let data = payload.get("data").and_then(Value::as_array).expect("data");

        // Claude clients do not read these keys, and a stray value here would be
        // a config the user cannot see or clear from the Claude editor.
        assert!(data[0].get("context_window").is_none());
        assert!(data[0].get("supported_reasoning_levels").is_none());
    }

    #[test]
    fn codex_catalog_payload_is_separate_from_models_endpoint() {
        let mut credential = api_credential("first", "openai");
        credential.config_json = serde_json::json!({
            "model_mappings": [{"from":"gpt-5.6-sol","to":"sol-upstream"}]
        })
        .to_string();

        let capability = parse_model_capability(&credential.config_json);
        let payload =
            crate::services::route_model_capability::codex_model_catalog_payload(&[capability]);
        let models = payload
            .get("models")
            .and_then(Value::as_array)
            .expect("codex models");
        assert_eq!(models.len(), 1);
        assert_eq!(
            models[0].get("slug").and_then(Value::as_str),
            Some("gpt-5.6-sol")
        );
        assert_eq!(
            models[0].get("display_name").and_then(Value::as_str),
            Some("gpt-5.6-sol")
        );
        assert_eq!(
            models[0].get("visibility").and_then(Value::as_str),
            Some("list")
        );
        assert_eq!(
            models[0].get("supported_in_api").and_then(Value::as_bool),
            Some(true)
        );
        assert!(payload.get("object").is_none());
        assert!(payload.get("data").is_none());
        assert!(build_route_models_list_payload("codex", &[credential])
            .get("models")
            .is_none());
    }

    #[test]
    fn count_tokens_path_is_recognized_across_version_spellings() {
        for path in [
            "/v1/messages/count_tokens",
            "/messages/count_tokens",
            "/v1/messages/count_tokens/",
        ] {
            assert!(
                is_anthropic_count_tokens_path(path),
                "should recognize {path}"
            );
        }
        // The chat endpoint itself, and unrelated sub-resources, must not match.
        for path in [
            "/v1/messages",
            "/v1/messages/batches",
            "/v1/responses",
            "/v1/count_tokens",
        ] {
            assert!(
                !is_anthropic_count_tokens_path(path),
                "should not recognize {path}"
            );
        }
    }

    #[test]
    fn count_tokens_estimate_covers_text_and_skips_base64() {
        let empty = estimate_anthropic_input_tokens(br#"{"messages":[]}"#);
        assert_eq!(empty, 0, "an empty request has no tokens");

        let body = serde_json::json!({
            "model": "claude-sonnet-4",
            "system": "You are concise.",
            "messages": [{"role": "user", "content": [{"type": "text", "text": "Hello there"}]}]
        });
        let counted = estimate_anthropic_input_tokens(&serde_json::to_vec(&body).unwrap());
        assert!(counted > 0, "text content must produce a positive estimate");

        // A base64 image must not inflate the estimate by its encoded size.
        let with_image = serde_json::json!({
            "model": "claude-sonnet-4",
            "system": "You are concise.",
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "Hello there"},
                {"type": "image", "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "A".repeat(100_000)
                }}
            ]}]
        });
        let with_image_count =
            estimate_anthropic_input_tokens(&serde_json::to_vec(&with_image).unwrap());
        assert!(
            with_image_count < counted + 500,
            "base64 payload must not dominate the estimate: {with_image_count} vs {counted}"
        );
    }

    #[test]
    fn count_tokens_response_uses_anthropic_shape() {
        let body = serde_json::json!({
            "model": "claude-sonnet-4",
            "messages": [{"role": "user", "content": "Count these tokens please"}]
        });
        let response = json_count_tokens_response(&serde_json::to_vec(&body).unwrap());

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
    }

    /// Sweeps how one upstream reacts to each way of declaring the 1M context
    /// window, so a report of "1M does not work on relay X" can be answered with
    /// evidence instead of a guess.
    ///
    /// Opt in with two env vars; nothing runs without them:
    ///
    /// ```text
    /// ONE_M_PROBE_URL=https://relay.example/v1/messages     /// ONE_M_PROBE_KEY=sk-...     ///   cargo test --lib probe_one_m_declaration -- --ignored --nocapture
    /// ```
    ///
    /// `ONE_M_PROBE_MODEL` overrides the upstream model name (default
    /// `claude-opus-5`) — send the real upstream name, not one of our aliases,
    /// since this bypasses the proxy's mapping.
    ///
    /// Use the rustls-backed client the proxy itself uses. A relay's edge may
    /// accept rustls while rejecting curl (Schannel) and Python (OpenSSL) at the
    /// TLS handshake — mistaking that for an upstream outage cost real debugging
    /// time on anyrouter.top, where every tool but this one failed to connect.
    ///
    /// Recorded result for anyrouter.top (2026-08-24): the header form is the only
    /// one it reads. Without it, every combination returns 400 "1m 上下文已经全量
    /// 可用，请启用 1m 上下文后重试"; with it, every combination returns 503. The
    /// body array (which cc-switch also sends, as `anthropic_beta`) and adaptive
    /// thinking changed nothing. So its 1M backend was simply unavailable — the
    /// relay demanded a declaration it could not then serve.
    #[tokio::test]
    #[ignore = "diagnostic: set ONE_M_PROBE_URL and ONE_M_PROBE_KEY to hit a live relay"]
    async fn probe_one_m_declaration() {
        let (Ok(url), Ok(key)) = (
            std::env::var("ONE_M_PROBE_URL"),
            std::env::var("ONE_M_PROBE_KEY"),
        ) else {
            println!("  set ONE_M_PROBE_URL and ONE_M_PROBE_KEY to run this probe");
            return;
        };
        let model =
            std::env::var("ONE_M_PROBE_MODEL").unwrap_or_else(|_| "claude-opus-5".to_string());
        let client = build_outbound_http_client(Some(Duration::from_secs(60))).expect("client");

        // cc-switch declares 1M in two places: the `anthropic-beta` *header* and an
        // `anthropic_beta` *body array* (note the underscore). This sweep isolates
        // which form a given relay reads, and whether adaptive thinking matters.
        for (label, header_one_m, body_one_m, adaptive) in [
            ("A 头无 体无        ", false, false, false),
            ("B 头有 体无        ", true, false, false),
            ("C 头无 体有        ", false, true, false),
            ("D 头有 体有        ", true, true, false),
            ("E 头有 体有 +thinking", true, true, true),
            ("F 头无 体有 +thinking", false, true, true),
        ] {
            let mut body = serde_json::json!({
                "model": model,
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}],
            });
            if body_one_m {
                body["anthropic_beta"] =
                    serde_json::json!([client_identity::ANTHROPIC_ONE_M_CONTEXT_BETA]);
            }
            if adaptive {
                body["thinking"] = serde_json::json!({"type": "adaptive"});
                body["output_config"] = serde_json::json!({"effort": "max"});
            }
            let beta = if header_one_m {
                "context-1m-2025-08-07,claude-code-20250219,interleaved-thinking-2025-05-14"
            } else {
                "claude-code-20250219,interleaved-thinking-2025-05-14"
            };
            match client
                .post(&url)
                .header("content-type", "application/json")
                .header("x-api-key", key.as_str())
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", beta)
                .header("user-agent", client_identity::CLAUDE_CODE_USER_AGENT)
                .body(body.to_string())
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    let text = response.text().await.unwrap_or_default();
                    let text: String = text.chars().take(160).collect();
                    println!("  [{label}] http={status} {text}");
                }
                Err(error) => println!("  [{label}] ERR {error}"),
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
}
