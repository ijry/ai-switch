use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
use crate::error::AppError;
use crate::services::http_client::build_outbound_http_client;
use crate::models::route_credential::{
    normalize_anthropic_api_key_field, ModelMapping, ANTHROPIC_API_KEY_FIELD,
    ANTHROPIC_AUTH_TOKEN_FIELD,
};
use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use chrono::Utc;
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
use uuid::Uuid;

const BIND_HOST: &str = "127.0.0.1";
const ROUTE_PROXY_KEY_CACHE_TTL: Duration = Duration::from_secs(30);
/// Public xAI Grok CLI OAuth client ID (CLIProxyAPI / Grok CLI).
const XAI_OAUTH_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
// Keep in sync with CLIProxyAPI xai_executor (cli-chat-proxy identity headers).
const GROK_CLI_CLIENT_VERSION: &str = "0.2.93";
const GROK_CLI_TOKEN_AUTH_VALUE: &str = "xai-grok-cli";
const GROK_CLI_CHAT_PROXY_MARKER: &str = "cli-chat-proxy.grok.com";
/// Refresh a short time before wall-clock expiry to avoid edge 401s.
const OAUTH_REFRESH_LEAD: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteProxyStatus {
    pub running: bool,
    pub bind_host: String,
    pub port: Option<u16>,
    pub base_url: Option<String>,
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

        let listener =
            TcpListener::bind((BIND_HOST, 0))
                .await
                .map_err(|err| AppError::Filesystem {
                    code: "filesystem.route_proxy_bind",
                    message: "Could not bind local route proxy".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;
        let addr = listener.local_addr().map_err(|err| AppError::Filesystem {
            code: "filesystem.route_proxy_addr",
            message: "Could not resolve route proxy address".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        let port = addr.port();
        let base_url = format!("http://{BIND_HOST}:{port}");

        let app_state = ProxyAppState {
            pool,
            key_cache: Arc::new(Mutex::new(RouteProxyKeyCache::default())),
        };
        let app = Router::new()
            .fallback(any(proxy_handler))
            .with_state(app_state);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let join_handle = tokio::spawn(async move {
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
        });

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
    match forward_request(&state, method, headers, uri, body).await {
        Ok(response) => response,
        Err(err) => json_error(StatusCode::BAD_GATEWAY, &err),
    }
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
    let inbound_key = extract_inbound_api_key(&headers, query.as_deref());
    let platform = resolve_platform(state, &path, &headers, inbound_key.as_deref())
        .await
        .map_err(|err| err.to_string())?;
    let credentials = load_pool_credentials(pool, &platform)
        .await
        .map_err(|err| err.to_string())?;
    let cursor = RoutePoolRepository::next_cursor_index(pool, &platform)
        .await
        .map_err(|err| err.to_string())?;
    let selected = pick_credential(&credentials, cursor)
        .ok_or_else(|| "No enabled route credentials in pool".to_string())?;
    let next_index = (cursor.rem_euclid(credentials.len() as i64) + 1) % credentials.len() as i64;
    let credential = maybe_refresh_official_credential(pool, selected).await?;

    let body_bytes = axum::body::to_bytes(body, 32 * 1024 * 1024)
        .await
        .map_err(|err| format!("Could not read proxy request body: {err}"))?;

    let mut outbound_headers = HeaderMap::new();
    for (name, value) in headers.iter() {
        if is_hop_by_hop_header(name) {
            continue;
        }
        outbound_headers.append(name.clone(), value.clone());
    }

    let (target_url, outbound_headers, outbound_body) = build_upstream_request(
        &credential,
        &platform,
        &path,
        query.as_deref(),
        outbound_headers,
        &body_bytes,
    )?;

    let client = build_outbound_http_client(None)?;
    let request = client
        .request(
            reqwest::Method::from_bytes(method.as_str().as_bytes())
                .map_err(|err| format!("Unsupported method: {err}"))?,
            &target_url,
        )
        .headers(map_to_reqwest_headers(&outbound_headers))
        .body(outbound_body);

    let upstream = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            mark_route_credential_unavailable(pool, &credential.id).await;
            return Err(format!("Upstream request failed: {err}"));
        }
    };

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if should_mark_proxy_account_unavailable(status) {
        mark_route_credential_unavailable(pool, &credential.id).await;
    }
    let upstream_headers = upstream.headers().clone();
    let response_bytes = upstream
        .bytes()
        .await
        .map_err(|err| format!("Could not read upstream response: {err}"))?;
    // Capture official subscription/quota signals (e.g. Grok free-usage-exhausted).
    if credential.kind == "official" {
        if let Ok(body_text) = std::str::from_utf8(&response_bytes) {
            let _ = maybe_persist_official_quota_from_response(pool, &credential, body_text).await;
        }
    }

    let token_count = extract_token_count(&response_bytes);
    let cost_micros = extract_cost_micros(&response_bytes);
    let metadata = serde_json::json!({
        "platform": platform,
        "route_credential_id": credential.id,
        "route_credential_name": credential.display_name,
        "path": path,
        "status": status.as_u16(),
    })
    .to_string();

    let _ =
        insert_route_credential_usage_event(pool, &credential.id, "request", 1, "count", &metadata)
            .await;
    if let Some(tokens) = token_count {
        if tokens > 0 {
            let _ = insert_route_credential_usage_event(
                pool,
                &credential.id,
                "token",
                tokens,
                "token",
                &metadata,
            )
            .await;
        }
    }
    if let Some(cost) = cost_micros {
        if cost > 0 {
            let _ = insert_route_credential_usage_event(
                pool,
                &credential.id,
                "cost",
                cost,
                "usd_micros",
                &metadata,
            )
            .await;
        }
    }
    let _ = RoutePoolRepository::save_cursor_index(pool, &platform, next_index).await;

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
        .map_err(|err| format!("Could not build proxy response: {err}"))
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

fn should_mark_proxy_account_unavailable(status: StatusCode) -> bool {
    matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
}

async fn mark_route_credential_unavailable(pool: &SqlitePool, credential_id: &str) {
    let _ = RouteCredentialRepository::update_status(pool, credential_id, "error").await;
}

async fn mark_route_credential_revoked(pool: &SqlitePool, credential_id: &str) {
    let _ = RouteCredentialRepository::update_status(pool, credential_id, "revoked").await;
}

fn json_error(status: StatusCode, message: &str) -> Response {
    let code = if message.contains("No enabled route credentials in pool") {
        "route_pool.empty"
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
    (status, [("content-type", "application/json")], body).into_response()
}

async fn resolve_platform(
    state: &ProxyAppState,
    path: &str,
    headers: &HeaderMap,
    inbound_key: Option<&str>,
) -> Result<String, AppError> {
    // Preferred: stable per-platform local proxy key written into CLI configs.
    // Keys are cached in memory and refreshed at most every 30s.
    if let Some(key) = inbound_key {
        if let Some(platform) = lookup_platform_by_proxy_key(state, key).await? {
            return Ok(normalize_route_platform(&platform));
        }
    }
    Ok(detect_platform(path, headers))
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

pub fn detect_platform(path: &str, headers: &HeaderMap) -> String {
    if let Some(value) = headers
        .get("x-ai-switch-platform")
        .and_then(|value| value.to_str().ok())
    {
        return normalize_route_platform(value);
    }

    let path_lower = path.to_lowercase();
    if path_lower.contains("anthropic")
        || path_lower.contains("claude")
        || path_lower.contains("/messages")
        || path_lower.contains("/v1/messages")
    {
        return "claude".to_string();
    }
    if path_lower.contains("gemini")
        || path_lower.contains("generativelanguage")
        || path_lower.contains(":generatecontent")
    {
        return "gemini".to_string();
    }
    if path_lower.contains("grok") || path_lower.contains("xai") || path_lower.contains("x.ai") {
        return "grok".to_string();
    }
    "codex".to_string()
}

pub fn normalize_route_platform(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if normalized.contains("claude") || normalized.contains("anthropic") {
        "claude".to_string()
    } else if normalized.contains("grok") || normalized.contains("xai") || normalized.contains("x.ai") {
        "grok".to_string()
    } else if normalized.contains("gemini") || normalized.contains("google") {
        "gemini".to_string()
    } else {
        "codex".to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedCredential {
    pub id: String,
    pub platform: String,
    pub kind: String,
    pub display_name: String,
    pub secret_payload_json: String,
    pub config_json: String,
}

async fn load_pool_credentials(
    pool: &SqlitePool,
    platform: &str,
) -> Result<Vec<SelectedCredential>, AppError> {
    let rows = sqlx::query(
        "SELECT c.id, c.platform, c.kind, c.display_name, c.secret_payload_json, c.config_json
         FROM route_pool_members rpm
         INNER JOIN route_credentials c ON c.id = rpm.route_credential_id
         WHERE rpm.platform = ?
           AND rpm.enabled = 1
           AND c.status = 'ok'
           AND (c.quota_remaining IS NULL OR c.quota_remaining > 0)
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

    Ok(rows
        .into_iter()
        .map(|row| SelectedCredential {
            id: row.get("id"),
            platform: row.get("platform"),
            kind: row.get("kind"),
            display_name: row.get("display_name"),
            secret_payload_json: row.get("secret_payload_json"),
            config_json: row.get("config_json"),
        })
        // Skip official accounts already known to have zero remaining quota.
        .filter(|credential| is_route_credential_quota_available(&credential.config_json))
        .collect())
}

pub fn pick_credential(items: &[SelectedCredential], cursor: i64) -> Option<&SelectedCredential> {
    if items.is_empty() {
        return None;
    }
    let index = cursor.rem_euclid(items.len() as i64) as usize;
    items.get(index)
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
    mut headers: HeaderMap,
    body: &[u8],
) -> Result<(String, HeaderMap, Vec<u8>), String> {
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
) -> Result<(String, HeaderMap, Vec<u8>), String> {
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
    let interface_format = string_value(config, "interface_format").unwrap_or("openai");
    let mappings = model_mappings(config);
    let rewritten_body = apply_model_mappings(body, &mappings);
    let mut target_url = build_target_url(base_url, path, query);

    match interface_format {
        "anthropic" | "anthropic-messages" => {
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
        "gemini" => {
            target_url = append_query_param(&target_url, "key", api_key);
        }
        "openai" | "openai-responses" => {
            insert_header(headers, "authorization", &format!("Bearer {api_key}"))?;
        }
        other => {
            return Err(format!("Unsupported interface format: {other}"));
        }
    }

    let _ = platform;
    Ok((target_url, headers.clone(), rewritten_body))
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
) -> Result<(String, HeaderMap, Vec<u8>), String> {
    // Apply credential-provided headers first (CPA may ship extra headers).
    apply_config_headers(headers, config)?;

    let access_token = resolve_official_access_token(credential, secret, config)?;
    insert_header(headers, "authorization", &format!("Bearer {access_token}"))?;
    if platform == "claude" {
        headers
            .entry(HeaderName::from_static("anthropic-version"))
            .or_insert(HeaderValue::from_static("2023-06-01"));
    }
    let base_url = string_value(config, "base_url").unwrap_or_else(|| default_official_base_url(platform));
    // cli-chat-proxy rejects unversioned clients with HTTP 426 (version = none).
    if is_official_grok_platform(platform) && is_grok_cli_chat_proxy_base_url(base_url) {
        apply_official_grok_cli_headers(headers)?;
    }
    let target_url = build_target_url(base_url, path, query);
    Ok((target_url, headers.clone(), body.to_vec()))
}

fn is_official_grok_platform(platform: &str) -> bool {
    matches!(platform, "grok" | "xai")
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
        let Some(value) = value.as_str().map(str::trim).filter(|item| !item.is_empty()) else {
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
                            <= Utc::now() + chrono::Duration::from_std(OAUTH_REFRESH_LEAD).unwrap_or_default();
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

    let refreshed = match refresh_oauth_access_token(
        &token_endpoint,
        &refresh_token,
        client_id.as_deref(),
    )
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

    secret_obj.insert("access_token".to_string(), Value::String(refreshed.access_token.clone()));
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
        if let Some(expired_at) = Utc::now().checked_add_signed(chrono::Duration::seconds(expires_in)) {
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
    config_obj.insert("last_refresh".to_string(), Value::String(Utc::now().to_rfc3339()));

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
        format!(
            "官方 OAuth 凭证已失效（revoked），请重新导入 CPA 授权文件。原始错误：{message}"
        )
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
    pub quota_remaining: Option<i64>,
    pub quota_limit: Option<i64>,
    pub quota_used: Option<i64>,
}

pub fn is_route_credential_quota_available(config_json: &str) -> bool {
    // Unknown/missing remaining means "not known exhausted" — keep selectable.
    let Ok(config) = parse_json_object(config_json, "config") else {
        return true;
    };
    match config.get("quota_remaining") {
        None | Some(Value::Null) => true,
        Some(Value::Number(value)) => value.as_i64().map(|remaining| remaining > 0).unwrap_or(true),
        Some(Value::String(value)) => value
            .trim()
            .parse::<i64>()
            .map(|remaining| remaining > 0)
            .unwrap_or(true),
        Some(_) => true,
    }
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

pub fn apply_official_quota_snapshot(config_json: &str, snapshot: &OfficialQuotaSnapshot) -> Result<String, String> {
    let mut config = parse_json_object(config_json, "config")?;
    let Some(object) = config.as_object_mut() else {
        return Err("Route credential config JSON must be an object".to_string());
    };
    if let Some(subscription_type) = &snapshot.subscription_type {
        object.insert(
            "subscription_type".to_string(),
            json!(subscription_type),
        );
    }
    if let Some(quota_remaining) = snapshot.quota_remaining {
        object.insert("quota_remaining".to_string(), json!(quota_remaining));
    }
    if let Some(quota_limit) = snapshot.quota_limit {
        object.insert("quota_limit".to_string(), json!(quota_limit));
    }
    if let Some(quota_used) = snapshot.quota_used {
        object.insert("quota_used".to_string(), json!(quota_used));
    }
    object.insert(
        "quota_updated_at".to_string(),
        json!(Utc::now().to_rfc3339()),
    );
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
    let next_config = apply_official_quota_snapshot(&credential.config_json, &snapshot).map_err(
        |message| AppError::Validation {
            code: "validation.route_credential_quota",
            message,
            details: Some(credential.id.clone()),
            recoverable: true,
        },
    )?;
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

fn model_mappings(config: &Value) -> Vec<ModelMapping> {
    config
        .get("model_mappings")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<ModelMapping>>(value).ok())
        .map(remove_placeholder_model_mappings)
        .unwrap_or_default()
}

fn remove_placeholder_model_mappings(mappings: Vec<ModelMapping>) -> Vec<ModelMapping> {
    mappings
        .into_iter()
        .filter(|mapping| {
            !is_placeholder_model(&mapping.from) && !is_placeholder_model(&mapping.to)
        })
        .collect()
}

fn is_placeholder_model(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value == "upstream-model"
}

fn insert_header(headers: &mut HeaderMap, name: &'static str, value: &str) -> Result<(), String> {
    let value =
        HeaderValue::from_str(value).map_err(|err| format!("Invalid header value: {err}"))?;
    headers.insert(HeaderName::from_static(name), value);
    Ok(())
}

fn default_official_base_url(platform: &str) -> &'static str {
    match platform {
        "claude" => "https://api.anthropic.com",
        // CLIProxyAPI xAI official API base for Grok.
        "grok" => "https://api.x.ai/v1",
        "gemini" => "https://generativelanguage.googleapis.com",
        _ => "https://api.openai.com",
    }
}

fn append_query_param(url: &str, key: &str, value: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{key}={value}")
}

pub fn build_target_url(base_url: &str, path: &str, query: Option<&str>) -> String {
    let base = base_url.trim().trim_end_matches('/');
    let normalized_path = if path.is_empty() {
        "".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let mut url = format!("{base}{normalized_path}");
    if let Some(query) = query {
        if !query.is_empty() {
            url.push('?');
            url.push_str(query);
        }
    }
    url
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

pub fn extract_token_count(body: &[u8]) -> Option<i64> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    if let Some(total) = value
        .pointer("/usage/total_tokens")
        .and_then(|item| item.as_i64())
    {
        return Some(total);
    }

    let input = value
        .pointer("/usage/input_tokens")
        .and_then(|item| item.as_i64())
        .or_else(|| {
            value
                .pointer("/usage/prompt_tokens")
                .and_then(|item| item.as_i64())
        })
        .unwrap_or(0);
    let output = value
        .pointer("/usage/output_tokens")
        .and_then(|item| item.as_i64())
        .or_else(|| {
            value
                .pointer("/usage/completion_tokens")
                .and_then(|item| item.as_i64())
        })
        .unwrap_or(0);

    let total = input + output;
    if total > 0 {
        Some(total)
    } else {
        None
    }
}

pub fn extract_cost_micros(body: &[u8]) -> Option<i64> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    if let Some(micros) = value
        .pointer("/usage/cost_micros")
        .and_then(|item| item.as_i64())
    {
        return Some(micros);
    }
    if let Some(usd) = value
        .pointer("/usage/cost_usd")
        .and_then(|item| item.as_f64())
    {
        return Some((usd * 1_000_000.0).round() as i64);
    }
    None
}

async fn insert_route_credential_usage_event(
    pool: &SqlitePool,
    route_credential_id: &str,
    metric_type: &str,
    amount: i64,
    unit: &str,
    metadata_json: &str,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO usage_events
         (id, route_credential_id, source_label, metric_type, amount, unit, metadata_json, created_at)
         VALUES (?, ?, 'route_proxy', ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(route_credential_id)
    .bind(metric_type)
    .bind(amount)
    .bind(unit)
    .bind(metadata_json)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|err| AppError::Database {
        code: "database.route_proxy_usage",
        message: "Could not record route proxy usage event".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_credential(name: &str, interface_format: &str) -> SelectedCredential {
        SelectedCredential {
            id: name.to_string(),
            platform: "codex".to_string(),
            kind: "api".to_string(),
            display_name: name.to_string(),
            secret_payload_json: r#"{"api_key":"sk-test"}"#.to_string(),
            config_json: serde_json::json!({
                "base_url": "https://api.example.com/v1",
                "interface_format": interface_format,
                "model_mappings": [{"from":"gpt-5","to":"up-gpt"}]
            })
            .to_string(),
        }
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
    fn extract_token_count_supports_openai_and_anthropic_shapes() {
        let openai = br#"{"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
        let anthropic = br#"{"usage":{"input_tokens":11,"output_tokens":7}}"#;
        assert_eq!(extract_token_count(openai), Some(15));
        assert_eq!(extract_token_count(anthropic), Some(18));
    }

    #[test]
    fn detect_platform_uses_header_and_path_hints() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ai-switch-platform", HeaderValue::from_static("gemini"));
        assert_eq!(detect_platform("/v1/chat/completions", &headers), "gemini");
        assert_eq!(detect_platform("/v1/messages", &HeaderMap::new()), "claude");

        let mut grok_headers = HeaderMap::new();
        grok_headers.insert("x-ai-switch-platform", HeaderValue::from_static("xai"));
        assert_eq!(detect_platform("/v1/chat/completions", &grok_headers), "grok");
        assert_eq!(detect_platform("/v1/grok/chat/completions", &HeaderMap::new()), "grok");
        assert_eq!(normalize_route_platform("Grok"), "grok");
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

    #[tokio::test]
    async fn resolve_platform_prefers_proxy_key_over_path_default() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
        use crate::database::{create_memory_pool, run_migrations};

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "grok", "sk-ai-switch-grok")
            .await
            .expect("store key");

        let state = ProxyAppState {
            pool,
            key_cache: Arc::new(Mutex::new(RouteProxyKeyCache::default())),
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-ai-switch-grok"),
        );
        // Same OpenAI path would default to codex without key mapping.
        let platform = resolve_platform(
            &state,
            "/v1/chat/completions",
            &headers,
            Some("sk-ai-switch-grok"),
        )
        .await
        .expect("resolve");
        assert_eq!(platform, "grok");

        // Second lookup should hit the in-memory cache (still within 30s TTL).
        let cached = resolve_platform(
            &state,
            "/v1/chat/completions",
            &headers,
            Some("sk-ai-switch-grok"),
        )
        .await
        .expect("cached resolve");
        assert_eq!(cached, "grok");

        let fallback = resolve_platform(&state, "/v1/chat/completions", &HeaderMap::new(), None)
            .await
            .expect("fallback");
        assert_eq!(fallback, "codex");
    }

    #[test]
    fn build_upstream_request_uses_official_cpa_base_url_and_headers() {
        let credential = SelectedCredential {
            id: "official-grok".to_string(),
            platform: "grok".to_string(),
            kind: "official".to_string(),
            display_name: "Grok OAuth".to_string(),
            secret_payload_json: r#"{"access_token":"at-xai","refresh_token":"rt-xai"}"#.to_string(),
            config_json: serde_json::json!({
                "base_url": "https://cli-chat-proxy.grok.com/v1",
                "token_endpoint": "https://auth.x.ai/oauth2/token",
                "headers": {
                    "User-Agent": "grok-cli",
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

        assert_eq!(
            url,
            "https://cli-chat-proxy.grok.com/v1/chat/completions"
        );
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
    fn build_upstream_request_skips_cli_headers_for_official_xai_api() {
        let credential = SelectedCredential {
            id: "official-grok-api".to_string(),
            platform: "grok".to_string(),
            kind: "official".to_string(),
            display_name: "Grok API".to_string(),
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
    fn is_route_credential_quota_available_filters_zero_remaining() {
        assert!(is_route_credential_quota_available("{}"));
        assert!(is_route_credential_quota_available(r#"{"quota_remaining":12}"#));
        assert!(!is_route_credential_quota_available(r#"{"quota_remaining":0}"#));
        assert!(!is_route_credential_quota_available(r#"{"quota_remaining":-1}"#));
        assert!(!is_route_credential_quota_available(r#"{"quota_remaining":"0"}"#));
        assert!(is_route_credential_quota_available(r#"{"quota_remaining":"5"}"#));
    }

    #[test]
    fn parse_official_quota_snapshot_from_free_usage_exhausted() {

        let body = r#"{
  "code": "subscription:free-usage-exhausted",
  "error": "You've used all the included free usage for model grok-4.5-build-free for now. Usage resets over a rolling 24-hour window — tokens (actual/limit): 1177205/1000000. Upgrade to a Grok subscription for higher limits: https://grok.com/supergrok"
}"#;
        let snapshot = parse_official_quota_snapshot(body).expect("snapshot");
        assert_eq!(snapshot.subscription_type.as_deref(), Some("free"));
        assert_eq!(snapshot.quota_remaining, Some(0));
        assert_eq!(snapshot.quota_used, Some(1_177_205));
        assert_eq!(snapshot.quota_limit, Some(1_000_000));

        let next = apply_official_quota_snapshot("{}", &snapshot).expect("config");
        assert!(next.contains("\"subscription_type\":\"free\""));
        assert!(next.contains("\"quota_remaining\":0"));
        assert!(next.contains("\"quota_used\":1177205"));
        assert!(next.contains("\"quota_limit\":1000000"));
        assert!(next.contains("quota_updated_at"));
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
        let token = "eyJhbGciOiJub25lIn0.eyJleHAiOjE4OTM0NTYwMDAsImNsaWVudF9pZCI6ImNpZC1mcm9tLWp3dCJ9.sig";
        assert_eq!(jwt_claim_i64(token, "exp"), Some(1893456000));
        assert_eq!(
            jwt_claim_string(token, "client_id").as_deref(),
            Some("cid-from-jwt")
        );
    }

}
