use crate::error::AppError;
use crate::services::cpa_import_service::ParsedOfficialCredential;
use serde_json::{json, Map, Value};
use std::path::Path;

const SHAPE_ERROR_CODE: &str = "validation.sub2api_shape";

pub fn parse_sub2api_text(
    platform: &str,
    text: &str,
) -> Result<Vec<ParsedOfficialCredential>, AppError> {
    let platform = normalize_platform(platform)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(shape_error("Sub2API import text is empty", None));
    }

    let value: Value = serde_json::from_str(trimmed)?;

    match value {
        Value::Object(object) => {
            if let Some(accounts) = object.get("accounts") {
                parse_sub2api_accounts_wrapper(&platform, accounts)
            } else if is_sub2api_object(&object) {
                parse_sub2api_object(&platform, &object).map(|credential| vec![credential])
            } else {
                Err(shape_error(
                    "Sub2API import JSON does not look like a sub2api credential",
                    Some(trimmed.to_string()),
                ))
            }
        }
        Value::Array(items) => {
            if !items.iter().any(is_sub2api_value) {
                return Err(shape_error(
                    "Sub2API import array does not contain sub2api credentials",
                    Some(trimmed.to_string()),
                ));
            }

            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let object = item.as_object().ok_or_else(|| {
                        validation_error(
                            "validation.sub2api_entry_object",
                            "Sub2API array entries must be objects",
                            Some(format!("Entry {index} is not an object")),
                        )
                    })?;
                    parse_sub2api_object(&platform, object)
                })
                .collect()
        }
        _ => Err(shape_error(
            "Sub2API import JSON must be an object or array",
            Some(trimmed.to_string()),
        )),
    }
}

pub fn parse_sub2api_file<P: AsRef<Path>>(
    platform: &str,
    path: P,
    content: &str,
) -> Result<Vec<ParsedOfficialCredential>, AppError> {
    parse_sub2api_text(platform, content).map_err(|err| with_file_context(err, path.as_ref()))
}

pub fn is_sub2api_shape_error(err: &AppError) -> bool {
    matches!(err, AppError::Validation { code, .. } if *code == SHAPE_ERROR_CODE)
}

fn parse_sub2api_accounts_wrapper(
    platform: &str,
    accounts: &Value,
) -> Result<Vec<ParsedOfficialCredential>, AppError> {
    let accounts = accounts.as_array().ok_or_else(|| {
        validation_error(
            "validation.sub2api_accounts_array",
            "Sub2API accounts wrapper must contain an accounts array",
            None,
        )
    })?;

    if !accounts.iter().any(is_sub2api_value) {
        return Err(shape_error(
            "Sub2API accounts wrapper does not contain sub2api credentials",
            None,
        ));
    }

    accounts
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let object = item.as_object().ok_or_else(|| {
                validation_error(
                    "validation.sub2api_entry_object",
                    "Sub2API accounts wrapper entries must be objects",
                    Some(format!("Entry {index} is not an object")),
                )
            })?;
            parse_sub2api_object(platform, object)
        })
        .collect()
}

fn parse_sub2api_object(
    platform: &str,
    object: &Map<String, Value>,
) -> Result<ParsedOfficialCredential, AppError> {
    let selected_platform = normalize_platform(platform)?;
    let declared_platform = declared_platform(object);
    let inferred_platform = declared_platform.clone().or_else(|| infer_platform(object));

    if let Some(raw_platform) = inferred_platform.as_deref() {
        if !platforms_match(&selected_platform, raw_platform) {
            return Err(validation_error(
                "validation.sub2api_platform_mismatch",
                "Sub2API credential platform does not match the selected platform",
                Some(format!("expected {selected_platform}, got {raw_platform}")),
            ));
        }
    }

    let email = first_string_path(
        object,
        &[
            &["credentials", "email"],
            &["email"],
            &["extra", "email"],
            &["credentials", "extra", "email"],
            &["credentials", "live_identity", "email"],
        ],
    );
    let display_name = first_string_path(
        object,
        &[
            &["display_name"],
            &["displayName"],
            &["name"],
            &["label"],
            &["credentials", "name"],
            &["credentials", "display_name"],
            &["credentials", "displayName"],
            &["credentials", "email"],
            &["email"],
        ],
    )
    .or_else(|| email.clone())
    .unwrap_or_else(|| "Official account".to_string());

    let auth_kind = first_string_path(
        object,
        &[
            &["credentials", "auth_kind"],
            &["credentials", "authKind"],
            &["credentials", "type"],
            &["auth_kind"],
            &["authKind"],
            &["auth_mode"],
            &["authMode"],
            &["type"],
        ],
    );
    let auth_mode = first_string_path(
        object,
        &[
            &["credentials", "auth_mode"],
            &["credentials", "authMode"],
            &["auth_mode"],
            &["authMode"],
        ],
    );
    let id_token = token_field(object, "id_token", "idToken");
    let access_token = token_field(object, "access_token", "accessToken");
    let refresh_token = token_field(object, "refresh_token", "refreshToken");
    let account_id = token_field(object, "account_id", "accountId");
    let workspace_id = token_field(object, "workspace_id", "workspaceId");
    let chatgpt_account_id = token_field(object, "chatgpt_account_id", "chatgptAccountId");
    let task_id = token_field(object, "task_id", "taskId");
    let agent_private_key = token_field(object, "agent_private_key", "agentPrivateKey");
    let agent_runtime_id = token_field(object, "agent_runtime_id", "agentRuntimeId");

    if access_token.is_none()
        && refresh_token.is_none()
        && id_token.is_none()
        && agent_private_key.is_none()
    {
        return Err(validation_error(
            "validation.sub2api_secret_required",
            "Sub2API credential requires access_token, refresh_token, id_token, or agent_private_key",
            None,
        ));
    }

    let last_refresh = first_value_path(
        object,
        &[
            &["credentials", "last_refresh"],
            &["credentials", "lastRefresh"],
            &["extra", "last_refresh"],
            &["extra", "lastRefresh"],
            &["last_refresh"],
            &["lastRefresh"],
        ],
    );
    let expired = first_value_path(object, &[&["credentials", "expired"], &["expired"]]);
    let expires_in = first_value_path(
        object,
        &[
            &["credentials", "expires_in"],
            &["credentials", "expiresIn"],
            &["expires_in"],
            &["expiresIn"],
        ],
    );
    let disabled = first_value_path(object, &[&["disabled"], &["credentials", "disabled"]]);
    let token_type = first_string_path(
        object,
        &[
            &["credentials", "token_type"],
            &["credentials", "tokenType"],
            &["token_type"],
            &["tokenType"],
        ],
    );
    let redirect_uri = first_string_path(
        object,
        &[
            &["credentials", "redirect_uri"],
            &["credentials", "redirectUri"],
            &["redirect_uri"],
            &["redirectUri"],
        ],
    );
    let base_url = first_string_path(
        object,
        &[
            &["credentials", "base_url"],
            &["credentials", "baseUrl"],
            &["base_url"],
            &["baseUrl"],
        ],
    );
    let token_endpoint = first_string_path(
        object,
        &[
            &["credentials", "token_endpoint"],
            &["credentials", "tokenEndpoint"],
            &["token_endpoint"],
            &["tokenEndpoint"],
        ],
    );
    let subscription_type = first_string_path(
        object,
        &[
            &["credentials", "plan_type"],
            &["credentials", "chatgpt_plan_type"],
            &["credentials", "live_identity", "plan"],
            &["credentials", "live_identity", "official_plan"],
            &["plan_type"],
            &["chatgpt_plan_type"],
        ],
    );
    let chatgpt_account_is_fedramp = first_value_path(
        object,
        &[
            &["credentials", "chatgpt_account_is_fedramp"],
            &["credentials", "chatgptAccountIsFedramp"],
            &["credentials", "live_identity", "chatgpt_account_is_fedramp"],
            &["credentials", "live_identity", "chatgptAccountIsFedramp"],
            &["chatgpt_account_is_fedramp"],
            &["chatgptAccountIsFedramp"],
        ],
    );
    let headers = first_value_path(object, &[&["credentials", "headers"], &["headers"]])
        .filter(Value::is_object)
        .unwrap_or(Value::Null);
    let headers = normalize_imported_headers(&selected_platform, &headers);
    let raw = Value::Object(object.clone());

    let secret_payload_json = json!({
        "id_token": id_token.clone(),
        "access_token": access_token.clone(),
        "refresh_token": refresh_token.clone(),
        "account_id": account_id.clone(),
        "workspace_id": workspace_id.clone(),
        "chatgpt_account_id": chatgpt_account_id.clone(),
        "task_id": task_id.clone(),
        "agent_private_key": agent_private_key.clone(),
        "agent_runtime_id": agent_runtime_id.clone(),
    })
    .to_string();

    let mut config = Map::new();
    config.insert("type".to_string(), json!(selected_platform.clone()));
    config.insert("import_format".to_string(), json!("sub2api"));
    config.insert("raw_type".to_string(), json!(auth_kind.clone()));
    config.insert("account_id".to_string(), json!(account_id.clone()));
    config.insert("workspace_id".to_string(), json!(workspace_id.clone()));
    config.insert(
        "chatgpt_account_id".to_string(),
        json!(chatgpt_account_id.clone()),
    );
    config.insert(
        "last_refresh".to_string(),
        last_refresh.unwrap_or(Value::Null),
    );
    config.insert("expired".to_string(), expired.unwrap_or(Value::Null));
    config.insert("expires_in".to_string(), expires_in.unwrap_or(Value::Null));
    config.insert("disabled".to_string(), disabled.unwrap_or(Value::Null));
    config.insert("raw".to_string(), raw);

    insert_optional_string(&mut config, "auth_kind", auth_kind);
    insert_optional_string(&mut config, "auth_mode", auth_mode);
    insert_optional_string(&mut config, "token_type", token_type);
    insert_optional_string(&mut config, "redirect_uri", redirect_uri);
    insert_optional_string(&mut config, "base_url", base_url);
    insert_optional_string(&mut config, "token_endpoint", token_endpoint);
    insert_optional_string(&mut config, "subscription_type", subscription_type);
    insert_optional_string(
        &mut config,
        "email_source",
        first_string_path(
            object,
            &[
                &["credentials", "email_source"],
                &["credentials", "live_identity", "email_source"],
                &["email_source"],
            ],
        ),
    );
    insert_optional_string(
        &mut config,
        "identity_source",
        first_string_path(
            object,
            &[
                &["credentials", "identity_source"],
                &["credentials", "live_identity", "identity_source"],
                &["identity_source"],
            ],
        ),
    );
    insert_optional_value(
        &mut config,
        "priority",
        first_value_path(object, &[&["priority"]]),
    );
    insert_optional_value(
        &mut config,
        "concurrency",
        first_value_path(object, &[&["concurrency"]]),
    );
    insert_optional_value(
        &mut config,
        "rate_multiplier",
        first_value_path(object, &[&["rate_multiplier"], &["rateMultiplier"]]),
    );
    insert_optional_value(
        &mut config,
        "auto_pause_on_expired",
        first_value_path(
            object,
            &[&["auto_pause_on_expired"], &["autoPauseOnExpired"]],
        ),
    );
    insert_optional_value(
        &mut config,
        "chatgpt_account_is_fedramp",
        chatgpt_account_is_fedramp,
    );
    if selected_platform == "grok" || !headers.is_null() {
        config.insert("headers".to_string(), headers);
    }

    Ok(ParsedOfficialCredential {
        display_name,
        email,
        secret_payload_json,
        config_json: Value::Object(config).to_string(),
    })
}

fn is_sub2api_value(value: &Value) -> bool {
    value.as_object().is_some_and(is_sub2api_object)
}

fn is_sub2api_object(object: &Map<String, Value>) -> bool {
    object.contains_key("credentials")
        || object.contains_key("tokens")
        || object.contains_key("platform")
        || object.contains_key("provider")
        || object.contains_key("priority")
        || object.contains_key("concurrency")
        || object.contains_key("rate_multiplier")
        || object.contains_key("auto_pause_on_expired")
}

fn declared_platform(object: &Map<String, Value>) -> Option<String> {
    first_string_path(
        object,
        &[
            &["platform"],
            &["provider"],
            &["app"],
            &["credentials", "platform"],
            &["credentials", "provider"],
            &["credentials", "app"],
        ],
    )
    .or_else(|| {
        let raw_type = first_string_path(object, &[&["type"], &["credentials", "type"]])?;
        let canonical = canonicalize_platform(&raw_type);
        if canonical == "oauth" || canonical == "api" || canonical == "official" {
            None
        } else {
            Some(raw_type)
        }
    })
}

fn infer_platform(object: &Map<String, Value>) -> Option<String> {
    let access_token = token_field(object, "access_token", "accessToken");
    let refresh_token = token_field(object, "refresh_token", "refreshToken");
    let token = refresh_token.or(access_token)?;
    if token.starts_with("sk-ant-o") {
        Some("claude".to_string())
    } else {
        None
    }
}

fn token_field(object: &Map<String, Value>, snake_case: &str, camel_case: &str) -> Option<String> {
    first_string_path(
        object,
        &[
            &["credentials", snake_case],
            &["credentials", camel_case],
            &["credentials", "tokens", snake_case],
            &["credentials", "tokens", camel_case],
            &["tokens", snake_case],
            &["tokens", camel_case],
            &[snake_case],
            &[camel_case],
        ],
    )
}

fn first_string_path(object: &Map<String, Value>, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        value_at_path(object, path)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn first_value_path(object: &Map<String, Value>, paths: &[&[&str]]) -> Option<Value> {
    paths
        .iter()
        .find_map(|path| value_at_path(object, path).cloned())
}

fn value_at_path<'a>(object: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    let mut current = object.get(*first)?;
    for key in rest {
        current = current.as_object()?.get(*key)?;
    }
    Some(current)
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), json!(value));
    }
}

fn insert_optional_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_string(), value);
    }
}

fn normalize_imported_headers(platform: &str, headers: &Value) -> Value {
    if platform != "grok" {
        return headers.clone();
    }

    let mut map = match headers {
        Value::Object(object) => object.clone(),
        _ => Map::new(),
    };
    map.insert("User-Agent".to_string(), json!("xai-grok-workspace/0.2.93"));
    map.insert("X-XAI-Token-Auth".to_string(), json!("xai-grok-cli"));
    map.insert("x-grok-client-version".to_string(), json!("0.2.93"));
    map.remove("X-Client-Name");
    map.remove("x-client-name");
    Value::Object(map)
}

fn normalize_platform(platform: &str) -> Result<String, AppError> {
    let platform = platform.trim();
    if platform.is_empty() {
        return Err(validation_error(
            "validation.platform_required",
            "Platform is required",
            None,
        ));
    }

    Ok(canonicalize_platform(platform))
}

fn canonicalize_platform(platform: &str) -> String {
    let normalized = platform.trim().to_lowercase();
    match normalized.as_str() {
        "anthropic" | "claude" => "claude".to_string(),
        "openai" | "chatgpt" | "codex" => "codex".to_string(),
        "gemini" | "google" => "gemini".to_string(),
        "xai" | "x-ai" => "grok".to_string(),
        _ if normalized.contains("grok") || normalized.contains("x.ai") => "grok".to_string(),
        _ => normalized,
    }
}

fn platforms_match(expected_platform: &str, raw_platform: &str) -> bool {
    canonicalize_platform(expected_platform) == canonicalize_platform(raw_platform)
}

fn shape_error(message: &str, details: Option<String>) -> AppError {
    validation_error(SHAPE_ERROR_CODE, message, details)
}

fn validation_error(code: &'static str, message: &str, details: Option<String>) -> AppError {
    AppError::Validation {
        code,
        message: message.to_string(),
        details,
        recoverable: true,
    }
}

fn with_file_context(err: AppError, path: &Path) -> AppError {
    let label = path.to_string_lossy();
    match err {
        AppError::Validation {
            code,
            message,
            details,
            recoverable,
        } => AppError::Validation {
            code,
            message: format!("{label}: {message}"),
            details,
            recoverable,
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sub2api_tokens_array() {
        let text = r#"[{
          "email": "a@example.com",
          "tokens": {
            "access_token": "sk-ant-oat01-aaaa",
            "refresh_token": "sk-ant-ort01-bbbb"
          }
        }]"#;

        let parsed = parse_sub2api_text("claude", text).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].email.as_deref(), Some("a@example.com"));
        assert!(parsed[0].secret_payload_json.contains("sk-ant-oat01-aaaa"));
        assert!(parsed[0].config_json.contains("\"type\":\"claude\""));
    }

    #[test]
    fn parses_k12_sub2api_codex_object() {
        let text = r#"{
          "name": "tallisbisaccia737@hotmail.com",
          "type": "oauth",
          "platform": "openai",
          "priority": 1,
          "concurrency": 100,
          "credentials": {
            "type": "oauth",
            "email": "tallisbisaccia737@hotmail.com",
            "task_id": "task-yyuYdYo1cTSEInmqPKxoATlQ",
            "id_token": "id-token",
            "auth_mode": "agentIdentity",
            "plan_type": "k12",
            "account_id": "7fbe4da7-1fab-4f6d-8210-a1eba367f805",
            "workspace_id": "7fbe4da7-1fab-4f6d-8210-a1eba367f805",
            "agent_private_key": "private-key",
            "chatgpt_plan_type": "k12",
            "chatgpt_account_id": "7fbe4da7-1fab-4f6d-8210-a1eba367f805",
            "live_identity": {
              "plan": "k12",
              "official_plan": "k12",
              "email_source": "agent_identity_credentials"
            }
          },
          "rate_multiplier": 1,
          "auto_pause_on_expired": true
        }"#;

        let parsed = parse_sub2api_text("codex", text).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].email.as_deref(),
            Some("tallisbisaccia737@hotmail.com")
        );
        assert!(parsed[0].secret_payload_json.contains("id-token"));
        assert!(parsed[0].secret_payload_json.contains("private-key"));
        assert!(parsed[0]
            .secret_payload_json
            .contains("task-yyuYdYo1cTSEInmqPKxoATlQ"));
        assert!(parsed[0].config_json.contains("\"type\":\"codex\""));
        assert!(parsed[0].config_json.contains("\"raw_type\":\"oauth\""));
        assert!(parsed[0]
            .config_json
            .contains("\"auth_mode\":\"agentIdentity\""));
        assert!(parsed[0]
            .config_json
            .contains("\"subscription_type\":\"k12\""));
        assert!(parsed[0].config_json.contains("\"priority\":1"));
        assert!(parsed[0].config_json.contains("\"concurrency\":100"));
        assert!(parsed[0]
            .config_json
            .contains("\"auto_pause_on_expired\":true"));
        assert!(parsed[0].config_json.contains("\"raw\""));
    }

    #[test]
    fn parses_sub2api_accounts_wrapper() {
        let text = r#"{
          "accounts": [{
            "platform": "openai",
            "name": "codex-user",
            "credentials": {
              "email": "codex@example.com",
              "access_token": "at",
              "refresh_token": "rt"
            }
          }]
        }"#;

        let parsed = parse_sub2api_text("codex", text).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].display_name, "codex-user");
        assert_eq!(parsed[0].email.as_deref(), Some("codex@example.com"));
        assert!(parsed[0]
            .secret_payload_json
            .contains("\"access_token\":\"at\""));
    }

    #[test]
    fn rejects_sub2api_platform_mismatch() {
        let text = r#"{
          "platform": "openai",
          "credentials": {
            "access_token": "at",
            "refresh_token": "rt"
          }
        }"#;

        let error = parse_sub2api_text("claude", text).unwrap_err();

        match error {
            AppError::Validation { code, message, .. } => {
                assert_eq!(code, "validation.sub2api_platform_mismatch");
                assert!(message.contains("platform"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn reports_shape_error_for_plain_cpa_object() {
        let text = r#"{"type":"codex","access_token":"at","refresh_token":"rt"}"#;
        let error = parse_sub2api_text("codex", text).unwrap_err();
        assert!(is_sub2api_shape_error(&error));
    }
}
