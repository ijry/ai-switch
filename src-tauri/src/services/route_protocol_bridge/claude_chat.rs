use super::{sse, TransformedBridgeResponse};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Non-empty stand-in for `reasoning_content` on a tool-call assistant message
/// when the real reasoning is unavailable. See [`convert_assistant_message`];
/// DeepSeek/MiMo require the field to be a non-empty string, not verbatim CoT.
const TOOL_CALL_REASONING_PLACEHOLDER: &str = "...";

pub(super) fn anthropic_request_to_chat(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Anthropic request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Anthropic request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    if let Some(model) = object.get("model") {
        result.insert("model".to_string(), model.clone());
    }

    let mut messages = Vec::new();
    if let Some(system) = object.get("system") {
        let content = anthropic_text(system, "system")?;
        if !content.is_empty() {
            messages.push(json!({"role": "system", "content": content}));
        }
    }
    if let Some(items) = object.get("messages").and_then(Value::as_array) {
        for item in items {
            messages.extend(convert_message(item)?);
        }
    }
    result.insert("messages".to_string(), Value::Array(messages));

    if let Some(max_tokens) = object.get("max_tokens") {
        result.insert("max_tokens".to_string(), max_tokens.clone());
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
        .map_err(|error| format!("Could not serialize Chat request: {error}"))
}

pub(super) fn chat_response_to_anthropic(
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
            body: chat_sse_to_anthropic(body)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }

    Ok(TransformedBridgeResponse {
        body: chat_json_to_anthropic(body)?,
        content_type: Some("application/json".to_string()),
    })
}

fn convert_message(message: &Value) -> Result<Vec<Value>, String> {
    let object = message
        .as_object()
        .ok_or_else(|| "Anthropic messages entries must be objects".to_string())?;
    let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
    let blocks = content_blocks(object.get("content").unwrap_or(&Value::Null))?;

    match role {
        "user" => convert_user_message(&blocks),
        "assistant" => convert_assistant_message(&blocks),
        other => Err(format!("Unsupported Anthropic message role: {other}")),
    }
}

fn convert_user_message(blocks: &[Value]) -> Result<Vec<Value>, String> {
    let mut chat_parts = Vec::new();
    let mut tool_messages = Vec::new();
    let mut has_non_text = false;

    for block in blocks {
        let object = block
            .as_object()
            .ok_or_else(|| "Anthropic content blocks must be objects".to_string())?;
        match object.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = object.get("text").and_then(Value::as_str).unwrap_or("");
                chat_parts.push(json!({"type": "text", "text": text}));
            }
            Some("image") => {
                has_non_text = true;
                chat_parts.push(convert_image_block(object)?);
            }
            Some("tool_result") => {
                let call_id = required_string(object, "tool_use_id", "tool_result")?;
                let content = object
                    .get("content")
                    .map(stringify_tool_result_content)
                    .transpose()?
                    .unwrap_or_default();
                tool_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": content
                }));
            }
            Some(other) => return Err(format!("Unsupported Anthropic user content type: {other}")),
            None => return Err("Anthropic content block is missing type".to_string()),
        }
    }

    let mut messages = Vec::new();
    if !chat_parts.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": chat_content(chat_parts, has_non_text)
        }));
    }
    messages.extend(tool_messages);
    Ok(messages)
}

fn convert_assistant_message(blocks: &[Value]) -> Result<Vec<Value>, String> {
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut tool_calls = Vec::new();

    for block in blocks {
        let object = block
            .as_object()
            .ok_or_else(|| "Anthropic content blocks must be objects".to_string())?;
        match object.get("type").and_then(Value::as_str) {
            Some("text") => {
                text.push_str(object.get("text").and_then(Value::as_str).unwrap_or(""));
            }
            Some("thinking") => {
                reasoning.push_str(object.get("thinking").and_then(Value::as_str).unwrap_or(""));
            }
            // Opaque provider-signed reasoning: no plaintext to forward, but the
            // turn still needs reasoning_content present when it carries tools.
            Some("redacted_thinking") => {}
            Some("tool_use") => {
                let id = required_string(object, "id", "tool_use")?;
                let name = required_string(object, "name", "tool_use")?;
                let arguments = serde_json::to_string(
                    object.get("input").unwrap_or(&Value::Object(Map::new())),
                )
                .map_err(|error| format!("Could not serialize Anthropic tool input: {error}"))?;
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {"name": name, "arguments": arguments}
                }));
            }
            Some(other) => {
                return Err(format!(
                    "Unsupported Anthropic assistant content type: {other}"
                ));
            }
            None => return Err("Anthropic content block is missing type".to_string()),
        }
    }

    let mut message = Map::new();
    message.insert("role".to_string(), Value::String("assistant".to_string()));
    if text.is_empty() && !tool_calls.is_empty() {
        message.insert("content".to_string(), Value::Null);
    } else {
        message.insert("content".to_string(), Value::String(text));
    }
    if !tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(tool_calls.clone()));
    }
    // DeepSeek/MiMo reject a follow-up turn whose tool-call assistant message has
    // no reasoning_content (400). Forward the real reasoning when present, else a
    // placeholder so the conversation survives lost/absent chain-of-thought.
    if !reasoning.trim().is_empty() {
        message.insert("reasoning_content".to_string(), Value::String(reasoning));
    } else if !tool_calls.is_empty() {
        message.insert(
            "reasoning_content".to_string(),
            Value::String(TOOL_CALL_REASONING_PLACEHOLDER.to_string()),
        );
    }
    Ok(vec![Value::Object(message)])
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
        "type": "image_url",
        "image_url": {"url": format!("data:{media_type};base64,{data}")}
    }))
}

fn chat_content(parts: Vec<Value>, has_non_text: bool) -> Value {
    if !has_non_text && parts.len() == 1 {
        return parts[0]
            .get("text")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));
    }
    Value::Array(parts)
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
        converted.push(json!({"type": "function", "function": function}));
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
            Ok(json!({"type": "function", "function": {"name": name}}))
        }
        Some("none") => Ok(Value::String("none".to_string())),
        Some(other) => Err(format!("Unsupported Anthropic tool_choice type: {other}")),
        None => Ok(Value::String("auto".to_string())),
    }
}

fn chat_json_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Chat response JSON is invalid: {error}"))?;
    let response_id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("msg_ai_switch");
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| "Chat response is missing choices[0]".to_string())?;
    let message = choice
        .get("message")
        .ok_or_else(|| "Chat response is missing choices[0].message".to_string())?;
    let content = chat_message_content_to_anthropic(message)?;
    let finish_reason = choice.get("finish_reason").and_then(Value::as_str);
    let stop_reason = if content
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
    {
        "tool_use"
    } else {
        chat_stop_reason(finish_reason)
    };
    let response = anthropic_message(
        response_id,
        model,
        content,
        stop_reason,
        chat_usage_to_anthropic(value.get("usage")),
    );
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Anthropic response: {error}"))
}

fn chat_message_content_to_anthropic(message: &Value) -> Result<Vec<Value>, String> {
    let mut content = Vec::new();
    let text = chat_message_text(message)?;
    if !text.is_empty() {
        content.push(json!({"type": "text", "text": text}));
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for tool_call in tool_calls {
            let call_id = tool_call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("call_ai_switch");
            let function = tool_call
                .get("function")
                .and_then(Value::as_object)
                .ok_or_else(|| "Chat tool call is missing function".to_string())?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let arguments = function
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
    }
    Ok(content)
}

fn chat_sse_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut aggregate = ChatStreamAggregate::default();
    for value in sse::parse_sse_data_records(body)? {
        aggregate.capture(&value);
        let Some(choice) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            continue;
        };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            aggregate.finish_reason = Some(reason.to_string());
        }
        let delta = choice.get("delta").unwrap_or(&Value::Null);
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            aggregate.text.push_str(text);
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for tool_call in tool_calls {
                aggregate.capture_tool_delta(tool_call);
            }
        }
    }

    let mut content = Vec::new();
    if !aggregate.text.is_empty() {
        content.push(json!({"type": "text", "text": aggregate.text}));
    }
    for tool in aggregate.tools.values() {
        content.push(json!({
            "type": "tool_use",
            "id": if tool.id.is_empty() { "call_ai_switch" } else { &tool.id },
            "name": if tool.name.is_empty() { "tool" } else { &tool.name },
            "input": parse_json_or_string(&tool.arguments)
        }));
    }
    let stop_reason = if content
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
    {
        "tool_use"
    } else {
        chat_stop_reason(aggregate.finish_reason.as_deref())
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
struct ChatStreamAggregate {
    response_id: String,
    model: String,
    text: String,
    tools: BTreeMap<usize, StreamToolCall>,
    finish_reason: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
}

impl ChatStreamAggregate {
    fn capture(&mut self, value: &Value) {
        if self.response_id.is_empty() {
            self.response_id = value
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("msg_ai_switch")
                .to_string();
        }
        if self.model.is_empty() {
            self.model = value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
        }
        if let Some(usage) = value.get("usage") {
            self.input_tokens = usage
                .get("prompt_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(self.input_tokens);
            self.output_tokens = usage
                .get("completion_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(self.output_tokens);
        }
    }

    fn capture_tool_delta(&mut self, value: &Value) {
        let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let tool = self.tools.entry(index).or_default();
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            tool.id = id.to_string();
        }
        if let Some(function) = value.get("function").and_then(Value::as_object) {
            if let Some(name) = function.get("name").and_then(Value::as_str) {
                tool.name.push_str(name);
            }
            if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                tool.arguments.push_str(arguments);
            }
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
    id: String,
    name: String,
    arguments: String,
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
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("text");
        match block_type {
            "text" => {
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
            "tool_use" => {
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
            other => return Err(format!("Unsupported Anthropic SSE content type: {other}")),
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

fn chat_message_text(message: &Value) -> Result<String, String> {
    match message.get("content") {
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(parts)) => Ok(parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.get("content").and_then(Value::as_str))
            })
            .collect::<Vec<_>>()
            .join("")),
        Some(Value::Null) | None => Ok(String::new()),
        Some(_) => Err("Chat message content has an unsupported shape".to_string()),
    }
}

fn chat_usage_to_anthropic(usage: Option<&Value>) -> Value {
    let Some(usage) = usage else {
        return json!({"input_tokens": 0, "output_tokens": 0});
    };
    json!({
        "input_tokens": usage.get("prompt_tokens").and_then(Value::as_i64).unwrap_or(0),
        "output_tokens": usage.get("completion_tokens").and_then(Value::as_i64).unwrap_or(0)
    })
}

fn chat_stop_reason(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("tool_calls") => "tool_use",
        Some("length") => "max_tokens",
        Some("content_filter") => "stop_sequence",
        _ => "end_turn",
    }
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

fn stringify_tool_result_content(value: &Value) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => Ok(parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("")),
        Value::Null => Ok(String::new()),
        _ => serde_json::to_string(value)
            .map_err(|error| format!("Could not serialize tool result content: {error}")),
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
    use super::{anthropic_request_to_chat, chat_response_to_anthropic};
    use serde_json::{json, Value};

    #[test]
    fn converts_anthropic_request_to_chat() {
        let body = json!({
            "model": "gpt-5.5",
            "system": "Be concise",
            "messages": [
                {"role":"user","content":[{"type":"text","text":"Find x"}]},
                {"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"lookup","input":{"key":"x"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"42"}]}
            ],
            "max_tokens": 64,
            "tools": [{"name":"lookup","input_schema":{"type":"object","properties":{}}}]
        });

        let converted: Value = serde_json::from_slice(
            &anthropic_request_to_chat(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(converted["messages"][0]["role"], "system");
        assert_eq!(converted["messages"][1]["role"], "user");
        assert_eq!(converted["messages"][2]["tool_calls"][0]["id"], "toolu_1");
        assert_eq!(converted["messages"][3]["role"], "tool");
        assert_eq!(converted["max_tokens"], 64);
        assert_eq!(converted["tools"][0]["function"]["name"], "lookup");
    }

    #[test]
    fn maps_thinking_block_to_reasoning_content() {
        let body = json!({
            "model": "mimo-v2.5-pro",
            "messages": [
                {"role":"user","content":"go"},
                {"role":"assistant","content":[
                    {"type":"thinking","thinking":"Need to look it up."},
                    {"type":"tool_use","id":"toolu_1","name":"lookup","input":{"q":"x"}}
                ]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"42"}]}
            ]
        });
        let converted: Value = serde_json::from_slice(
            &anthropic_request_to_chat(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(converted["messages"][1]["role"], "assistant");
        assert_eq!(converted["messages"][1]["reasoning_content"], "Need to look it up.");
        assert_eq!(converted["messages"][1]["tool_calls"][0]["id"], "toolu_1");
        assert_eq!(converted["messages"][2]["role"], "tool");
    }

    #[test]
    fn injects_placeholder_reasoning_for_tool_call_without_thinking() {
        let body = json!({
            "model": "mimo-v2.5-pro",
            "messages": [
                {"role":"user","content":"go"},
                {"role":"assistant","content":[
                    {"type":"tool_use","id":"toolu_1","name":"lookup","input":{"q":"x"}}
                ]}
            ]
        });
        let converted: Value = serde_json::from_slice(
            &anthropic_request_to_chat(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        let reasoning = converted["messages"][1]["reasoning_content"].as_str();
        assert!(reasoning.is_some_and(|text| !text.trim().is_empty()));
    }

    #[test]
    fn converts_chat_response_to_anthropic_json() {
        let upstream = json!({
            "id": "chatcmpl_1",
            "model": "gpt-5.5",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "hello",
                    "tool_calls": [{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"key\":\"x\"}"}}]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens":3,"completion_tokens":5}
        });

        let converted = chat_response_to_anthropic(
            200,
            Some("application/json"),
            serde_json::to_vec(&upstream).unwrap().as_slice(),
        )
        .unwrap();
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(output["type"], "message");
        assert_eq!(output["content"][0]["type"], "text");
        assert_eq!(output["content"][1]["type"], "tool_use");
        assert_eq!(output["stop_reason"], "tool_use");
        assert_eq!(output["usage"]["input_tokens"], 3);
        assert_eq!(output["usage"]["output_tokens"], 5);
    }
}
