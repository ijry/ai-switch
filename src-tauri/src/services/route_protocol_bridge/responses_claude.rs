#[cfg(test)]
mod tests {
    use super::{anthropic_response_to_responses, responses_request_to_anthropic};
    use serde_json::Value;

    #[test]
    fn converts_responses_request_to_claude_messages() {
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "instructions": "Be concise",
            "input": [
                {"role": "user", "content": [{"type": "input_text", "text": "Find x"}]},
                {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{\"key\":\"x\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "42"}
            ],
            "max_output_tokens": 64,
            "temperature": 0.2,
            "tools": [{
                "type": "function",
                "name": "lookup",
                "description": "Lookup value",
                "parameters": {"type":"object","properties":{"key":{"type":"string"}}}
            }]
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_anthropic(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(converted["system"], "Be concise");
        assert_eq!(converted["messages"][0]["role"], "user");
        assert_eq!(converted["messages"][0]["content"][0]["type"], "text");
        assert_eq!(converted["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(
            converted["messages"][2]["content"][0]["type"],
            "tool_result"
        );
        assert_eq!(converted["max_tokens"], 64);
        assert_eq!(converted["tools"][0]["input_schema"]["type"], "object");
    }

    #[test]
    fn converts_responses_input_image_to_anthropic_image_block() {
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "input": [
                {
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": "See image"},
                        {"type": "input_image", "image_url": "data:image/png;base64,aGVsbG8="}
                    ]
                }
            ]
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_anthropic(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(converted["messages"][0]["content"][0]["type"], "text");
        assert_eq!(converted["messages"][0]["content"][1]["type"], "image");
        assert_eq!(converted["messages"][0]["content"][1]["source"]["type"], "base64");
        assert_eq!(
            converted["messages"][0]["content"][1]["source"]["media_type"],
            "image/png"
        );
        assert_eq!(
            converted["messages"][0]["content"][1]["source"]["data"],
            "aGVsbG8="
        );
    }

    #[test]
    fn converts_anthropic_response_to_responses_json() {
        let upstream = serde_json::json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [
                {"type": "text", "text": "hello"},
                {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {"key":"x"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 3, "output_tokens": 5}
        });

        let converted = anthropic_response_to_responses(
            200,
            Some("application/json"),
            serde_json::to_vec(&upstream).unwrap().as_slice(),
        )
        .unwrap();
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(output["object"], "response");
        assert_eq!(output["id"], "msg_1");
        assert_eq!(output["output_text"], "hello");
        assert_eq!(output["output"][1]["type"], "function_call");
        assert_eq!(output["output"][1]["call_id"], "toolu_1");
        assert_eq!(output["usage"]["input_tokens"], 3);
        assert_eq!(output["usage"]["output_tokens"], 5);
    }

    #[test]
    fn converts_anthropic_sse_to_responses_events() {
        let upstream = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-sonnet-4\",\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );

        let converted =
            anthropic_response_to_responses(200, Some("text/event-stream"), upstream.as_bytes())
                .unwrap();
        let output = String::from_utf8(converted.body).unwrap();

        assert!(output.contains("event: response.created"));
        assert!(output.contains("event: response.output_text.delta"));
        assert!(output.contains("\"delta\":\"hello\""));
        assert!(output.contains("event: response.completed"));
    }
}

use super::{common::parse_base64_data_url, sse, TransformedBridgeResponse};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(super) fn responses_request_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Responses request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Responses request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    if let Some(model) = object.get("model") {
        result.insert("model".to_string(), model.clone());
    }
    if let Some(instructions) = object.get("instructions") {
        let system = text_value(instructions, "instructions")?;
        if !system.is_empty() {
            result.insert("system".to_string(), Value::String(system));
        }
    }
    let mut messages = Vec::new();
    if let Some(input) = object.get("input") {
        messages.extend(convert_input(input)?);
    }
    result.insert("messages".to_string(), Value::Array(messages));
    if let Some(max_tokens) = object.get("max_output_tokens") {
        result.insert("max_tokens".to_string(), max_tokens.clone());
    }
    copy_fields(object, &mut result, &["temperature", "top_p", "stream"]);
    if let Some(stop) = object.get("stop") {
        result.insert("stop_sequences".to_string(), stop.clone());
    }
    if let Some(tools) = object.get("tools") {
        result.insert("tools".to_string(), convert_tools(tools)?);
    }

    serde_json::to_vec(&Value::Object(result))
        .map_err(|error| format!("Could not serialize Anthropic request: {error}"))
}

pub(super) fn anthropic_response_to_responses(
    status: u16,
    content_type: Option<&str>,
    body: &[u8],
) -> Result<TransformedBridgeResponse, String> {
    if !(200..300).contains(&status) {
        return Ok(TransformedBridgeResponse {
            body: body.to_vec(),
            content_type: content_type.map(str::to_string),
        });
    }
    if content_type.is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
        || looks_like_sse(body)
    {
        let response = anthropic_sse_to_responses_json(body)?;
        return Ok(TransformedBridgeResponse {
            body: sse::responses_events_from_completed_response(&response)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }
    Ok(TransformedBridgeResponse {
        body: anthropic_json_to_responses(body)?,
        content_type: Some("application/json".to_string()),
    })
}

fn anthropic_json_to_responses(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Anthropic response JSON is invalid: {error}"))?;
    if value.get("error").is_some() {
        return Ok(body.to_vec());
    }
    anthropic_value_to_responses_json(&value)
}

fn anthropic_value_to_responses_json(value: &Value) -> Result<Vec<u8>, String> {
    let response_id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_ai_switch");
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let stop_reason = value.get("stop_reason").and_then(Value::as_str);
    let (output, output_text) = anthropic_content_to_responses_output(
        response_id,
        value.get("content").and_then(Value::as_array),
    )?;
    let response = json!({
        "id": response_id,
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": responses_status(stop_reason),
        "model": model,
        "output": output,
        "output_text": output_text,
        "error": Value::Null,
        "incomplete_details": incomplete_details(stop_reason),
        "usage": anthropic_usage_to_responses(value.get("usage")),
    });
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Responses response: {error}"))
}

fn anthropic_sse_to_responses_json(body: &[u8]) -> Result<Value, String> {
    let mut state = AnthropicSseState::default();
    for value in sse::parse_sse_data_records(body)? {
        match value.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                if let Some(message) = value.get("message") {
                    state.capture_message(message);
                }
            }
            Some("content_block_start") => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let block = value
                    .get("content_block")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "text", "text": ""}));
                state.blocks.insert(index, block);
            }
            Some("content_block_delta") => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                state.apply_delta(index, value.get("delta").unwrap_or(&Value::Null))?;
            }
            Some("message_delta") => {
                if let Some(stop_reason) = value
                    .pointer("/delta/stop_reason")
                    .and_then(Value::as_str)
                {
                    state.stop_reason = stop_reason.to_string();
                }
                if let Some(output_tokens) = value.pointer("/usage/output_tokens").and_then(Value::as_i64)
                {
                    state.output_tokens = output_tokens;
                }
            }
            Some("message_stop") => {}
            _ => {}
        }
    }

    let message = json!({
        "id": state.response_id(),
        "model": state.model(),
        "content": state.blocks.into_values().collect::<Vec<_>>(),
        "stop_reason": if state.stop_reason.is_empty() { "end_turn" } else { &state.stop_reason },
        "usage": {
            "input_tokens": state.input_tokens,
            "output_tokens": state.output_tokens
        }
    });
    let bytes = anthropic_value_to_responses_json(&message)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Could not parse buffered Responses JSON: {error}"))
}

#[derive(Debug, Default)]
struct AnthropicSseState {
    id: String,
    model: String,
    input_tokens: i64,
    output_tokens: i64,
    stop_reason: String,
    blocks: BTreeMap<usize, Value>,
}

impl AnthropicSseState {
    fn capture_message(&mut self, message: &Value) {
        if self.id.is_empty() {
            self.id = message
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("resp_ai_switch")
                .to_string();
        }
        if self.model.is_empty() {
            self.model = message
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
        }
        self.input_tokens = message
            .pointer("/usage/input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(self.input_tokens);
    }

    fn apply_delta(&mut self, index: usize, delta: &Value) -> Result<(), String> {
        let block = self
            .blocks
            .entry(index)
            .or_insert_with(|| json!({"type": "text", "text": ""}));
        match delta.get("type").and_then(Value::as_str) {
            Some("text_delta") => {
                let text = delta.get("text").and_then(Value::as_str).unwrap_or("");
                let current = block.get("text").and_then(Value::as_str).unwrap_or("");
                block["text"] = Value::String(format!("{current}{text}"));
            }
            Some("input_json_delta") => {
                let partial_json = delta
                    .get("partial_json")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let current = block
                    .get("_partial_json")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                block["_partial_json"] = Value::String(format!("{current}{partial_json}"));
                if let Some(input) = serde_json::from_str::<Value>(
                    block
                        .get("_partial_json")
                        .and_then(Value::as_str)
                        .unwrap_or("{}"),
                )
                .ok()
                {
                    block["input"] = input;
                }
            }
            Some(other) => return Err(format!("Unsupported Anthropic SSE delta type: {other}")),
            None => {}
        }
        Ok(())
    }

    fn response_id(&self) -> &str {
        if self.id.is_empty() {
            "resp_ai_switch"
        } else {
            &self.id
        }
    }

    fn model(&self) -> &str {
        if self.model.is_empty() {
            "unknown"
        } else {
            &self.model
        }
    }
}

fn convert_input(input: &Value) -> Result<Vec<Value>, String> {
    match input {
        Value::String(text) => Ok(vec![json!({"role": "user", "content": [text_block(text)]})]),
        Value::Array(items) => items.iter().map(convert_input_item).collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err("Responses input must be a string or array".to_string()),
    }
}

fn convert_input_item(item: &Value) -> Result<Value, String> {
    let object = item
        .as_object()
        .ok_or_else(|| "Responses input items must be JSON objects".to_string())?;
    match object.get("type").and_then(Value::as_str) {
        Some("function_call") => {
            let call_id = required_string(object, "call_id", "function_call")?;
            let name = required_string(object, "name", "function_call")?;
            let arguments = object
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let input = serde_json::from_str::<Value>(arguments)
                .unwrap_or_else(|_| Value::String(arguments.to_string()));
            Ok(json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": call_id,
                    "name": name,
                    "input": input
                }]
            }))
        }
        Some("function_call_output") => {
            let call_id = required_string(object, "call_id", "function_call_output")?;
            let output = object
                .get("output")
                .map(stringify_content)
                .transpose()?
                .unwrap_or_default();
            Ok(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": output
                }]
            }))
        }
        Some("message") | None if object.contains_key("role") => {
            let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = object
                .get("content")
                .map(convert_message_content)
                .transpose()?
                .unwrap_or_else(Vec::new);
            Ok(json!({"role": role, "content": content}))
        }
        Some(other) => Err(format!("Unsupported Responses input item type: {other}")),
        None => Err("Responses input item is missing role or type".to_string()),
    }
}

fn convert_message_content(content: &Value) -> Result<Vec<Value>, String> {
    match content {
        Value::String(text) => Ok(vec![text_block(text)]),
        Value::Array(parts) => parts.iter().map(convert_content_part).collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err("Responses message content must be a string or array".to_string()),
    }
}

fn convert_content_part(part: &Value) -> Result<Value, String> {
    let object = part
        .as_object()
        .ok_or_else(|| "Responses content parts must be objects".to_string())?;
    match object.get("type").and_then(Value::as_str) {
        Some("input_text" | "output_text" | "text") => {
            let text = required_string(object, "text", "text content")?;
            Ok(text_block(text))
        }
        Some("input_image") => {
            let image_url = required_string(object, "image_url", "input_image")?;
            let Some((media_type, data)) = parse_base64_data_url(image_url) else {
                return Err("Anthropic bridge only supports base64 data URL images".to_string());
            };
            Ok(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": media_type,
                    "data": data
                }
            }))
        }
        Some(other) => Err(format!("Unsupported Responses content type: {other}")),
        None => Err("Responses content part is missing type".to_string()),
    }
}

fn convert_tools(tools: &Value) -> Result<Value, String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| "Responses tools must be an array".to_string())?;
    let mut converted = Vec::with_capacity(tools.len());
    for tool in tools {
        let object = tool
            .as_object()
            .ok_or_else(|| "Responses tool entries must be objects".to_string())?;
        let tool_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("function");
        if tool_type != "function" {
            return Err(format!("Unsupported Responses tool type: {tool_type}"));
        }
        let name = required_string(object, "name", "function tool")?;
        let mut converted_tool = Map::new();
        converted_tool.insert("name".to_string(), Value::String(name.to_string()));
        if let Some(description) = object.get("description") {
            converted_tool.insert("description".to_string(), description.clone());
        }
        converted_tool.insert(
            "input_schema".to_string(),
            object
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
        );
        converted.push(Value::Object(converted_tool));
    }
    Ok(Value::Array(converted))
}

fn anthropic_content_to_responses_output(
    response_id: &str,
    content: Option<&Vec<Value>>,
) -> Result<(Vec<Value>, String), String> {
    let mut output = Vec::new();
    let mut text = String::new();
    let mut message_content = Vec::new();
    let Some(content) = content else {
        return Ok((output, text));
    };
    for item in content {
        match item.get("type").and_then(Value::as_str) {
            Some("text") => {
                let item_text = item.get("text").and_then(Value::as_str).unwrap_or("");
                text.push_str(item_text);
                message_content.push(json!({
                    "type": "output_text",
                    "text": item_text,
                    "annotations": [],
                    "logprobs": []
                }));
            }
            Some("tool_use") => {
                let call_id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_ai_switch");
                let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
                let arguments =
                    serde_json::to_string(item.get("input").unwrap_or(&Value::Object(Map::new())))
                        .map_err(|error| {
                            format!("Could not serialize Anthropic tool input: {error}")
                        })?;
                output.push(json!({
                    "id": format!("fc_{}_{}", sanitize_id(response_id), output.len()),
                    "type": "function_call",
                    "status": "completed",
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments
                }));
            }
            Some(other) => return Err(format!("Unsupported Anthropic content type: {other}")),
            None => return Err("Anthropic content block is missing type".to_string()),
        }
    }
    if !message_content.is_empty() {
        output.insert(
            0,
            json!({
                "id": format!("msg_{}", sanitize_id(response_id)),
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": message_content
            }),
        );
    }
    Ok((output, text))
}

fn anthropic_usage_to_responses(usage: Option<&Value>) -> Value {
    let Some(usage) = usage else {
        return Value::Null;
    };
    let input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    json!({
        "input_tokens": input_tokens,
        "input_tokens_details": {"cached_tokens": 0},
        "output_tokens": output_tokens,
        "output_tokens_details": {"reasoning_tokens": 0},
        "total_tokens": input_tokens + output_tokens
    })
}

fn responses_status(stop_reason: Option<&str>) -> &'static str {
    match stop_reason {
        Some("max_tokens") => "incomplete",
        _ => "completed",
    }
}

fn incomplete_details(stop_reason: Option<&str>) -> Value {
    match stop_reason {
        Some("max_tokens") => json!({"reason": "max_output_tokens"}),
        _ => Value::Null,
    }
}

fn text_value(value: &Value, label: &str) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => parts
            .iter()
            .map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| format!("Responses {label} entries must contain text"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.join("\n")),
        _ => Err(format!("Responses {label} must be text")),
    }
}

fn text_block(text: &str) -> Value {
    json!({"type": "text", "text": text})
}

fn stringify_content(value: &Value) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Null => Ok(String::new()),
        _ => serde_json::to_string(value)
            .map_err(|error| format!("Could not serialize tool output: {error}")),
    }
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Responses {label} is missing {key}"))
}

fn copy_fields(source: &Map<String, Value>, target: &mut Map<String, Value>, fields: &[&str]) {
    for field in fields {
        if let Some(value) = source.get(*field) {
            target.insert((*field).to_string(), value.clone());
        }
    }
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn looks_like_sse(body: &[u8]) -> bool {
    std::str::from_utf8(body).ok().is_some_and(|text| {
        text.lines()
            .any(|line| line.trim_start().starts_with("data:"))
    })
}
