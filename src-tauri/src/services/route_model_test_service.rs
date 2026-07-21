use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::route_credential::ModelMapping;
use crate::models::route_pool::{RoutePoolModelTestOutcome, RoutePoolModelTestRequest};
use crate::services::http_client::build_outbound_http_client;
use crate::services::route_pool_service::normalize_platform;
use crate::services::route_proxy_service::{
    build_upstream_request, extract_cost_micros, extract_token_count,
    is_route_credential_quota_available, maybe_persist_official_quota_from_response,
    maybe_refresh_official_credential, SelectedCredential,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use std::time::{Duration, Instant};

pub struct RouteModelTestService;

pub const MODEL_TEST_PROMPT: &str = "Reply with exactly: ai-switch-ok";
pub const MODEL_TEST_RESPONSE_LIMIT: usize = 16 * 1024;
const DEFAULT_REQUEST_PAGE: i64 = 1;
const DEFAULT_REQUEST_PAGE_SIZE: i64 = 20;
const ROUTE_MODEL_TEST_SOURCE: &str = "route_pool_model_test";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTestRequestParts {
    pub interface_format: String,
    pub request_path: String,
    pub base_url: Option<String>,
    pub target_url: Option<String>,
    pub request_body_json: String,
}

impl RouteModelTestService {
    pub async fn test_model(
        pool: &SqlitePool,
        request: RoutePoolModelTestRequest,
    ) -> Result<RoutePoolModelTestOutcome, AppError> {
        let platform = normalize_platform(&request.platform)?;
        let requested_model = request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string);
        let requested_account_id = request
            .account_id
            .as_deref()
            .map(str::trim)
            .filter(|account_id| !account_id.is_empty())
            .map(str::to_string);
        let cursor = RoutePoolRepository::next_cursor_index(pool, &platform).await?;

        let (credential, next_index) = if let Some(account_id) = requested_account_id {
            (
                load_account_credential(pool, &platform, &account_id).await?,
                cursor,
            )
        } else {
            let credentials = load_pool_credentials(pool, &platform).await?;

            if credentials.is_empty() {
                return Err(AppError::Validation {
                    code: "validation.route_pool_empty",
                    message: "Route pool has no enabled accounts".to_string(),
                    details: Some(platform),
                    recoverable: true,
                });
            }

            let selected_index = cursor.rem_euclid(credentials.len() as i64) as usize;
            let next_index = (selected_index + 1) as i64 % credentials.len() as i64;
            (credentials[selected_index].clone(), next_index)
        };
        let credential = maybe_refresh_official_credential(pool, &credential)
            .await
            .map_err(|error| AppError::Validation {
                code: "validation.route_credential_refresh",
                message: error,
                details: Some(credential.id.clone()),
                recoverable: true,
            })?;
        let start = Instant::now();

        let parts =
            match build_model_test_request(&credential, &platform, requested_model.as_deref()) {
                Ok(parts) => parts,
                Err(error) => {
                    let fallback_parts = fallback_request_parts(&credential, &platform);
                    return finish_outcome(
                        pool,
                        &platform,
                        credential,
                        fallback_parts,
                        next_index,
                        None,
                        String::new(),
                        None,
                        Some(error),
                        false,
                        elapsed_ms(start),
                        None,
                        None,
                    )
                    .await;
                }
            };

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );

        let (target_url, upstream_headers, upstream_body) = match build_upstream_request(
            &credential,
            &platform,
            &parts.request_path,
            None,
            headers,
            parts.request_body_json.as_bytes(),
        ) {
            Ok(request) => request,
            Err(error) => {
                return finish_outcome(
                    pool,
                    &platform,
                    credential,
                    parts,
                    next_index,
                    None,
                    String::new(),
                    None,
                    Some(error),
                    false,
                    elapsed_ms(start),
                    None,
                    None,
                )
                .await;
            }
        };

        let parts = ModelTestRequestParts {
            request_body_json: pretty_json_bytes(&upstream_body),
            target_url: Some(target_url.clone()),
            ..parts
        };

        let client = match build_outbound_http_client(Some(Duration::from_secs(30))) {
            Ok(client) => client,
            Err(error) => {
                return finish_outcome(
                    pool,
                    &platform,
                    credential,
                    parts,
                    next_index,
                    None,
                    String::new(),
                    None,
                    Some(error),
                    false,
                    elapsed_ms(start),
                    None,
                    None,
                )
                .await;
            }
        };

        let send_result =
            send_model_test_request(client, &target_url, upstream_headers, upstream_body).await;
        let duration_ms = elapsed_ms(start);

        match send_result {
            Ok((status, success, body)) => {
                let token_count = extract_token_count(&body);
                let cost_micros = extract_cost_micros(&body);
                let response_body =
                    sanitize_for_storage(&credential, &truncate_response_body(&body));
                let response_text =
                    extract_model_test_response_text(&parts.interface_format, &response_body);

                finish_outcome(
                    pool,
                    &platform,
                    credential,
                    parts,
                    next_index,
                    Some(status),
                    response_body,
                    response_text,
                    None,
                    success,
                    duration_ms,
                    token_count,
                    cost_micros,
                )
                .await
            }
            Err(error) => {
                let error = sanitize_for_storage(&credential, &error);
                finish_outcome(
                    pool,
                    &platform,
                    credential,
                    parts,
                    next_index,
                    None,
                    String::new(),
                    None,
                    Some(error),
                    false,
                    duration_ms,
                    None,
                    None,
                )
                .await
            }
        }
    }
}

pub fn build_model_test_request(
    credential: &SelectedCredential,
    platform: &str,
    requested_model: Option<&str>,
) -> Result<ModelTestRequestParts, String> {
    let config = parse_json_object(&credential.config_json, "config")?;
    let interface_format = interface_format_for(credential, platform, &config);
    let base_url = string_value(&config, "base_url").map(str::to_string);
    let mappings = model_mappings(&config);
    let model = request_model(platform, &interface_format, &mappings, requested_model);

    let (request_path, request_body) = match interface_format.as_str() {
        "openai" => (
            "/chat/completions".to_string(),
            json!({
                "model": model,
                "messages": [{"role": "user", "content": MODEL_TEST_PROMPT}],
                "temperature": 0,
                "max_tokens": 16
            }),
        ),
        "openai-responses" => (
            "/responses".to_string(),
            json!({
                "model": model,
                "input": MODEL_TEST_PROMPT,
                "temperature": 0,
                "max_output_tokens": 16
            }),
        ),
        "anthropic" | "anthropic-messages" => (
            "/v1/messages".to_string(),
            json!({
                "model": model,
                "messages": [{"role": "user", "content": MODEL_TEST_PROMPT}],
                "max_tokens": 16
            }),
        ),
        "gemini" => (
            format!(
                "/v1beta/models/{}:generateContent",
                gemini_path_model(&mappings, requested_model)
            ),
            json!({
                "contents": [{
                    "role": "user",
                    "parts": [{"text": MODEL_TEST_PROMPT}]
                }],
                "generationConfig": {
                    "temperature": 0,
                    "maxOutputTokens": 16
                }
            }),
        ),
        other => return Err(format!("Unsupported interface format: {other}")),
    };

    Ok(ModelTestRequestParts {
        interface_format,
        request_path,
        base_url,
        target_url: None,
        request_body_json: serde_json::to_string_pretty(&request_body)
            .map_err(|err| format!("Could not serialize test request body: {err}"))?,
    })
}

pub fn extract_model_test_response_text(interface_format: &str, body: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(body).ok()?;

    if matches!(interface_format, "openai" | "openai-responses") {
        if let Some(text) = text_at(&value, "/choices/0/message/content") {
            return Some(text.to_string());
        }
        if let Some(text) = text_at(&value, "/output_text") {
            return Some(text.to_string());
        }
        if let Some(items) = value.pointer("/output").and_then(Value::as_array) {
            for item in items {
                if let Some(content_items) = item.get("content").and_then(Value::as_array) {
                    for content in content_items {
                        if let Some(text) = content.get("text").and_then(Value::as_str) {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                return Some(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    if matches!(interface_format, "anthropic" | "anthropic-messages") {
        if let Some(text) = text_at(&value, "/content/0/text") {
            return Some(text.to_string());
        }
    }

    if interface_format == "gemini" {
        if let Some(text) = text_at(&value, "/candidates/0/content/parts/0/text") {
            return Some(text.to_string());
        }
    }

    None
}

pub fn truncate_response_body(body: &[u8]) -> String {
    String::from_utf8_lossy(&body[..body.len().min(MODEL_TEST_RESPONSE_LIMIT)]).to_string()
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

fn interface_format_for(credential: &SelectedCredential, platform: &str, config: &Value) -> String {
    if credential.kind == "api" {
        return string_value(config, "interface_format")
            .unwrap_or("openai")
            .to_string();
    }

    match platform {
        "codex" => "openai-responses".to_string(),
        "claude" => "anthropic".to_string(),
        // Grok/xAI defaults to OpenAI-compatible chat completions.
        "grok" => "openai".to_string(),
        "gemini" => "gemini".to_string(),
        _ => "openai".to_string(),
    }
}

fn default_model_for(interface_format: &str) -> &'static str {
    match interface_format {
        "anthropic" | "anthropic-messages" => "claude-sonnet-4-20250514",
        "gemini" => "gemini-2.5-flash",
        _ => "gpt-5",
    }
}

fn default_model_for_platform(platform: &str, interface_format: &str) -> String {
    if platform == "grok" {
        return "grok-3".to_string();
    }
    default_model_for(interface_format).to_string()
}

fn request_model(
    platform: &str,
    interface_format: &str,
    mappings: &[ModelMapping],
    requested_model: Option<&str>,
) -> String {
    if let Some(model) = requested_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        return model.to_string();
    }

    mappings
        .first()
        .map(|mapping| mapping.from.trim())
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_model_for_platform(platform, interface_format))
}

fn gemini_path_model(mappings: &[ModelMapping], requested_model: Option<&str>) -> String {
    if let Some(model) = requested_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        return mappings
            .iter()
            .find(|mapping| mapping.from.trim() == model)
            .map(|mapping| mapping.to.trim())
            .filter(|target| !target.is_empty())
            .unwrap_or(model)
            .to_string();
    }

    mappings
        .first()
        .map(|mapping| mapping.to.trim())
        .filter(|model| !model.is_empty())
        .unwrap_or("gemini-2.5-flash")
        .to_string()
}

fn text_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
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
           AND (c.primary_remain IS NULL OR c.primary_remain > 0)
           AND (c.weekly_remain IS NULL OR c.weekly_remain > 0)
         ORDER BY rpm.sort_order ASC, rpm.created_at ASC",
    )
    .bind(platform)
    .fetch_all(pool)
    .await
    .map_err(|err| AppError::Database {
        code: "database.route_model_test_credentials",
        message: "Could not load route credentials for model test".to_string(),
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
        // Pool routing/testing should not keep hitting free-usage exhausted accounts.
        .filter(|credential| is_route_credential_quota_available(&credential.config_json))
        .collect())
}

async fn load_account_credential(
    pool: &SqlitePool,
    platform: &str,
    account_id: &str,
) -> Result<SelectedCredential, AppError> {
    let row = sqlx::query(
        "SELECT id, platform, kind, display_name, secret_payload_json, config_json
         FROM route_credentials
         WHERE id = ? AND platform = ?",
    )
    .bind(account_id)
    .bind(platform)
    .fetch_optional(pool)
    .await
    .map_err(|err| AppError::Database {
        code: "database.route_model_test_account",
        message: "Could not load route credential for model test".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    row.map(|row| SelectedCredential {
        id: row.get("id"),
        platform: row.get("platform"),
        kind: row.get("kind"),
        display_name: row.get("display_name"),
        secret_payload_json: row.get("secret_payload_json"),
        config_json: row.get("config_json"),
    })
    .ok_or_else(|| AppError::Validation {
        code: "validation.route_model_test_account_not_found",
        message: "Route credential does not exist for this platform".to_string(),
        details: Some(account_id.to_string()),
        recoverable: true,
    })
}

async fn send_model_test_request(
    client: reqwest::Client,
    target_url: &str,
    headers: HeaderMap,
    body: Vec<u8>,
) -> Result<(u16, bool, Vec<u8>), String> {
    let upstream = client
        .post(target_url)
        .headers(map_to_reqwest_headers(&headers))
        .body(body)
        .send()
        .await
        .map_err(|err| {
            let mut message = format!("Upstream model test request failed: {err}");
            if err.is_connect() || err.is_timeout() {
                message.push_str(
                    " (check network/proxy; Windows system proxy is applied when configured)",
                );
            }
            message
        })?;
    let status = upstream.status();
    let body = upstream
        .bytes()
        .await
        .map_err(|err| format!("Could not read model test response: {err}"))?
        .to_vec();

    Ok((status.as_u16(), status.is_success(), body))
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

#[allow(clippy::too_many_arguments)]
async fn finish_outcome(
    pool: &SqlitePool,
    platform: &str,
    credential: SelectedCredential,
    parts: ModelTestRequestParts,
    next_index: i64,
    response_status: Option<u16>,
    response_body: String,
    response_text: Option<String>,
    error_message: Option<String>,
    success: bool,
    duration_ms: i64,
    token_count: Option<i64>,
    cost_micros: Option<i64>,
) -> Result<RoutePoolModelTestOutcome, AppError> {
    // Official accounts may report free/quota exhaustion in response bodies.
    if !response_body.trim().is_empty() {
        let _ = maybe_persist_official_quota_from_response(pool, &credential, &response_body).await;
    }
    if !success
        && should_mark_model_test_account_unavailable(response_status, error_message.as_deref())
    {
        RouteCredentialRepository::update_status(pool, &credential.id, "error").await?;
    }

    let error_message = error_message.map(|value| sanitize_for_storage(&credential, &value));
    let metadata = metadata_json(
        platform,
        &credential,
        &parts,
        response_status,
        success,
        duration_ms,
        &response_body,
        response_text.as_deref(),
        error_message.as_deref(),
    );

    RoutePoolRepository::insert_usage_event(
        pool,
        &credential.id,
        ROUTE_MODEL_TEST_SOURCE,
        "request",
        1,
        "count",
        &metadata,
    )
    .await?;

    if let Some(tokens) = token_count {
        if tokens > 0 {
            RoutePoolRepository::insert_usage_event(
                pool,
                &credential.id,
                ROUTE_MODEL_TEST_SOURCE,
                "token",
                tokens,
                "token",
                &metadata,
            )
            .await?;
        }
    }

    if let Some(cost) = cost_micros {
        if cost > 0 {
            RoutePoolRepository::insert_usage_event(
                pool,
                &credential.id,
                ROUTE_MODEL_TEST_SOURCE,
                "cost",
                cost,
                "usd_micros",
                &metadata,
            )
            .await?;
        }
    }

    RoutePoolRepository::save_cursor_index(pool, platform, next_index).await?;

    Ok(RoutePoolModelTestOutcome {
        platform: platform.to_string(),
        selected_account_id: credential.id,
        selected_account_name: credential.display_name,
        interface_format: parts.interface_format,
        request_path: parts.request_path,
        base_url: parts.base_url,
        target_url: parts.target_url,
        request_body_json: parts.request_body_json,
        response_status,
        response_body,
        response_text,
        error_message,
        success,
        duration_ms,
        stats: RoutePoolRepository::stats(
            pool,
            platform,
            None,
            DEFAULT_REQUEST_PAGE,
            DEFAULT_REQUEST_PAGE_SIZE,
        )
        .await?,
    })
}

fn should_mark_model_test_account_unavailable(
    response_status: Option<u16>,
    error_message: Option<&str>,
) -> bool {
    if matches!(response_status, Some(401 | 403)) {
        return true;
    }
    let Some(message) = error_message else {
        return false;
    };
    let lower = message.to_ascii_lowercase();
    lower.contains("upstream model test request failed")
        || lower.contains("invalid_grant")
        || lower.contains("refresh token has been revoked")
        || lower.contains("官方 oauth 凭证已失效")
}

fn metadata_json(
    platform: &str,
    credential: &SelectedCredential,
    parts: &ModelTestRequestParts,
    response_status: Option<u16>,
    success: bool,
    duration_ms: i64,
    response_body: &str,
    response_text: Option<&str>,
    error_message: Option<&str>,
) -> String {
    json!({
        "source": "ui_model_connectivity_test",
        "request_kind": "model_connectivity",
        "platform": platform,
        "route_credential_id": credential.id,
        "route_credential_name": credential.display_name,
        "interface_format": parts.interface_format,
        "path": parts.request_path,
        "base_url": parts.base_url,
        "target_url": parts.target_url,
        "status": response_status,
        "success": success,
        "duration_ms": duration_ms,
        "request_body_json": parts.request_body_json,
        "response_body": response_body,
        "response_text": response_text,
        "error_message": error_message,
    })
    .to_string()
}

fn fallback_request_parts(
    credential: &SelectedCredential,
    platform: &str,
) -> ModelTestRequestParts {
    let config = serde_json::from_str::<Value>(&credential.config_json)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    ModelTestRequestParts {
        interface_format: interface_format_for(credential, platform, &config),
        request_path: String::new(),
        base_url: string_value(&config, "base_url").map(str::to_string),
        target_url: None,
        request_body_json: String::new(),
    }
}

fn pretty_json_bytes(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| text.to_string())
}

fn elapsed_ms(start: Instant) -> i64 {
    start.elapsed().as_millis().min(i64::MAX as u128) as i64
}

fn sanitize_for_storage(credential: &SelectedCredential, value: &str) -> String {
    let mut sanitized = value.to_string();
    for secret in sensitive_secret_values(&credential.secret_payload_json) {
        sanitized = sanitized.replace(&secret, "[redacted]");
    }
    sanitized
}

fn sensitive_secret_values(secret_payload_json: &str) -> Vec<String> {
    let Ok(Value::Object(secret)) = serde_json::from_str::<Value>(secret_payload_json) else {
        return Vec::new();
    };
    let sensitive_keys = [
        "api_key",
        "access_token",
        "refresh_token",
        "id_token",
        "authorization",
        "x-api-key",
    ];

    secret
        .into_iter()
        .filter(|(key, _)| {
            let key = key.to_ascii_lowercase();
            sensitive_keys.contains(&key.as_str())
        })
        .filter_map(|(_, value)| value.as_str().map(str::to_string))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::route_pool::{RoutePoolModelTestRequest, SetRoutePoolMembersInput};
    use crate::services::route_pool_service::RoutePoolService;
    use axum::{routing::post, Json, Router};
    use serde_json::{json, Value};
    use tokio::net::TcpListener;

    fn api_credential(interface_format: &str) -> SelectedCredential {
        SelectedCredential {
            id: "cred-api".to_string(),
            platform: "codex".to_string(),
            kind: "api".to_string(),
            display_name: "API Account".to_string(),
            secret_payload_json: r#"{"api_key":"sk-test"}"#.to_string(),
            config_json: json!({
                "base_url": "https://api.example.com/v1",
                "interface_format": interface_format,
                "model_mappings": [{"from":"gpt-5","to":"up-gpt"}]
            })
            .to_string(),
        }
    }

    fn official_credential(platform: &str) -> SelectedCredential {
        SelectedCredential {
            id: "cred-official".to_string(),
            platform: platform.to_string(),
            kind: "official".to_string(),
            display_name: "Official Account".to_string(),
            secret_payload_json: r#"{"access_token":"at"}"#.to_string(),
            config_json: "{}".to_string(),
        }
    }

    async fn start_json_test_server(status: axum::http::StatusCode, body: Value) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || async move { (status, Json(body.clone())) }),
        );
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        format!("http://{addr}/v1")
    }

    async fn create_api_credential(pool: &SqlitePool, base_url: &str) -> String {
        RouteCredentialRepository::create(
            pool,
            "codex",
            "api",
            "API Account",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            &json!({
                "base_url": base_url,
                "interface_format": "openai",
                "model_mappings": [{"from":"gpt-5","to":"up-gpt"}]
            })
            .to_string(),
            r#"{"config_toml":""}"#,
        )
        .await
        .expect("credential")
        .id
    }

    #[test]
    fn builds_openai_chat_test_request() {
        let request =
            build_model_test_request(&api_credential("openai"), "codex", None).expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "openai");
        assert_eq!(request.request_path, "/chat/completions");
        assert_eq!(
            body.pointer("/model").and_then(Value::as_str),
            Some("gpt-5")
        );
        assert_eq!(
            body.pointer("/messages/0/content").and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT),
        );
        assert_eq!(
            body.pointer("/max_tokens").and_then(Value::as_i64),
            Some(16)
        );
    }

    #[test]
    fn builds_openai_chat_test_request_with_explicit_model() {
        let request = build_model_test_request(&api_credential("openai"), "codex", Some("gpt-4o"))
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(
            body.pointer("/model").and_then(Value::as_str),
            Some("gpt-4o")
        );
    }

    #[test]
    fn builds_openai_responses_test_request() {
        let request = build_model_test_request(&api_credential("openai-responses"), "codex", None)
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "openai-responses");
        assert_eq!(request.request_path, "/responses");
        assert_eq!(
            body.pointer("/model").and_then(Value::as_str),
            Some("gpt-5")
        );
        assert_eq!(
            body.pointer("/input").and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT)
        );
        assert_eq!(
            body.pointer("/max_output_tokens").and_then(Value::as_i64),
            Some(16)
        );
    }

    #[test]
    fn builds_openai_responses_test_request_for_official_codex() {
        let request = build_model_test_request(&official_credential("codex"), "codex", None)
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "openai-responses");
        assert_eq!(request.request_path, "/responses");
        assert_eq!(
            body.pointer("/input").and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT)
        );
    }

    #[test]
    fn builds_openai_test_request_for_official_grok() {
        let request = build_model_test_request(&official_credential("grok"), "grok", None)
            .expect("request");
        assert_eq!(request.interface_format, "openai");
        assert_eq!(request.request_path, "/chat/completions");
        assert!(request.request_body_json.contains("\"model\": \"grok-3\"")
            || request.request_body_json.contains("\"model\":\"grok-3\""));
    }

    #[test]
    fn builds_anthropic_test_request_for_official_claude() {
        let request = build_model_test_request(&official_credential("claude"), "claude", None)
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "anthropic");
        assert_eq!(request.request_path, "/v1/messages");
        assert_eq!(
            body.pointer("/model").and_then(Value::as_str),
            Some("claude-sonnet-4-20250514"),
        );
        assert_eq!(
            body.pointer("/messages/0/content").and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT),
        );
    }

    #[test]
    fn builds_gemini_test_request_and_uses_mapping_target_in_path() {
        let request =
            build_model_test_request(&api_credential("gemini"), "gemini", None).expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "gemini");
        assert_eq!(
            request.request_path,
            "/v1beta/models/up-gpt:generateContent"
        );
        assert_eq!(
            body.pointer("/contents/0/parts/0/text")
                .and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT),
        );
        assert_eq!(
            body.pointer("/generationConfig/maxOutputTokens")
                .and_then(Value::as_i64),
            Some(16),
        );
    }

    #[test]
    fn builds_gemini_test_request_with_explicit_model_path() {
        let request =
            build_model_test_request(&api_credential("gemini"), "gemini", Some("gemini-1.5-pro"))
                .expect("request");

        assert_eq!(
            request.request_path,
            "/v1beta/models/gemini-1.5-pro:generateContent"
        );
    }

    #[test]
    fn builds_gemini_test_request_with_explicit_mapping_target_path() {
        let request = build_model_test_request(&api_credential("gemini"), "gemini", Some("gpt-5"))
            .expect("request");

        assert_eq!(
            request.request_path,
            "/v1beta/models/up-gpt:generateContent"
        );
    }

    #[test]
    fn builds_gemini_test_request_ignores_placeholder_mapping_target() {
        let mut credential = api_credential("gemini");
        credential.config_json = json!({
            "base_url": "https://api.example.com/v1",
            "interface_format": "gemini",
            "model_mappings": [{"from":"gpt-5","to":"upstream-model"}]
        })
        .to_string();
        let request = build_model_test_request(&credential, "gemini", None).expect("request");

        assert_eq!(
            request.request_path,
            "/v1beta/models/gemini-2.5-flash:generateContent"
        );
        assert!(!request.request_path.contains("upstream-model"));
    }

    #[test]
    fn extracts_model_text_from_supported_response_shapes() {
        assert_eq!(
            extract_model_test_response_text(
                "openai",
                r#"{"choices":[{"message":{"content":"ai-switch-ok"}}]}"#,
            )
            .as_deref(),
            Some("ai-switch-ok"),
        );
        assert_eq!(
            extract_model_test_response_text(
                "openai-responses",
                r#"{"output_text":"ai-switch-ok"}"#
            )
            .as_deref(),
            Some("ai-switch-ok"),
        );
        assert_eq!(
            extract_model_test_response_text(
                "anthropic",
                r#"{"content":[{"type":"text","text":"ai-switch-ok"}]}"#,
            )
            .as_deref(),
            Some("ai-switch-ok"),
        );
        assert_eq!(
            extract_model_test_response_text(
                "gemini",
                r#"{"candidates":[{"content":{"parts":[{"text":"ai-switch-ok"}]}}]}"#,
            )
            .as_deref(),
            Some("ai-switch-ok"),
        );
    }

    #[test]
    fn truncates_response_body_to_safe_limit() {
        let body = vec![b'a'; MODEL_TEST_RESPONSE_LIMIT + 10];
        assert_eq!(
            truncate_response_body(&body).len(),
            MODEL_TEST_RESPONSE_LIMIT
        );
    }

    #[tokio::test]
    async fn test_model_records_success_metadata_and_usage() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "choices": [{"message": {"content": "ai-switch-ok"}}],
                "usage": {"prompt_tokens": 5, "completion_tokens": 3, "cost_micros": 42}
            }),
        )
        .await;
        let credential_id = create_api_credential(&pool, &base_url).await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![credential_id.clone()],
            },
        )
        .await
        .expect("members");

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: None,
                model: None,
            },
        )
        .await
        .expect("outcome");
        let expected_target_url = format!("{base_url}/chat/completions");

        assert!(outcome.success);
        assert_eq!(outcome.selected_account_id, credential_id);
        assert_eq!(outcome.selected_account_name, "API Account");
        assert_eq!(outcome.interface_format, "openai");
        assert_eq!(outcome.request_path, "/chat/completions");
        assert_eq!(outcome.base_url.as_deref(), Some(base_url.as_str()));
        assert_eq!(
            outcome.target_url.as_deref(),
            Some(expected_target_url.as_str())
        );
        assert_eq!(outcome.response_status, Some(200));
        assert_eq!(outcome.response_text.as_deref(), Some("ai-switch-ok"));
        assert!(outcome.request_body_json.contains("up-gpt"));
        assert_eq!(outcome.stats.request_count, 1);
        assert_eq!(outcome.stats.token_count, 8);
        assert_eq!(outcome.stats.cost_micros, 42);
        assert_eq!(outcome.stats.requests.len(), 1);
        assert_eq!(
            outcome.stats.requests[0].source_label,
            ROUTE_MODEL_TEST_SOURCE
        );

        let metadata: Value =
            serde_json::from_str(&outcome.stats.requests[0].metadata_json).expect("metadata");
        assert_eq!(
            metadata.pointer("/request_kind").and_then(Value::as_str),
            Some("model_connectivity")
        );
        assert_eq!(
            metadata.pointer("/success").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            metadata.pointer("/status").and_then(Value::as_i64),
            Some(200)
        );
        assert!(metadata
            .pointer("/request_body_json")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains(MODEL_TEST_PROMPT));
        assert_eq!(
            metadata.pointer("/target_url").and_then(Value::as_str),
            Some(expected_target_url.as_str())
        );
        assert!(metadata
            .pointer("/response_body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("ai-switch-ok"));
        assert!(!outcome.stats.requests[0].metadata_json.contains("sk-test"));
    }

    #[tokio::test]
    async fn test_model_can_target_single_account_without_pool_membership() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "choices": [{"message": {"content": "ai-switch-ok"}}],
                "usage": {"prompt_tokens": 2, "completion_tokens": 1}
            }),
        )
        .await;
        let credential_id = create_api_credential(&pool, &base_url).await;

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: Some(credential_id.clone()),
                model: Some("gpt-4o".to_string()),
            },
        )
        .await
        .expect("outcome");

        assert!(outcome.success);
        assert_eq!(outcome.selected_account_id, credential_id);
        assert!(outcome.request_body_json.contains("gpt-4o"));
        assert_eq!(outcome.stats.request_count, 1);
    }

    #[tokio::test]
    async fn pool_model_test_skips_accounts_with_zero_quota_remaining() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "choices": [{"message": {"content": "ai-switch-ok"}}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1}
            }),
        )
        .await;

        let exhausted = RouteCredentialRepository::create(
            &pool,
            "grok",
            "official",
            "Exhausted Free",
            Some("exhausted@example.com".to_string()),
            "ok",
            None,
            r#"{"access_token":"at-exhausted"}"#,
            &json!({
                "base_url": format!("{base_url}/exhausted"),
                "type": "grok",
                "subscription_type": "free",
                "primary_remain": 0
            })
            .to_string(),
            r#"{"auth_json":"{}","config_toml":""}"#,
        )
        .await
        .expect("exhausted");

        let available = RouteCredentialRepository::create(
            &pool,
            "grok",
            "official",
            "Available Free",
            Some("available@example.com".to_string()),
            "ok",
            None,
            r#"{"access_token":"at-available"}"#,
            &json!({
                "base_url": base_url,
                "type": "grok"
            })
            .to_string(),
            r#"{"auth_json":"{}","config_toml":""}"#,
        )
        .await
        .expect("available");

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "grok".to_string(),
                account_ids: vec![exhausted.id.clone(), available.id.clone()],
            },
        )
        .await
        .expect("members");

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "grok".to_string(),
                account_id: None,
                model: Some("grok-4.5".to_string()),
            },
        )
        .await
        .expect("outcome");

        assert!(outcome.success);
        assert_eq!(outcome.selected_account_id, available.id);
        assert_ne!(outcome.selected_account_id, exhausted.id);
    }

    #[tokio::test]
    async fn persists_official_free_usage_exhausted_quota() {

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let body = json!({
            "code": "subscription:free-usage-exhausted",
            "error": "You've used all the included free usage for model grok-4.5-build-free for now. Usage resets over a rolling 24-hour window — tokens (actual/limit): 1177205/1000000."
        });
        let base_url = start_json_test_server(axum::http::StatusCode::TOO_MANY_REQUESTS, body).await;
        let created = RouteCredentialRepository::create(
            &pool,
            "grok",
            "official",
            "Grok Free",
            Some("free@example.com".to_string()),
            "ok",
            None,
            r#"{"access_token":"at-test"}"#,
            &json!({
                "base_url": base_url,
                "type": "grok",
                "auth_kind": "oauth"
            })
            .to_string(),
            r#"{"auth_json":"{}","config_toml":""}"#,
        )
        .await
        .expect("create official");

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "grok".to_string(),
                account_id: Some(created.id.clone()),
                model: Some("grok-4.5".to_string()),
            },
        )
        .await
        .expect("outcome");

        assert!(!outcome.success);
        let credential = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .expect("credential");
        assert!(credential.config_json.contains("\"subscription_type\":\"free\""));
        assert!(credential.config_json.contains("\"primary_remain\":0"));
        assert!(credential.config_json.contains("\"quota_remaining\":0"));
        assert!(credential.config_json.contains("\"quota_used\":1177205"));
        assert!(credential.config_json.contains("\"quota_limit\":1000000"));
        assert_eq!(credential.subscription_type.as_deref(), Some("free"));
        assert_eq!(credential.primary_remain, Some(0));
        assert_eq!(credential.quota_remaining, Some(0));
        assert_eq!(credential.quota_used, Some(1_177_205));
        assert_eq!(credential.quota_limit, Some(1_000_000));
        assert!(credential.quota_updated_at.is_some());
        assert!(credential.reset_primary.is_some());
    }

    #[tokio::test]
    async fn test_model_returns_failed_outcome_for_http_errors() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let base_url = start_json_test_server(
            axum::http::StatusCode::UNAUTHORIZED,
            json!({"error": {"message": "bad key"}}),
        )
        .await;
        let credential_id = create_api_credential(&pool, &base_url).await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![credential_id.clone()],
            },
        )
        .await
        .expect("members");

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: None,
                model: None,
            },
        )
        .await
        .expect("outcome");

        assert!(!outcome.success);
        assert_eq!(outcome.response_status, Some(401));
        assert!(outcome.response_body.contains("bad key"));
        assert_eq!(outcome.error_message, None);
        assert_eq!(outcome.stats.request_count, 1);
        assert_eq!(outcome.stats.token_count, 0);

        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "error");
    }

    #[tokio::test]
    async fn test_model_rejects_empty_pool() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: None,
                model: None,
            },
        )
        .await
        .expect_err("empty pool");

        match error {
            AppError::Validation { code, .. } => {
                assert_eq!(code, "validation.route_pool_empty");
            }
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn sanitizes_secret_values_before_storage() {
        let credential = api_credential("openai");

        assert_eq!(
            sanitize_for_storage(&credential, "request failed for key sk-test"),
            "request failed for key [redacted]"
        );
    }
}
