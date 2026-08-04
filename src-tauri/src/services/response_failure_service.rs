use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticResponseFailure {
    pub message: String,
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
}
