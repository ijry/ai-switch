use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) type ResponsesToolNamespaces = BTreeMap<String, String>;

pub(super) fn responses_reasoning_effort(object: &Map<String, Value>) -> Option<String> {
    let effort = object
        .get("reasoning")
        .and_then(Value::as_object)
        .and_then(|reasoning| reasoning.get("effort"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|effort| !effort.is_empty())?
        .to_ascii_lowercase();
    (effort != "none").then_some(effort)
}

pub(super) fn chat_reasoning_effort(effort: &str) -> Option<&'static str> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" | "xhigh" | "max" | "ultra" => Some("high"),
        _ => None,
    }
}

pub(super) fn anthropic_thinking_budget(effort: &str, max_tokens: Option<i64>) -> Option<i64> {
    let requested = match effort.trim().to_ascii_lowercase().as_str() {
        "low" => 2_048,
        "medium" => 8_192,
        "high" => 16_384,
        "xhigh" => 32_768,
        "max" => 65_536,
        "ultra" => 131_072,
        _ => return None,
    };
    let budget = max_tokens.map_or(requested, |max_tokens| {
        requested.min(max_tokens.saturating_sub(1))
    });
    (budget >= 1_024).then_some(budget)
}

pub(super) fn gemini_thinking_config(effort: &str, model: &str) -> Option<Value> {
    let effort = effort.trim().to_ascii_lowercase();
    if effort == "none" {
        return None;
    }
    if model.to_ascii_lowercase().contains("gemini-3") {
        return Some(serde_json::json!({
            "thinkingLevel": if effort == "low" { "low" } else { "high" }
        }));
    }
    let budget = match effort.as_str() {
        "low" => 1_024,
        "medium" => 4_096,
        "high" => 8_192,
        "xhigh" => 16_384,
        "max" => 32_768,
        "ultra" => 65_536,
        _ => return None,
    };
    Some(serde_json::json!({ "thinkingBudget": budget }))
}

/// Responses API reasoning items carry model reasoning from a previous turn.
/// Upstream chat/anthropic/gemini bridges have no equivalent input shape, so
/// these items are dropped during conversion.
pub(super) fn is_reasoning_input_item(item: &Value) -> bool {
    item.as_object()
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        .map(|item_type| item_type.eq_ignore_ascii_case("reasoning"))
        .unwrap_or(false)
}

pub(super) fn flatten_responses_function_tools(
    tools: &Value,
) -> Result<Vec<Map<String, Value>>, String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| "Responses tools must be an array".to_string())?;
    let mut flattened = Vec::new();
    collect_responses_function_tools(tools, None, &mut flattened)?;
    Ok(flattened)
}

pub(super) fn responses_tool_namespaces_from_body(
    body: &[u8],
) -> Result<ResponsesToolNamespaces, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Responses request JSON is invalid: {error}"))?;
    responses_tool_namespaces(value.get("tools"))
}
pub(super) fn responses_tool_namespaces(
    tools: Option<&Value>,
) -> Result<ResponsesToolNamespaces, String> {
    let Some(tools) = tools else {
        return Ok(BTreeMap::new());
    };
    let tools = tools
        .as_array()
        .ok_or_else(|| "Responses tools must be an array".to_string())?;
    let mut namespaces = BTreeMap::new();
    collect_responses_tool_namespaces(tools, None, &mut namespaces)?;
    Ok(namespaces)
}

fn collect_responses_function_tools(
    tools: &[Value],
    namespace: Option<&str>,
    flattened: &mut Vec<Map<String, Value>>,
) -> Result<(), String> {
    for tool in tools {
        let object = tool
            .as_object()
            .ok_or_else(|| "Responses tool entries must be objects".to_string())?;
        let tool_type = object
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("function");
        match tool_type {
            "function" => {
                let mut function = object.clone();
                if let (Some(namespace), Some(name)) = (
                    namespace.filter(|value| !value.is_empty()),
                    object.get("name").and_then(Value::as_str),
                ) {
                    function.insert(
                        "name".to_string(),
                        Value::String(qualified_response_tool_name(namespace, name)),
                    );
                }
                flattened.push(function);
            }
            "namespace" => {
                let nested_namespace = object
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| object.get("namespace").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .or(namespace);
                if let Some(nested) = object.get("tools") {
                    let nested = nested
                        .as_array()
                        .ok_or_else(|| "Responses namespace tools must be an array".to_string())?;
                    collect_responses_function_tools(nested, nested_namespace, flattened)?;
                }
            }
            "custom" => flattened.push(object.clone()),
            other if is_responses_builtin_tool_type(other) => {}
            other => return Err(format!("Unsupported Responses tool type: {other}")),
        }
    }
    Ok(())
}

pub(super) fn is_responses_builtin_tool_type(tool_type: &str) -> bool {
    matches!(
        tool_type,
        "web_search"
            | "web_search_preview"
            | "file_search"
            | "computer_use_preview"
            | "code_interpreter"
            | "image_generation"
            | "local_shell"
            | "shell"
            | "apply_patch"
            | "mcp"
            | "container_file_citation"
    )
}

fn collect_responses_tool_namespaces(
    tools: &[Value],
    namespace: Option<&str>,
    namespaces: &mut ResponsesToolNamespaces,
) -> Result<(), String> {
    for tool in tools {
        let object = tool
            .as_object()
            .ok_or_else(|| "Responses tool entries must be objects".to_string())?;
        let tool_type = object
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("function");
        match tool_type {
            "function" => {
                if let (Some(namespace), Some(name)) =
                    (namespace, object.get("name").and_then(Value::as_str))
                {
                    let namespace = namespace.trim();
                    let name = name.trim();
                    if !namespace.is_empty() && !name.is_empty() {
                        let qualified_name = qualified_response_tool_name(namespace, name);
                        namespaces.insert(qualified_name, namespace.to_string());
                        namespaces
                            .entry(name.to_string())
                            .or_insert_with(|| namespace.to_string());
                    }
                }
            }
            "namespace" => {
                let nested_namespace = object
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| object.get("namespace").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .or(namespace);
                if let Some(nested) = object.get("tools") {
                    let nested = nested
                        .as_array()
                        .ok_or_else(|| "Responses namespace tools must be an array".to_string())?;
                    collect_responses_tool_namespaces(nested, nested_namespace, namespaces)?;
                }
            }
            "custom" => {}
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn response_tool_namespace<'a>(
    name: &str,
    namespaces: &'a ResponsesToolNamespaces,
) -> Option<&'a str> {
    namespaces.get(name).map(String::as_str)
}

pub(super) fn response_tool_name<'a>(
    name: &'a str,
    namespaces: &ResponsesToolNamespaces,
) -> &'a str {
    let Some(namespace) = response_tool_namespace(name, namespaces) else {
        return name;
    };
    let prefix = qualified_response_tool_name(namespace, "");
    name.strip_prefix(&prefix).unwrap_or(name)
}

pub(super) fn qualified_response_tool_name(namespace: &str, name: &str) -> String {
    let namespace = namespace.trim_end_matches('_');
    if namespace.is_empty() {
        return name.to_string();
    }
    format!("{namespace}__{name}")
}

pub(super) fn response_tool_parameters(object: &Map<String, Value>) -> Value {
    object
        .get("parameters")
        .or_else(|| object.get("inputSchema"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}))
}
/// Renders Anthropic `tool_result` content as the plain string that Chat,
/// Responses, and Gemini all require for a tool result.
///
/// Non-text blocks cannot survive as-is in a string field, but dropping them
/// silently is worse than saying so: an MCP screenshot tool would return an
/// empty result and the model would answer as though it had seen nothing.
/// Each one is replaced by a short marker instead.
///
/// Never returns an empty string for a non-empty result — several
/// OpenAI-compatible gateways reject a `tool` message whose content is `""`.
pub(super) fn stringify_tool_result_content(value: &Value) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => {
            let rendered = parts
                .iter()
                .map(tool_result_part_to_text)
                .collect::<Vec<_>>();
            let joined = rendered
                .iter()
                .filter(|part| !part.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            Ok(joined)
        }
        Value::Null => Ok(String::new()),
        _ => serde_json::to_string(value)
            .map_err(|error| format!("Could not serialize tool result content: {error}")),
    }
}

/// Renders one `tool_result` content block. Media becomes a marker naming what
/// was there, so the model can ask for it another way instead of assuming the
/// tool returned nothing.
fn tool_result_part_to_text(part: &Value) -> String {
    let Some(object) = part.as_object() else {
        return part.as_str().map(str::to_string).unwrap_or_default();
    };
    if let Some(text) = object.get("text").and_then(Value::as_str) {
        return text.to_string();
    }
    match object.get("type").and_then(Value::as_str) {
        Some("image") => {
            let media_type = object
                .get("source")
                .and_then(Value::as_object)
                .and_then(|source| source.get("media_type"))
                .and_then(Value::as_str)
                .unwrap_or("image");
            format!("[ai-switch: tool returned an image ({media_type}) that this upstream cannot receive in a tool result]")
        }
        Some("document") => {
            "[ai-switch: tool returned a document that this upstream cannot receive in a tool result]"
                .to_string()
        }
        Some(other) => format!("[ai-switch: tool returned unsupported content of type {other}]"),
        None => String::new(),
    }
}

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
    strip_version_segments(path).trim_end_matches('/') == expected.trim_start_matches('/')
}

/// Matches a sub-resource of a create path, e.g. `messages/count_tokens`
/// against `("messages", "count_tokens")`.
pub(super) fn is_create_subpath(path: &str, expected: &str, sub: &str) -> bool {
    let remaining = strip_version_segments(path).trim_end_matches('/');
    let Some(rest) = remaining.strip_prefix(expected.trim_start_matches('/')) else {
        return false;
    };
    rest.strip_prefix('/') == Some(sub)
}

/// Strips leading API version segments (`v1`, `v1beta`, …) so path matching is
/// insensitive to how the client spells the version prefix.
fn strip_version_segments(path: &str) -> &str {
    let normalized = path.trim();
    let mut remaining = normalized
        .strip_prefix('/')
        .unwrap_or(normalized)
        .trim_start_matches('/');
    while let Some(first) = remaining.split('/').next() {
        if !is_version_segment(first) {
            break;
        }
        remaining = remaining[first.len()..].trim_start_matches('/');
    }
    remaining
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

/// Models that reject `max_tokens` and require `max_completion_tokens`: the
/// o-series (o1, o3, o4-mini, …).
pub(super) fn requires_max_completion_tokens(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    model.len() > 1
        && model.starts_with('o')
        && model
            .as_bytes()
            .get(1)
            .is_some_and(|byte| byte.is_ascii_digit())
}

pub(super) fn gemini_endpoint(model: &str, streaming: bool) -> (String, Option<String>) {
    let model = normalize_gemini_model_id(model);
    if streaming {
        (
            format!("/v1beta/models/{model}:streamGenerateContent"),
            Some("alt=sse".to_string()),
        )
    } else {
        (format!("/v1beta/models/{model}:generateContent"), None)
    }
}

/// Strips a leading `models/` (or `/`) so the endpoint format string cannot
/// produce a doubled prefix like `/v1beta/models/models/gemini-2.5-pro`, which
/// the upstream rejects. Model mappings and client env vars both supply that form.
fn normalize_gemini_model_id(model: &str) -> &str {
    let trimmed = model.trim().trim_start_matches('/');
    trimmed.strip_prefix("models/").unwrap_or(trimmed)
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

#[cfg(test)]
mod tests {
    use super::{is_create_path, is_create_subpath, stringify_tool_result_content};
    use serde_json::json;

    /// An MCP screenshot tool returns an image. Dropping it silently made the
    /// model answer as though the tool had returned nothing at all.
    #[test]
    fn tool_result_image_becomes_a_visible_marker() {
        let rendered = stringify_tool_result_content(&json!([
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="}}
        ]))
        .unwrap();

        assert!(
            !rendered.trim().is_empty(),
            "an image-only result must not render as empty"
        );
        assert!(rendered.contains("image/png"), "rendered={rendered}");
        // The base64 payload itself must not be inlined.
        assert!(!rendered.contains("iVBORw0KGgo="), "rendered={rendered}");
    }

    #[test]
    fn tool_result_keeps_text_alongside_media() {
        let rendered = stringify_tool_result_content(&json!([
            {"type": "text", "text": "Screenshot captured."},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}
        ]))
        .unwrap();

        assert!(
            rendered.contains("Screenshot captured."),
            "rendered={rendered}"
        );
        assert!(rendered.contains("image/png"), "rendered={rendered}");
    }

    #[test]
    fn tool_result_plain_text_is_unchanged() {
        assert_eq!(
            stringify_tool_result_content(&json!("42")).unwrap(),
            "42",
            "a plain string result must pass through verbatim"
        );
        assert_eq!(
            stringify_tool_result_content(&json!([{"type": "text", "text": "42"}])).unwrap(),
            "42"
        );
    }

    #[test]
    fn create_path_matching_ignores_version_prefixes() {
        assert!(is_create_path("/v1/messages", "messages"));
        assert!(is_create_path("/messages", "messages"));
        assert!(is_create_path("/v1beta/messages/", "messages"));
        assert!(!is_create_path("/v1/messages/count_tokens", "messages"));
        assert!(!is_create_path("/v1/responses", "messages"));
    }

    #[test]
    fn create_subpath_matches_only_the_named_sub_resource() {
        assert!(is_create_subpath(
            "/v1/messages/count_tokens",
            "messages",
            "count_tokens"
        ));
        assert!(is_create_subpath(
            "/messages/count_tokens",
            "messages",
            "count_tokens"
        ));
        // The parent path and a different sub-resource must not match.
        assert!(!is_create_subpath(
            "/v1/messages",
            "messages",
            "count_tokens"
        ));
        assert!(!is_create_subpath(
            "/v1/messages/batches",
            "messages",
            "count_tokens"
        ));
        // A deeper path must not match either.
        assert!(!is_create_subpath(
            "/v1/messages/count_tokens/extra",
            "messages",
            "count_tokens"
        ));
    }
}
