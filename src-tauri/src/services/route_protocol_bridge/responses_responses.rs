use super::common::{response_tool_name, response_tool_namespace, ResponsesToolNamespaces};
use super::TransformedBridgeResponse;
use serde_json::{Map, Value};

pub(super) fn responses_request_to_responses(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Responses request JSON is invalid: {error}"))?;
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| "Responses request body must be a JSON object".to_string())?;

    if let Some(tools) = object.get("tools") {
        let flattened = flatten_native_responses_tools(tools, None)?;
        let tools = flattened
            .into_iter()
            .map(normalize_response_tool)
            .map(Value::Object)
            .collect();
        object.insert("tools".to_string(), Value::Array(tools));
    }

    serde_json::to_vec(&Value::Object(object))
        .map_err(|error| format!("Could not serialize Responses request: {error}"))
}

fn flatten_native_responses_tools(
    tools: &Value,
    namespace: Option<&str>,
) -> Result<Vec<Map<String, Value>>, String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| "Responses tools must be an array".to_string())?;
    let mut flattened = Vec::new();
    for tool in tools {
        let object = tool
            .as_object()
            .ok_or_else(|| "Responses tool entries must be objects".to_string())?;
        let tool_type = object
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("function");
        if tool_type == "namespace" {
            let nested_namespace = object
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| object.get("namespace").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or(namespace);
            if let Some(nested_tools) = object.get("tools") {
                flattened.extend(flatten_native_responses_tools(
                    nested_tools,
                    nested_namespace,
                )?);
            }
            continue;
        }

        let mut function = object.clone();
        if tool_type == "function" {
            if let (Some(namespace), Some(name)) = (
                namespace.filter(|value| !value.is_empty()),
                object.get("name").and_then(Value::as_str),
            ) {
                function.insert(
                    "name".to_string(),
                    Value::String(super::common::qualified_response_tool_name(namespace, name)),
                );
            }
            function = normalize_response_tool(function);
        }
        flattened.push(function);
    }
    Ok(flattened)
}

pub(super) fn responses_response_to_responses(
    status: u16,
    content_type: Option<&str>,
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<TransformedBridgeResponse, String> {
    if !(200..300).contains(&status) || tool_namespaces.is_empty() {
        return Ok(TransformedBridgeResponse {
            body: body.to_vec(),
            content_type: content_type.map(str::to_string),
        });
    }

    if content_type.is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
        || looks_like_sse(body)
    {
        return Ok(TransformedBridgeResponse {
            body: responses_sse_to_responses(body, tool_namespaces)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }

    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Responses response JSON is invalid: {error}"))?;
    let value = restore_tool_namespaces(value, tool_namespaces);
    Ok(TransformedBridgeResponse {
        body: serde_json::to_vec(&value)
            .map_err(|error| format!("Could not serialize Responses response: {error}"))?,
        content_type: Some("application/json".to_string()),
    })
}

fn normalize_response_tool(mut tool: Map<String, Value>) -> Map<String, Value> {
    if !tool.contains_key("parameters") {
        if let Some(input_schema) = tool.remove("inputSchema") {
            tool.insert("parameters".to_string(), input_schema);
        }
    } else {
        tool.remove("inputSchema");
    }
    tool
}

fn restore_tool_namespaces(mut value: Value, tool_namespaces: &ResponsesToolNamespaces) -> Value {
    match &mut value {
        Value::Array(items) => {
            for item in items {
                *item = restore_tool_namespaces(item.take(), tool_namespaces);
            }
        }
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("function_call") {
                if let Some(name) = object.get("name").and_then(Value::as_str) {
                    if let Some(namespace) = response_tool_namespace(name, tool_namespaces) {
                        let response_name = response_tool_name(name, tool_namespaces).to_string();
                        object.insert("name".to_string(), Value::String(response_name));
                        object.insert(
                            "namespace".to_string(),
                            Value::String(namespace.to_string()),
                        );
                    }
                }
            }
            for item in object.values_mut() {
                let current = item.take();
                *item = restore_tool_namespaces(current, tool_namespaces);
            }
        }
        _ => {}
    }
    value
}

fn responses_sse_to_responses(
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Vec<u8>, String> {
    let text = String::from_utf8_lossy(body).replace("\r\n", "\n");
    let mut output = String::new();
    for block in text.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        let data = block
            .lines()
            .filter_map(|line| line.trim().strip_prefix("data:").map(str::trim))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            output.push_str(block);
            output.push_str("\n\n");
            continue;
        }
        if data == "[DONE]" {
            output.push_str(block);
            output.push_str("\n\n");
            continue;
        }
        // This path only rewrites tool names, so a record we cannot parse is
        // forwarded untouched instead of failing the whole stream.
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            output.push_str(block);
            output.push_str("\n\n");
            continue;
        };
        for line in block
            .lines()
            .filter(|line| !line.trim().starts_with("data:"))
        {
            output.push_str(line);
            output.push('\n');
        }
        output.push_str("data: ");
        output.push_str(
            &serde_json::to_string(&restore_tool_namespaces(value, tool_namespaces))
                .map_err(|error| format!("Could not serialize Responses SSE data: {error}"))?,
        );
        output.push_str("\n\n");
    }
    Ok(output.into_bytes())
}

fn looks_like_sse(body: &[u8]) -> bool {
    String::from_utf8_lossy(body)
        .lines()
        .any(|line| line.trim_start().starts_with("data:"))
}
