use crate::error::AppError;
use crate::models::route_credential::{
    normalize_anthropic_api_key_field, ANTHROPIC_API_KEY_FIELD, ANTHROPIC_AUTH_TOKEN_FIELD,
};
use crate::models::route_pool::{FetchedRouteModel, RouteModelsFetchRequest};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::StatusCode;
use serde::Deserialize;
use std::time::Duration;

pub struct RouteModelFetchService;

const FETCH_TIMEOUT_SECS: u64 = 15;
const ERROR_BODY_MAX_CHARS: usize = 512;
const USER_AGENT_VALUE: &str = "ai-switch/0.1";

const KNOWN_COMPAT_SUFFIXES: &[&str] = &[
    "/api/claudecode",
    "/api/anthropic",
    "/apps/anthropic",
    "/api/coding",
    "/claudecode",
    "/anthropic",
    "/step_plan",
    "/coding",
    "/claude",
];

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Option<Vec<ModelEntry>>,
    models: Option<Vec<GeminiModelEntry>>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
    owned_by: Option<String>,
    #[serde(default)]
    supports_1m: Option<bool>,
    #[serde(default, rename = "supports1m")]
    supports_1m_camel: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GeminiModelEntry {
    name: String,
    #[serde(default, rename = "displayName")]
    display_name: Option<String>,
}

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
        let candidates = if interface_format == "gemini" {
            build_gemini_models_url_candidates(base_url)
        } else {
            build_models_url_candidates(base_url)
        }
        .map_err(|err| {
            validation_error(
                "validation.route_models_endpoint",
                "Could not build model list endpoint",
                Some(err),
            )
        })?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
            .build()
            .map_err(|err| {
                validation_error(
                    "validation.route_models_client",
                    "Could not initialize model list client",
                    Some(err.to_string()),
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
                let parsed = response.json::<ModelsResponse>().await.map_err(|err| {
                    validation_error(
                        "validation.route_models_parse",
                        "Could not parse model list response",
                        Some(err.to_string()),
                    )
                })?;
                let models = normalize_models_response(parsed);
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

    let mut candidates = Vec::new();
    if ends_with_version_segment(trimmed) {
        candidates.push(format!("{trimmed}/models"));
        if !trimmed.ends_with("/v1") {
            candidates.push(format!("{trimmed}/v1/models"));
        }
    } else {
        candidates.push(format!("{trimmed}/v1/models"));
    }

    if let Some(stripped) = strip_compat_suffix(trimmed) {
        let root = stripped.trim_end_matches('/');
        if !root.is_empty() && root.contains("://") {
            candidates.push(format!("{root}/v1/models"));
            candidates.push(format!("{root}/models"));
        }
    }

    Ok(deduplicate(candidates))
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
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    match interface_format {
        "anthropic" | "anthropic-messages" => {
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
        }
        "gemini" => {}
        _ => {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {api_key}"))
                    .map_err(|err| format!("Invalid authorization header: {err}"))?,
            );
        }
    }
    Ok(headers)
}

fn normalize_models_response(response: ModelsResponse) -> Vec<FetchedRouteModel> {
    let mut models: Vec<FetchedRouteModel> = response
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|entry| FetchedRouteModel {
            id: entry.id.trim().to_string(),
            owned_by: entry.owned_by,
            supports_1m: truthy_option(entry.supports_1m.or(entry.supports_1m_camel)),
        })
        .chain(
            response
                .models
                .unwrap_or_default()
                .into_iter()
                .map(|entry| {
                    let id = entry
                        .name
                        .trim()
                        .strip_prefix("models/")
                        .unwrap_or_else(|| entry.name.trim())
                        .to_string();
                    FetchedRouteModel {
                        id,
                        owned_by: entry.display_name,
                        supports_1m: None,
                    }
                }),
        )
        .filter(|model| !model.id.is_empty())
        .collect();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id.as_str() == right.id.as_str());
    models
}

fn truthy_option(value: Option<bool>) -> Option<bool> {
    value.filter(|enabled| *enabled)
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

fn strip_compat_suffix(base_url: &str) -> Option<&str> {
    for suffix in KNOWN_COMPAT_SUFFIXES {
        if base_url.ends_with(*suffix) {
            return Some(&base_url[..base_url.len() - suffix.len()]);
        }
    }
    None
}

fn ends_with_version_segment(url: &str) -> bool {
    let last = url.rsplit('/').next().unwrap_or("");
    last.strip_prefix('v').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
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
            vec!["https://api.example.com/v1/models"]
        );
    }

    #[test]
    fn builds_versioned_openai_candidates() {
        assert_eq!(
            build_models_url_candidates("https://open.bigmodel.cn/api/coding/paas/v4")
                .expect("candidates"),
            vec![
                "https://open.bigmodel.cn/api/coding/paas/v4/models",
                "https://open.bigmodel.cn/api/coding/paas/v4/v1/models",
            ]
        );
    }

    #[test]
    fn strips_anthropic_compat_suffix_candidates() {
        assert_eq!(
            build_models_url_candidates("https://api.z.ai/api/anthropic").expect("candidates"),
            vec![
                "https://api.z.ai/api/anthropic/v1/models",
                "https://api.z.ai/v1/models",
                "https://api.z.ai/models",
            ]
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
        let response: ModelsResponse = serde_json::from_value(serde_json::json!({
            "data": [
                {"id": "claude-sonnet-5", "owned_by": "gateway", "supports1m": true},
                {"id": "gpt-4o", "owned_by": "openai"}
            ],
            "models": [{"name": "models/gemini-2.5-flash", "displayName": "Gemini Flash"}]
        }))
        .expect("response");
        let models = normalize_models_response(response);

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
