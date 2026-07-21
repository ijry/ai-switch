use crate::error::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedOfficialCredential {
    pub display_name: String,
    pub email: Option<String>,
    pub secret_payload_json: String,
    pub config_json: String,
}

pub fn parse_cpa_text(
    platform: &str,
    text: &str,
) -> Result<Vec<ParsedOfficialCredential>, AppError> {
    let platform = normalize_platform(platform)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(validation_error(
            "validation.cpa_empty",
            "CPA import text is empty",
            None,
        ));
    }

    let value: Value = serde_json::from_str(trimmed)?;
    reject_wrapper_export(&value)?;

    match value {
        Value::Object(object) => {
            parse_cpa_object(&platform, &object).map(|credential| vec![credential])
        }
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let object = item.as_object().ok_or_else(|| {
                    validation_error(
                        "validation.cpa_entry_object",
                        "CPA array entries must be objects",
                        Some(format!("Entry {index} is not an object")),
                    )
                })?;
                parse_cpa_object(&platform, object)
            })
            .collect(),
        _ => Err(validation_error(
            "validation.cpa_shape",
            "CPA import JSON must be an object or array",
            Some(trimmed.to_string()),
        )),
    }
}

pub fn parse_cpa_file<P: AsRef<Path>>(
    platform: &str,
    path: P,
    content: &str,
) -> Result<Vec<ParsedOfficialCredential>, AppError> {
    parse_cpa_text(platform, content).map_err(|err| with_file_context(err, path.as_ref()))
}

fn parse_cpa_object(
    platform: &str,
    object: &Map<String, Value>,
) -> Result<ParsedOfficialCredential, AppError> {
    let raw_type = string_field(object, &["type"]);
    if let Some(raw_type) = raw_type.as_deref() {
        if !cpa_types_match(platform, raw_type) {
            return Err(validation_error(
                "validation.cpa_platform_mismatch",
                "CPA credential type does not match the selected platform",
                Some(format!("expected {platform}, got {raw_type}")),
            ));
        }
    }

    let email = string_field(object, &["email"]);
    let display_name = email
        .clone()
        .unwrap_or_else(|| "Official account".to_string());

    let id_token = token_field(object, "id_token", "idToken");
    let access_token = token_field(object, "access_token", "accessToken");
    let refresh_token = token_field(object, "refresh_token", "refreshToken");
    let account_id = token_field(object, "account_id", "accountId");

    if access_token.is_none() && refresh_token.is_none() {
        return Err(validation_error(
            "validation.cpa_secret_required",
            "CPA credential requires access_token or refresh_token",
            None,
        ));
    }

    let last_refresh = value_field(object, &["last_refresh", "lastRefresh"]);
    let expired = value_field(object, &["expired"]);

    let secret_payload_json = json!({
        "id_token": id_token,
        "access_token": access_token,
        "refresh_token": refresh_token,
        "account_id": account_id,
    })
    .to_string();

    let config_json = json!({
        "type": platform,
        "account_id": account_id,
        "last_refresh": last_refresh,
        "expired": expired,
        "raw_type": raw_type,
    })
    .to_string();

    Ok(ParsedOfficialCredential {
        display_name,
        email,
        secret_payload_json,
        config_json,
    })
}

fn reject_wrapper_export(value: &Value) -> Result<(), AppError> {
    let Some(accounts) = value
        .as_object()
        .and_then(|object| object.get("accounts"))
        .and_then(Value::as_array)
    else {
        return Ok(());
    };

    if accounts.iter().any(|account| {
        account
            .as_object()
            .is_some_and(|object| object.contains_key("credentials"))
    }) {
        return Err(validation_error(
            "validation.cpa_wrapper_unsupported",
            "CPA accounts wrapper export is not supported",
            None,
        ));
    }

    Ok(())
}

fn token_field(object: &Map<String, Value>, snake_case: &str, camel_case: &str) -> Option<String> {
    string_field(object, &[snake_case, camel_case]).or_else(|| {
        object
            .get("tokens")
            .and_then(Value::as_object)
            .and_then(|tokens| string_field(tokens, &[snake_case, camel_case]))
    })
}

fn string_field(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn value_field(object: &Map<String, Value>, keys: &[&str]) -> Value {
    keys.iter()
        .find_map(|key| object.get(*key).cloned())
        .unwrap_or(Value::Null)
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

    Ok(canonicalize_cpa_platform(platform))
}

fn canonicalize_cpa_platform(platform: &str) -> String {
    let normalized = platform.trim().to_lowercase();
    // CLIProxyAPI xAI exports may use type "xai" while the app platform id is "grok".
    if normalized.contains("grok")
        || normalized == "xai"
        || normalized.contains("x.ai")
        || normalized == "x-ai"
    {
        return "grok".to_string();
    }
    platform.trim().to_string()
}

fn cpa_types_match(expected_platform: &str, raw_type: &str) -> bool {
    canonicalize_cpa_platform(expected_platform) == canonicalize_cpa_platform(raw_type)
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
    fn parses_cliproxyapi_codex_object() {
        let text = r#"{
          "type":"codex",
          "email":"a@example.com",
          "id_token":"id",
          "access_token":"at",
          "refresh_token":"rt_1",
          "account_id":"ac_1"
        }"#;
        let parsed = parse_cpa_text("codex", text).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].email.as_deref(), Some("a@example.com"));
        assert!(parsed[0].secret_payload_json.contains("rt_1"));
    }

    #[test]
    fn parses_nested_and_camel_case_array() {
        let text = r#"[{
          "type":"claude",
          "email":"b@example.com",
          "tokens":{"accessToken":"sk-ant-oat01-x","refreshToken":"sk-ant-ort01-y"}
        }]"#;
        let parsed = parse_cpa_text("claude", text).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn rejects_accounts_wrapper_export() {
        let text = r#"{"accounts":[{"provider":"anthropic","email":"x@example.com","credentials":{"access_token":"a","refresh_token":"b"}}]}"#;
        let err = parse_cpa_text("claude", text).unwrap_err();
        assert!(
            format!("{err:?}").contains("cpa_wrapper_unsupported")
                || format!("{err}").contains("wrapper")
        );
    }

    #[test]
    fn rejects_platform_mismatch() {
        let text = r#"{"type":"claude","access_token":"a","refresh_token":"b"}"#;
        assert!(parse_cpa_text("codex", text).is_err());
    }

    #[test]
    fn accepts_xai_type_alias_for_grok_platform() {
        let text = r#"{
          "type":"xai",
          "email":"g@example.com",
          "access_token":"at-xai",
          "refresh_token":"rt-xai"
        }"#;
        let parsed = parse_cpa_text("grok", text).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].email.as_deref(), Some("g@example.com"));
        assert!(parsed[0].secret_payload_json.contains("at-xai"));
        assert!(parsed[0].config_json.contains("\"type\":\"grok\""));
    }

    #[test]
    fn accepts_grok_platform_when_import_target_is_xai_alias() {
        let text = r#"{"type":"grok","access_token":"at","refresh_token":"rt"}"#;
        let parsed = parse_cpa_text("xai", text).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].config_json.contains("\"type\":\"grok\""));
    }
}
