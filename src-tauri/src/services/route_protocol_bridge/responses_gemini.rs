#[cfg(test)]
mod tests {
    use super::{gemini_response_to_responses, responses_request_to_gemini};
    use serde_json::Value;

    #[test]
    fn converts_responses_request_to_gemini_generate_content() {
        let body = serde_json::json!({
            "model": "gemini-2.5-flash",
            "instructions": "Be concise",
            "input": [{"role":"user","content":[{"type":"input_text","text":"hello"}]}],
            "max_output_tokens": 32,
            "temperature": 0,
            "tools": [{"type":"function","name":"lookup","parameters":{"type":"object","properties":{}}}]
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(
            converted["systemInstruction"]["parts"][0]["text"],
            "Be concise"
        );
        assert_eq!(converted["contents"][0]["role"], "user");
        assert_eq!(converted["contents"][0]["parts"][0]["text"], "hello");
        assert_eq!(converted["generationConfig"]["maxOutputTokens"], 32);
        assert_eq!(
            converted["tools"][0]["functionDeclarations"][0]["name"],
            "lookup"
        );
    }

    #[test]
    fn converts_responses_request_input_image_and_function_result_to_gemini() {
        let body = serde_json::json!({
            "model": "gemini-2.5-flash",
            "input": [
                {
                    "role": "user",
                    "content": [
                        {"type": "input_image", "image_url": "data:image/png;base64,aGVsbG8="}
                    ]
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "lookup",
                    "arguments": "{\"key\":\"x\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "42"
                }
            ]
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(
            converted["contents"][0]["parts"][0]["inlineData"]["mimeType"],
            "image/png"
        );
        assert_eq!(
            converted["contents"][0]["parts"][0]["inlineData"]["data"],
            "aGVsbG8="
        );
        assert_eq!(
            converted["contents"][1]["parts"][0]["functionCall"]["name"],
            "lookup"
        );
        assert_eq!(
            converted["contents"][2]["parts"][0]["functionResponse"]["name"],
            "lookup"
        );
    }

    #[test]
    fn converts_gemini_sse_to_responses_events() {
        let body = concat!(
            "data: {\"responseId\":\"resp_1\",\"model\":\"gemini-2.5-flash\",\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"hel\"}]},\"finishReason\":null}],\"usageMetadata\":{\"promptTokenCount\":3,\"candidatesTokenCount\":1,\"totalTokenCount\":4}}\n\n",
            "data: {\"responseId\":\"resp_1\",\"model\":\"gemini-2.5-flash\",\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"lo\"},{\"functionCall\":{\"name\":\"lookup\",\"args\":{\"key\":\"x\"}}}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":3,\"candidatesTokenCount\":5,\"totalTokenCount\":8}}\n\n",
            "data: [DONE]\n\n"
        );

        let converted = gemini_response_to_responses(
            200,
            Some("text/event-stream"),
            body.as_bytes(),
        )
        .unwrap();
        let output = String::from_utf8(converted.body).unwrap();

        assert!(output.contains("event: response.created"));
        assert!(output.contains("event: response.output_text.delta"));
        assert!(output.contains("event: response.function_call_arguments.delta"));
        assert!(output.contains("event: response.completed"));
    }
}

use super::{common::parse_base64_data_url, sse, TransformedBridgeResponse};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(super) fn responses_request_to_gemini(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Responses request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Responses request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    if let Some(instructions) = object.get("instructions") {
        let system = text_value(instructions, "instructions")?;
        if !system.is_empty() {
            result.insert(
                "systemInstruction".to_string(),
                json!({"parts": [{"text": system}]}),
            );
        }
    }
    if let Some(input) = object.get("input") {
        result.insert("contents".to_string(), convert_input(input)?);
    }
    if let Some(max_tokens) = object.get("max_output_tokens") {
        result.insert(
            "generationConfig".to_string(),
            json!({"maxOutputTokens": max_tokens}),
        );
    }
    if let Some(temperature) = object.get("temperature") {
        result
            .entry("generationConfig".to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| "generationConfig must be an object".to_string())?
            .insert("temperature".to_string(), temperature.clone());
    }
    if let Some(tools) = object.get("tools") {
        result.insert("tools".to_string(), convert_tools(tools)?);
    }

    serde_json::to_vec(&Value::Object(result))
        .map_err(|error| format!("Could not serialize Gemini request: {error}"))
}

pub(super) fn gemini_response_to_responses(
    _status: u16,
    content_type: Option<&str>,
    body: &[u8],
) -> Result<TransformedBridgeResponse, String> {
    if !(200..300).contains(&_status) {
        return Ok(TransformedBridgeResponse {
            body: body.to_vec(),
            content_type: content_type.map(str::to_string),
        });
    }
    if content_type.is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
        || looks_like_sse(body)
    {
        return Ok(TransformedBridgeResponse {
            body: gemini_sse_to_responses(body)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }
    Ok(TransformedBridgeResponse {
        body: gemini_json_to_responses(body)?,
        content_type: Some("application/json".to_string()),
    })
}

fn gemini_json_to_responses(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Gemini response JSON is invalid: {error}"))?;
    let response_id = value
        .get("responseId")
        .and_then(Value::as_str)
        .unwrap_or("resp_ai_switch");
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
    let (output, text) = gemini_parts_to_responses_output(response_id, parts)?;
    let finish_reason = candidate.get("finishReason").and_then(Value::as_str);
    let response = json!({
        "id": response_id,
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": responses_status(finish_reason),
        "model": model,
        "output": output,
        "output_text": text,
        "error": Value::Null,
        "incomplete_details": incomplete_details(finish_reason),
        "usage": gemini_usage_to_responses(value.get("usageMetadata")),
    });
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Responses response: {error}"))
}

fn gemini_sse_to_responses(body: &[u8]) -> Result<Vec<u8>, String> {
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
        if let Some(finish_reason) = candidate.get("finishReason").and_then(Value::as_str) {
            aggregate.finish_reason = Some(finish_reason.to_string());
        }
        if let Some(parts) = candidate
            .get("content")
            .and_then(Value::as_object)
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
        {
            aggregate.capture_parts(parts)?;
        }
    }

    let gemini_response = aggregate.into_gemini_response();
    let responses_json = gemini_json_to_responses(
        serde_json::to_vec(&gemini_response)
            .map_err(|error| format!("Could not serialize buffered Gemini response: {error}"))?
            .as_slice(),
    )?;
    let response = serde_json::from_slice::<Value>(&responses_json)
        .map_err(|error| format!("Could not parse buffered Responses JSON: {error}"))?;
    sse::responses_events_from_completed_response(&response)
}

fn convert_input(input: &Value) -> Result<Value, String> {
    match input {
        Value::String(text) => Ok(Value::Array(vec![json!({
            "role": "user",
            "parts": [{"text": text}]
        })])),
        Value::Array(items) => {
            let mut state = GeminiInputState::default();
            let mut contents = Vec::new();
            for item in items {
                let object = item
                    .as_object()
                    .ok_or_else(|| "Responses input items must be JSON objects".to_string())?;
                match object.get("type").and_then(Value::as_str) {
                    Some("message") | None if object.contains_key("role") => {
                        contents.push(convert_message(object)?);
                    }
                    Some("function_call") => {
                        contents.push(convert_function_call(object, &mut state)?);
                    }
                    Some("function_call_output") => {
                        contents.push(convert_function_result(object, &state)?);
                    }
                    Some(other) => {
                        return Err(format!("Unsupported Responses input item type: {other}"));
                    }
                    None => return Err("Responses input item is missing role or type".to_string()),
                }
            }
            Ok(Value::Array(contents))
        }
        Value::Null => Ok(Value::Array(Vec::new())),
        _ => Err("Responses input must be a string or array".to_string()),
    }
}

fn convert_message(object: &Map<String, Value>) -> Result<Value, String> {
    let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
    let parts = object
        .get("content")
        .map(convert_message_content)
        .transpose()?
        .unwrap_or_default();
    Ok(json!({"role": role, "parts": parts}))
}

fn convert_message_content(content: &Value) -> Result<Vec<Value>, String> {
    match content {
        Value::String(text) => Ok(vec![json!({"text": text})]),
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
            Ok(json!({"text": text}))
        }
        Some("input_image") => {
            let image_url = required_string(object, "image_url", "input_image")?;
            let Some((mime_type, data)) = parse_base64_data_url(image_url) else {
                return Err("Gemini bridge only supports base64 data URL images".to_string());
            };
            Ok(json!({
                "inlineData": {
                    "mimeType": mime_type,
                    "data": data
                }
            }))
        }
        Some(other) => Err(format!("Unsupported Responses content type: {other}")),
        None => Err("Responses content part is missing type".to_string()),
    }
}

fn convert_function_call(
    object: &Map<String, Value>,
    state: &mut GeminiInputState,
) -> Result<Value, String> {
    let call_id = required_string(object, "call_id", "function_call")?;
    let name = required_string(object, "name", "function_call")?;
    state
        .function_names
        .insert(call_id.to_string(), name.to_string());
    let arguments = object
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| Value::String("{}".to_string()));
    Ok(json!({
        "role": "model",
        "parts": [{
            "functionCall": {
                "name": name,
                "args": arguments
            }
        }]
    }))
}

fn convert_function_result(
    object: &Map<String, Value>,
    state: &GeminiInputState,
) -> Result<Value, String> {
    let call_id = required_string(object, "call_id", "function_call_output")?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| state.function_names.get(call_id).map(|value: &String| value.as_str()))
        .ok_or_else(|| format!("Gemini bridge cannot resolve function name for call_id `{call_id}`"))?;
    let output = object
        .get("output")
        .map(stringify_content)
        .transpose()?
        .unwrap_or_default();
    Ok(json!({
        "role": "user",
        "parts": [{
            "functionResponse": {
                "name": name,
                "response": {"output": output}
            }
        }]
    }))
}

fn convert_tools(tools: &Value) -> Result<Value, String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| "Responses tools must be an array".to_string())?;
    let mut declarations = Vec::with_capacity(tools.len());
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
        let mut declaration = Map::new();
        declaration.insert("name".to_string(), Value::String(name.to_string()));
        if let Some(description) = object.get("description") {
            declaration.insert("description".to_string(), description.clone());
        }
        declaration.insert(
            "parameters".to_string(),
            object
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
        );
        declarations.push(Value::Object(declaration));
    }
    Ok(json!([{"functionDeclarations": declarations}]))
}

fn gemini_parts_to_responses_output(
    response_id: &str,
    parts: &[Value],
) -> Result<(Vec<Value>, String), String> {
    let mut output = Vec::new();
    let mut text = String::new();
    let mut message_parts = Vec::new();
    for part in parts {
        let object = part
            .as_object()
            .ok_or_else(|| "Gemini content parts must be objects".to_string())?;
        if let Some(text_value) = object.get("text").and_then(Value::as_str) {
            text.push_str(text_value);
            message_parts.push(json!({
                "type": "output_text",
                "text": text_value,
                "annotations": [],
                "logprobs": []
            }));
            continue;
        }
        if let Some(function_call) = object.get("functionCall").and_then(Value::as_object) {
            let name = function_call
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let arguments = function_call
                .get("args")
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new()));
            output.push(json!({
                "id": gemini_call_id(response_id, output.len()),
                "type": "function_call",
                "status": "completed",
                "call_id": gemini_call_id(response_id, output.len()),
                "name": name,
                "arguments": serde_json::to_string(&arguments)
                    .map_err(|error| format!("Could not serialize Gemini function args: {error}"))?
            }));
            continue;
        }
    }
    if !message_parts.is_empty() {
        output.insert(
            0,
            json!({
                "id": format!("msg_{}", sanitize_id(response_id)),
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": message_parts
            }),
        );
    }
    Ok((output, text))
}

fn gemini_call_id(response_id: &str, index: usize) -> String {
    format!("fc_{}_{}", sanitize_id(response_id), index)
}

fn gemini_usage_to_responses(usage: Option<&Value>) -> Value {
    let Some(usage) = usage else {
        return Value::Null;
    };
    let input_tokens = usage
        .get("promptTokenCount")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("candidatesTokenCount")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let total_tokens = usage
        .get("totalTokenCount")
        .and_then(Value::as_i64)
        .unwrap_or(input_tokens + output_tokens);
    json!({
        "input_tokens": input_tokens,
        "input_tokens_details": {"cached_tokens": 0},
        "output_tokens": output_tokens,
        "output_tokens_details": {"reasoning_tokens": 0},
        "total_tokens": total_tokens
    })
}

#[derive(Debug, Default)]
struct GeminiInputState {
    function_names: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
struct GeminiStreamAggregate {
    response_id: String,
    model: String,
    finish_reason: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: Option<i64>,
    parts: BTreeMap<usize, Value>,
}

impl GeminiStreamAggregate {
    fn capture_envelope(&mut self, value: &Value) {
        if self.response_id.is_empty() {
            self.response_id = value
                .get("responseId")
                .and_then(Value::as_str)
                .unwrap_or("resp_ai_switch")
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
            self.total_tokens = usage
                .get("totalTokenCount")
                .and_then(Value::as_i64)
                .or(self.total_tokens);
        }
    }

    fn capture_parts(&mut self, parts: &[Value]) -> Result<(), String> {
        for (index, part) in parts.iter().enumerate() {
            let object = part
                .as_object()
                .ok_or_else(|| "Gemini content parts must be objects".to_string())?;
            if let Some(text) = object.get("text").and_then(Value::as_str) {
                let entry = self
                    .parts
                    .entry(index)
                    .or_insert_with(|| json!({"text": ""}));
                if let Some(existing_text) = entry.get("text").and_then(Value::as_str) {
                    entry["text"] = Value::String(format!("{existing_text}{text}"));
                } else {
                    *entry = json!({"text": text});
                }
                continue;
            }
            if let Some(function_call) = object.get("functionCall").and_then(Value::as_object) {
                let entry = self
                    .parts
                    .entry(index)
                    .or_insert_with(|| json!({"functionCall": {}}));
                if !entry.is_object() {
                    *entry = json!({"functionCall": {}});
                }
                let entry_object = entry
                    .as_object_mut()
                    .ok_or_else(|| "Gemini part must be an object".to_string())?;
                let function_entry = entry_object
                    .entry("functionCall".to_string())
                    .or_insert_with(|| json!({}));
                if !function_entry.is_object() {
                    *function_entry = json!({});
                }
                let function_object = function_entry.as_object_mut().ok_or_else(|| {
                    "Gemini functionCall entry must be an object".to_string()
                })?;
                if let Some(name) = function_call.get("name").and_then(Value::as_str) {
                    function_object.insert("name".to_string(), Value::String(name.to_string()));
                }
                if let Some(args) = function_call.get("args") {
                    function_object.insert("args".to_string(), args.clone());
                }
            }
        }
        Ok(())
    }

    fn into_gemini_response(self) -> Value {
        let parts = self.parts.into_values().collect::<Vec<_>>();
        json!({
            "responseId": if self.response_id.is_empty() { "resp_ai_switch" } else { &self.response_id },
            "model": if self.model.is_empty() { "unknown" } else { &self.model },
            "candidates": [{
                "content": {"role": "model", "parts": parts},
                "finishReason": self.finish_reason.as_deref().unwrap_or("STOP")
            }],
            "usageMetadata": {
                "promptTokenCount": self.input_tokens,
                "candidatesTokenCount": self.output_tokens,
                "totalTokenCount": self.total_tokens.unwrap_or(self.input_tokens + self.output_tokens)
            }
        })
    }
}

fn responses_status(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("MAX_TOKENS") => "incomplete",
        Some("SAFETY") => "failed",
        _ => "completed",
    }
}

fn incomplete_details(finish_reason: Option<&str>) -> Value {
    match finish_reason {
        Some("MAX_TOKENS") => json!({"reason": "max_output_tokens"}),
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
