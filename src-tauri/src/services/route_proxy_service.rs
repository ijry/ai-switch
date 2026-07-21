use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
use crate::error::AppError;
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
use serde_json::Value;
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
    let credential = pick_credential(&credentials, cursor)
        .ok_or_else(|| "No enabled route credentials in pool".to_string())?;
    let next_index = (cursor.rem_euclid(credentials.len() as i64) + 1) % credentials.len() as i64;

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
        credential,
        &platform,
        &path,
        query.as_deref(),
        outbound_headers,
        &body_bytes,
    )?;

    let client = reqwest::Client::new();
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
         WHERE rpm.platform = ? AND rpm.enabled = 1 AND c.status = 'ok'
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
) -> Result<(String, HeaderMap, Vec<u8>), String> {
    let access_token = string_value(secret, "access_token").ok_or_else(|| {
        if string_value(secret, "refresh_token").is_some() {
            "route_credential.refresh_only_unsupported".to_string()
        } else {
            format!(
                "Route credential {} is missing access_token",
                credential.display_name
            )
        }
    })?;
    insert_header(headers, "authorization", &format!("Bearer {access_token}"))?;
    if platform == "claude" {
        headers
            .entry(HeaderName::from_static("anthropic-version"))
            .or_insert(HeaderValue::from_static("2023-06-01"));
    }
    let target_url = build_target_url(default_official_base_url(platform), path, query);
    Ok((target_url, headers.clone(), body.to_vec()))
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
}
