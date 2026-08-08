use super::{sse, TransformedBridgeResponse};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(super) fn anthropic_request_to_gemini(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Anthropic request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Anthropic request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    if let Some(system) = object.get("system") {
        let system = anthropic_text(system, "system")?;
        if !system.is_empty() {
            result.insert(
                "systemInstruction".to_string(),
                json!({"parts": [{"text": system}]}),
            );
        }
    }
    if let Some(messages) = object.get("messages") {
        result.insert("contents".to_string(), convert_messages(messages)?);
    }
    let generation_config = convert_generation_config(object);
    if !generation_config.is_empty() {
        result.insert(
            "generationConfig".to_string(),
            Value::Object(generation_config),
        );
    }
    if let Some(tools) = object.get("tools") {
        result.insert("tools".to_string(), convert_tools(tools)?);
    }

    serde_json::to_vec(&Value::Object(result))
        .map_err(|error| format!("Could not serialize Gemini request: {error}"))
}

pub(super) fn gemini_response_to_anthropic(
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
            body: gemini_sse_to_anthropic(body)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }

    Ok(TransformedBridgeResponse {
        body: gemini_json_to_anthropic(body)?,
        content_type: Some("application/json".to_string()),
    })
}

fn convert_messages(messages: &Value) -> Result<Value, String> {
    let messages = messages
        .as_array()
        .ok_or_else(|| "Anthropic messages must be an array".to_string())?;
    let mut contents = Vec::with_capacity(messages.len());
    for message in messages {
        let object = message
            .as_object()
            .ok_or_else(|| "Anthropic messages entries must be objects".to_string())?;
        let role = match object.get("role").and_then(Value::as_str).unwrap_or("user") {
            "assistant" => "model",
            "user" => "user",
            other => return Err(format!("Unsupported Anthropic message role: {other}")),
        };
        let parts = convert_content_blocks(object.get("content").unwrap_or(&Value::Null))?;
        contents.push(json!({"role": role, "parts": parts}));
    }
    Ok(Value::Array(contents))
}

fn convert_content_blocks(content: &Value) -> Result<Vec<Value>, String> {
    let blocks = content_blocks(content)?;
    let mut parts = Vec::with_capacity(blocks.len());
    for block in blocks {
        let object = block
            .as_object()
            .ok_or_else(|| "Anthropic content blocks must be objects".to_string())?;
        match object.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = object.get("text").and_then(Value::as_str).unwrap_or("");
                parts.push(json!({"text": text}));
            }
            Some("image") => parts.push(convert_image_block(object)?),
            Some("tool_use") => {
                let name = required_string(object, "name", "tool_use")?;
                let input = object.get("input").cloned().unwrap_or_else(|| json!({}));
                parts.push(json!({"functionCall": {"name": name, "args": input}}));
            }
            Some("tool_result") => {
                let name = required_string(object, "tool_use_id", "tool_result")?;
                let output = object
                    .get("content")
                    .map(stringify_tool_result_content)
                    .transpose()?
                    .unwrap_or_default();
                parts.push(json!({
                    "functionResponse": {
                        "name": name,
                        "response": {"output": output}
                    }
                }));
            }
            Some(other) => return Err(format!("Unsupported Anthropic content type: {other}")),
            None => return Err("Anthropic content block is missing type".to_string()),
        }
    }
    Ok(parts)
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
    Ok(json!({
        "inlineData": {
            "mimeType": required_string(source, "media_type", "image source")?,
            "data": required_string(source, "data", "image source")?
        }
    }))
}

fn convert_generation_config(object: &Map<String, Value>) -> Map<String, Value> {
    let mut generation_config = Map::new();
    if let Some(max_tokens) = object.get("max_tokens") {
        generation_config.insert("maxOutputTokens".to_string(), max_tokens.clone());
    }
    if let Some(temperature) = object.get("temperature") {
        generation_config.insert("temperature".to_string(), temperature.clone());
    }
    if let Some(top_p) = object.get("top_p") {
        generation_config.insert("topP".to_string(), top_p.clone());
    }
    if let Some(stop) = object.get("stop_sequences") {
        generation_config.insert("stopSequences".to_string(), stop.clone());
    }
    generation_config
}

fn convert_tools(tools: &Value) -> Result<Value, String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| "Anthropic tools must be an array".to_string())?;
    let mut declarations = Vec::with_capacity(tools.len());
    for tool in tools {
        let object = tool
            .as_object()
            .ok_or_else(|| "Anthropic tool entries must be objects".to_string())?;
        let name = required_string(object, "name", "tool")?;
        let mut declaration = Map::new();
        declaration.insert("name".to_string(), Value::String(name.to_string()));
        if let Some(description) = object.get("description") {
            declaration.insert("description".to_string(), description.clone());
        }
        declaration.insert(
            "parameters".to_string(),
            object
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
        );
        declarations.push(Value::Object(declaration));
    }
    Ok(json!([{"functionDeclarations": declarations}]))
}

fn gemini_json_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Gemini response JSON is invalid: {error}"))?;
    let response_id = value
        .get("responseId")
        .and_then(Value::as_str)
        .unwrap_or("msg_ai_switch");
    let model = value
        .get("modelVersion")
        .and_then(Value::as_str)
        .or_else(|| value.get("model").and_then(Value::as_str))
        .unwrap_or("unknown");
    let candidate = value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| "Gemini response is missing candidates[0]".to_string())?;
    let parts = candidate
        .get("content")
        .and_then(Value::as_object)
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .ok_or_else(|| "Gemini response is missing content.parts".to_string())?;
    let content = gemini_parts_to_anthropic_content(parts)?;
    let finish_reason = candidate.get("finishReason").and_then(Value::as_str);
    let stop_reason = gemini_stop_reason(finish_reason, &content);
    let response = anthropic_message(
        response_id,
        model,
        content,
        stop_reason,
        gemini_usage_to_anthropic(value.get("usageMetadata")),
    );
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Anthropic response: {error}"))
}

fn gemini_sse_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut aggregate = GeminiStreamAggregate::default();
    for value in sse::parse_sse_data_records(body)? {
        aggregate.capture_envelope(&value);
        let Some(candidate) = value
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
        else {
            continue;
        };
        if let Some(reason) = candidate.get("finishReason").and_then(Value::as_str) {
            aggregate.finish_reason = Some(reason.to_string());
        }
        if let Some(parts) = candidate
            .get("content")
            .and_then(Value::as_object)
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
        {
            aggregate.capture_parts(parts);
        }
    }

    let mut content = Vec::new();
    if !aggregate.text.is_empty() {
        content.push(json!({"type": "text", "text": aggregate.text}));
    }
    for tool in aggregate.tools.values() {
        content.push(json!({
            "type": "tool_use",
            "id": tool.id.as_str(),
            "name": tool.name.as_str(),
            "input": tool.input.clone()
        }));
    }
    let stop_reason = gemini_stop_reason(aggregate.finish_reason.as_deref(), &content);
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
struct GeminiStreamAggregate {
    response_id: String,
    model: String,
    text: String,
    tools: BTreeMap<usize, StreamToolCall>,
    finish_reason: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
}

impl GeminiStreamAggregate {
    fn capture_envelope(&mut self, value: &Value) {
        if self.response_id.is_empty() {
            self.response_id = value
                .get("responseId")
                .and_then(Value::as_str)
                .unwrap_or("msg_ai_switch")
                .to_string();
        }
        if self.model.is_empty() {
            self.model = value
                .get("modelVersion")
                .and_then(Value::as_str)
                .or_else(|| value.get("model").and_then(Value::as_str))
                .unwrap_or("unknown")
                .to_string();
        }
        if let Some(usage) = value.get("usageMetadata") {
            self.input_tokens = usage
                .get("promptTokenCount")
                .and_then(Value::as_i64)
                .unwrap_or(self.input_tokens);
            self.output_tokens = usage
                .get("candidatesTokenCount")
                .and_then(Value::as_i64)
                .unwrap_or(self.output_tokens);
        }
    }

    fn capture_parts(&mut self, parts: &[Value]) {
        for part in parts {
            let Some(object) = part.as_object() else {
                continue;
            };
            if let Some(text) = object.get("text").and_then(Value::as_str) {
                self.text.push_str(text);
            }
            if let Some(function_call) = object.get("functionCall").and_then(Value::as_object) {
                let index = self.tools.len();
                let name = function_call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                let input = function_call
                    .get("args")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                self.tools.insert(
                    index,
                    StreamToolCall {
                        id: format!("{name}_{index}"),
                        name: name.to_string(),
                        input,
                    },
                );
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

#[derive(Debug)]
struct StreamToolCall {
    id: String,
    name: String,
    input: Value,
}

fn gemini_parts_to_anthropic_content(parts: &[Value]) -> Result<Vec<Value>, String> {
    let mut content = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let object = part
            .as_object()
            .ok_or_else(|| "Gemini content parts must be objects".to_string())?;
        if let Some(text) = object.get("text").and_then(Value::as_str) {
            content.push(json!({"type": "text", "text": text}));
            continue;
        }
        if let Some(function_call) = object.get("functionCall").and_then(Value::as_object) {
            let name = function_call
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            content.push(json!({
                "type": "tool_use",
                "id": format!("{name}_{index}"),
                "name": name,
                "input": function_call.get("args").cloned().unwrap_or_else(|| json!({}))
            }));
        }
    }
    Ok(content)
}

fn gemini_stop_reason(finish_reason: Option<&str>, content: &[Value]) -> &'static str {
    if content
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
    {
        return "tool_use";
    }
    match finish_reason {
        Some("MAX_TOKENS") => "max_tokens",
        Some("SAFETY" | "RECITATION") => "stop_sequence",
        _ => "end_turn",
    }
}

fn gemini_usage_to_anthropic(usage: Option<&Value>) -> Value {
    let Some(usage) = usage else {
        return json!({"input_tokens": 0, "output_tokens": 0});
    };
    json!({
        "input_tokens": usage.get("promptTokenCount").and_then(Value::as_i64).unwrap_or(0),
        "output_tokens": usage.get("candidatesTokenCount").and_then(Value::as_i64).unwrap_or(0)
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
    use super::{anthropic_request_to_gemini, gemini_response_to_anthropic};
    use serde_json::{json, Value};

    #[test]
    fn converts_anthropic_request_to_gemini() {
        let body = json!({
            "model": "gemini-2.5-flash",
            "system": "Be concise",
            "messages": [{"role":"user","content":[{"type":"text","text":"Find x"}]}],
            "max_tokens": 64,
            "tools": [{"name":"lookup","input_schema":{"type":"object","properties":{}}}]
        });

        let converted: Value = serde_json::from_slice(
            &anthropic_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(
            converted["systemInstruction"]["parts"][0]["text"],
            "Be concise"
        );
        assert_eq!(converted["contents"][0]["role"], "user");
        assert_eq!(converted["contents"][0]["parts"][0]["text"], "Find x");
        assert_eq!(
            converted["tools"][0]["functionDeclarations"][0]["name"],
            "lookup"
        );
    }

    #[test]
    fn converts_gemini_response_to_anthropic_json() {
        let upstream = json!({
            "responseId": "resp_1",
            "modelVersion": "gemini-2.5-flash",
            "candidates": [{
                "content": {"role":"model","parts":[{"text":"hello"}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount":3,"candidatesTokenCount":5}
        });

        let converted = gemini_response_to_anthropic(
            200,
            Some("application/json"),
            serde_json::to_vec(&upstream).unwrap().as_slice(),
        )
        .unwrap();
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(output["type"], "message");
        assert_eq!(output["content"][0]["type"], "text");
        assert_eq!(output["usage"]["input_tokens"], 3);
        assert_eq!(output["usage"]["output_tokens"], 5);
    }
}
