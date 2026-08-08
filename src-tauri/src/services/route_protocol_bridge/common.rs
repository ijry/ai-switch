use serde_json::Value;

pub(super) fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

pub(super) fn is_create_path(path: &str, expected: &str) -> bool {
    let normalized = normalize_path(path);
    let mut remaining = normalized.trim_start_matches('/');
    while let Some(first) = remaining.split('/').next() {
        if !is_version_segment(first) {
            break;
        }
        remaining = remaining[first.len()..].trim_start_matches('/');
    }
    remaining.trim_end_matches('/') == expected.trim_start_matches('/')
}

pub(super) fn request_streaming(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}

pub(super) fn gemini_model_from_body(body: &[u8]) -> Result<String, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Request JSON is invalid: {error}"))?;
    value
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Gemini bridge request is missing model".to_string())
}

pub(super) fn gemini_endpoint(model: &str, streaming: bool) -> (String, Option<String>) {
    if streaming {
        (
            format!("/v1beta/models/{model}:streamGenerateContent"),
            Some("alt=sse".to_string()),
        )
    } else {
        (format!("/v1beta/models/{model}:generateContent"), None)
    }
}

pub(super) fn parse_base64_data_url(value: &str) -> Option<(String, String)> {
    let value = value.trim();
    let (metadata, data) = value.strip_prefix("data:")?.split_once(',')?;
    let mut parts = metadata.split(';');
    let media_type = parts.next()?.trim();
    if media_type.is_empty() || !parts.any(|part| part.eq_ignore_ascii_case("base64")) {
        return None;
    }
    Some((media_type.to_string(), data.to_string()))
}

fn is_version_segment(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix('v') else {
        return false;
    };
    !rest.is_empty() && rest.chars().next().is_some_and(|ch| ch.is_ascii_digit())
}
