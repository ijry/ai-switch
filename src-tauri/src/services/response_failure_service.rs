use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticResponseFailure {
    pub message: String,
}

/// Returns whether a semantic response failure is a temporary upstream
/// capacity issue rather than an account-specific failure.
pub fn is_transient_response_failure(message: &str) -> bool {
    message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
        .contains("our servers are currently overloaded")
}

pub fn detect_response_failed(body: &[u8]) -> Option<SemanticResponseFailure> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(failure) = detect_value(&value) {
            return Some(failure);
        }
    }
    let text = std::str::from_utf8(body).ok()?;
    for line in text.lines() {
        let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            if let Some(failure) = detect_value(&value) {
                return Some(failure);
            }
        }
    }
    None
}

fn detect_value(value: &Value) -> Option<SemanticResponseFailure> {
    let failed = value.get("type").and_then(Value::as_str) == Some("response.failed")
        || value.pointer("/response/status").and_then(Value::as_str) == Some("failed")
        || value.pointer("/status").and_then(Value::as_str) == Some("failed");
    if !failed {
        return None;
    }
    let message = value
        .pointer("/response/error/message")
        .or_else(|| value.pointer("/error/message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .unwrap_or("Upstream response reported failure")
        .chars()
        .take(512)
        .collect();
    Some(SemanticResponseFailure { message })
}

/// Upstream `error.code` values that mean the account should be auto-paused
/// (暂停): the gateway hit a hard, time-windowed quota that won't recover by
/// retrying the same account this window, so we stop routing to it.
pub const AUTO_PAUSE_ERROR_CODES: &[&str] = &["global_fixed_window_quota_exhausted"];

/// Returns the matched upstream error code when the response body carries one of
/// [`AUTO_PAUSE_ERROR_CODES`], inspecting both plain JSON and SSE `data:` frames.
pub fn detect_auto_pause_code(body: &[u8]) -> Option<String> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(code) = detect_pause_code_value(&value) {
            return Some(code);
        }
    }
    let text = std::str::from_utf8(body).ok()?;
    for line in text.lines() {
        let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            if let Some(code) = detect_pause_code_value(&value) {
                return Some(code);
            }
        }
    }
    None
}

fn detect_pause_code_value(value: &Value) -> Option<String> {
    let code = value
        .pointer("/error/code")
        .or_else(|| value.pointer("/code"))
        .and_then(Value::as_str)?;
    if AUTO_PAUSE_ERROR_CODES.contains(&code) {
        Some(code.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_response_failed_json_and_nested_status() {
        let body = br#"{"type":"response.failed","response":{"status":"failed","error":{"message":"maintenance"}}}"#;
        assert_eq!(detect_response_failed(body).unwrap().message, "maintenance");
        assert!(detect_response_failed(br#"{"response":{"status":"failed"}}"#).is_some());
    }

    #[test]
    fn detects_sse_and_ignores_done_and_success() {
        let body = br#": keepalive
data: [DONE]
data: {"type":"response.failed","error":{"message":"down"}}
"#;
        assert_eq!(detect_response_failed(body).unwrap().message, "down");
        assert!(detect_response_failed(br#"{"type":"response.completed"}"#).is_none());
    }

    #[test]
    fn identifies_overloaded_server_response_as_transient() {
        assert!(is_transient_response_failure(
            "Our servers are currently overloaded. Please try again later."
        ));
        assert!(is_transient_response_failure(
            " our   servers are currently overloaded "
        ));
        assert!(!is_transient_response_failure("invalid model"));
    }

    #[test]
    fn detects_auto_pause_error_code_in_json() {
        let body = r#"{"error":{"message":"本时段全站额度已用完","type":"rate_limit_error","code":"global_fixed_window_quota_exhausted"}}"#;
        assert_eq!(
            detect_auto_pause_code(body.as_bytes()).as_deref(),
            Some("global_fixed_window_quota_exhausted")
        );
    }

    #[test]
    fn detects_auto_pause_error_code_in_sse_frames() {
        let body = br#": keepalive
data: {"error":{"code":"global_fixed_window_quota_exhausted"}}
"#;
        assert_eq!(
            detect_auto_pause_code(body).as_deref(),
            Some("global_fixed_window_quota_exhausted")
        );
    }

    #[test]
    fn ignores_unrelated_error_codes() {
        assert!(detect_auto_pause_code(br#"{"error":{"code":"rate_limit_error"}}"#).is_none());
        assert!(detect_auto_pause_code(br#"{"error":{"message":"boom"}}"#).is_none());
        assert!(detect_auto_pause_code(br#"{}"#).is_none());
    }
}
