use super::common::stringify_tool_result_content;
use super::{sse, TransformedBridgeResponse};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(super) fn anthropic_request_to_responses(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Anthropic request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Anthropic request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    if let Some(model) = object.get("model") {
        result.insert("model".to_string(), model.clone());
    }
    if let Some(system) = object.get("system") {
        let instructions = anthropic_text(system, "system")?;
        if !instructions.is_empty() {
            result.insert("instructions".to_string(), Value::String(instructions));
        }
    }
    result.insert(
        "input".to_string(),
        object
            .get("messages")
            .map(convert_messages)
            .transpose()?
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );
    if let Some(max_tokens) = object.get("max_tokens") {
        result.insert("max_output_tokens".to_string(), max_tokens.clone());
    }
    copy_fields(object, &mut result, &["temperature", "top_p", "stream"]);
    if let Some(stop) = object.get("stop_sequences") {
        result.insert("stop".to_string(), stop.clone());
    }
    if let Some(tools) = object.get("tools") {
        result.insert("tools".to_string(), convert_tools(tools)?);
    }
    if let Some(tool_choice) = object.get("tool_choice") {
        result.insert("tool_choice".to_string(), convert_tool_choice(tool_choice)?);
    }

    serde_json::to_vec(&Value::Object(result))
        .map_err(|error| format!("Could not serialize Responses request: {error}"))
}

pub(super) fn responses_response_to_anthropic(
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
        return Ok(TransformedBridgeResponse {
            body: responses_sse_to_anthropic(body)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }

    Ok(TransformedBridgeResponse {
        body: responses_json_to_anthropic(body)?,
        content_type: Some("application/json".to_string()),
    })
}

fn convert_messages(messages: &Value) -> Result<Value, String> {
    let messages = messages
        .as_array()
        .ok_or_else(|| "Anthropic messages must be an array".to_string())?;
    let mut input = Vec::new();
    for message in messages {
        let object = message
            .as_object()
            .ok_or_else(|| "Anthropic messages entries must be objects".to_string())?;
        let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
        let blocks = content_blocks(object.get("content").unwrap_or(&Value::Null))?;
        match role {
            "user" => convert_user_message(&blocks, &mut input)?,
            "assistant" => convert_assistant_message(&blocks, &mut input)?,
            other => return Err(format!("Unsupported Anthropic message role: {other}")),
        }
    }
    Ok(Value::Array(input))
}

fn convert_user_message(blocks: &[Value], input: &mut Vec<Value>) -> Result<(), String> {
    let mut content = Vec::new();
    for block in blocks {
        let object = block
            .as_object()
            .ok_or_else(|| "Anthropic content blocks must be objects".to_string())?;
        match object.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = object.get("text").and_then(Value::as_str).unwrap_or("");
                content.push(json!({"type": "input_text", "text": text}));
            }
            Some("image") => content.push(convert_image_block(object)?),
            Some("tool_result") => {
                if !content.is_empty() {
                    input.push(json!({"type": "message", "role": "user", "content": content}));
                    content = Vec::new();
                }
                let call_id = required_string(object, "tool_use_id", "tool_result")?;
                let mut output = object
                    .get("content")
                    .map(stringify_tool_result_content)
                    .transpose()?
                    .unwrap_or_default();
                // Anthropic marks a failed tool call with `is_error`; the
                // Responses API has no equivalent field on function_call_output.
                if object.get("is_error").and_then(Value::as_bool) == Some(true) {
                    output = format!("[tool error] {output}");
                }
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": output
                }));
            }
            // Unknown/newer block types are skipped: a degraded turn beats a 502
            // that also marks every credential in the pool as failed.
            Some(_) | None => {}
        }
    }
    if !content.is_empty() {
        input.push(json!({"type": "message", "role": "user", "content": content}));
    }
    Ok(())
}

fn convert_assistant_message(blocks: &[Value], input: &mut Vec<Value>) -> Result<(), String> {
    let mut content = Vec::new();
    for block in blocks {
        let object = block
            .as_object()
            .ok_or_else(|| "Anthropic content blocks must be objects".to_string())?;
        match object.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = object.get("text").and_then(Value::as_str).unwrap_or("");
                content.push(json!({"type": "output_text", "text": text}));
            }
            Some("tool_use") => {
                if !content.is_empty() {
                    input.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": content
                    }));
                    content = Vec::new();
                }
                let call_id = required_string(object, "id", "tool_use")?;
                let name = required_string(object, "name", "tool_use")?;
                let arguments = serde_json::to_string(
                    object.get("input").unwrap_or(&Value::Object(Map::new())),
                )
                .map_err(|error| format!("Could not serialize Anthropic tool input: {error}"))?;
                input.push(json!({
                    "type": "function_call",
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments
                }));
            }
            // Claude Code replays thinking blocks in assistant history whenever
            // extended thinking is on. The Responses API has no inbound slot for
            // them, so drop them instead of failing every turn after the first.
            Some("thinking") | Some("redacted_thinking") => {}
            Some(_) | None => {}
        }
    }
    if !content.is_empty() {
        input.push(json!({"type": "message", "role": "assistant", "content": content}));
    }
    Ok(())
}

fn convert_image_block(object: &Map<String, Value>) -> Result<Value, String> {
    let source = object
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| "Anthropic image block is missing source".to_string())?;
    let source_type = source
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("base64");
    if source_type != "base64" {
        return Err(format!(
            "Unsupported Anthropic image source type: {source_type}"
        ));
    }
    let media_type = required_string(source, "media_type", "image source")?;
    let data = required_string(source, "data", "image source")?;
    Ok(json!({
        "type": "input_image",
        "image_url": format!("data:{media_type};base64,{data}")
    }))
}

fn convert_tools(tools: &Value) -> Result<Value, String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| "Anthropic tools must be an array".to_string())?;
    let mut converted = Vec::with_capacity(tools.len());
    for tool in tools {
        let object = tool
            .as_object()
            .ok_or_else(|| "Anthropic tool entries must be objects".to_string())?;
        let name = required_string(object, "name", "tool")?;
        let mut function = Map::new();
        function.insert("type".to_string(), Value::String("function".to_string()));
        function.insert("name".to_string(), Value::String(name.to_string()));
        if let Some(description) = object.get("description") {
            function.insert("description".to_string(), description.clone());
        }
        function.insert(
            "parameters".to_string(),
            object
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
        );
        converted.push(Value::Object(function));
    }
    Ok(Value::Array(converted))
}

fn convert_tool_choice(tool_choice: &Value) -> Result<Value, String> {
    let Some(object) = tool_choice.as_object() else {
        return Ok(tool_choice.clone());
    };
    match object.get("type").and_then(Value::as_str) {
        Some("auto") => Ok(Value::String("auto".to_string())),
        Some("any") => Ok(Value::String("required".to_string())),
        Some("tool") => {
            let name = required_string(object, "name", "tool_choice")?;
            Ok(json!({"type": "function", "name": name}))
        }
        Some("none") => Ok(Value::String("none".to_string())),
        Some(other) => Err(format!("Unsupported Anthropic tool_choice type: {other}")),
        None => Ok(Value::String("auto".to_string())),
    }
}

fn responses_json_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Responses response JSON is invalid: {error}"))?;
    let response_id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("msg_ai_switch");
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let content = responses_output_to_anthropic_content(&value)?;
    let stop_reason = responses_stop_reason(&value, &content);
    let response = anthropic_message(
        response_id,
        model,
        content,
        stop_reason,
        responses_usage_to_anthropic(value.get("usage")),
    );
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Anthropic response: {error}"))
}

fn responses_output_to_anthropic_content(value: &Value) -> Result<Vec<Value>, String> {
    if value.get("status").and_then(Value::as_str) == Some("failed") {
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("Responses upstream returned an error");
        return Ok(vec![json!({"type": "text", "text": message})]);
    }

    let mut content = Vec::new();
    if let Some(output) = value.get("output").and_then(Value::as_array) {
        for item in output {
            match item.get("type").and_then(Value::as_str) {
                Some("message") => {
                    if let Some(parts) = item.get("content").and_then(Value::as_array) {
                        for part in parts {
                            if let Some(text) = response_text_part(part) {
                                content.push(json!({"type": "text", "text": text}));
                            }
                        }
                    }
                }
                Some("function_call") => {
                    let call_id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(Value::as_str)
                        .unwrap_or("call_ai_switch");
                    let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
                    let arguments = item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("{}");
                    content.push(json!({
                        "type": "tool_use",
                        "id": call_id,
                        "name": name,
                        "input": parse_json_or_string(arguments)
                    }));
                }
                Some(_) | None => {}
            }
        }
    }

    if content.is_empty() {
        if let Some(text) = value.get("output_text").and_then(Value::as_str) {
            if !text.is_empty() {
                content.push(json!({"type": "text", "text": text}));
            }
        }
    }
    Ok(content)
}

fn responses_sse_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut aggregate = ResponsesStreamAggregate::default();
    for value in sse::parse_sse_data_records(body)? {
        aggregate.capture_response(value.get("response"));
        match value.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    aggregate.text.push_str(delta);
                }
            }
            Some("response.output_item.added" | "response.output_item.done") => {
                if let Some(item) = value.get("item") {
                    aggregate.capture_output_item(
                        value
                            .get("output_index")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as usize,
                        item,
                    );
                }
            }
            Some("response.function_call_arguments.delta") => {
                let index = value
                    .get("output_index")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                let tool = aggregate.tools.entry(index).or_default();
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    tool.arguments.push_str(delta);
                }
            }
            Some("response.completed" | "response.incomplete" | "response.failed") => {
                aggregate.capture_response(value.get("response"));
            }
            _ => {}
        }
    }

    let mut content = Vec::new();
    if !aggregate.text.is_empty() {
        content.push(json!({"type": "text", "text": aggregate.text}));
    }
    for tool in aggregate.tools.values() {
        content.push(json!({
            "type": "tool_use",
            "id": if tool.call_id.is_empty() { "call_ai_switch" } else { &tool.call_id },
            "name": if tool.name.is_empty() { "tool" } else { &tool.name },
            "input": parse_json_or_string(&tool.arguments)
        }));
    }
    let stop_reason = if content
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
    {
        "tool_use"
    } else if aggregate.status == "incomplete" {
        "max_tokens"
    } else if aggregate.status == "failed" {
        // "error" is not in Anthropic's stop_reason enum, and Claude Code
        // validates against it. A failed turn reads as a refusal.
        "refusal"
    } else {
        "end_turn"
    };
    Ok(anthropic_sse(
        aggregate.response_id(),
        aggregate.model(),
        content,
        stop_reason,
        aggregate.input_tokens,
        aggregate.output_tokens,
    )?
    .into_bytes())
}

#[derive(Debug, Default)]
struct ResponsesStreamAggregate {
    response_id: String,
    model: String,
    status: String,
    text: String,
    tools: BTreeMap<usize, StreamToolCall>,
    input_tokens: i64,
    output_tokens: i64,
}

impl ResponsesStreamAggregate {
    fn capture_response(&mut self, response: Option<&Value>) {
        let Some(response) = response else {
            return;
        };
        if self.response_id.is_empty() {
            self.response_id = response
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("msg_ai_switch")
                .to_string();
        }
        if self.model.is_empty() {
            self.model = response
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
        }
        if let Some(status) = response.get("status").and_then(Value::as_str) {
            self.status = status.to_string();
        }
        if let Some(usage) = response.get("usage") {
            self.input_tokens = usage
                .get("input_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(self.input_tokens);
            self.output_tokens = usage
                .get("output_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(self.output_tokens);
        }
    }

    fn capture_output_item(&mut self, index: usize, item: &Value) {
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            return;
        }
        let tool = self.tools.entry(index).or_default();
        if let Some(call_id) = item.get("call_id").and_then(Value::as_str) {
            tool.call_id = call_id.to_string();
        }
        if let Some(name) = item.get("name").and_then(Value::as_str) {
            tool.name = name.to_string();
        }
        if let Some(arguments) = item.get("arguments").and_then(Value::as_str) {
            tool.arguments = arguments.to_string();
        }
    }

    fn response_id(&self) -> &str {
        if self.response_id.is_empty() {
            "msg_ai_switch"
        } else {
            &self.response_id
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

#[derive(Debug, Default)]
struct StreamToolCall {
    call_id: String,
    name: String,
    arguments: String,
}

fn response_text_part(part: &Value) -> Option<&str> {
    match part.get("type").and_then(Value::as_str) {
        Some("output_text" | "text" | "input_text") => part.get("text").and_then(Value::as_str),
        _ => None,
    }
}

fn responses_stop_reason(value: &Value, content: &[Value]) -> &'static str {
    if content
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
    {
        return "tool_use";
    }
    match value.get("status").and_then(Value::as_str) {
        Some("incomplete") => "max_tokens",
        // "error" is not a valid Anthropic stop_reason.
        Some("failed") => "refusal",
        _ => "end_turn",
    }
}

fn responses_usage_to_anthropic(usage: Option<&Value>) -> Value {
    let Some(usage) = usage else {
        return json!({"input_tokens": 0, "output_tokens": 0});
    };
    json!({
        "input_tokens": usage.get("input_tokens").and_then(Value::as_i64).unwrap_or(0),
        "output_tokens": usage.get("output_tokens").and_then(Value::as_i64).unwrap_or(0)
    })
}

fn anthropic_message(
    id: &str,
    model: &str,
    content: Vec<Value>,
    stop_reason: &str,
    usage: Value,
) -> Value {
    json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason,
        "stop_sequence": Value::Null,
        "usage": usage
    })
}

fn anthropic_sse(
    id: &str,
    model: &str,
    content: Vec<Value>,
    stop_reason: &str,
    input_tokens: i64,
    output_tokens: i64,
) -> Result<String, String> {
    let mut output = String::new();
    push_sse_event(
        &mut output,
        "message_start",
        json!({
            "type": "message_start",
            "message": anthropic_message(
                id,
                model,
                Vec::new(),
                "end_turn",
                json!({"input_tokens": input_tokens, "output_tokens": 0})
            )
        }),
    )?;
    for (index, block) in content.iter().enumerate() {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                push_sse_event(
                    &mut output,
                    "content_block_start",
                    json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {"type": "text", "text": ""}
                    }),
                )?;
                if !text.is_empty() {
                    push_sse_event(
                        &mut output,
                        "content_block_delta",
                        json!({
                            "type": "content_block_delta",
                            "index": index,
                            "delta": {"type": "text_delta", "text": text}
                        }),
                    )?;
                }
                push_sse_event(
                    &mut output,
                    "content_block_stop",
                    json!({"type": "content_block_stop", "index": index}),
                )?;
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_ai_switch");
                let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                let partial_json = serde_json::to_string(&input)
                    .map_err(|error| format!("Could not serialize tool input: {error}"))?;
                push_sse_event(
                    &mut output,
                    "content_block_start",
                    json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}
                    }),
                )?;
                push_sse_event(
                    &mut output,
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": index,
                        "delta": {"type": "input_json_delta", "partial_json": partial_json}
                    }),
                )?;
                push_sse_event(
                    &mut output,
                    "content_block_stop",
                    json!({"type": "content_block_stop", "index": index}),
                )?;
            }
            Some(other) => return Err(format!("Unsupported Anthropic SSE content type: {other}")),
            None => {}
        }
    }
    push_sse_event(
        &mut output,
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": {"stop_reason": stop_reason, "stop_sequence": Value::Null},
            "usage": {"output_tokens": output_tokens}
        }),
    )?;
    push_sse_event(&mut output, "message_stop", json!({"type": "message_stop"}))?;
    Ok(output)
}

fn anthropic_text(value: &Value, label: &str) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => parts
            .iter()
            .map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| format!("Anthropic {label} entries must contain text"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.join("\n")),
        Value::Null => Ok(String::new()),
        _ => Err(format!("Anthropic {label} must be text")),
    }
}

fn content_blocks(value: &Value) -> Result<Vec<Value>, String> {
    match value {
        Value::String(text) => Ok(vec![json!({"type": "text", "text": text})]),
        Value::Array(items) => Ok(items.clone()),
        Value::Null => Ok(Vec::new()),
        _ => Err("Anthropic content must be a string or array".to_string()),
    }
}

fn parse_json_or_string(value: &str) -> Value {
    serde_json::from_str::<Value>(value).unwrap_or_else(|_| Value::String(value.to_string()))
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
        .ok_or_else(|| format!("Anthropic {label} is missing {key}"))
}

fn copy_fields(source: &Map<String, Value>, target: &mut Map<String, Value>, fields: &[&str]) {
    for field in fields {
        if let Some(value) = source.get(*field) {
            target.insert((*field).to_string(), value.clone());
        }
    }
}

fn push_sse_event(output: &mut String, event: &str, value: Value) -> Result<(), String> {
    output.push_str("event: ");
    output.push_str(event);
    output.push('\n');
    output.push_str("data: ");
    output.push_str(
        &serde_json::to_string(&value)
            .map_err(|error| format!("Could not serialize Anthropic SSE event: {error}"))?,
    );
    output.push_str("\n\n");
    Ok(())
}

fn looks_like_sse(body: &[u8]) -> bool {
    std::str::from_utf8(body).ok().is_some_and(|text| {
        text.lines()
            .any(|line| line.trim_start().starts_with("data:"))
    })
}

#[cfg(test)]
mod tests {
    use super::{anthropic_request_to_responses, responses_response_to_anthropic};
    use serde_json::{json, Value};

    #[test]
    fn converts_anthropic_request_to_responses() {
        let body = json!({
            "model": "gpt-5.5",
            "system": "Be concise",
            "messages": [{"role":"user","content":[{"type":"text","text":"Find x"}]}],
            "max_tokens": 64,
            "tools": [{"name":"lookup","input_schema":{"type":"object","properties":{}}}]
        });

        let converted: Value = serde_json::from_slice(
            &anthropic_request_to_responses(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(converted["input"][0]["role"], "user");
        assert_eq!(converted["instructions"], "Be concise");
        assert_eq!(converted["max_output_tokens"], 64);
        assert_eq!(converted["tools"][0]["type"], "function");
    }

    #[test]
    fn converts_responses_response_to_anthropic_json() {
        let upstream = json!({
            "id": "resp_1",
            "model": "gpt-5.5",
            "status": "completed",
            "output": [
                {"type":"message","role":"assistant","content":[{"type":"output_text","text":"hello"}]},
                {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{\"key\":\"x\"}"}
            ],
            "usage": {"input_tokens":3,"output_tokens":5}
        });

        let converted = responses_response_to_anthropic(
            200,
            Some("application/json"),
            serde_json::to_vec(&upstream).unwrap().as_slice(),
        )
        .unwrap();
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(output["content"][0]["type"], "text");
        assert_eq!(output["content"][1]["type"], "tool_use");
        assert_eq!(output["stop_reason"], "tool_use");
    }

    /// Claude Code replays thinking blocks in assistant history whenever extended
    /// thinking is on, so erroring here wedges every session after turn one.
    #[test]
    fn drops_replayed_thinking_blocks() {
        let body = json!({
            "model": "gpt-5",
            "max_tokens": 128,
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Find x"}]},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "Let me look.", "signature": "sig"},
                    {"type": "redacted_thinking", "data": "opaque"},
                    {"type": "text", "text": "Found it."}
                ]},
                {"role": "user", "content": [{"type": "text", "text": "thanks"}]}
            ]
        });

        let converted: Value = serde_json::from_slice(
            &anthropic_request_to_responses(&serde_json::to_vec(&body).unwrap())
                .expect("replayed thinking must not fail the request"),
        )
        .unwrap();

        let rendered = serde_json::to_string(&converted).unwrap();
        assert!(
            !rendered.contains("Let me look."),
            "reasoning must not be forwarded as input: {rendered}"
        );
        assert!(rendered.contains("Found it."));
    }
}
