use crate::error::AppError;
use crate::models::platform::ApiDialect;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use url::Url;

pub fn canonical_json(value: &Value) -> Result<String, AppError> {
    Ok(serde_json::to_string(&sort_json(value))?)
}

pub fn canonical_fingerprint(
    kind: &str,
    projected_without_metadata: &Value,
) -> Result<String, AppError> {
    let material = match kind.trim().to_ascii_lowercase().as_str() {
        "api" => api_fingerprint_material(projected_without_metadata)?,
        "official" => official_fingerprint_material(projected_without_metadata)?,
        _ => {
            return Err(validation_error(
                "validation.transfer_fingerprint_kind",
                "Credential fingerprint kind is not supported",
                None,
            ));
        }
    };
    let canonical = canonical_json(&material)?;
    let digest = Sha256::digest(canonical.as_bytes());
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn api_fingerprint_material(value: &Value) -> Result<Value, AppError> {
    let input = object_required(value)?;
    let object = input
        .get("credential")
        .and_then(Value::as_object)
        .unwrap_or(input);
    let api_key = string_at(object, &["api-key"])
        .or_else(|| string_at(object, &["api_key"]))
        .or_else(|| string_at(object, &["api-key-entries", "0", "api-key"]))
        .or_else(|| string_at(object, &["api_key_entries", "0", "api-key"]))
        .or_else(|| string_at(object, &["api_key_entries", "0", "api_key"]))
        .ok_or_else(|| {
            validation_error(
                "validation.transfer_fingerprint_api_key",
                "API fingerprint requires an API key",
                None,
            )
        })?;
    let endpoint = string_at(object, &["base-url"])
        .or_else(|| string_at(object, &["base_url"]))
        .map(normalize_endpoint)
        .transpose()?
        .unwrap_or_default();
    let section = input
        .get("cpa_section")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            object
                .contains_key("api-key-entries")
                .then_some("openai-compatibility")
        })
        .or_else(|| {
            object
                .contains_key("api_key_entries")
                .then_some("openai-compatibility")
        })
        .ok_or_else(|| {
            validation_error(
                "validation.transfer_fingerprint_api_section",
                "API fingerprint requires an explicit CPA section",
                None,
            )
        })?;
    let dialect_input = input
        .get("interface_format")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_dialect_for_section(section));
    let dialect = ApiDialect::parse(dialect_input).map_err(|_| {
        validation_error(
            "validation.transfer_fingerprint_api_dialect",
            "API fingerprint dialect is not supported",
            None,
        )
    })?;
    if expected_dialect_for_section(section) != Some(dialect) {
        return Err(validation_error(
            "validation.transfer_fingerprint_api_dialect",
            "API fingerprint dialect does not match its CPA section",
            None,
        ));
    }
    let headers = object.get("headers").cloned().unwrap_or_else(|| json!({}));
    let headers = normalize_headers(&headers);
    let models = input
        .get("model_mappings")
        .cloned()
        .or_else(|| object.get("models").cloned())
        .unwrap_or_else(|| json!([]));
    let models = normalize_model_mappings(&models);
    let responses_custom_tool_compat = input
        .get("responses_custom_tool_compat")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let api_key_field = input
        .get("api_key_field")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default();

    Ok(json!({
        "kind": "api",
        "api_key": api_key,
        "endpoint": endpoint,
        "dialect": dialect.as_str(),
        "cpa_section": section,
        "headers": headers,
        "model_mappings": models,
        "responses_custom_tool_compat": responses_custom_tool_compat,
        "api_key_field": api_key_field,
    }))
}

fn official_fingerprint_material(value: &Value) -> Result<Value, AppError> {
    let object = object_required(value)?;
    let agent_identity = find_string(object, &["auth_mode", "authMode", "auth_kind", "authKind"])
        .is_some_and(|mode| normalize_auth_mode(mode) == "agentidentity")
        || find_string(object, &["agent_private_key", "agentPrivateKey"]).is_some();

    if agent_identity {
        return Ok(json!({
            "kind": "official-agent-identity",
            "agent_private_key": required_string(object, &["agent_private_key", "agentPrivateKey"], "agent_private_key")?,
            "agent_runtime_id": required_string(object, &["agent_runtime_id", "agentRuntimeId"], "agent_runtime_id")?,
            "task_id": required_string(object, &["task_id", "taskId"], "task_id")?,
            "account_id": find_string(object, &["account_id", "accountId", "chatgpt_account_id", "chatgptAccountId"]).unwrap_or_default(),
            "workspace_id": find_string(object, &["workspace_id", "workspaceId"]).unwrap_or_default(),
        }));
    }

    if let Some(refresh_token) = find_string(object, &["refresh_token", "refreshToken"]) {
        let auth_endpoint = find_string(
            object,
            &["token_endpoint", "tokenEndpoint", "base_url", "baseUrl"],
        )
        .map(normalize_endpoint)
        .transpose()?
        .unwrap_or_default();
        return Ok(json!({
            "kind": "official-refresh-token",
            "refresh_token": refresh_token,
            "account_id": find_string(object, &["account_id", "accountId", "chatgpt_account_id", "chatgptAccountId"]).unwrap_or_default(),
            "workspace_id": find_string(object, &["workspace_id", "workspaceId"]).unwrap_or_default(),
            "authentication_endpoint": auth_endpoint,
            "mode": find_string(object, &["auth_mode", "authMode", "auth_kind", "authKind"]).map(normalize_auth_mode).unwrap_or_default(),
        }));
    }

    let access_token = find_string(object, &["access_token", "accessToken"]).ok_or_else(|| {
        validation_error(
            "validation.transfer_fingerprint_access_token",
            "Official fingerprint requires an access or refresh token",
            None,
        )
    })?;
    Ok(json!({
        "kind": "official-access-token",
        "access_token": access_token,
    }))
}

fn normalize_headers(value: &Value) -> Value {
    let Some(headers) = value.as_object() else {
        return json!([]);
    };
    let mut normalized = headers
        .iter()
        .filter_map(|(name, value)| {
            let value = match value {
                Value::String(value) => value.trim().to_string(),
                Value::Null => return None,
                other => canonical_json(other).ok()?,
            };
            Some(json!({
                "name": name.trim().to_ascii_lowercase(),
                "value": value,
            }))
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| canonical_sort_key(left).cmp(&canonical_sort_key(right)));
    Value::Array(normalized)
}

fn normalize_model_mappings(value: &Value) -> Value {
    let Some(models) = value.as_array() else {
        return json!([]);
    };
    let mut normalized = models
        .iter()
        .filter_map(|model| {
            let model = model.as_object()?;
            let mut entry = Map::new();
            for aliases in [
                &["name", "to"][..],
                &["alias", "from"][..],
                &["display-name", "label"][..],
            ] {
                if let Some(value) = find_string(model, aliases) {
                    entry.insert(aliases[0].to_string(), json!(value));
                }
            }
            let max_context_length = model
                .get("max-context-length")
                .and_then(Value::as_u64)
                .or_else(|| {
                    (model.get("supports_1m").and_then(Value::as_bool) == Some(true))
                        .then_some(1_048_576)
                });
            if let Some(max_context_length) = max_context_length {
                entry.insert("max-context-length".to_string(), json!(max_context_length));
            }
            (!entry.is_empty()).then_some(Value::Object(entry))
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| canonical_sort_key(left).cmp(&canonical_sort_key(right)));
    Value::Array(normalized)
}

fn sort_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(Map::from_iter(
            object
                .iter()
                .map(|(key, value)| (key.clone(), sort_json(value)))
                .collect::<BTreeMap<_, _>>(),
        )),
        Value::Array(values) => Value::Array(values.iter().map(sort_json).collect()),
        other => other.clone(),
    }
}

fn normalize_endpoint(value: &str) -> Result<String, AppError> {
    let mut url = Url::parse(value.trim()).map_err(|_| {
        validation_error(
            "validation.transfer_fingerprint_endpoint",
            "Credential endpoint is not a valid URL",
            None,
        )
    })?;
    let scheme = url.scheme().to_ascii_lowercase();
    if scheme != url.scheme() {
        url.set_scheme(&scheme).map_err(|_| {
            validation_error(
                "validation.transfer_fingerprint_endpoint",
                "Credential endpoint scheme is invalid",
                None,
            )
        })?;
    }
    if let Some(host) = url.host_str().map(str::to_ascii_lowercase) {
        url.set_host(Some(&host)).map_err(|_| {
            validation_error(
                "validation.transfer_fingerprint_endpoint",
                "Credential endpoint host is invalid",
                None,
            )
        })?;
    }
    if matches!(
        (url.scheme(), url.port()),
        ("http", Some(80)) | ("https", Some(443))
    ) {
        url.set_port(None).map_err(|_| {
            validation_error(
                "validation.transfer_fingerprint_endpoint",
                "Credential endpoint port is invalid",
                None,
            )
        })?;
    }
    url.set_fragment(None);
    let trimmed_path = url.path().trim_end_matches('/').to_string();
    url.set_path(&trimmed_path);
    Ok(url.to_string())
}

fn default_dialect_for_section(section: &str) -> &'static str {
    expected_dialect_for_section(section)
        .map(ApiDialect::as_str)
        .unwrap_or("")
}

fn expected_dialect_for_section(section: &str) -> Option<ApiDialect> {
    match section {
        "claude-api-key" => Some(ApiDialect::Anthropic),
        "gemini-api-key" => Some(ApiDialect::Gemini),
        "codex-api-key" => Some(ApiDialect::OpenAiResponses),
        "xai-api-key" | "openai-compatibility" => Some(ApiDialect::OpenAi),
        _ => None,
    }
}

fn normalize_auth_mode(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn object_required(value: &Value) -> Result<&Map<String, Value>, AppError> {
    value.as_object().ok_or_else(|| {
        validation_error(
            "validation.transfer_fingerprint_object",
            "Credential fingerprint input must be an object",
            None,
        )
    })
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    keys: &[&str],
    field: &'static str,
) -> Result<&'a str, AppError> {
    find_string(object, keys).ok_or_else(|| {
        validation_error(
            "validation.transfer_fingerprint_field",
            "Credential fingerprint input is missing a required field",
            Some(field.to_string()),
        )
    })
}

fn find_string<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn string_at<'a>(object: &'a Map<String, Value>, path: &[&str]) -> Option<&'a str> {
    let (first, rest) = path.split_first()?;
    let mut current = object.get(*first)?;
    for key in rest {
        current = if let Ok(index) = key.parse::<usize>() {
            current.as_array()?.get(index)?
        } else {
            current.as_object()?.get(*key)?
        };
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn canonical_sort_key(value: &Value) -> String {
    canonical_json(value).unwrap_or_default()
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
    fn canonical_json_recursively_sorts_object_keys() {
        assert_eq!(
            canonical_json(&json!({"z": 1, "a": {"d": 4, "b": 2}})).unwrap(),
            r#"{"a":{"b":2,"d":4},"z":1}"#
        );
    }

    #[test]
    fn api_fingerprint_is_stable_across_endpoint_header_and_mapping_order() {
        let first = json!({
            "credential": {
                "api-key": "sk-test",
                "base-url": "HTTPS://API.EXAMPLE.COM:443/v1/",
                "headers": {"X-Z": "z", "x-a": "a"},
                "models": [
                    {"name": "provider-b", "alias": "b"},
                    {"name": "provider-a", "alias": "a", "max-context-length": 1048576}
                ]
            },
            "cpa_section": "codex-api-key",
            "interface_format": "openai-responses",
            "responses_custom_tool_compat": true
        });
        let second = json!({
            "credential": {
                "models": [
                    {"alias": "a", "supports_1m": true, "to": "provider-a"},
                    {"alias": "b", "to": "provider-b"}
                ],
                "headers": {"x-a": "a", "x-z": "z"},
                "base-url": "https://api.example.com/v1",
                "api-key": "sk-test"
            },
            "interface_format": "openai-responses",
            "cpa_section": "codex-api-key",
            "responses_custom_tool_compat": true
        });

        assert_eq!(
            canonical_fingerprint("api", &first).unwrap(),
            canonical_fingerprint("api", &second).unwrap()
        );
    }

    #[test]
    fn api_fingerprint_changes_for_compatibility_fields() {
        let base = json!({
            "credential": {
                "api-key": "sk-test",
                "base-url": "https://api.example.com/v1"
            },
            "cpa_section": "codex-api-key",
            "interface_format": "openai-responses",
            "responses_custom_tool_compat": false
        });
        let mut changed = base.clone();
        changed["responses_custom_tool_compat"] = json!(true);
        assert_ne!(
            canonical_fingerprint("api", &base).unwrap(),
            canonical_fingerprint("api", &changed).unwrap()
        );
    }

    #[test]
    fn endpoint_normalization_preserves_query_values_ending_in_slash() {
        let with_slash = json!({
            "credential": {
                "api-key": "sk-test",
                "base-url": "https://api.example.com/v1?prefix=/"
            },
            "cpa_section": "codex-api-key",
            "interface_format": "openai-responses"
        });
        let without_slash = json!({
            "credential": {
                "api-key": "sk-test",
                "base-url": "https://api.example.com/v1?prefix="
            },
            "cpa_section": "codex-api-key",
            "interface_format": "openai-responses"
        });

        assert_ne!(
            canonical_fingerprint("api", &with_slash).unwrap(),
            canonical_fingerprint("api", &without_slash).unwrap()
        );
    }

    #[test]
    fn api_dialect_canonical_value_has_stable_fingerprint() {
        let canonical = json!({
            "credential": {
                "api-key": "sk-test",
                "base-url": "https://api.example.com"
            },
            "cpa_section": "claude-api-key",
            "interface_format": "anthropic"
        });
        let same = json!({
            "credential": {
                "api-key": "sk-test",
                "base-url": "https://api.example.com"
            },
            "cpa_section": "claude-api-key",
            "interface_format": "anthropic"
        });

        assert_eq!(
            canonical_fingerprint("api", &canonical).unwrap(),
            canonical_fingerprint("api", &same).unwrap()
        );
    }

    #[test]
    fn api_dialect_must_match_the_cpa_section() {
        let contradictory = json!({
            "credential": {
                "api-key": "sk-test",
                "base-url": "https://api.example.com"
            },
            "cpa_section": "claude-api-key",
            "interface_format": "gemini"
        });

        let error = canonical_fingerprint("api", &contradictory).unwrap_err();
        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.transfer_fingerprint_api_dialect",
                details: None,
                ..
            }
        ));
    }

    #[test]
    fn api_fingerprint_safe_input_prevents_dialect_collisions_without_metadata() {
        let claude = json!({
            "credential": {
                "api-key": "same-key",
                "base-url": "https://api.example.com",
                "models": []
            },
            "cpa_section": "claude-api-key",
            "interface_format": "anthropic",
            "responses_custom_tool_compat": false,
            "model_mappings": []
        });
        let gemini = json!({
            "credential": {
                "api-key": "same-key",
                "base-url": "https://api.example.com",
                "models": []
            },
            "cpa_section": "gemini-api-key",
            "interface_format": "gemini",
            "responses_custom_tool_compat": false,
            "model_mappings": []
        });

        assert_ne!(
            canonical_fingerprint("api", &claude).unwrap(),
            canonical_fingerprint("api", &gemini).unwrap()
        );
    }

    #[test]
    fn api_fingerprint_safe_input_prevents_compatibility_flag_collisions_without_metadata() {
        let disabled = json!({
            "credential": {
                "api-key": "same-key",
                "base-url": "https://api.example.com",
                "models": []
            },
            "cpa_section": "codex-api-key",
            "interface_format": "openai-responses",
            "responses_custom_tool_compat": false,
            "model_mappings": []
        });
        let mut enabled = disabled.clone();
        enabled["responses_custom_tool_compat"] = json!(true);

        assert_ne!(
            canonical_fingerprint("api", &disabled).unwrap(),
            canonical_fingerprint("api", &enabled).unwrap()
        );
    }

    #[test]
    fn refresh_token_fingerprint_ignores_rotating_access_metadata() {
        let first = json!({
            "refresh_token": "refresh",
            "access_token": "access-old",
            "id_token": "id-old",
            "account_id": "account",
            "workspace_id": "workspace",
            "token_endpoint": "HTTPS://LOGIN.EXAMPLE.COM:443/oauth/",
            "auth_mode": "oauth",
            "expires_in": 3600,
            "last_refresh": "yesterday",
            "x-ai-switch": {"display_name": "Old name"}
        });
        let second = json!({
            "refresh_token": "refresh",
            "access_token": "access-new",
            "id_token": "id-new",
            "account_id": "account",
            "workspace_id": "workspace",
            "token_endpoint": "https://login.example.com/oauth",
            "auth_mode": "OAuth",
            "expires_in": 7200,
            "last_refresh": "today",
            "request_count": 1000,
            "cooldown_until": "tomorrow"
        });

        assert_eq!(
            canonical_fingerprint("official", &first).unwrap(),
            canonical_fingerprint("official", &second).unwrap()
        );
    }

    #[test]
    fn access_only_and_agent_identity_fingerprints_use_required_secrets() {
        let access_a = json!({"access_token": "access-a", "expired": false});
        let access_b = json!({"access_token": "access-b", "expired": false});
        assert_ne!(
            canonical_fingerprint("official", &access_a).unwrap(),
            canonical_fingerprint("official", &access_b).unwrap()
        );

        let agent = json!({
            "auth_mode": "agentIdentity",
            "agent_private_key": "private",
            "agent_runtime_id": "runtime",
            "task_id": "task",
            "workspace_id": "workspace"
        });
        let mut changed = agent.clone();
        changed["task_id"] = json!("other-task");
        assert_ne!(
            canonical_fingerprint("official", &agent).unwrap(),
            canonical_fingerprint("official", &changed).unwrap()
        );
    }

    #[test]
    fn fingerprint_errors_do_not_serialize_synthetic_secrets() {
        let secret = "synthetic-secret-value";
        let error = canonical_fingerprint(
            &format!("unknown-kind-{secret}"),
            &json!({"access_token": "safe-test-value", "base-url": "not a url"}),
        )
        .unwrap_err();
        let serialized = serde_json::to_string(&crate::error::ApiError::from(error)).unwrap();
        assert!(!serialized.contains(secret));
    }
}
