use super::TransformedBridgeResponse;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(super) fn responses_request_to_chat(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Responses request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Responses request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    if let Some(model) = object.get("model") {
        result.insert("model".to_string(), model.clone());
    }

    let mut messages = Vec::new();
    if let Some(instructions) = object.get("instructions") {
        let content = text_value(instructions, "instructions")?;
        if !content.is_empty() {
            messages.push(json!({"role": "system", "content": content}));
        }
    }
    if let Some(input) = object.get("input") {
        messages.extend(convert_input(input)?);
    }
    result.insert("messages".to_string(), Value::Array(messages));

    if let Some(limit) = object.get("max_output_tokens") {
        let model = object
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let key = if is_openai_o_series(model) {
            "max_completion_tokens"
        } else {
            "max_tokens"
        };
        result.insert(key.to_string(), limit.clone());
    }

    copy_fields(
        object,
        &mut result,
        &[
            "temperature",
            "top_p",
            "parallel_tool_calls",
            "stream",
            "stop",
            "presence_penalty",
            "frequency_penalty",
            "seed",
            "service_tier",
            "user",
        ],
    );

    if object.get("stream").and_then(Value::as_bool) == Some(true) {
        result.insert("stream_options".to_string(), json!({"include_usage": true}));
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

pub(super) fn chat_response_to_responses(
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
            body: chat_sse_to_responses(body)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }

    Ok(TransformedBridgeResponse {
        body: chat_json_to_responses(body)?,
        content_type: Some("application/json".to_string()),
    })
}

fn chat_json_to_responses(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Chat response JSON is invalid: {error}"))?;
    if value.get("error").is_some() {
        return serde_json::to_vec(&failed_response_from_error(&value))
            .map_err(|error| format!("Could not serialize Responses error: {error}"));
    }
    let response_id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_ai_switch");
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
    let text = chat_message_text(message)?;
    let tool_calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let output = build_output_items(response_id, &text, &tool_calls, true)?;
    let finish_reason = choice.get("finish_reason").and_then(Value::as_str);
    let status = responses_status(finish_reason);
    let response = response_object(
        response_id,
        model,
        status,
        output,
        chat_usage_to_responses(value.get("usage")),
        finish_reason,
    );
    let mut response = response;
    response["output_text"] = Value::String(text);

    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Responses response: {error}"))
}

fn chat_sse_to_responses(body: &[u8]) -> Result<Vec<u8>, String> {
    let text = String::from_utf8_lossy(body).replace("\r\n", "\n");
    let mut state = ChatStreamState::default();
    let mut output = String::new();
    let mut sequence_number = 0_u64;
    let mut saw_done = false;

    for block in text.split("\n\n") {
        let data = block
            .lines()
            .filter_map(|line| line.trim().strip_prefix("data:").map(str::trim))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            saw_done = true;
            break;
        }
        let value = serde_json::from_str::<Value>(&data)
            .map_err(|error| format!("Chat SSE data is invalid JSON: {error}"))?;
        if value.get("error").is_some() {
            ensure_stream_started(&mut state, &mut output, &mut sequence_number);
            push_sse_event(
                &mut output,
                "response.failed",
                failed_response_event(&state, &value, sequence_number),
            )?;
            return Ok(output.into_bytes());
        }

        state.capture_envelope(&value);
        ensure_stream_started(&mut state, &mut output, &mut sequence_number);
        if let Some(usage) = value.get("usage") {
            state.usage = Some(usage.clone());
        }
        let Some(choice) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            continue;
        };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            state.finish_reason = Some(reason.to_string());
        }
        let delta = choice.get("delta").unwrap_or(&Value::Null);
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            emit_text_delta(&mut state, &mut output, &mut sequence_number, content)?;
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for tool_call in tool_calls {
                emit_tool_call_delta(&mut state, &mut output, &mut sequence_number, tool_call)?;
            }
        }
    }

    if !state.started && !saw_done {
        return Err("Chat SSE response did not contain data events".to_string());
    }
    ensure_stream_started(&mut state, &mut output, &mut sequence_number);
    finish_stream(&mut state, &mut output, &mut sequence_number)?;
    Ok(output.into_bytes())
}

#[derive(Debug, Default)]
struct ChatStreamState {
    response_id: String,
    model: String,
    created_at: i64,
    started: bool,
    text_started: bool,
    text: String,
    tools: BTreeMap<usize, StreamToolCall>,
    next_output_index: usize,
    finish_reason: Option<String>,
    usage: Option<Value>,
}

impl ChatStreamState {
    fn capture_envelope(&mut self, value: &Value) {
        if self.response_id.is_empty() {
            self.response_id = value
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("resp_ai_switch")
                .to_string();
        }
        if self.model.is_empty() {
            self.model = value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
        }
        if self.created_at == 0 {
            self.created_at = value
                .get("created")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| chrono::Utc::now().timestamp());
        }
    }

    fn response_id(&self) -> &str {
        if self.response_id.is_empty() {
            "resp_ai_switch"
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
    output_index: usize,
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    started: bool,
}

fn ensure_stream_started(
    state: &mut ChatStreamState,
    output: &mut String,
    sequence_number: &mut u64,
) {
    if state.started {
        return;
    }
    if state.created_at == 0 {
        state.created_at = chrono::Utc::now().timestamp();
    }
    let created = response_object(
        state.response_id(),
        state.model(),
        "in_progress",
        Vec::new(),
        Value::Null,
        None,
    );
    let _ = push_sse_event(
        output,
        "response.created",
        json!({
            "type": "response.created",
            "sequence_number": *sequence_number,
            "response": created
        }),
    );
    *sequence_number += 1;
    let in_progress = response_object(
        state.response_id(),
        state.model(),
        "in_progress",
        Vec::new(),
        Value::Null,
        None,
    );
    let _ = push_sse_event(
        output,
        "response.in_progress",
        json!({
            "type": "response.in_progress",
            "sequence_number": *sequence_number,
            "response": in_progress
        }),
    );
    *sequence_number += 1;
    state.started = true;
}

fn emit_text_delta(
    state: &mut ChatStreamState,
    output: &mut String,
    sequence_number: &mut u64,
    delta: &str,
) -> Result<(), String> {
    let output_index = 0;
    let item_id = message_item_id(state.response_id());
    if !state.text_started {
        push_sse_event(
            output,
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "sequence_number": *sequence_number,
                "output_index": output_index,
                "item": message_output_item(&item_id, "in_progress", "")
            }),
        )?;
        *sequence_number += 1;
        push_sse_event(
            output,
            "response.content_part.added",
            json!({
                "type": "response.content_part.added",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": output_index,
                "content_index": 0,
                "part": output_text_part("")
            }),
        )?;
        *sequence_number += 1;
        state.text_started = true;
        state.next_output_index = 1;
    }
    state.text.push_str(delta);
    push_sse_event(
        output,
        "response.output_text.delta",
        json!({
            "type": "response.output_text.delta",
            "sequence_number": *sequence_number,
            "item_id": item_id,
            "output_index": output_index,
            "content_index": 0,
            "delta": delta,
            "logprobs": []
        }),
    )?;
    *sequence_number += 1;
    Ok(())
}

fn emit_tool_call_delta(
    state: &mut ChatStreamState,
    output: &mut String,
    sequence_number: &mut u64,
    value: &Value,
) -> Result<(), String> {
    let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
    if !state.tools.contains_key(&index) {
        let output_index = state.next_output_index;
        state.next_output_index += 1;
        state.tools.insert(
            index,
            StreamToolCall {
                output_index,
                item_id: format!("fc_{}_{}", sanitize_id(state.response_id()), index),
                call_id: value
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_ai_switch")
                    .to_string(),
                ..StreamToolCall::default()
            },
        );
    }
    let tool = state
        .tools
        .get_mut(&index)
        .expect("tool call inserted before mutation");
    if let Some(id) = value.get("id").and_then(Value::as_str) {
        tool.call_id = id.to_string();
    }
    if let Some(function) = value.get("function") {
        if let Some(name) = function.get("name").and_then(Value::as_str) {
            tool.name.push_str(name);
        }
    }
    if !tool.started {
        push_sse_event(
            output,
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "sequence_number": *sequence_number,
                "output_index": tool.output_index,
                "item": function_call_output_item(
                    &tool.item_id,
                    &tool.call_id,
                    &tool.name,
                    "",
                    "in_progress"
                )
            }),
        )?;
        *sequence_number += 1;
        tool.started = true;
    }
    if let Some(arguments) = value
        .get("function")
        .and_then(|function| function.get("arguments"))
        .and_then(Value::as_str)
    {
        tool.arguments.push_str(arguments);
        push_sse_event(
            output,
            "response.function_call_arguments.delta",
            json!({
                "type": "response.function_call_arguments.delta",
                "sequence_number": *sequence_number,
                "item_id": tool.item_id,
                "output_index": tool.output_index,
                "delta": arguments
            }),
        )?;
        *sequence_number += 1;
    }
    Ok(())
}

fn finish_stream(
    state: &mut ChatStreamState,
    output: &mut String,
    sequence_number: &mut u64,
) -> Result<(), String> {
    let mut final_output = Vec::new();
    if state.text_started {
        let item_id = message_item_id(state.response_id());
        push_sse_event(
            output,
            "response.output_text.done",
            json!({
                "type": "response.output_text.done",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "text": state.text,
                "logprobs": []
            }),
        )?;
        *sequence_number += 1;
        push_sse_event(
            output,
            "response.content_part.done",
            json!({
                "type": "response.content_part.done",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": 0,
                "content_index": 0,
                "part": output_text_part(&state.text)
            }),
        )?;
        *sequence_number += 1;
        let item = message_output_item(&item_id, "completed", &state.text);
        push_sse_event(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "sequence_number": *sequence_number,
                "output_index": 0,
                "item": item
            }),
        )?;
        *sequence_number += 1;
        final_output.push(message_output_item(&item_id, "completed", &state.text));
    }
    for tool in state.tools.values() {
        push_sse_event(
            output,
            "response.function_call_arguments.done",
            json!({
                "type": "response.function_call_arguments.done",
                "sequence_number": *sequence_number,
                "item_id": tool.item_id,
                "output_index": tool.output_index,
                "arguments": tool.arguments
            }),
        )?;
        *sequence_number += 1;
        let item = function_call_output_item(
            &tool.item_id,
            &tool.call_id,
            &tool.name,
            &tool.arguments,
            "completed",
        );
        push_sse_event(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "sequence_number": *sequence_number,
                "output_index": tool.output_index,
                "item": item
            }),
        )?;
        *sequence_number += 1;
        final_output.push(function_call_output_item(
            &tool.item_id,
            &tool.call_id,
            &tool.name,
            &tool.arguments,
            "completed",
        ));
    }
    final_output.sort_by_key(|item| {
        if item.get("type").and_then(Value::as_str) == Some("message") {
            0
        } else {
            1
        }
    });
    let finish_reason = state.finish_reason.as_deref();
    let status = responses_status(finish_reason);
    let response = response_object(
        state.response_id(),
        state.model(),
        status,
        final_output,
        chat_usage_to_responses(state.usage.as_ref()),
        finish_reason,
    );
    let event_name = if status == "failed" {
        "response.failed"
    } else if status == "incomplete" {
        "response.incomplete"
    } else {
        "response.completed"
    };
    push_sse_event(
        output,
        event_name,
        json!({
            "type": event_name,
            "sequence_number": *sequence_number,
            "response": response
        }),
    )?;
    Ok(())
}

fn build_output_items(
    response_id: &str,
    text: &str,
    tool_calls: &[Value],
    completed: bool,
) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    if !text.is_empty() {
        output.push(message_output_item(
            &message_item_id(response_id),
            if completed {
                "completed"
            } else {
                "in_progress"
            },
            text,
        ));
    }
    for (index, tool_call) in tool_calls.iter().enumerate() {
        let call_id = tool_call
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("call_ai_switch");
        let function = tool_call
            .get("function")
            .ok_or_else(|| "Chat tool call is missing function".to_string())?;
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "Chat tool call is missing function.name".to_string())?;
        let arguments = function
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or("{}");
        output.push(function_call_output_item(
            &format!("fc_{}_{}", sanitize_id(response_id), index),
            call_id,
            name,
            arguments,
            if completed {
                "completed"
            } else {
                "in_progress"
            },
        ));
    }
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

fn response_object(
    response_id: &str,
    model: &str,
    status: &str,
    output: Vec<Value>,
    usage: Value,
    finish_reason: Option<&str>,
) -> Value {
    json!({
        "id": response_id,
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": status,
        "background": false,
        "error": Value::Null,
        "incomplete_details": incomplete_details(finish_reason),
        "instructions": Value::Null,
        "max_output_tokens": Value::Null,
        "model": model,
        "output": output,
        "parallel_tool_calls": true,
        "previous_response_id": Value::Null,
        "reasoning": {"effort": Value::Null, "summary": Value::Null},
        "store": false,
        "temperature": Value::Null,
        "text": {"format": {"type": "text"}},
        "tool_choice": "auto",
        "tools": [],
        "top_p": Value::Null,
        "truncation": "disabled",
        "usage": usage,
        "metadata": {}
    })
}

fn message_output_item(item_id: &str, status: &str, text: &str) -> Value {
    json!({
        "id": item_id,
        "type": "message",
        "status": status,
        "role": "assistant",
        "content": [output_text_part(text)]
    })
}

fn output_text_part(text: &str) -> Value {
    json!({
        "type": "output_text",
        "text": text,
        "annotations": [],
        "logprobs": []
    })
}

fn function_call_output_item(
    item_id: &str,
    call_id: &str,
    name: &str,
    arguments: &str,
    status: &str,
) -> Value {
    json!({
        "id": item_id,
        "type": "function_call",
        "status": status,
        "call_id": call_id,
        "name": name,
        "arguments": arguments
    })
}

fn chat_usage_to_responses(usage: Option<&Value>) -> Value {
    let Some(usage) = usage else {
        return Value::Null;
    };
    let input_tokens = usage
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(input_tokens + output_tokens);
    let cached_tokens = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_i64)
        .or_else(|| usage.get("prompt_cache_hit_tokens").and_then(Value::as_i64))
        .unwrap_or(0);
    let reasoning_tokens = usage
        .pointer("/completion_tokens_details/reasoning_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let mut result = json!({
        "input_tokens": input_tokens,
        "input_tokens_details": {"cached_tokens": cached_tokens},
        "output_tokens": output_tokens,
        "output_tokens_details": {"reasoning_tokens": reasoning_tokens},
        "total_tokens": total_tokens
    });
    if let (Some(result), Some(original)) = (result.as_object_mut(), usage.as_object()) {
        for (key, value) in original {
            result.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    result
}

fn responses_status(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("length" | "content_filter") => "incomplete",
        Some("error") => "failed",
        _ => "completed",
    }
}

fn incomplete_details(finish_reason: Option<&str>) -> Value {
    match finish_reason {
        Some("length") => json!({"reason": "max_output_tokens"}),
        Some("content_filter") => json!({"reason": "content_filter"}),
        _ => Value::Null,
    }
}

fn failed_response_from_error(value: &Value) -> Value {
    let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("Chat upstream returned an error");
    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| json!("resp_ai_switch")),
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": "failed",
        "error": {
            "code": value.pointer("/error/code").cloned().unwrap_or(Value::Null),
            "message": message
        },
        "output": [],
        "usage": Value::Null
    })
}

fn failed_response_event(state: &ChatStreamState, value: &Value, sequence_number: u64) -> Value {
    let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("Chat upstream returned an error");
    json!({
        "type": "response.failed",
        "sequence_number": sequence_number,
        "response": {
            "id": state.response_id(),
            "object": "response",
            "created_at": chrono::Utc::now().timestamp(),
            "status": "failed",
            "model": state.model(),
            "error": {
                "code": value.pointer("/error/code").cloned().unwrap_or(Value::Null),
                "message": message
            },
            "output": [],
            "usage": Value::Null
        }
    })
}

fn push_sse_event(output: &mut String, event: &str, value: Value) -> Result<(), String> {
    output.push_str("event: ");
    output.push_str(event);
    output.push('\n');
    output.push_str("data: ");
    output.push_str(
        &serde_json::to_string(&value)
            .map_err(|error| format!("Could not serialize Responses SSE event: {error}"))?,
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

fn message_item_id(response_id: &str) -> String {
    format!("msg_{}", sanitize_id(response_id))
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

fn convert_input(input: &Value) -> Result<Vec<Value>, String> {
    match input {
        Value::String(text) => Ok(vec![json!({"role": "user", "content": text})]),
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
            Ok(json!({
                "role": "assistant",
                "content": Value::Null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {"name": name, "arguments": arguments}
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
                "role": "tool",
                "tool_call_id": call_id,
                "content": output
            }))
        }
        Some("message") | None if object.contains_key("role") => {
            let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = object
                .get("content")
                .map(convert_message_content)
                .transpose()?
                .unwrap_or(Value::String(String::new()));
            Ok(json!({"role": role, "content": content}))
        }
        Some(other) => Err(format!("Unsupported Responses input item type: {other}")),
        None => Err("Responses input item is missing role or type".to_string()),
    }
}

fn convert_message_content(content: &Value) -> Result<Value, String> {
    match content {
        Value::String(text) => Ok(Value::String(text.clone())),
        Value::Array(parts) => {
            let mut converted = Vec::new();
            for part in parts {
                let object = part
                    .as_object()
                    .ok_or_else(|| "Responses content parts must be objects".to_string())?;
                match object.get("type").and_then(Value::as_str) {
                    Some("input_text" | "output_text" | "text") => {
                        let text = required_string(object, "text", "text content")?;
                        converted.push(json!({"type": "text", "text": text}));
                    }
                    Some("input_image") => {
                        let image_url = required_string(object, "image_url", "input_image")?;
                        converted.push(json!({
                            "type": "image_url",
                            "image_url": {"url": image_url}
                        }));
                    }
                    Some(other) => {
                        return Err(format!("Unsupported Responses content type: {other}"));
                    }
                    None => return Err("Responses content part is missing type".to_string()),
                }
            }
            if converted.len() == 1
                && converted[0].get("type").and_then(Value::as_str) == Some("text")
            {
                return Ok(converted[0]
                    .get("text")
                    .cloned()
                    .unwrap_or_else(|| Value::String(String::new())));
            }
            Ok(Value::Array(converted))
        }
        Value::Null => Ok(Value::String(String::new())),
        _ => Err("Responses message content must be a string or array".to_string()),
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
        let mut function = Map::new();
        function.insert("name".to_string(), Value::String(name.to_string()));
        if let Some(description) = object.get("description") {
            function.insert("description".to_string(), description.clone());
        }
        function.insert(
            "parameters".to_string(),
            object
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
        );
        if let Some(strict) = object.get("strict") {
            function.insert("strict".to_string(), strict.clone());
        }
        converted.push(json!({"type": "function", "function": function}));
    }
    Ok(Value::Array(converted))
}

fn convert_tool_choice(tool_choice: &Value) -> Result<Value, String> {
    let Some(object) = tool_choice.as_object() else {
        return Ok(tool_choice.clone());
    };
    match object.get("type").and_then(Value::as_str) {
        Some("function") => {
            let name = required_string(object, "name", "function tool choice")?;
            Ok(json!({"type": "function", "function": {"name": name}}))
        }
        Some(other) => Err(format!("Unsupported Responses tool choice type: {other}")),
        None => Ok(tool_choice.clone()),
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

fn copy_fields(source: &Map<String, Value>, target: &mut Map<String, Value>, fields: &[&str]) {
    for field in fields {
        if let Some(value) = source.get(*field) {
            target.insert((*field).to_string(), value.clone());
        }
    }
}

fn is_openai_o_series(model: &str) -> bool {
    model.len() > 1
        && model.starts_with('o')
        && model
            .as_bytes()
            .get(1)
            .is_some_and(|byte| byte.is_ascii_digit())
}
