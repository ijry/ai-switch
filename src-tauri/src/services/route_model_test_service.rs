use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::{ApiError, AppError};
use crate::models::platform::{ApiDialect, CapabilityRule, PlatformId, PlatformOperation};
use crate::models::route_credential::{
    is_fallback_mapping, ModelMapping, RouteCredentialFailurePolicy,
};
use crate::models::route_pool::{
    RoutePoolModelTestOutcome, RoutePoolModelTestRequest, RouteUsageBreakdown,
};
use crate::services::client_identity;
use crate::services::http_client::{
    build_outbound_http_client, build_outbound_http_client_with_root_certificate,
};
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::response_failure_service::{
    detect_response_failed, is_quota_exhaustion_failure, stream_disconnected_before_completion,
    SemanticResponseFailure, STREAM_DISCONNECTED_FAILURE_MESSAGE,
};
use crate::services::route_credential_activity::{
    RouteCredentialActivityLease, RouteCredentialActivityRegistry,
};
use crate::services::route_protocol_bridge::transform_response as transform_protocol_bridge_response;
use crate::services::route_proxy_service::{
    apply_estimated_price, build_target_url, build_upstream_request_with_bridge,
    classify_proxy_failure, credential_indexes_by_priority, extract_response_model,
    extract_usage_breakdown, maybe_persist_official_quota_from_response,
    maybe_refresh_official_credential, normalize_api_upstream_path, select_pool_credentials,
    ProxyFailureKind, RouteProxyService, SelectedCredential, TurnReminderMode,
    ROUTE_PROXY_TRACE_HEADER,
};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use std::time::{Duration, Instant};
use uuid::Uuid;

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
        Self::test_model_with_activity(pool, &RouteCredentialActivityRegistry::default(), request)
            .await
    }

    pub async fn test_model_with_activity(
        pool: &SqlitePool,
        activity: &RouteCredentialActivityRegistry,
        request: RoutePoolModelTestRequest,
    ) -> Result<RoutePoolModelTestOutcome, AppError> {
        let platform_id = PlatformId::parse(&request.platform)?;
        let model_test_rule =
            PlatformCapabilityService::require(platform_id, PlatformOperation::ModelTest)?;
        let platform = platform_id.as_str().to_string();
        let interface_override =
            validate_model_test_interface_override(&platform, request.interface_format.as_deref())?;
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
        let explicit_account_test = requested_account_id.is_some();
        let cursor = RoutePoolRepository::next_cursor_index(pool, &platform).await?;

        let (credential, next_index, activity_lease) = if let Some(account_id) =
            requested_account_id
        {
            (
                load_account_credential(pool, &platform, &account_id).await?,
                cursor,
                None,
            )
        } else {
            let credentials = filter_model_test_credentials(
                select_pool_credentials(pool, &platform).await?,
                &model_test_rule,
            );

            if credentials.is_empty() {
                return Err(AppError::Validation {
                    code: "validation.route_pool_empty",
                    message: "Route pool has no enabled accounts".to_string(),
                    details: Some(platform),
                    recoverable: true,
                });
            }

            let mut selected = None;
            for selected_index in credential_indexes_by_priority(&credentials, cursor) {
                let credential = credentials[selected_index].clone();
                let Some(lease) = activity
                    .try_acquire(&platform, &credential.id, credential.max_concurrency)
                    .await
                else {
                    continue;
                };
                let next_index = (selected_index + 1) as i64 % credentials.len() as i64;
                selected = Some((credential, next_index, lease));
                break;
            }
            let Some((credential, next_index, lease)) = selected else {
                return Err(AppError::Validation {
                    code: "route_pool.concurrency_exhausted",
                    message: "All route pool accounts are at their concurrency limit".to_string(),
                    details: Some(platform.clone()),
                    recoverable: true,
                });
            };
            (credential, next_index, Some(lease))
        };
        validate_model_test_credential(platform_id, &credential)?;
        let _activity_lease: RouteCredentialActivityLease = match activity_lease {
            Some(lease) => lease,
            None => activity
                .try_acquire(&platform, &credential.id, credential.max_concurrency)
                .await
                .ok_or_else(|| AppError::Validation {
                    code: "route_pool.concurrency_exhausted",
                    message: "All route pool accounts are at their concurrency limit".to_string(),
                    details: Some(platform.clone()),
                    recoverable: true,
                })?,
        };
        let credential = maybe_refresh_official_credential(pool, &credential, Some(activity))
            .await
            .map_err(|error| AppError::Validation {
                code: "validation.route_credential_refresh",
                message: error,
                details: Some(credential.id.clone()),
                recoverable: true,
            })?;
        let failure_policy =
            RouteCredentialFailurePolicy::from_config_json(&credential.config_json);
        let start = Instant::now();

        let parts = match build_model_test_request(
            &credential,
            &platform,
            requested_model.as_deref(),
            interface_override.as_deref(),
        ) {
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
                    RouteUsageBreakdown::default(),
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

        let upstream_request = match build_upstream_request_with_bridge(
            &credential,
            &platform,
            &parts.request_path,
            None,
            headers,
            parts.request_body_json.as_bytes(),
            // The probe asks the model to reply with exactly `ai-switch-ok`. An
            // account whose reminder says "answer in Chinese" contradicts that
            // directly, so applying it here would make every probe against such
            // an account fail permanently.
            TurnReminderMode::Skip,
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
                    RouteUsageBreakdown::default(),
                )
                .await;
            }
        };
        let target_url = upstream_request.target_url.clone();
        let bridge_kind = upstream_request.bridge_kind;

        let parts = ModelTestRequestParts {
            request_body_json: pretty_json_bytes(&upstream_request.body),
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
                    RouteUsageBreakdown::default(),
                )
                .await;
            }
        };

        let send_result = send_model_test_request(
            client,
            &target_url,
            upstream_request.headers,
            upstream_request.body,
            failure_policy,
        )
        .await;
        let duration_ms = elapsed_ms(start);

        match send_result {
            Ok((status, transport_success, mut body)) => {
                if let Some(bridge_kind) = bridge_kind {
                    match transform_protocol_bridge_response(
                        bridge_kind,
                        status,
                        Some("application/json"),
                        &body,
                    ) {
                        Ok(response) => body = response.body,
                        Err(error) => {
                            let response_body =
                                sanitize_for_storage(&credential, &truncate_response_body(&body));
                            return finish_outcome(
                                pool,
                                &platform,
                                credential,
                                parts,
                                next_index,
                                Some(status),
                                response_body,
                                None,
                                Some(error),
                                false,
                                duration_ms,
                                RouteUsageBreakdown::default(),
                            )
                            .await;
                        }
                    }
                }
                let semantic_failure = detect_response_failed(&body);
                let success = transport_success && semantic_failure.is_none();
                let usage = extract_usage_breakdown(&body);
                let mut usage = usage;
                // Price from the model the upstream reported, falling back to the
                // one requested, so a connectivity test records the same cost
                // basis as a real proxied request.
                let priced_model =
                    extract_response_model(&body).or_else(|| requested_model.clone());
                apply_estimated_price(&mut usage, priced_model.as_deref());
                let response_body =
                    sanitize_for_storage(&credential, &truncate_response_body(&body));
                let response_text = extract_model_test_response_text(
                    model_test_response_format(&platform, &parts.interface_format),
                    &response_body,
                );
                let error_message = semantic_failure
                    .map(|failure| failure.message)
                    .filter(|_| !matches!(status, 401 | 403));

                let outcome = finish_outcome(
                    pool,
                    &platform,
                    credential,
                    parts,
                    next_index,
                    Some(status),
                    response_body,
                    response_text,
                    error_message,
                    success,
                    duration_ms,
                    usage,
                )
                .await?;
                if explicit_account_test && outcome.success {
                    RouteCredentialRepository::recover_after_explicit_test(
                        pool,
                        &outcome.selected_account_id,
                    )
                    .await?;
                }
                Ok(outcome)
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
                    RouteUsageBreakdown::default(),
                )
                .await
            }
        }
    }

    pub async fn test_model_through_proxy(
        pool: &SqlitePool,
        request: RoutePoolModelTestRequest,
        route_proxy_base_url: &str,
    ) -> Result<RoutePoolModelTestOutcome, AppError> {
        Self::test_model_through_proxy_with_root_certificate(
            pool,
            request,
            route_proxy_base_url,
            None,
        )
        .await
    }

    pub async fn test_model_through_proxy_with_root_certificate(
        pool: &SqlitePool,
        request: RoutePoolModelTestRequest,
        route_proxy_base_url: &str,
        root_certificate_pem: Option<&[u8]>,
    ) -> Result<RoutePoolModelTestOutcome, AppError> {
        let requested_account_id = request
            .account_id
            .as_deref()
            .map(str::trim)
            .filter(|account_id| !account_id.is_empty());
        if requested_account_id.is_some() {
            return Self::test_model(pool, request).await;
        }

        let platform_id = PlatformId::parse(&request.platform)?;
        let model_test_rule =
            PlatformCapabilityService::require(platform_id, PlatformOperation::ModelTest)?;
        let platform = platform_id.as_str().to_string();
        let interface_override =
            validate_model_test_interface_override(&platform, request.interface_format.as_deref())?;
        let requested_model = request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string);
        let cursor = RoutePoolRepository::next_cursor_index(pool, &platform).await?;
        let credentials = filter_model_test_credentials(
            select_pool_credentials(pool, &platform).await?,
            &model_test_rule,
        );
        if credentials.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_pool_empty",
                message: "Route pool has no enabled accounts".to_string(),
                details: Some(platform),
                recoverable: true,
            });
        }

        let selected_index = cursor.rem_euclid(credentials.len() as i64) as usize;
        let credential = credentials[selected_index].clone();
        validate_model_test_credential(platform_id, &credential)?;
        let failure_policy =
            RouteCredentialFailurePolicy::from_config_json(&credential.config_json);
        let start = Instant::now();
        let parts = match build_model_test_request(
            &credential,
            &platform,
            requested_model.as_deref(),
            interface_override.as_deref(),
        ) {
            Ok(parts) => parts,
            Err(error) => {
                let fallback_parts = fallback_request_parts(&credential, &platform);
                return finish_proxy_outcome(
                    pool,
                    &platform,
                    credential,
                    fallback_parts,
                    None,
                    None,
                    None,
                    None,
                    None,
                    String::new(),
                    None,
                    Some(error),
                    false,
                    elapsed_ms(start),
                )
                .await;
            }
        };

        let entry_path = normalize_local_model_test_entry_path(
            &platform,
            &parts.interface_format,
            &parts.request_path,
        );
        let entry_url = join_proxy_entry_url(route_proxy_base_url, &entry_path);
        let trace_id = Uuid::new_v4().to_string();
        let proxy_key = RouteProxyService::get_or_create_platform_key(pool, &platform).await?;
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            header::AUTHORIZATION,
            header_value(&format!("Bearer {proxy_key}"), "authorization")?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("x-ai-switch-platform"),
            header_value(&platform, "platform")?,
        );
        headers.insert(
            HeaderName::from_static(ROUTE_PROXY_TRACE_HEADER),
            header_value(&trace_id, "trace id")?,
        );

        let client = match build_outbound_http_client_with_root_certificate(
            Some(Duration::from_secs(30)),
            root_certificate_pem,
        ) {
            Ok(client) => client,
            Err(error) => {
                return finish_proxy_outcome(
                    pool,
                    &platform,
                    credential,
                    parts,
                    Some(entry_url),
                    Some(entry_path),
                    Some(trace_id),
                    None,
                    None,
                    String::new(),
                    None,
                    Some(error),
                    false,
                    elapsed_ms(start),
                )
                .await;
            }
        };
        let send_result = send_proxy_model_test_request(
            client,
            &entry_url,
            headers,
            parts.request_body_json.as_bytes().to_vec(),
            failure_policy,
        )
        .await;
        let duration_ms = elapsed_ms(start);
        let trace = load_route_proxy_model_test_trace(pool, &trace_id).await?;

        match send_result {
            Ok((status, success, body)) => {
                let response_body =
                    sanitize_for_storage(&credential, &truncate_response_body(&body));
                let response_text = extract_model_test_response_text(
                    model_test_response_format(&platform, &parts.interface_format),
                    &response_body,
                );
                finish_proxy_outcome(
                    pool,
                    &platform,
                    credential,
                    parts,
                    Some(entry_url),
                    Some(entry_path),
                    Some(trace_id),
                    trace,
                    Some(status),
                    response_body,
                    response_text,
                    None,
                    success,
                    duration_ms,
                )
                .await
            }
            Err(error) => {
                let error = sanitize_for_storage(&credential, &error);
                finish_proxy_outcome(
                    pool,
                    &platform,
                    credential,
                    parts,
                    Some(entry_url),
                    Some(entry_path),
                    Some(trace_id),
                    trace,
                    None,
                    String::new(),
                    None,
                    Some(error),
                    false,
                    duration_ms,
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
    interface_override: Option<&str>,
) -> Result<ModelTestRequestParts, String> {
    let platform_id = PlatformId::parse(platform).map_err(format_app_error)?;
    let rule = PlatformCapabilityService::require(platform_id, PlatformOperation::ModelTest)
        .map_err(format_app_error)?;
    if !rule.credential_kinds.is_empty()
        && !rule
            .credential_kinds
            .iter()
            .any(|kind| kind == &credential.kind)
    {
        return Err("capability.unavailable: credential kind is not supported".to_string());
    }
    if credential.kind != "api" {
        PlatformCapabilityService::require(platform_id, PlatformOperation::OfficialAccountRouting)
            .map_err(format_app_error)?;
    }
    let config = parse_json_object(&credential.config_json, "config")?;
    let dialect = match interface_override {
        Some(value) => ApiDialect::parse(value).map_err(format_app_error)?,
        None => interface_format_for(credential, platform_id, &config).map_err(format_app_error)?,
    };
    let interface_format = dialect.as_str().to_string();
    let base_url = string_value(&config, "base_url").map(str::to_string);
    if rule.requires_base_url && base_url.is_none() {
        return Err("validation.base_url_required: API base URL is required".to_string());
    }
    let mappings = model_mappings(&config);
    let model = request_model(platform, &interface_format, &mappings, requested_model);

    let (request_path, request_body) = match platform {
        "codex" => (
            "/responses".to_string(),
            codex_probe_body(&model, &credential.id, &interface_format),
        ),
        "claude" => (
            "/v1/messages".to_string(),
            anthropic_probe_body(&model, &credential.id),
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
        _ => match interface_format.as_str() {
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
            "anthropic" => (
                "/v1/messages".to_string(),
                anthropic_probe_body(&model, &credential.id),
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
        },
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

/// Probe body for the Anthropic dialect, shaped like a real Claude Code request.
///
/// The bare `model`/`messages`/`max_tokens` triple is a valid Anthropic request
/// but reads as a non-CLI client to relays that gate on the Claude Code
/// signature (sub2api's `claude_code_only` group flag). Those relays require a
/// `system` block scoring against Claude Code's own prompt *and* a parseable
/// `metadata.user_id`, so a probe without both fails with `this group only
/// allows Claude Code clients` while the same account works from the real CLI.
fn anthropic_probe_body(model: &str, credential_id: &str) -> Value {
    json!({
        "model": model,
        "system": [{
            "type": "text",
            "text": client_identity::CLAUDE_CODE_SYSTEM_PROMPT
        }],
        "messages": [{"role": "user", "content": MODEL_TEST_PROMPT}],
        "metadata": {
            "user_id": client_identity::claude_code_metadata_user_id(credential_id)
        },
        "max_tokens": 16
    })
}

/// Probe body for the Codex platform, in Responses shape.
///
/// When the upstream speaks Anthropic, the protocol bridge rewrites this into a
/// `/v1/messages` request — and that request has to clear the same Claude Code
/// gate as a native Anthropic probe. The bridge derives `system` from
/// `instructions` and forwards `metadata`, so both are seeded here rather than
/// patched onto the converted body.
fn codex_probe_body(model: &str, credential_id: &str, interface_format: &str) -> Value {
    let mut body = json!({
        "model": model,
        "input": MODEL_TEST_PROMPT,
        "temperature": 0,
        "max_output_tokens": 16
    });

    if interface_format == "anthropic" {
        body["instructions"] = json!(client_identity::CLAUDE_CODE_SYSTEM_PROMPT);
        body["metadata"] = json!({
            "user_id": client_identity::claude_code_metadata_user_id(credential_id)
        });
    }

    body
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

    if interface_format == "anthropic" {
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

/// Mirrors `route_model_capability::is_placeholder_model`. Intentionally does
/// NOT cover the route sentinels (`*`, `claude-subagent`): filtering those here
/// would silently delete the fallback and subagent features at parse time.
fn is_placeholder_model(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value == "upstream-model"
}

fn interface_format_for(
    credential: &SelectedCredential,
    platform: PlatformId,
    config: &Value,
) -> Result<ApiDialect, AppError> {
    if credential.kind == "api" {
        return match string_value(config, "interface_format") {
            Some(value) => ApiDialect::parse(value),
            None if matches!(
                platform,
                PlatformId::OpenCode | PlatformId::OpenClaw | PlatformId::Hermes
            ) =>
            {
                Err(api_dialect_required())
            }
            None => platform
                .default_api_credential_dialect()
                .ok_or_else(api_dialect_required),
        };
    }

    PlatformCapabilityService::require(platform, PlatformOperation::OfficialAccountRouting)?;
    match platform {
        PlatformId::Codex => Ok(ApiDialect::OpenAiResponses),
        PlatformId::Claude => Ok(ApiDialect::Anthropic),
        PlatformId::Grok => Ok(ApiDialect::OpenAi),
        PlatformId::Gemini => Ok(ApiDialect::Gemini),
        PlatformId::OpenCode | PlatformId::OpenClaw | PlatformId::Hermes => {
            Err(AppError::Validation {
                code: "capability.unavailable",
                message: "Official account routing is unavailable".to_string(),
                details: Some(platform.as_str().to_string()),
                recoverable: true,
            })
        }
    }
}

fn api_dialect_required() -> AppError {
    AppError::Validation {
        code: "validation.api_dialect_required",
        message: "API dialect is required".to_string(),
        details: None,
        recoverable: true,
    }
}

fn default_model_for(interface_format: &str) -> &'static str {
    match interface_format {
        "anthropic" => "claude-sonnet-4-20250514",
        "gemini" => "gemini-2.5-flash",
        _ => "gpt-5.5",
    }
}

fn default_model_for_platform(platform: &str, interface_format: &str) -> String {
    if platform == "grok" {
        return "grok-4.5".to_string();
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

    // Skip the catch-all sentinel: it is not a model name, so probing it would
    // send a meaningless request and display the sentinel as the tested model.
    mappings
        .iter()
        .find(|mapping| !is_fallback_mapping(mapping))
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
        .iter()
        .find(|mapping| !is_fallback_mapping(mapping))
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

async fn load_account_credential(
    pool: &SqlitePool,
    platform: &str,
    account_id: &str,
) -> Result<SelectedCredential, AppError> {
    let row = sqlx::query(
        "SELECT id, platform, kind, display_name, status, route_priority, max_concurrency,
                secret_payload_json, config_json,
                next_retry_at, cooldown_until
         FROM route_credentials
         WHERE id = ? AND platform = ? AND archived_at IS NULL",
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

    let Some(row) = row else {
        return Err(AppError::Validation {
            code: "validation.route_model_test_account_not_found",
            message: "Route credential does not exist for this platform".to_string(),
            details: Some(account_id.to_string()),
            recoverable: true,
        });
    };
    Ok(SelectedCredential {
        id: row.get("id"),
        platform: row.get("platform"),
        kind: row.get("kind"),
        display_name: row.get("display_name"),
        status: row.get("status"),
        route_priority: row.get("route_priority"),
        max_concurrency: row.get("max_concurrency"),
        secret_payload_json: row.get("secret_payload_json"),
        config_json: row.get("config_json"),
    })
}

async fn send_model_test_request(
    client: reqwest::Client,
    target_url: &str,
    headers: HeaderMap,
    body: Vec<u8>,
    failure_policy: RouteCredentialFailurePolicy,
) -> Result<(u16, bool, Vec<u8>), String> {
    let headers = map_to_reqwest_headers(&headers);
    let streaming_request = request_body_requests_stream(&body);
    for attempt in 0..=failure_policy.retry_count {
        let upstream = match client
            .post(target_url)
            .headers(headers.clone())
            .body(body.clone())
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let message = upstream_model_test_request_error_message(&error);
                if attempt < failure_policy.retry_count {
                    wait_for_model_test_retry(failure_policy).await;
                    continue;
                }
                return Err(message);
            }
        };
        let status = upstream.status();
        let content_type = upstream
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        let content_type = content_type.map(str::to_string);
        let body = match upstream.bytes().await {
            Ok(body) => body.to_vec(),
            Err(error) => {
                if attempt < failure_policy.retry_count {
                    wait_for_model_test_retry(failure_policy).await;
                    continue;
                }
                return Err(format!("Could not read model test response: {error}"));
            }
        };

        let status_code = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let semantic_failure = detect_response_failed(&body).or_else(|| {
            stream_disconnected_before_completion(&body, content_type.as_deref(), streaming_request)
                .then(|| SemanticResponseFailure {
                    code: None,
                    error_type: None,
                    message: STREAM_DISCONNECTED_FAILURE_MESSAGE.to_string(),
                })
        });
        let definitive_quota_failure = semantic_failure
            .as_ref()
            .is_some_and(is_quota_exhaustion_failure);
        if attempt < failure_policy.retry_count
            && !definitive_quota_failure
            && ((!status_code.is_success()
                && !matches!(
                    status_code,
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                ))
                || (semantic_failure.is_some()
                    && !matches!(
                        status_code,
                        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                    )))
        {
            wait_for_model_test_retry(failure_policy).await;
            continue;
        }

        return Ok((status.as_u16(), status.is_success(), body));
    }

    Err("Upstream model test request failed after retries".to_string())
}

async fn send_proxy_model_test_request(
    client: reqwest::Client,
    entry_url: &str,
    headers: HeaderMap,
    body: Vec<u8>,
    failure_policy: RouteCredentialFailurePolicy,
) -> Result<(u16, bool, Vec<u8>), String> {
    let headers = map_to_reqwest_headers(&headers);
    for attempt in 0..=failure_policy.retry_count {
        let response = match client
            .post(entry_url)
            .headers(headers.clone())
            .body(body.clone())
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let message = route_proxy_model_test_request_error_message(&error);
                if attempt < failure_policy.retry_count {
                    wait_for_model_test_retry(failure_policy).await;
                    continue;
                }
                return Err(message);
            }
        };
        let status = response.status();
        let body = match response.bytes().await {
            Ok(body) => body.to_vec(),
            Err(error) => {
                if attempt < failure_policy.retry_count {
                    wait_for_model_test_retry(failure_policy).await;
                    continue;
                }
                return Err(format!("Could not read route proxy test response: {error}"));
            }
        };

        return Ok((status.as_u16(), status.is_success(), body));
    }

    Err("Route proxy model test request failed after retries".to_string())
}

fn request_body_requests_stream(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}

async fn wait_for_model_test_retry(failure_policy: RouteCredentialFailurePolicy) {
    if failure_policy.retry_interval_ms > 0 {
        tokio::time::sleep(Duration::from_millis(
            failure_policy.retry_interval_ms.into(),
        ))
        .await;
    }
}

fn upstream_model_test_request_error_message(error: &reqwest::Error) -> String {
    let mut message = format!("Upstream model test request failed: {error}");
    if error.is_connect() || error.is_timeout() {
        message.push_str(" (check network/proxy; Windows system proxy is applied when configured)");
    }
    message
}

fn route_proxy_model_test_request_error_message(error: &reqwest::Error) -> String {
    let mut message = format!("Route proxy model test request failed: {error}");
    if error.is_connect() || error.is_timeout() {
        message.push_str(" (check whether the local route proxy is running and reachable)");
    }
    message
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

fn header_value(value: &str, label: &str) -> Result<HeaderValue, AppError> {
    HeaderValue::from_str(value).map_err(|err| AppError::Validation {
        code: "validation.route_model_test_header",
        message: format!("Could not build route model test {label} header"),
        details: Some(err.to_string()),
        recoverable: true,
    })
}

fn join_proxy_entry_url(base_url: &str, entry_path: &str) -> String {
    build_target_url(base_url, entry_path, None)
}

fn normalize_local_model_test_entry_path(
    platform: &str,
    interface_format: &str,
    request_path: &str,
) -> String {
    match platform {
        "codex" => strip_local_v1_prefix(request_path),
        "claude" => ensure_leading_slash(request_path),
        "gemini" => ensure_leading_slash(request_path),
        _ => normalize_api_upstream_path(interface_format, request_path),
    }
}

fn model_test_response_format<'a>(platform: &str, interface_format: &'a str) -> &'a str {
    match platform {
        "codex" => "openai-responses",
        "claude" => "anthropic",
        "gemini" => "gemini",
        _ => interface_format,
    }
}

fn strip_local_v1_prefix(path: &str) -> String {
    let path = ensure_leading_slash(path);
    path.strip_prefix("/v1/")
        .map(|rest| format!("/{rest}"))
        .unwrap_or(path)
}

fn ensure_leading_slash(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn validate_model_test_interface_override(
    platform: &str,
    requested: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let allowed = match platform {
        "codex" | "claude" => {
            matches!(
                requested,
                "openai" | "openai-responses" | "anthropic" | "gemini"
            )
        }
        "gemini" => requested == "gemini",
        "grok" | "opencode" | "openclaw" | "hermes" => requested == "openai",
        _ => false,
    };
    if !allowed {
        return Err(AppError::Validation {
            code: "validation.route_model_test_interface_format",
            message: "Unsupported model test interface format".to_string(),
            details: Some(format!("{platform}:{requested}")),
            recoverable: true,
        });
    }

    Ok(Some(requested.to_string()))
}

#[derive(Debug, Clone, Default)]
struct RouteProxyModelTestTrace {
    route_credential_id: Option<String>,
    route_credential_name: Option<String>,
    target_url: Option<String>,
}

async fn load_route_proxy_model_test_trace(
    pool: &SqlitePool,
    trace_id: &str,
) -> Result<Option<RouteProxyModelTestTrace>, AppError> {
    let rows = sqlx::query(
        "SELECT route_credential_id, metadata_json
         FROM usage_events
         WHERE source_label = 'route_proxy'
           AND metric_type = 'request'
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .fetch_all(pool)
    .await
    .map_err(|err| AppError::Database {
        code: "database.route_model_test_trace",
        message: "Could not load route proxy model test trace".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    for row in rows {
        let row_credential_id: Option<String> = row.try_get("route_credential_id").ok();
        let metadata_json: String = row.get("metadata_json");
        let metadata = serde_json::from_str::<Value>(&metadata_json).unwrap_or_else(|_| json!({}));
        if string_value(&metadata, "trace_id") != Some(trace_id) {
            continue;
        }

        return Ok(Some(RouteProxyModelTestTrace {
            route_credential_id: string_value(&metadata, "route_credential_id")
                .map(str::to_string)
                .or(row_credential_id),
            route_credential_name: string_value(&metadata, "route_credential_name")
                .map(str::to_string),
            target_url: string_value(&metadata, "target_url").map(str::to_string),
        }));
    }

    Ok(None)
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
    usage: RouteUsageBreakdown,
) -> Result<RoutePoolModelTestOutcome, AppError> {
    // Official accounts may report free/quota exhaustion in response bodies.
    if !response_body.trim().is_empty() {
        let _ = maybe_persist_official_quota_from_response(pool, &credential, &response_body).await;
    }
    if success {
        let _ = RouteCredentialRepository::clear_transient_failure(pool, &credential.id).await;
        if should_restore_model_test_account_status(&credential.status) {
            RouteCredentialRepository::update_status(pool, &credential.id, "ok").await?;
        }
    } else {
        let status = response_status.and_then(|value| axum::http::StatusCode::from_u16(value).ok());
        let quota_failure = detect_response_failed(response_body.as_bytes())
            .is_some_and(|failure| is_quota_exhaustion_failure(&failure));
        if quota_failure {
            RouteCredentialRepository::update_status(pool, &credential.id, "error").await?;
        } else if let Some(status) = status.filter(|status| !status.is_success()) {
            let message = error_message
                .as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| format!("upstream returned {}", status.as_u16()));
            RouteCredentialRepository::record_transient_failure(
                pool,
                &credential.id,
                "model_test_status",
                &message,
                Some(response_body.as_bytes()),
            )
            .await?;
        } else if let Some(failure) = detect_response_failed(response_body.as_bytes()) {
            RouteCredentialRepository::record_transient_failure(
                pool,
                &credential.id,
                "semantic_response_transient",
                &failure.message,
                Some(response_body.as_bytes()),
            )
            .await?;
        } else {
            let failure_kind = classify_proxy_failure(
                status,
                error_message.as_deref().or_else(|| {
                    (!response_body.trim().is_empty()).then_some(response_body.as_str())
                }),
            );
            match failure_kind {
                ProxyFailureKind::Permanent => {
                    RouteCredentialRepository::update_status(pool, &credential.id, "revoked")
                        .await?;
                }
                ProxyFailureKind::Transient => {
                    let message = error_message
                        .as_deref()
                        .unwrap_or("model test request failed");
                    let _ = RouteCredentialRepository::record_transient_failure(
                        pool,
                        &credential.id,
                        "model_test",
                        message,
                        Some(response_body.as_bytes()),
                    )
                    .await;
                }
                ProxyFailureKind::None => {}
            }
        }
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

    RoutePoolRepository::insert_request_event(
        pool,
        &credential.id,
        ROUTE_MODEL_TEST_SOURCE,
        &metadata,
        &usage,
    )
    .await?;

    RoutePoolRepository::save_cursor_index(pool, platform, next_index).await?;

    Ok(RoutePoolModelTestOutcome {
        platform: platform.to_string(),
        selected_account_id: credential.id,
        selected_account_name: credential.display_name,
        via_route_proxy: false,
        route_proxy_entry_url: None,
        route_proxy_entry_path: None,
        route_proxy_trace_id: None,
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

#[allow(clippy::too_many_arguments)]
async fn finish_proxy_outcome(
    pool: &SqlitePool,
    platform: &str,
    credential: SelectedCredential,
    parts: ModelTestRequestParts,
    route_proxy_entry_url: Option<String>,
    route_proxy_entry_path: Option<String>,
    route_proxy_trace_id: Option<String>,
    trace: Option<RouteProxyModelTestTrace>,
    response_status: Option<u16>,
    response_body: String,
    response_text: Option<String>,
    error_message: Option<String>,
    success: bool,
    duration_ms: i64,
) -> Result<RoutePoolModelTestOutcome, AppError> {
    let selected_account_id = trace
        .as_ref()
        .and_then(|trace| trace.route_credential_id.clone())
        .unwrap_or_else(|| credential.id.clone());
    let selected_account_name = trace
        .as_ref()
        .and_then(|trace| trace.route_credential_name.clone())
        .unwrap_or_else(|| credential.display_name.clone());
    let target_url = trace
        .as_ref()
        .and_then(|trace| trace.target_url.clone())
        .or(parts.target_url.clone());

    Ok(RoutePoolModelTestOutcome {
        platform: platform.to_string(),
        selected_account_id,
        selected_account_name,
        via_route_proxy: route_proxy_entry_url.is_some(),
        route_proxy_entry_url,
        route_proxy_entry_path: route_proxy_entry_path.clone(),
        route_proxy_trace_id,
        interface_format: parts.interface_format,
        request_path: route_proxy_entry_path.unwrap_or(parts.request_path),
        base_url: parts.base_url,
        target_url,
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

fn should_restore_model_test_account_status(status: &str) -> bool {
    matches!(status, "error" | "warning")
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
        interface_format: PlatformId::parse(platform)
            .ok()
            .and_then(|platform| interface_format_for(credential, platform, &config).ok())
            .map(|dialect| dialect.as_str().to_string())
            .unwrap_or_default(),
        request_path: String::new(),
        base_url: string_value(&config, "base_url").map(str::to_string),
        target_url: None,
        request_body_json: String::new(),
    }
}

fn filter_model_test_credentials(
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

fn validate_model_test_credential(
    platform: PlatformId,
    credential: &SelectedCredential,
) -> Result<(), AppError> {
    // Paused (暂停) accounts are intentionally testable: an explicit per-account
    // test is how a user probes whether a paused account has recovered, and a
    // successful test restores it to "ok" via recover_after_explicit_test.
    let rule = PlatformCapabilityService::require(platform, PlatformOperation::ModelTest)?;
    if !rule.credential_kinds.is_empty()
        && !rule
            .credential_kinds
            .iter()
            .any(|kind| kind == &credential.kind)
    {
        return Err(AppError::Validation {
            code: "capability.unavailable",
            message: "Credential kind is unavailable for this platform operation".to_string(),
            details: Some(credential.kind.clone()),
            recoverable: true,
        });
    }
    if credential.kind != "api" {
        PlatformCapabilityService::require(platform, PlatformOperation::OfficialAccountRouting)?;
    }
    Ok(())
}

fn format_app_error(error: AppError) -> String {
    let error = ApiError::from(error);
    format!("{}: {}", error.code, error.message)
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
    use crate::models::route_credential::DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT;
    use crate::models::route_pool::{RoutePoolModelTestRequest, SetRoutePoolMembersInput};
    use crate::paths::AppPaths;
    use crate::services::route_pool_service::RoutePoolService;
    use crate::services::route_proxy_https_service::RouteProxyHttpsService;
    use crate::services::route_proxy_service::{
        RouteProxyRuntimeState, RouteProxyService, RouteProxyTransport,
    };
    use axum::{routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn api_credential(interface_format: &str) -> SelectedCredential {
        SelectedCredential {
            id: "cred-api".to_string(),
            platform: "codex".to_string(),
            kind: "api".to_string(),
            display_name: "API Account".to_string(),
            status: "ok".to_string(),
            route_priority: 3,
            max_concurrency: 1,
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
            status: "ok".to_string(),
            route_priority: 3,
            max_concurrency: 1,
            secret_payload_json: r#"{"access_token":"at"}"#.to_string(),
            config_json: "{}".to_string(),
        }
    }

    #[test]
    fn partial_platform_api_model_test_uses_explicit_dialect() {
        let mut credential = api_credential("openai");
        credential.platform = "hermes".to_string();

        let request = build_model_test_request(&credential, "hermes", None, None)
            .expect("explicit Hermes API dialect");

        assert_eq!(request.interface_format, "openai");
        assert_eq!(request.request_path, "/chat/completions");
    }

    #[test]
    fn partial_platform_official_model_test_is_unavailable() {
        let error = build_model_test_request(&official_credential("hermes"), "hermes", None, None)
            .expect_err("Hermes official model testing is unavailable");

        assert!(error.contains("capability.unavailable"));
    }

    #[test]
    fn partial_platform_api_model_test_requires_explicit_dialect() {
        let mut credential = api_credential("openai");
        credential.platform = "hermes".to_string();
        credential.config_json = json!({
            "base_url": "https://api.example.com/v1",
            "model_mappings": []
        })
        .to_string();

        let error = build_model_test_request(&credential, "hermes", None, None)
            .expect_err("Hermes API model tests require a dialect");

        assert!(error.contains("validation.api_dialect_required"));
    }

    #[test]
    fn codex_model_test_builds_local_responses_body_for_anthropic_upstream() {
        let credential = api_credential("anthropic");
        let request =
            build_model_test_request(&credential, "codex", Some("claude-sonnet-4-20250514"), None)
                .unwrap();

        assert_eq!(request.interface_format, "anthropic");
        assert_eq!(request.request_path, "/responses");
        let body: Value = serde_json::from_str(&request.request_body_json).unwrap();
        assert_eq!(body["input"], MODEL_TEST_PROMPT);
        assert_eq!(body["max_output_tokens"], 16);
    }

    #[test]
    fn codex_probe_survives_the_claude_code_gate_after_bridging_to_anthropic() {
        // Codex can target an Anthropic upstream, and the converted request hits
        // `/v1/messages` — the same endpoint sub2api's `claude_code_only` group
        // gates. The probe therefore has to carry the signature through the
        // bridge, not just on the native Anthropic path.
        let credential = api_credential("anthropic");
        let request =
            build_model_test_request(&credential, "codex", Some("claude-sonnet-4-20250514"), None)
                .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        // `instructions` is what the bridge turns into the Anthropic `system`
        // block; the conversion itself is covered in responses_claude.
        assert_eq!(
            body.pointer("/instructions").and_then(Value::as_str),
            Some(client_identity::CLAUDE_CODE_SYSTEM_PROMPT),
        );

        let user_id = body
            .pointer("/metadata/user_id")
            .and_then(Value::as_str)
            .expect("metadata.user_id");
        let parsed: Value = serde_json::from_str(user_id).expect("user_id is json");
        assert_eq!(parsed["device_id"].as_str().map(str::len), Some(64));
    }

    #[test]
    fn codex_probe_stays_clean_for_non_anthropic_upstreams() {
        // The Claude Code signature is meaningless to an OpenAI-dialect relay and
        // `instructions` would become a real system prompt that competes with the
        // probe's "reply with exactly" instruction.
        let request = build_model_test_request(&api_credential("openai"), "codex", None, None)
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert!(body.get("instructions").is_none());
        assert!(body.get("metadata").is_none());
    }

    #[test]
    fn claude_model_test_builds_local_messages_body_for_openai_upstream() {
        let credential = api_credential("openai");
        let request =
            build_model_test_request(&credential, "claude", Some("gpt-5.5"), None).unwrap();

        assert_eq!(request.interface_format, "openai");
        assert_eq!(request.request_path, "/v1/messages");
        let body: Value = serde_json::from_str(&request.request_body_json).unwrap();
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["max_tokens"], 16);
    }

    async fn start_json_test_server(status: axum::http::StatusCode, body: Value) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let versioned_body = body.clone();
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(move || {
                    let body = versioned_body.clone();
                    async move { (status, Json(body)) }
                }),
            )
            .route(
                "/chat/completions",
                post(move || {
                    let body = body.clone();
                    async move { (status, Json(body)) }
                }),
            );
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        format!("http://{addr}/v1")
    }

    async fn start_flaky_body_test_server(
        failed_attempts: usize,
        success_body: &'static str,
    ) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let addr = listener.local_addr().expect("addr");
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
        (format!("http://{addr}/v1"), calls)
    }

    fn unversioned_base_url(versioned_base_url: &str) -> String {
        versioned_base_url
            .strip_suffix("/v1")
            .expect("versioned base url")
            .to_string()
    }

    async fn create_api_credential(pool: &SqlitePool, base_url: &str) -> String {
        create_api_credential_with_config(pool, base_url, json!({})).await
    }

    async fn create_api_credential_with_config(
        pool: &SqlitePool,
        base_url: &str,
        extra_config: Value,
    ) -> String {
        let mut config = json!({
            "base_url": base_url,
            "interface_format": "openai",
            "model_mappings": [{"from":"gpt-5","to":"up-gpt"}]
        });
        if let (Some(config), Some(extra_config)) =
            (config.as_object_mut(), extra_config.as_object())
        {
            config.extend(extra_config.clone());
        }
        RouteCredentialRepository::create(
            pool,
            "codex",
            "api",
            "API Account",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            &config.to_string(),
            r#"{"config_toml":""}"#,
        )
        .await
        .expect("credential")
        .id
    }

    #[test]
    fn builds_openai_chat_test_request() {
        let request = build_model_test_request(&api_credential("openai"), "codex", None, None)
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "openai");
        assert_eq!(request.request_path, "/responses");
        assert_eq!(
            body.pointer("/model").and_then(Value::as_str),
            Some("gpt-5")
        );
        assert_eq!(
            body.pointer("/input").and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT),
        );
        assert_eq!(
            body.pointer("/max_output_tokens").and_then(Value::as_i64),
            Some(16)
        );
    }

    #[test]
    fn builds_openai_chat_test_request_with_explicit_model() {
        let request =
            build_model_test_request(&api_credential("openai"), "codex", Some("gpt-4o"), None)
                .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(
            body.pointer("/model").and_then(Value::as_str),
            Some("gpt-4o")
        );
    }

    #[test]
    fn builds_openai_responses_test_request() {
        let request =
            build_model_test_request(&api_credential("openai-responses"), "codex", None, None)
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
        let request = build_model_test_request(&official_credential("codex"), "codex", None, None)
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
    fn model_test_interface_override_selects_responses_request_shape() {
        let request = build_model_test_request(
            &api_credential("openai"),
            "codex",
            Some("gpt-5.5"),
            Some("openai-responses"),
        )
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
    fn model_test_interface_override_selects_chat_completions_request_shape() {
        let request = build_model_test_request(
            &api_credential("openai-responses"),
            "codex",
            Some("gpt-5.5"),
            Some("openai"),
        )
        .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "openai");
        assert_eq!(request.request_path, "/responses");
        assert_eq!(
            body.pointer("/input").and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT)
        );
    }

    #[test]
    fn model_test_interface_override_validates_scope_and_values() {
        assert_eq!(
            validate_model_test_interface_override("codex", Some("openai"))
                .expect("valid override")
                .as_deref(),
            Some("openai")
        );
        assert_eq!(
            validate_model_test_interface_override("codex", Some("gemini"))
                .expect("valid Codex Gemini override")
                .as_deref(),
            Some("gemini")
        );
        assert_eq!(
            validate_model_test_interface_override("claude", Some("openai"))
                .expect("valid Claude OpenAI override")
                .as_deref(),
            Some("openai")
        );
        assert!(validate_model_test_interface_override("gemini", Some("openai")).is_err());
        assert_eq!(
            validate_model_test_interface_override("hermes", Some("openai"))
                .expect("valid Hermes OpenAI override")
                .as_deref(),
            Some("openai")
        );
        assert_eq!(
            validate_model_test_interface_override("claude", None).expect("missing override"),
            None
        );
    }

    #[test]
    fn builds_openai_test_request_for_official_grok() {
        let request = build_model_test_request(&official_credential("grok"), "grok", None, None)
            .expect("request");
        assert_eq!(request.interface_format, "openai");
        assert_eq!(request.request_path, "/chat/completions");
        assert!(
            request
                .request_body_json
                .contains("\"model\": \"grok-4.5\"")
                || request.request_body_json.contains("\"model\":\"grok-4.5\"")
        );
    }

    #[test]
    fn builds_anthropic_test_request_for_official_claude() {
        let request =
            build_model_test_request(&official_credential("claude"), "claude", None, None)
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
    fn anthropic_probe_carries_the_claude_code_signature() {
        // Relays with sub2api's `claude_code_only` group flag score the `system`
        // block against Claude Code's own prompt and parse `metadata.user_id`.
        // A probe missing either is rejected with "this group only allows Claude
        // Code clients" even though the same account works from the real CLI.
        for credential in [api_credential("anthropic"), official_credential("claude")] {
            let request =
                build_model_test_request(&credential, "claude", None, None).expect("request");
            let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

            assert_eq!(
                body.pointer("/system/0/text").and_then(Value::as_str),
                Some(client_identity::CLAUDE_CODE_SYSTEM_PROMPT),
            );
            assert_eq!(
                body.pointer("/system/0/type").and_then(Value::as_str),
                Some("text"),
            );

            let user_id = body
                .pointer("/metadata/user_id")
                .and_then(Value::as_str)
                .expect("metadata.user_id");
            let parsed: Value = serde_json::from_str(user_id).expect("user_id is json");
            assert_eq!(
                parsed["device_id"].as_str().map(str::len),
                Some(64),
                "gate regex requires 64 hex chars",
            );
            assert_eq!(parsed["session_id"].as_str().map(str::len), Some(36));
        }
    }

    #[test]
    fn builds_gemini_test_request_and_uses_mapping_target_in_path() {
        let request = build_model_test_request(&api_credential("gemini"), "gemini", None, None)
            .expect("request");
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
        let request = build_model_test_request(
            &api_credential("gemini"),
            "gemini",
            Some("gemini-1.5-pro"),
            None,
        )
        .expect("request");

        assert_eq!(
            request.request_path,
            "/v1beta/models/gemini-1.5-pro:generateContent"
        );
    }

    #[test]
    fn builds_gemini_test_request_with_explicit_mapping_target_path() {
        let request =
            build_model_test_request(&api_credential("gemini"), "gemini", Some("gpt-5"), None)
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
        let request = build_model_test_request(&credential, "gemini", None, None).expect("request");

        assert_eq!(
            request.request_path,
            "/v1beta/models/gemini-2.5-flash:generateContent"
        );
        assert!(!request.request_path.contains("upstream-model"));
    }

    #[test]
    fn proxy_entry_url_deduplicates_versioned_proxy_base_url() {
        assert_eq!(
            join_proxy_entry_url("http://127.0.0.1:43111/v1", "/v1/chat/completions"),
            "http://127.0.0.1:43111/v1/chat/completions"
        );
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
                "usage": {
                    "prompt_tokens": 5,
                    "completion_tokens": 3,
                    "prompt_cache_hit_tokens": 2,
                    "price_cny": 7.1
                }
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
                interface_format: None,
            },
        )
        .await
        .expect("outcome");
        let expected_target_url = format!("{base_url}/chat/completions");

        assert!(outcome.success);
        assert_eq!(outcome.selected_account_id, credential_id);
        assert_eq!(outcome.selected_account_name, "API Account");
        assert_eq!(outcome.interface_format, "openai");
        assert_eq!(outcome.request_path, "/responses");
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
        assert_eq!(outcome.stats.input_token_count, 5);
        assert_eq!(outcome.stats.output_token_count, 3);
        assert_eq!(outcome.stats.cache_token_count, 2);
        assert_eq!(outcome.stats.cost_micros, 1_000_000);
        assert_eq!(outcome.stats.requests.len(), 1);
        assert_eq!(
            outcome.stats.requests[0].source_label,
            ROUTE_MODEL_TEST_SOURCE
        );
        assert_eq!(outcome.stats.requests[0].input_tokens, Some(5));
        assert_eq!(outcome.stats.requests[0].output_tokens, Some(3));
        assert_eq!(outcome.stats.requests[0].cache_tokens, Some(2));
        assert_eq!(outcome.stats.requests[0].price_cny_micros, Some(7_100_000));
        assert_eq!(
            outcome.stats.requests[0].price_currency.as_deref(),
            Some("cny")
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
    async fn test_model_retries_transient_body_read_errors_before_recording_failure() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let (base_url, calls) = start_flaky_body_test_server(
            DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT as usize,
            r#"{"choices":[{"message":{"content":"ai-switch-ok"}}]}"#,
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
                interface_format: None,
            },
        )
        .await
        .expect("outcome");

        assert!(outcome.success);
        assert_eq!(outcome.response_status, Some(200));
        assert_eq!(outcome.response_text.as_deref(), Some("ai-switch-ok"));
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
    }

    #[tokio::test]
    async fn test_model_uses_account_specific_retry_count() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let (base_url, calls) = start_flaky_body_test_server(
            4,
            r#"{"choices":[{"message":{"content":"ai-switch-ok"}}]}"#,
        )
        .await;
        let credential_id = create_api_credential_with_config(
            &pool,
            &base_url,
            json!({
                "failure_policy": {
                    "retry_count": 4,
                    "retry_interval_ms": 0,
                    "semantic_error_threshold": 10
                }
            }),
        )
        .await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![credential_id],
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
                interface_format: None,
            },
        )
        .await
        .expect("outcome");

        assert!(outcome.success);
        assert_eq!(calls.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn test_model_uses_unversioned_openai_api_base_url_as_configured() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let versioned_base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "choices": [{"message": {"content": "ai-switch-ok"}}],
                "usage": {"prompt_tokens": 2, "completion_tokens": 1}
            }),
        )
        .await;
        let base_url = unversioned_base_url(&versioned_base_url);
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
                interface_format: None,
            },
        )
        .await
        .expect("outcome");

        assert_eq!(
            outcome.target_url.as_deref(),
            Some(format!("{base_url}/chat/completions").as_str())
        );
        assert!(outcome.success);
    }

    #[tokio::test]
    async fn the_probe_body_carries_no_turn_reminder() {
        // The probe asks the model to reply with exactly `ai-switch-ok`. An
        // account whose reminder says "answer in Chinese" contradicts that head
        // on, so if the reminder reached the probe every test against such an
        // account would fail and the account would look broken when it was fine.
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
        let credential_id = create_api_credential_with_config(
            &pool,
            &base_url,
            json!({"turn_reminder": true, "turn_reminder_text": "请用简体中文回复。"}),
        )
        .await;

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
                interface_format: None,
            },
        )
        .await
        .expect("outcome");

        assert!(outcome.success);
        assert!(
            !outcome.request_body_json.contains("请用简体中文回复"),
            "probe body must not carry the reminder: {}",
            outcome.request_body_json
        );
    }

    #[tokio::test]
    async fn test_model_through_proxy_reports_proxy_entry_and_selected_account() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let versioned_base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "choices": [{"message": {"content": "ai-switch-ok"}}],
                "usage": {
                    "prompt_tokens": 4,
                    "completion_tokens": 2,
                    "prompt_cache_hit_tokens": 1,
                    "price_cny": 7.1
                }
            }),
        )
        .await;
        let base_url = unversioned_base_url(&versioned_base_url);
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

        let route_proxy_state = RouteProxyRuntimeState::default();
        let proxy_status =
            RouteProxyService::start(&route_proxy_state, pool.clone(), RouteProxyTransport::Http)
                .await
                .expect("proxy start");
        let proxy_base_url = proxy_status.base_url.expect("proxy base url");

        let outcome = RouteModelTestService::test_model_through_proxy(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: None,
                model: Some("gpt-5".to_string()),
                interface_format: None,
            },
            &proxy_base_url,
        )
        .await
        .expect("outcome");

        let _ = RouteProxyService::stop(&route_proxy_state).await;

        let expected_entry_url = format!("{proxy_base_url}/responses");
        let expected_target_url = format!("{base_url}/chat/completions");
        assert!(outcome.via_route_proxy);
        assert_eq!(
            outcome.route_proxy_entry_url.as_deref(),
            Some(expected_entry_url.as_str())
        );
        assert_eq!(
            outcome.route_proxy_entry_path.as_deref(),
            Some("/responses")
        );
        assert!(outcome.route_proxy_trace_id.is_some());
        assert_eq!(outcome.selected_account_id, credential_id);
        assert_eq!(outcome.selected_account_name, "API Account");
        assert_eq!(outcome.request_path, "/responses");
        assert_eq!(
            outcome.target_url.as_deref(),
            Some(expected_target_url.as_str())
        );
        assert_eq!(outcome.response_status, Some(200));
        assert_eq!(outcome.response_text.as_deref(), Some("ai-switch-ok"));
        assert_eq!(outcome.stats.requests.len(), 1);
        assert_eq!(outcome.stats.requests[0].source_label, "route_proxy");
        assert_eq!(outcome.stats.input_token_count, 4);
        assert_eq!(outcome.stats.output_token_count, 2);
        assert_eq!(outcome.stats.cache_token_count, 1);
        assert_eq!(outcome.stats.cost_micros, 1_000_000);
        assert_eq!(outcome.stats.requests[0].price_cny_micros, Some(7_100_000));
    }

    #[tokio::test]
    async fn test_model_through_https_proxy_uses_local_root_certificate() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let versioned_base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "choices": [{"message": {"content": "ai-switch-ok"}}],
                "usage": {"prompt_tokens": 4, "completion_tokens": 2, "cost_micros": 9}
            }),
        )
        .await;
        let base_url = unversioned_base_url(&versioned_base_url);
        let credential_id = create_api_credential(&pool, &base_url).await;

        RoutePoolService::set_members(
            &pool,
            SetRoutePoolMembersInput {
                platform: "codex".to_string(),
                account_ids: vec![credential_id],
            },
        )
        .await
        .expect("members");

        let temp = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
        let material = RouteProxyHttpsService::ensure_material(&paths)
            .await
            .expect("https material");
        let root_certificate_pem = tokio::fs::read(&material.root_certificate_pem)
            .await
            .expect("root certificate");
        let route_proxy_state = RouteProxyRuntimeState::default();
        let proxy_status = RouteProxyService::start(
            &route_proxy_state,
            pool.clone(),
            RouteProxyTransport::Https {
                certificate_pem_path: material.server_certificate_pem,
                private_key_pem_path: material.server_private_key_pem,
            },
        )
        .await
        .expect("proxy start");
        let proxy_base_url = proxy_status.base_url.expect("proxy base url");

        let outcome = RouteModelTestService::test_model_through_proxy_with_root_certificate(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: None,
                model: Some("gpt-5".to_string()),
                interface_format: None,
            },
            &proxy_base_url,
            Some(&root_certificate_pem),
        )
        .await
        .expect("outcome");

        let _ = RouteProxyService::stop(&route_proxy_state).await;

        assert!(proxy_base_url.starts_with("https://"));
        assert!(outcome.success);
        assert_eq!(outcome.response_status, Some(200));
        assert_eq!(outcome.response_text.as_deref(), Some("ai-switch-ok"));
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
                interface_format: None,
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
    async fn test_model_restores_error_account_status_on_success() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "choices": [{"message": {"content": "ai-switch-ok"}}]
            }),
        )
        .await;
        let credential_id = create_api_credential(&pool, &base_url).await;
        RouteCredentialRepository::update_status(&pool, &credential_id, "error")
            .await
            .expect("status");

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: Some(credential_id.clone()),
                model: None,
                interface_format: None,
            },
        )
        .await
        .expect("outcome");

        assert!(outcome.success);

        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "ok");
    }

    #[tokio::test]
    async fn test_model_restores_paused_account_status_on_success() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "choices": [{"message": {"content": "ai-switch-ok"}}]
            }),
        )
        .await;
        let credential_id = create_api_credential(&pool, &base_url).await;
        RouteCredentialRepository::update_status(&pool, &credential_id, "paused")
            .await
            .expect("status");

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: Some(credential_id.clone()),
                model: None,
                interface_format: None,
            },
        )
        .await
        .expect("outcome");

        assert!(outcome.success);

        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "ok");
    }

    #[tokio::test]
    async fn test_model_keeps_revoked_account_status_on_success() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "choices": [{"message": {"content": "ai-switch-ok"}}]
            }),
        )
        .await;
        let credential_id = create_api_credential(&pool, &base_url).await;
        RouteCredentialRepository::update_status(&pool, &credential_id, "revoked")
            .await
            .expect("status");

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: Some(credential_id.clone()),
                model: None,
                interface_format: None,
            },
        )
        .await
        .expect("outcome");

        assert!(outcome.success);

        let credential = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("credential");
        assert_eq!(credential.status, "revoked");
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
                interface_format: None,
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
        let base_url =
            start_json_test_server(axum::http::StatusCode::TOO_MANY_REQUESTS, body).await;
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
                interface_format: None,
            },
        )
        .await
        .expect("outcome");

        assert!(!outcome.success);
        let credential = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .expect("credential");
        assert!(credential
            .config_json
            .contains("\"subscription_type\":\"free\""));
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
                interface_format: None,
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
        assert_eq!(credential.status, "ok");
        assert_eq!(credential.transient_failure_count, 1);
        // Cooldown is opt-in, so the failure counts but schedules no backoff.
        assert!(credential.next_retry_at.is_none());
    }

    #[tokio::test]
    async fn test_model_overloaded_response_keeps_account_ok() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let base_url = start_json_test_server(
            axum::http::StatusCode::OK,
            json!({
                "type": "response.failed",
                "response": {
                    "status": "failed",
                    "error": {
                        "message": "Our servers are currently overloaded. Please try again later."
                    }
                }
            }),
        )
        .await;
        let credential = RouteCredentialRepository::create(
            &pool,
            "hermes",
            "api",
            "Overloaded API Account",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            &json!({
                "base_url": base_url,
                "interface_format": "openai",
                "model_mappings": []
            })
            .to_string(),
            r#"{"config_toml":""}"#,
        )
        .await
        .expect("credential");

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "hermes".to_string(),
                account_id: Some(credential.id.clone()),
                model: None,
                interface_format: Some("openai".to_string()),
            },
        )
        .await
        .expect("outcome");

        assert!(!outcome.success);
        assert_eq!(outcome.response_status, Some(200));
        let stored = RouteCredentialRepository::get(&pool, &credential.id)
            .await
            .expect("stored credential");
        assert_eq!(stored.status, "ok");
        assert_eq!(stored.transient_failure_count, 1);
        assert_eq!(
            stored.last_failure_kind.as_deref(),
            Some("semantic_response_transient")
        );
    }

    #[tokio::test]
    async fn test_model_new_api_insufficient_balance_marks_account_error() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let base_url = start_json_test_server(
            axum::http::StatusCode::FORBIDDEN,
            json!({
                "error": {
                    "type": "new_api_error",
                    "message": "用户额度不足, 剩余额度: ＄-0.398052 (request id: 202609020218166141364498268d9d6A3V7Qkt0)"
                },
                "type": "error"
            }),
        )
        .await;
        let credential_id = create_api_credential(&pool, &base_url).await;

        let outcome = RouteModelTestService::test_model(
            &pool,
            RoutePoolModelTestRequest {
                platform: "codex".to_string(),
                account_id: Some(credential_id.clone()),
                model: None,
                interface_format: None,
            },
        )
        .await
        .expect("outcome");

        assert!(!outcome.success);
        let stored = RouteCredentialRepository::get(&pool, &credential_id)
            .await
            .expect("stored credential");
        assert_eq!(stored.status, "error");
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
                interface_format: None,
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

    fn mapping(from: &str, to: &str) -> ModelMapping {
        ModelMapping {
            from: from.to_string(),
            to: to.to_string(),
            label: None,
            supports_1m: None,
        }
    }

    #[test]
    fn request_model_skips_the_fallback_sentinel_when_no_model_is_requested() {
        // "claude-model" is not a model name; probing it would send a
        // meaningless request.
        let mappings = vec![
            mapping("claude-model", "catch-all"),
            mapping("claude-sonnet-alias", "x"),
        ];

        assert_eq!(
            request_model("claude", "anthropic", &mappings, None),
            "claude-sonnet-alias"
        );
    }

    #[test]
    fn request_model_falls_back_to_the_platform_default_when_only_a_fallback_is_configured() {
        let mappings = vec![mapping("claude-model", "catch-all")];

        assert_eq!(
            request_model("claude", "anthropic", &mappings, None),
            "claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn request_model_keeps_the_subagent_alias_as_a_probe_default() {
        // Unlike the catch-all, the subagent alias is routable and rewrites to a
        // real model.
        let mappings = vec![mapping("claude-subagent", "provider-haiku")];

        assert_eq!(
            request_model("claude", "anthropic", &mappings, None),
            "claude-subagent"
        );
    }

    #[test]
    fn gemini_path_model_skips_the_fallback_sentinel() {
        let mappings = vec![
            mapping("claude-model", "catch-all"),
            mapping("gemini-2.5-flash", "provider-flash"),
        ];

        assert_eq!(gemini_path_model(&mappings, None), "provider-flash");
    }

    #[test]
    fn placeholder_filter_keeps_the_route_sentinels() {
        // `is_placeholder_model` is duplicated in this file; if a refactor ever
        // teaches it about the catch-all or subagent alias, both features die
        // silently.
        let kept = remove_placeholder_model_mappings(vec![
            mapping("claude-model", "catch-all"),
            mapping("claude-subagent", "provider-haiku"),
            mapping("upstream-model", "dropped"),
        ]);

        assert_eq!(
            kept.iter().map(|m| m.from.as_str()).collect::<Vec<_>>(),
            vec!["claude-model", "claude-subagent"]
        );
    }
}
