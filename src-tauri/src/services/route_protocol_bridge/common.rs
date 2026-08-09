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
