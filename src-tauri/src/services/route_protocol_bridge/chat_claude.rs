//! Inbound OpenAI Chat Completions bridged to an Anthropic Messages upstream.
//!
//! The mirror of [`super::claude_chat`], which carries Anthropic inbound to a
//! chat upstream. This direction exists because a client's wire shape is a
//! property of the client, not of the pool: the third-party clients that only
//! speak chat completions are configured for the claude platform too, and the
//! claude platform's default credential dialect is Anthropic.

use super::TransformedBridgeResponse;
use serde_json::{json, Map, Value};

/// Anthropic requires `max_tokens`; a chat client that omits it still needs a
/// number, and this is large enough not to truncate a real answer.
const DEFAULT_MAX_TOKENS: u64 = 4096;

pub(super) fn chat_request_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Chat request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Chat request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    if let Some(model) = object.get("model") {
        result.insert("model".to_string(), model.clone());
    }

    let mut system = Vec::new();
    let mut messages = Vec::new();
    if let Some(items) = object.get("messages").and_then(Value::as_array) {
        for item in items {
            convert_message(item, &mut system, &mut messages)?;
        }
    }
    if !system.is_empty() {
        result.insert("system".to_string(), Value::Array(system));
    }
    result.insert("messages".to_string(), Value::Array(messages));

    // Anthropic rejects a request without it, so an absent cap becomes a default
    // rather than a 400 the client cannot act on.
    let max_tokens = object
        .get("max_tokens")
        .or_else(|| object.get("max_completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MAX_TOKENS);
    result.insert("max_tokens".to_string(), json!(max_tokens));

    for field in ["temperature", "top_p", "stream"] {
        if let Some(found) = object.get(field) {
            result.insert(field.to_string(), found.clone());
        }
    }
    if let Some(stop) = object.get("stop") {
        result.insert("stop_sequences".to_string(), stop_sequences(stop));
    }
    if let Some(tools) = object.get("tools") {
        result.insert("tools".to_string(), convert_tools(tools)?);
    }
    if let Some(tool_choice) = object.get("tool_choice") {
        if let Some(converted) = convert_tool_choice(tool_choice)? {
            result.insert("tool_choice".to_string(), converted);
        }
    }

    serde_json::to_vec(&Value::Object(result))
        .map_err(|error| format!("Could not serialize Anthropic request: {error}"))
}

pub(super) fn anthropic_response_to_chat(
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
            body: anthropic_sse_to_chat(body)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }

    Ok(TransformedBridgeResponse {
        body: anthropic_json_to_chat(body)?,
        content_type: Some("application/json".to_string()),
    })
}

/// Routes one chat message into either the Anthropic `system` array or the
/// `messages` array.
///
/// Anthropic carries the system prompt outside `messages` and has no `tool`
/// role: a tool result is a `tool_result` block on a *user* message. Both are
/// shape changes rather than field renames, which is why this cannot be a
/// straight copy.
fn convert_message(
    message: &Value,
    system: &mut Vec<Value>,
    messages: &mut Vec<Value>,
) -> Result<(), String> {
    let object = message
        .as_object()
        .ok_or_else(|| "Chat messages entries must be objects".to_string())?;
    let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
    match role {
        "system" | "developer" => {
            let text = message_text(object.get("content"))?;
            if !text.is_empty() {
                system.push(json!({"type": "text", "text": text}));
            }
            Ok(())
        }
        "user" => {
            let blocks = user_blocks(object.get("content"))?;
            if !blocks.is_empty() {
                messages.push(json!({"role": "user", "content": blocks}));
            }
            Ok(())
        }
        "assistant" => {
            let blocks = assistant_blocks(object)?;
            if !blocks.is_empty() {
                messages.push(json!({"role": "assistant", "content": blocks}));
            }
            Ok(())
        }
        "tool" => {
            let id = object
                .get("tool_call_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Chat tool message is missing tool_call_id".to_string())?;
            let mut block = Map::new();
            block.insert("type".to_string(), json!("tool_result"));
            block.insert("tool_use_id".to_string(), json!(id));
            block.insert(
                "content".to_string(),
                json!(message_text(object.get("content"))?),
            );
            messages.push(json!({"role": "user", "content": [Value::Object(block)]}));
            Ok(())
        }
        other => Err(format!("Unsupported Chat message role: {other}")),
    }
}

/// Flattens chat content — a bare string, or the multi-part array — to text.
fn message_text(content: Option<&Value>) -> Result<String, String> {
    match content {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(parts)) => {
            let mut text = String::new();
            for part in parts {
                if let Some(found) = part.get("text").and_then(Value::as_str) {
                    text.push_str(found);
                }
            }
            Ok(text)
        }
        Some(other) => Err(format!("Unsupported Chat content shape: {other}")),
    }
}

fn user_blocks(content: Option<&Value>) -> Result<Vec<Value>, String> {
    match content {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(text)) => Ok(if text.is_empty() {
            Vec::new()
        } else {
            vec![json!({"type": "text", "text": text})]
        }),
        Some(Value::Array(parts)) => {
            let mut blocks = Vec::new();
            for part in parts {
                let kind = part.get("type").and_then(Value::as_str).unwrap_or("text");
                match kind {
                    "text" => {
                        let text = part.get("text").and_then(Value::as_str).unwrap_or_default();
                        if !text.is_empty() {
                            blocks.push(json!({"type": "text", "text": text}));
                        }
                    }
                    "image_url" => blocks.push(convert_image_part(part)?),
                    other => return Err(format!("Unsupported Chat content part: {other}")),
                }
            }
            Ok(blocks)
        }
        Some(other) => Err(format!("Unsupported Chat content shape: {other}")),
    }
}

/// `image_url` carries either a data URL or a remote URL; Anthropic takes the
/// former as base64 with an explicit media type and the latter as a `url` source.
fn convert_image_part(part: &Value) -> Result<Value, String> {
    let url = part
        .pointer("/image_url/url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Chat image part is missing image_url.url".to_string())?;
    if let Some(rest) = url.strip_prefix("data:") {
        let (media_type, data) = rest
            .split_once(";base64,")
            .ok_or_else(|| "Chat image data URL must be base64".to_string())?;
        return Ok(json!({
            "type": "image",
            "source": {"type": "base64", "media_type": media_type, "data": data}
        }));
    }
    Ok(json!({
        "type": "image",
        "source": {"type": "url", "url": url}
    }))
}

/// An assistant turn carries text and `tool_calls` side by side in chat; in
/// Anthropic both are content blocks, and the arguments stop being a JSON string.
fn assistant_blocks(object: &Map<String, Value>) -> Result<Vec<Value>, String> {
    let mut blocks = Vec::new();
    let text = message_text(object.get("content"))?;
    if !text.is_empty() {
        blocks.push(json!({"type": "text", "text": text}));
    }
    if let Some(calls) = object.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Chat tool_call is missing id".to_string())?;
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .ok_or_else(|| "Chat tool_call is missing function.name".to_string())?;
            let arguments = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            blocks.push(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": parse_json_object_or_empty(arguments)
            }));
        }
    }
    Ok(blocks)
}

/// Tool arguments arrive as a JSON *string* in chat and as an object in
/// Anthropic. A partial or empty string becomes `{}` rather than failing the
/// turn: the upstream will answer for a wrong argument set, but a rejected
/// request strands the client with nothing.
fn parse_json_object_or_empty(raw: &str) -> Value {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(map)) => Value::Object(map),
        _ => json!({}),
    }
}

fn stop_sequences(stop: &Value) -> Value {
    match stop {
        Value::String(text) => json!([text]),
        Value::Array(items) => Value::Array(items.clone()),
        _ => json!([]),
    }
}

fn convert_tools(tools: &Value) -> Result<Value, String> {
    let items = tools
        .as_array()
        .ok_or_else(|| "Chat tools must be an array".to_string())?;
    let mut converted = Vec::new();
    for tool in items {
        let function = tool
            .get("function")
            .and_then(Value::as_object)
            .ok_or_else(|| "Chat tool is missing function".to_string())?;
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "Chat tool is missing function.name".to_string())?;
        let mut entry = Map::new();
        entry.insert("name".to_string(), json!(name));
        if let Some(description) = function.get("description") {
            entry.insert("description".to_string(), description.clone());
        }
        entry.insert(
            "input_schema".to_string(),
            function
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
        );
        converted.push(Value::Object(entry));
    }
    Ok(Value::Array(converted))
}

/// `none` has no Anthropic equivalent, so it is dropped: sending `auto` instead
/// would let the model call a tool the client just said not to call, and
/// inventing a rejection would fail a request the upstream would have served.
fn convert_tool_choice(tool_choice: &Value) -> Result<Option<Value>, String> {
    match tool_choice {
        Value::String(text) => match text.as_str() {
            "auto" => Ok(Some(json!({"type": "auto"}))),
            "required" | "any" => Ok(Some(json!({"type": "any"}))),
            "none" => Ok(None),
            other => Err(format!("Unsupported Chat tool_choice: {other}")),
        },
        Value::Object(_) => {
            let name = tool_choice
                .pointer("/function/name")
                .and_then(Value::as_str)
                .ok_or_else(|| "Chat tool_choice object needs function.name".to_string())?;
            Ok(Some(json!({"type": "tool", "name": name})))
        }
        Value::Null => Ok(None),
        other => Err(format!("Unsupported Chat tool_choice shape: {other}")),
    }
}

fn anthropic_json_to_chat(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Anthropic response JSON is invalid: {error}"))?;
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    if let Some(blocks) = value.get("content").and_then(Value::as_array) {
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => text.push_str(
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                Some("tool_use") => tool_calls.push(tool_call_from_block(block, tool_calls.len())),
                _ => {}
            }
        }
    }

    let mut message = Map::new();
    message.insert("role".to_string(), json!("assistant"));
    // Null rather than "" when the turn is only tool calls: several chat clients
    // render an empty string as an empty assistant bubble.
    message.insert(
        "content".to_string(),
        if text.is_empty() && !tool_calls.is_empty() {
            Value::Null
        } else {
            json!(text)
        },
    );
    if !tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(tool_calls.clone()));
    }

    let stop_reason = value.get("stop_reason").and_then(Value::as_str);
    let response = json!({
        "id": value.get("id").and_then(Value::as_str).unwrap_or("chatcmpl-bridge"),
        "object": "chat.completion",
        "created": 0,
        "model": value.get("model").and_then(Value::as_str).unwrap_or_default(),
        "choices": [{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": finish_reason(stop_reason, !tool_calls.is_empty()),
        }],
        "usage": usage_to_chat(value.get("usage")),
    });
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Chat response: {error}"))
}

fn tool_call_from_block(block: &Value, index: usize) -> Value {
    json!({
        "index": index,
        "id": block.get("id").and_then(Value::as_str).unwrap_or_default(),
        "type": "function",
        "function": {
            "name": block.get("name").and_then(Value::as_str).unwrap_or_default(),
            // Chat carries arguments as a JSON string, Anthropic as an object.
            "arguments": block
                .get("input")
                .map(|input| input.to_string())
                .unwrap_or_else(|| "{}".to_string()),
        }
    })
}

/// `tool_use` outranks the stop reason: a client that sees `stop` alongside
/// `tool_calls` will not run the tool.
fn finish_reason(stop_reason: Option<&str>, has_tool_calls: bool) -> &'static str {
    if has_tool_calls {
        return "tool_calls";
    }
    match stop_reason {
        Some("max_tokens") => "length",
        Some("tool_use") => "tool_calls",
        _ => "stop",
    }
}

fn usage_to_chat(usage: Option<&Value>) -> Value {
    let input = usage
        .and_then(|value| value.get("input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .and_then(|value| value.get("output_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "prompt_tokens": input,
        "completion_tokens": output,
        "total_tokens": input + output,
    })
}

fn looks_like_sse(body: &[u8]) -> bool {
    std::str::from_utf8(body).ok().is_some_and(|text| {
        text.lines()
            .any(|line| line.trim_start().starts_with("data:"))
    })
}

/// Translates the Anthropic event stream into chat chunks as they arrive.
///
/// Deliberately incremental rather than accumulate-then-emit: a chat client
/// streams to show progress, and replaying one whole message at the end would
/// make every turn appear to hang and then finish instantly.
fn anthropic_sse_to_chat(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = String::new();
    let mut id = "chatcmpl-bridge".to_string();
    let mut model = String::new();
    let mut role_sent = false;
    // Anthropic block indices count text blocks too, so tool calls need their own
    // contiguous numbering: a client keys its accumulator on this index.
    let mut tool_index_by_block: Vec<(u64, usize)> = Vec::new();
    let mut stop_reason: Option<String> = None;
    let mut usage: Option<Value> = None;

    for value in super::sse::parse_sse_data_records(body)? {
        match value.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                if let Some(message) = value.get("message") {
                    if let Some(found) = message.get("id").and_then(Value::as_str) {
                        id = found.to_string();
                    }
                    if let Some(found) = message.get("model").and_then(Value::as_str) {
                        model = found.to_string();
                    }
                    if let Some(found) = message.get("usage") {
                        usage = Some(found.clone());
                    }
                }
                if !role_sent {
                    push_chunk(&mut output, &id, &model, json!({"role": "assistant"}), None)?;
                    role_sent = true;
                }
            }
            Some("content_block_start") => {
                let block_index = value.get("index").and_then(Value::as_u64).unwrap_or(0);
                let block = value.get("content_block");
                if block
                    .and_then(|found| found.get("type"))
                    .and_then(Value::as_str)
                    == Some("tool_use")
                {
                    let tool_index = tool_index_by_block.len();
                    tool_index_by_block.push((block_index, tool_index));
                    let block = block.expect("checked above");
                    push_chunk(
                        &mut output,
                        &id,
                        &model,
                        json!({"tool_calls": [{
                            "index": tool_index,
                            "id": block.get("id").and_then(Value::as_str).unwrap_or_default(),
                            "type": "function",
                            "function": {
                                "name": block.get("name").and_then(Value::as_str).unwrap_or_default(),
                                "arguments": ""
                            }
                        }]}),
                        None,
                    )?;
                }
            }
            Some("content_block_delta") => {
                let block_index = value.get("index").and_then(Value::as_u64).unwrap_or(0);
                match value.pointer("/delta/type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let text = value
                            .pointer("/delta/text")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if !text.is_empty() {
                            push_chunk(&mut output, &id, &model, json!({"content": text}), None)?;
                        }
                    }
                    Some("input_json_delta") => {
                        let partial = value
                            .pointer("/delta/partial_json")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let tool_index = tool_index_by_block
                            .iter()
                            .find(|(block, _)| *block == block_index)
                            .map(|(_, index)| *index)
                            .unwrap_or(0);
                        push_chunk(
                            &mut output,
                            &id,
                            &model,
                            json!({"tool_calls": [{
                                "index": tool_index,
                                "function": {"arguments": partial}
                            }]}),
                            None,
                        )?;
                    }
                    _ => {}
                }
            }
            Some("message_delta") => {
                if let Some(found) = value.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    stop_reason = Some(found.to_string());
                }
                if let Some(found) = value.get("usage") {
                    usage = Some(merge_usage(usage.as_ref(), found));
                }
            }
            _ => {}
        }
    }

    let finish = finish_reason(stop_reason.as_deref(), !tool_index_by_block.is_empty());
    push_chunk(&mut output, &id, &model, json!({}), Some(finish))?;
    // A usage-only chunk is what `stream_options.include_usage` produces, so a
    // client that reads token counts from the stream still finds them.
    let usage_chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": 0,
        "model": model,
        "choices": [],
        "usage": usage_to_chat(usage.as_ref()),
    });
    output.push_str(&format!("data: {usage_chunk}\n\n"));
    output.push_str("data: [DONE]\n\n");
    Ok(output.into_bytes())
}

fn merge_usage(existing: Option<&Value>, incoming: &Value) -> Value {
    let mut merged = existing
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(incoming) = incoming.as_object() {
        for (key, value) in incoming {
            merged.insert(key.clone(), value.clone());
        }
    }
    Value::Object(merged)
}

fn push_chunk(
    output: &mut String,
    id: &str,
    model: &str,
    delta: Value,
    finish_reason: Option<&str>,
) -> Result<(), String> {
    let chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": 0,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason,
        }],
    });
    output.push_str(&format!("data: {chunk}\n\n"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_prompt_leaves_messages_and_a_tool_result_becomes_a_user_block() {
        let body = json!({
            "model": "claude-opus-5",
            "messages": [
                {"role": "system", "content": "be terse"},
                {"role": "user", "content": "weather?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
                }]},
                {"role": "tool", "tool_call_id": "call_1", "content": "18C"}
            ],
            "tools": [{"type": "function", "function": {
                "name": "get_weather",
                "description": "look up weather",
                "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
            }}],
            "tool_choice": "required"
        });

        let converted = chat_request_to_anthropic(&serde_json::to_vec(&body).unwrap()).unwrap();
        let value: Value = serde_json::from_slice(&converted).unwrap();

        // Anthropic carries the system prompt outside `messages`.
        assert_eq!(value["system"][0]["text"], "be terse");
        assert_eq!(value["messages"].as_array().unwrap().len(), 3);
        assert_eq!(value["messages"][0]["role"], "user");
        // Arguments stop being a JSON string.
        assert_eq!(value["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(value["messages"][1]["content"][0]["input"]["city"], "SF");
        // There is no `tool` role: a result is a block on a user message.
        assert_eq!(value["messages"][2]["role"], "user");
        assert_eq!(value["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(value["messages"][2]["content"][0]["tool_use_id"], "call_1");
        assert_eq!(value["tools"][0]["input_schema"]["type"], "object");
        assert!(value["tools"][0].get("parameters").is_none());
        assert_eq!(value["tool_choice"]["type"], "any");
        // Anthropic rejects a request with no cap, so an absent one is defaulted.
        assert_eq!(value["max_tokens"], 4096);
    }

    #[test]
    fn tool_choice_none_is_dropped_rather_than_turned_into_auto() {
        let body = json!({"model": "m", "messages": [], "tool_choice": "none"});
        let converted = chat_request_to_anthropic(&serde_json::to_vec(&body).unwrap()).unwrap();
        let value: Value = serde_json::from_slice(&converted).unwrap();
        // `auto` would let the model call a tool the client just forbade.
        assert!(value.get("tool_choice").is_none());
    }

    #[test]
    fn a_tool_using_answer_becomes_chat_tool_calls_with_a_matching_finish_reason() {
        let upstream = json!({
            "id": "msg_1",
            "model": "claude-opus-5",
            "content": [
                {"type": "text", "text": "checking"},
                {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"city": "SF"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 12, "output_tokens": 7}
        });

        let converted = anthropic_response_to_chat(
            200,
            Some("application/json"),
            &serde_json::to_vec(&upstream).unwrap(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(value["object"], "chat.completion");
        assert_eq!(value["choices"][0]["message"]["content"], "checking");
        let call = &value["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(call["id"], "toolu_1");
        assert_eq!(call["function"]["name"], "get_weather");
        // Chat carries arguments as a string.
        assert_eq!(call["function"]["arguments"], "{\"city\":\"SF\"}");
        assert_eq!(value["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(value["usage"]["prompt_tokens"], 12);
        assert_eq!(value["usage"]["total_tokens"], 19);
    }

    #[test]
    fn an_upstream_error_is_passed_through_untouched() {
        let body = br#"{"type":"error","error":{"type":"overloaded_error"}}"#;
        let converted = anthropic_response_to_chat(529, Some("application/json"), body).unwrap();
        assert_eq!(converted.body, body.to_vec());
    }

    #[test]
    fn the_event_stream_is_translated_incrementally_with_its_own_tool_call_indices() {
        // Block index 0 is text, so the tool call is Anthropic block 1 but chat
        // tool_call index 0: a client keys its argument accumulator on that index,
        // and reusing the block index would leave a gap it never fills.
        let upstream = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-opus-5\",\"usage\":{\"input_tokens\":4,\"output_tokens\":0}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"get_weather\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\":\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"SF\\\"}\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":9}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );

        let converted =
            anthropic_response_to_chat(200, Some("text/event-stream"), upstream.as_bytes())
                .unwrap();
        assert_eq!(converted.content_type.as_deref(), Some("text/event-stream"));
        let output = String::from_utf8(converted.body).unwrap();
        let chunks: Vec<Value> = output
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter(|payload| *payload != "[DONE]")
            .map(|payload| serde_json::from_str(payload).unwrap())
            .collect();

        assert_eq!(chunks[0]["choices"][0]["delta"]["role"], "assistant");
        assert_eq!(chunks[1]["choices"][0]["delta"]["content"], "hi");
        let opening = &chunks[2]["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(opening["index"], 0, "chat numbering, not the block index");
        assert_eq!(opening["id"], "toolu_1");
        assert_eq!(opening["function"]["name"], "get_weather");
        // Argument fragments arrive as deltas, still on chat index 0.
        assert_eq!(
            chunks[3]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            "{\"city\":"
        );
        assert_eq!(
            chunks[4]["choices"][0]["delta"]["tool_calls"][0]["index"],
            0
        );
        // Terminal chunk carries the finish reason, then a usage-only chunk.
        let terminal = chunks.iter().rev().nth(1).unwrap();
        assert_eq!(terminal["choices"][0]["finish_reason"], "tool_calls");
        let usage = chunks.last().unwrap();
        assert_eq!(usage["usage"]["prompt_tokens"], 4);
        assert_eq!(usage["usage"]["completion_tokens"], 9);
        assert!(output.trim_end().ends_with("data: [DONE]"));
    }
}
