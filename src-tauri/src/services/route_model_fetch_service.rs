use crate::error::AppError;
use crate::models::route_credential::{
    normalize_anthropic_api_key_field, ANTHROPIC_API_KEY_FIELD, ANTHROPIC_AUTH_TOKEN_FIELD,
};
use crate::models::route_pool::{FetchedRouteModel, RouteModelsFetchRequest};
use crate::services::client_identity;
use crate::services::http_client::build_outbound_http_client;
use crate::services::route_proxy_service::build_target_url;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;

pub struct RouteModelFetchService;

const FETCH_TIMEOUT_SECS: u64 = 15;
const ERROR_BODY_MAX_CHARS: usize = 512;
const USER_AGENT_VALUE: &str = "ai-switch/0.1";

impl RouteModelFetchService {
    pub async fn fetch(
        request: RouteModelsFetchRequest,
    ) -> Result<Vec<FetchedRouteModel>, AppError> {
        let base_url = request.base_url.trim();
        let api_key = request.api_key.trim();
        if base_url.is_empty() {
            return Err(validation_error(
                "validation.route_models_base_url_required",
                "Base URL is required to fetch models",
                None,
            ));
        }
        if api_key.is_empty() {
            return Err(validation_error(
                "validation.route_models_api_key_required",
                "API Key is required to fetch models",
                None,
            ));
        }

        let interface_format = request
            .interface_format
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("openai");
        let candidates =
            build_model_list_url_candidates(base_url, interface_format).map_err(|err| {
                validation_error(
                    "validation.route_models_endpoint",
                    "Could not build model list endpoint",
                    Some(err),
                )
            })?;

        let client = build_outbound_http_client(Some(Duration::from_secs(FETCH_TIMEOUT_SECS)))
            .map_err(|err| {
                validation_error(
                    "validation.route_models_client",
                    "Could not initialize model list client",
                    Some(err),
                )
            })?;

        let mut last_err: Option<String> = None;
        for raw_url in &candidates {
            let url = if interface_format == "gemini" {
                append_query_param(raw_url, "key", api_key)
            } else {
                raw_url.clone()
            };
            let mut headers =
                model_fetch_headers(api_key, interface_format, request.api_key_field.as_deref())
                    .map_err(|err| {
                        validation_error(
                            "validation.route_models_headers",
                            "Could not build model list request headers",
                            Some(err),
                        )
                    })?;
            if interface_format == "gemini" {
                headers.remove(AUTHORIZATION);
                headers.remove("x-api-key");
            }

            let response = match client.get(&url).headers(headers).send().await {
                Ok(response) => response,
                Err(err) => {
                    last_err = Some(format!("{raw_url}: {err}"));
                    continue;
                }
            };
            let status = response.status();
            if status.is_success() {
                let body = response.text().await.map_err(|err| {
                    validation_error(
                        "validation.route_models_parse",
                        "Could not read model list response",
                        Some(err.to_string()),
                    )
                })?;
                let parsed = serde_json::from_str::<Value>(&body).map_err(|err| {
                    validation_error(
                        "validation.route_models_parse",
                        "Could not parse model list response",
                        Some(format!("{err}; response: {}", truncate_body(body.clone()))),
                    )
                })?;
                let models = normalize_models_response(&parsed);
                return Ok(models);
            }

            let body = truncate_body(response.text().await.unwrap_or_default());
            let message = format!("{raw_url}: HTTP {status}: {body}");
            if status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED {
                last_err = Some(message);
                continue;
            }
            return Err(validation_error(
                "validation.route_models_http",
                "Model list request failed",
                Some(message),
            ));
        }

        Err(validation_error(
            "validation.route_models_all_failed",
            "All model list endpoints failed",
            last_err,
        ))
    }
}

pub fn build_models_url_candidates(base_url: &str) -> Result<Vec<String>, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Base URL is empty".to_string());
    }

    Ok(vec![format!("{trimmed}/models")])
}

fn build_model_list_url_candidates(
    base_url: &str,
    interface_format: &str,
) -> Result<Vec<String>, String> {
    match interface_format {
        "anthropic" => build_anthropic_models_url_candidates(base_url),
        "gemini" => build_gemini_models_url_candidates(base_url),
        _ => build_models_url_candidates(base_url),
    }
}

pub fn build_anthropic_models_url_candidates(base_url: &str) -> Result<Vec<String>, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Base URL is empty".to_string());
    }

    Ok(deduplicate(vec![
        build_target_url(trimmed, "/v1/models", None),
        build_target_url(trimmed, "/models", None),
    ]))
}

pub fn build_gemini_models_url_candidates(base_url: &str) -> Result<Vec<String>, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Base URL is empty".to_string());
    }

    let mut candidates = Vec::new();
    if trimmed.ends_with("/v1beta") || trimmed.ends_with("/v1") {
        candidates.push(format!("{trimmed}/models"));
    } else {
        candidates.push(format!("{trimmed}/v1beta/models"));
        candidates.push(format!("{trimmed}/v1/models"));
    }

    Ok(deduplicate(candidates))
}

fn model_fetch_headers(
    api_key: &str,
    interface_format: &str,
    api_key_field: Option<&str>,
) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    // The fetch client has no decompression support; require an uncompressed body.
    headers.insert("accept-encoding", HeaderValue::from_static("identity"));
    match interface_format {
        "anthropic" => {
            match normalize_anthropic_api_key_field(api_key_field)? {
                ANTHROPIC_AUTH_TOKEN_FIELD => {
                    headers.insert(
                        AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {api_key}"))
                            .map_err(|err| format!("Invalid authorization header: {err}"))?,
                    );
                }
                ANTHROPIC_API_KEY_FIELD => {
                    headers.insert(
                        "x-api-key",
                        HeaderValue::from_str(api_key)
                            .map_err(|err| format!("Invalid x-api-key header: {err}"))?,
                    );
                }
                _ => unreachable!("normalize_anthropic_api_key_field returns known constants"),
            }
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
            // Impersonate Claude Code so client-fingerprinting gateways
            // (e.g. agentrouter.org) don't reject the model list request.
            headers.insert(
                "anthropic-beta",
                HeaderValue::from_static(client_identity::CLAUDE_CODE_DEFAULT_BETA),
            );
            for (name, value) in client_identity::claude_code_identity_headers() {
                insert_static_header(&mut headers, name, value)?;
            }
        }
        "gemini" => {
            headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        }
        _ => {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {api_key}"))
                    .map_err(|err| format!("Invalid authorization header: {err}"))?,
            );
            // Impersonate the Codex CLI for OpenAI/Responses-style gateways.
            for (name, value) in client_identity::codex_cli_identity_headers() {
                insert_static_header(&mut headers, name, &value)?;
            }
        }
    }
    Ok(headers)
}

fn insert_static_header(
    headers: &mut HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), String> {
    let value = HeaderValue::from_str(value)
        .map_err(|err| format!("Invalid header value for {name}: {err}"))?;
    headers.insert(HeaderName::from_static(name), value);
    Ok(())
}

fn normalize_models_response(response: &Value) -> Vec<FetchedRouteModel> {
    let mut models = Vec::new();
    append_model_entries(response, &mut models);
    models.sort_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id.as_str() == right.id.as_str());
    models
}

fn append_model_entries(value: &Value, models: &mut Vec<FetchedRouteModel>) {
    match value {
        Value::Array(entries) => {
            for entry in entries {
                append_model_entries(entry, models);
            }
        }
        Value::Object(object) => {
            let mut found_nested_entries = false;
            for key in ["data", "models", "items"] {
                if let Some(entries) = object.get(key) {
                    found_nested_entries = true;
                    append_model_entries(entries, models);
                }
            }
            if found_nested_entries {
                return;
            }

            let Some(id) = ["id", "name", "model", "slug"]
                .iter()
                .find_map(|key| object.get(*key).and_then(Value::as_str))
                .map(normalize_model_id)
                .filter(|id| !id.is_empty())
            else {
                return;
            };
            let owned_by = [
                "owned_by",
                "ownedBy",
                "provider",
                "display_name",
                "displayName",
            ]
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_str))
            .map(str::to_string);
            let supports_1m = ["supports_1m", "supports1m"]
                .iter()
                .find_map(|key| object.get(*key).and_then(Value::as_bool))
                .filter(|enabled| *enabled);
            models.push(FetchedRouteModel {
                id,
                owned_by,
                supports_1m,
            });
        }
        Value::String(id) => {
            let id = normalize_model_id(id);
            if !id.is_empty() {
                models.push(FetchedRouteModel {
                    id,
                    owned_by: None,
                    supports_1m: None,
                });
            }
        }
        _ => {}
    }
}

fn normalize_model_id(value: &str) -> String {
    value
        .trim()
        .strip_prefix("models/")
        .unwrap_or_else(|| value.trim())
        .to_string()
}

fn append_query_param(url: &str, key: &str, value: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!(
        "{url}{separator}{key}={}",
        percent_encode_query_value(value)
    )
}

fn percent_encode_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn truncate_body(body: String) -> String {
    if body.chars().count() <= ERROR_BODY_MAX_CHARS {
        body
    } else {
        let mut truncated: String = body.chars().take(ERROR_BODY_MAX_CHARS).collect();
        truncated.push_str("...");
        truncated
    }
}

fn deduplicate(candidates: Vec<String>) -> Vec<String> {
    let mut unique = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if !unique.iter().any(|existing| existing == &candidate) {
            unique.push(candidate);
        }
    }
    unique
}

fn validation_error(code: &'static str, message: &str, details: Option<String>) -> AppError {
    AppError::Validation {
        code,
        message: message.to_string(),
        details,
        recoverable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_plain_openai_candidates() {
        assert_eq!(
            build_models_url_candidates("https://api.example.com").expect("candidates"),
            vec!["https://api.example.com/models"]
        );
    }

    #[test]
    fn appends_models_to_openai_base_url_path() {
        assert_eq!(
            build_models_url_candidates("https://new.sharedchat.cc/codex").expect("candidates"),
            vec!["https://new.sharedchat.cc/codex/models"]
        );
    }

    #[test]
    fn builds_model_list_candidates_from_interface_format() {
        assert_eq!(
            build_model_list_url_candidates("https://api.anthropic.com", "anthropic")
                .expect("candidates"),
            vec![
                "https://api.anthropic.com/v1/models",
                "https://api.anthropic.com/models",
            ]
        );
        assert_eq!(
            build_model_list_url_candidates("https://api.anthropic.com/v1", "anthropic")
                .expect("candidates"),
            vec!["https://api.anthropic.com/v1/models"]
        );
        assert_eq!(
            build_model_list_url_candidates("https://api.example.com/v1", "openai-responses")
                .expect("candidates"),
            vec!["https://api.example.com/v1/models"]
        );
    }

    #[test]
    fn builds_versioned_openai_candidates() {
        assert_eq!(
            build_models_url_candidates("https://open.bigmodel.cn/api/coding/paas/v4")
                .expect("candidates"),
            vec!["https://open.bigmodel.cn/api/coding/paas/v4/models"]
        );
    }

    #[test]
    fn keeps_compat_suffix_as_part_of_base_url() {
        assert_eq!(
            build_models_url_candidates("https://api.z.ai/api/anthropic").expect("candidates"),
            vec!["https://api.z.ai/api/anthropic/models"]
        );
    }

    #[test]
    fn builds_gemini_candidates() {
        assert_eq!(
            build_gemini_models_url_candidates("https://generativelanguage.googleapis.com")
                .expect("candidates"),
            vec![
                "https://generativelanguage.googleapis.com/v1beta/models",
                "https://generativelanguage.googleapis.com/v1/models",
            ]
        );
    }

    #[test]
    fn normalizes_openai_and_gemini_responses() {
        let response = serde_json::json!({
            "data": [
                {"id": "claude-sonnet-5", "owned_by": "gateway", "supports1m": true},
                {"id": "gpt-4o", "owned_by": "openai"}
            ],
            "models": [{"name": "models/gemini-2.5-flash", "displayName": "Gemini Flash"}]
        });
        let models = normalize_models_response(&response);

        assert_eq!(
            models,
            vec![
                FetchedRouteModel {
                    id: "claude-sonnet-5".to_string(),
                    owned_by: Some("gateway".to_string()),
                    supports_1m: Some(true),
                },
                FetchedRouteModel {
                    id: "gemini-2.5-flash".to_string(),
                    owned_by: Some("Gemini Flash".to_string()),
                    supports_1m: None,
                },
                FetchedRouteModel {
                    id: "gpt-4o".to_string(),
                    owned_by: Some("openai".to_string()),
                    supports_1m: None,
                },
            ]
        );
    }

    #[test]
    fn normalizes_anthropic_models_response() {
        let response = serde_json::json!({
            "data": [
                {
                    "type": "model",
                    "id": "claude-sonnet-4-20250514",
                    "display_name": "Claude Sonnet 4",
                    "created_at": "2025-05-14T00:00:00Z"
                }
            ],
            "has_more": false,
            "first_id": "claude-sonnet-4-20250514",
            "last_id": "claude-sonnet-4-20250514"
        });
        let models = normalize_models_response(&response);

        assert_eq!(
            models,
            vec![FetchedRouteModel {
                id: "claude-sonnet-4-20250514".to_string(),
                owned_by: Some("Claude Sonnet 4".to_string()),
                supports_1m: None,
            }]
        );
    }

    #[test]
    fn builds_anthropic_headers_by_api_key_field() {
        let default_headers = model_fetch_headers("sk-test", "anthropic", None).expect("headers");
        assert_eq!(
            default_headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("sk-test")
        );
        assert!(default_headers.get(AUTHORIZATION).is_none());

        let bearer_headers =
            model_fetch_headers("sk-test", "anthropic", Some("ANTHROPIC_AUTH_TOKEN"))
                .expect("headers");
        assert_eq!(
            bearer_headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer sk-test")
        );
        assert!(bearer_headers.get("x-api-key").is_none());
    }
}
