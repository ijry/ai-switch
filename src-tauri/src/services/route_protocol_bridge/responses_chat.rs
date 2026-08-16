use super::common::{
    chat_reasoning_effort, flatten_responses_function_tools, is_responses_builtin_tool_type,
    response_tool_name, response_tool_namespace, response_tool_parameters,
    responses_reasoning_effort, responses_tool_namespaces, ResponsesToolNamespaces,
};
use super::TransformedBridgeResponse;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Non-empty stand-in used when a `tool_calls` assistant message must carry
/// `reasoning_content` (DeepSeek/MiMo protocol) but the real reasoning was lost.
/// Kept short and neutral; DeepSeek-family models only require the field to be a
/// non-empty string, not to match the original chain-of-thought.
const TOOL_CALL_REASONING_PLACEHOLDER: &str = "...";
const CHAT_AGENT_CONTINUATION_INSTRUCTION: &str = "Chat Completions tool-call compatibility: when tools are available and more work is needed, include the tool call in the same assistant response. If the upstream cannot combine progress text with tool_calls, omit the progress text and emit the tool call directly. Do not end the response with only a progress update, plan, or statement of the next action; use a text-only response only when the task is complete or the user explicitly requested analysis only.";

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
    let tool_namespaces = responses_tool_namespaces(object.get("tools"))?;
    if let Some(input) = object.get("input") {
        messages.extend(convert_input(input, &tool_namespaces)?);
    }
    normalize_empty_message_content(&mut messages);
    result.insert("messages".to_string(), Value::Array(messages));

    if let Some(effort) =
        responses_reasoning_effort(object).and_then(|effort| chat_reasoning_effort(&effort))
    {
        result.insert("reasoning_effort".to_string(), json!(effort));
    }

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

    let mut has_tools = false;
    if let Some(tools) = object.get("tools") {
        let converted_tools = convert_tools(tools)?;
        has_tools = converted_tools
            .as_array()
            .is_some_and(|tools| !tools.is_empty());
        if has_tools {
            result.insert("tools".to_string(), converted_tools);
            add_agent_tool_continuation_instruction(&mut result);
        }
    }
    if let Some(tool_choice) = object.get("tool_choice") {
        if let Some(converted_tool_choice) = convert_tool_choice(tool_choice, has_tools)? {
            result.insert("tool_choice".to_string(), converted_tool_choice);
        }
    }

    serde_json::to_vec(&Value::Object(result))
        .map_err(|error| format!("Could not serialize Chat request: {error}"))
}

pub(super) fn chat_response_to_responses(
    status: u16,
    content_type: Option<&str>,
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
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
            body: chat_sse_to_responses(body, tool_namespaces)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }

    Ok(TransformedBridgeResponse {
        body: chat_json_to_responses(body, tool_namespaces)?,
        content_type: Some("application/json".to_string()),
    })
}

fn chat_json_to_responses(
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Vec<u8>, String> {
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
    let mut output = build_output_items(response_id, &text, &tool_calls, true, tool_namespaces)?;
    if let Some(reasoning) = message_reasoning_text(message) {
        output.insert(0, reasoning_output_item(&reasoning_item_id(response_id), reasoning));
    }
    if output.is_empty() {
        output.push(message_output_item(&message_item_id(response_id), "completed", ""));
    }
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

fn chat_sse_to_responses(
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Vec<u8>, String> {
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
        if let Some(reasoning) = delta_reasoning_text(delta) {
            emit_reasoning_delta(&mut state, &mut output, &mut sequence_number, reasoning)?;
        }
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            emit_text_delta(&mut state, &mut output, &mut sequence_number, content)?;
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for tool_call in tool_calls {
                emit_tool_call_delta(
                    &mut state,
                    &mut output,
                    &mut sequence_number,
                    tool_call,
                    tool_namespaces,
                )?;
            }
        }
    }

    if !state.started && !saw_done {
        return Err("Chat SSE response did not contain data events".to_string());
    }
    ensure_stream_started(&mut state, &mut output, &mut sequence_number);
    finish_stream(
        &mut state,
        &mut output,
        &mut sequence_number,
        tool_namespaces,
    )?;
    Ok(output.into_bytes())
}

#[derive(Debug, Default)]
struct ChatStreamState {
    response_id: String,
    model: String,
    created_at: i64,
    started: bool,
    reasoning_started: bool,
    reasoning: String,
    reasoning_output_index: usize,
    text_started: bool,
    text: String,
    text_output_index: usize,
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

fn emit_reasoning_delta(
    state: &mut ChatStreamState,
    output: &mut String,
    sequence_number: &mut u64,
    delta: &str,
) -> Result<(), String> {
    let item_id = reasoning_item_id(state.response_id());
    if !state.reasoning_started {
        state.reasoning_output_index = state.next_output_index;
        state.next_output_index += 1;
        let output_index = state.reasoning_output_index;
        push_sse_event(
            output,
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "sequence_number": *sequence_number,
                "output_index": output_index,
                "item": reasoning_output_item(&item_id, "")
            }),
        )?;
        *sequence_number += 1;
        push_sse_event(
            output,
            "response.reasoning_summary_part.added",
            json!({
                "type": "response.reasoning_summary_part.added",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": output_index,
                "summary_index": 0,
                "part": {"type": "summary_text", "text": ""}
            }),
        )?;
        *sequence_number += 1;
        state.reasoning_started = true;
    }
    let output_index = state.reasoning_output_index;
    state.reasoning.push_str(delta);
    push_sse_event(
        output,
        "response.reasoning_summary_text.delta",
        json!({
            "type": "response.reasoning_summary_text.delta",
            "sequence_number": *sequence_number,
            "item_id": item_id,
            "output_index": output_index,
            "summary_index": 0,
            "delta": delta
        }),
    )?;
    *sequence_number += 1;
    Ok(())
}

fn emit_text_delta(
    state: &mut ChatStreamState,
    output: &mut String,
    sequence_number: &mut u64,
    delta: &str,
) -> Result<(), String> {
    let item_id = message_item_id(state.response_id());
    if !state.text_started {
        state.text_output_index = state.next_output_index;
        state.next_output_index += 1;
        let output_index = state.text_output_index;
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
    }
    let output_index = state.text_output_index;
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
    tool_namespaces: &ResponsesToolNamespaces,
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
                    "in_progress",
                    tool_namespaces,
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
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<(), String> {
    let mut final_output = Vec::new();
    if state.reasoning_started {
        let item_id = reasoning_item_id(state.response_id());
        let output_index = state.reasoning_output_index;
        push_sse_event(
            output,
            "response.reasoning_summary_text.done",
            json!({
                "type": "response.reasoning_summary_text.done",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": output_index,
                "summary_index": 0,
                "text": state.reasoning
            }),
        )?;
        *sequence_number += 1;
        push_sse_event(
            output,
            "response.reasoning_summary_part.done",
            json!({
                "type": "response.reasoning_summary_part.done",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": output_index,
                "summary_index": 0,
                "part": {"type": "summary_text", "text": state.reasoning}
            }),
        )?;
        *sequence_number += 1;
        let item = reasoning_output_item(&item_id, &state.reasoning);
        push_sse_event(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "sequence_number": *sequence_number,
                "output_index": output_index,
                "item": item
            }),
        )?;
        *sequence_number += 1;
        final_output.push(reasoning_output_item(&item_id, &state.reasoning));
    }
    if state.text_started {
        let item_id = message_item_id(state.response_id());
        let output_index = state.text_output_index;
        push_sse_event(
            output,
            "response.output_text.done",
            json!({
                "type": "response.output_text.done",
                "sequence_number": *sequence_number,
                "item_id": item_id,
                "output_index": output_index,
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
                "output_index": output_index,
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
                "output_index": output_index,
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
            tool_namespaces,
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
            tool_namespaces,
        ));
    }
    final_output.sort_by_key(|item| match item.get("type").and_then(Value::as_str) {
        Some("reasoning") => 0,
        Some("message") => 1,
        _ => 2,
    });
    if final_output.is_empty() {
        let item_id = message_item_id(state.response_id());
        final_output.push(message_output_item(&item_id, "completed", ""));
    }
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
    tool_namespaces: &ResponsesToolNamespaces,
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
            tool_namespaces,
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
    tool_namespaces: &ResponsesToolNamespaces,
) -> Value {
    let mut item = json!({
        "id": item_id,
        "type": "function_call",
        "status": status,
        "call_id": call_id,
        "name": name,
        "arguments": arguments
    });
    let response_name = response_tool_name(name, tool_namespaces);
    item["name"] = Value::String(response_name.to_string());
    if let Some(namespace) = response_tool_namespace(name, tool_namespaces) {
        item["namespace"] = Value::String(namespace.to_string());
    }
    item
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

fn reasoning_item_id(response_id: &str) -> String {
    format!("rs_{}", sanitize_id(response_id))
}

fn reasoning_output_item(item_id: &str, text: &str) -> Value {
    let summary = if text.is_empty() {
        json!([])
    } else {
        json!([{"type": "summary_text", "text": text}])
    };
    json!({
        "id": item_id,
        "type": "reasoning",
        "summary": summary
    })
}

fn delta_reasoning_text(delta: &Value) -> Option<&str> {
    delta
        .get("reasoning_content")
        .and_then(Value::as_str)
        .or_else(|| delta.get("reasoning").and_then(Value::as_str))
        .filter(|text| !text.is_empty())
}

fn message_reasoning_text(message: &Value) -> Option<&str> {
    message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .or_else(|| message.get("reasoning").and_then(Value::as_str))
        .filter(|text| !text.is_empty())
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

fn convert_input(
    input: &Value,
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Vec<Value>, String> {
    match input {
        Value::String(text) => Ok(vec![json!({"role": "user", "content": text})]),
        Value::Array(items) => convert_input_items(items, tool_namespaces),
        Value::Object(object) => {
            convert_input_items(&[Value::Object(object.clone())], tool_namespaces)
        }
        Value::Null => Ok(Vec::new()),
        _ => Err("Responses input must be a string, object, or array".to_string()),
    }
}

fn convert_input_items(
    items: &[Value],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Vec<Value>, String> {
    let mut messages = Vec::new();
    let mut pending_tool_calls = Vec::new();
    let mut pending_reasoning = None;
    let mut last_assistant_index = None;

    for item in items {
        convert_input_item(
            item,
            tool_namespaces,
            &mut messages,
            &mut pending_tool_calls,
            &mut pending_reasoning,
            &mut last_assistant_index,
        )?;
    }

    flush_pending_tool_calls(
        &mut messages,
        &mut pending_tool_calls,
        &mut pending_reasoning,
        &mut last_assistant_index,
    );
    attach_pending_reasoning_to_previous_assistant(
        &mut messages,
        last_assistant_index,
        &mut pending_reasoning,
    );
    ensure_tool_call_reasoning(&mut messages);
    Ok(messages)
}

/// Chat gateways (DeepSeek/MiMo relays, e.g. v2ex) reject a request with
/// `Message content must not be empty` when any message has empty/whitespace
/// content and no tool_calls to justify it. Codex history can produce such
/// messages (an assistant turn that was pure reasoning, an empty text part, an
/// empty tool output). Drop the empty conversational messages and give empty
/// `tool` results a placeholder (they can't be dropped — they pair with a
/// tool_call_id). Assistant messages that carry tool_calls keep their null
/// content, which is spec-compliant.
fn normalize_empty_message_content(messages: &mut Vec<Value>) {
    messages.retain_mut(|message| {
        let Some(object) = message.as_object_mut() else {
            return true;
        };
        let has_tool_calls = object
            .get("tool_calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| !calls.is_empty());
        if has_tool_calls {
            return true;
        }
        let content_empty = match object.get("content") {
            None | Some(Value::Null) => true,
            Some(Value::String(text)) => text.trim().is_empty(),
            Some(Value::Array(parts)) => parts.is_empty(),
            _ => false,
        };
        if !content_empty {
            return true;
        }
        if object.get("role").and_then(Value::as_str) == Some("tool") {
            object.insert("content".to_string(), Value::String(" ".to_string()));
            true
        } else {
            false
        }
    });
}

/// MiMo/DeepSeek reject a follow-up turn with `400 The reasoning_content in the
/// thinking mode must be passed back to the API` when an assistant message that
/// carries `tool_calls` has no `reasoning_content`. That happens whenever the
/// upstream reasoning was lost — client-side context compaction, a tool turn the
/// model emitted without reasoning, or history predating reasoning round-trip.
/// Guarantee the field is present (placeholder when the real one is gone) so the
/// conversation keeps working instead of hard-failing.
fn ensure_tool_call_reasoning(messages: &mut [Value]) {
    for message in messages.iter_mut() {
        let Some(object) = message.as_object_mut() else {
            continue;
        };
        let is_assistant = object.get("role").and_then(Value::as_str) == Some("assistant");
        let has_tool_calls = object
            .get("tool_calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| !calls.is_empty());
        if !is_assistant || !has_tool_calls {
            continue;
        }
        let has_reasoning = object
            .get("reasoning_content")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty());
        if !has_reasoning {
            object.insert(
                "reasoning_content".to_string(),
                Value::String(TOOL_CALL_REASONING_PLACEHOLDER.to_string()),
            );
        }
    }
}

fn convert_input_item(
    item: &Value,
    tool_namespaces: &ResponsesToolNamespaces,
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
    last_assistant_index: &mut Option<usize>,
) -> Result<(), String> {
    let object = item
        .as_object()
        .ok_or_else(|| "Responses input items must be JSON objects".to_string())?;
    match object.get("type").and_then(Value::as_str) {
        Some("function_call") => {
            append_pending_reasoning(pending_reasoning, reasoning_text(item));
            pending_tool_calls.push(function_call_to_chat(object, tool_namespaces)?);
        }
        Some("function_call_output") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
            messages.push(tool_result_message(object, "function_call_output")?);
            *last_assistant_index = None;
        }
        Some("custom_tool_call") | Some("tool_search_call") => {
            append_pending_reasoning(pending_reasoning, reasoning_text(item));
            pending_tool_calls.push(synthetic_tool_call(object)?);
        }
        Some("custom_tool_call_output") | Some("tool_search_output") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
            messages.push(tool_result_message(object, "tool_output")?);
            *last_assistant_index = None;
        }
        Some("reasoning") => {
            append_pending_reasoning(pending_reasoning, reasoning_text(item));
        }
        Some("input_text") | Some("input_image") | Some("input_file") | Some("input_audio") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
            let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
            let message = json!({
                "role": chat_role(role),
                "content": convert_message_content(&Value::Array(vec![item.clone()]))?
            });
            append_message_with_reasoning(
                messages,
                message,
                pending_reasoning,
                last_assistant_index,
            );
        }
        Some("message") | None if object.contains_key("role") || object.contains_key("content") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
            let content = object
                .get("content")
                .map(convert_message_content)
                .transpose()?
                .unwrap_or(Value::String(String::new()));
            let message = json!({
                "role": chat_role(object.get("role").and_then(Value::as_str).unwrap_or("user")),
                "content": content
            });
            append_message_with_reasoning(
                messages,
                message,
                pending_reasoning,
                last_assistant_index,
            );
        }
        Some("web_search_call")
        | Some("web_search_call_output")
        | Some("file_search_call")
        | Some("file_search_call_output")
        | Some("computer_call")
        | Some("computer_call_output")
        | Some("local_shell_call")
        | Some("local_shell_call_output") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
        }
        Some(other) => return Err(format!("Unsupported Responses input item type: {other}")),
        None => return Err("Responses input item is missing role or type".to_string()),
    }
    Ok(())
}

fn flush_pending_tool_calls(
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
    last_assistant_index: &mut Option<usize>,
) {
    if pending_tool_calls.is_empty() {
        return;
    }
    let mut message = json!({
        "role": "assistant",
        "content": Value::Null,
        "tool_calls": std::mem::take(pending_tool_calls)
    });
    attach_reasoning_content(&mut message, pending_reasoning.take());
    *last_assistant_index = Some(messages.len());
    messages.push(message);
}

fn append_message_with_reasoning(
    messages: &mut Vec<Value>,
    mut message: Value,
    pending_reasoning: &mut Option<String>,
    last_assistant_index: &mut Option<usize>,
) {
    let is_assistant = message.get("role").and_then(Value::as_str) == Some("assistant");
    if is_assistant {
        attach_reasoning_content(&mut message, pending_reasoning.take());
        *last_assistant_index = Some(messages.len());
    } else {
        attach_pending_reasoning_to_previous_assistant(
            messages,
            *last_assistant_index,
            pending_reasoning,
        );
        *last_assistant_index = None;
    }
    messages.push(message);
}

fn attach_pending_reasoning_to_previous_assistant(
    messages: &mut [Value],
    last_assistant_index: Option<usize>,
    pending_reasoning: &mut Option<String>,
) {
    let Some(reasoning) = pending_reasoning.take() else {
        return;
    };
    let Some(message) = last_assistant_index.and_then(|index| messages.get_mut(index)) else {
        return;
    };
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    attach_reasoning_content(message, Some(reasoning));
}

fn attach_reasoning_content(message: &mut Value, reasoning: Option<String>) {
    let Some(reasoning) = reasoning.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    let Some(object) = message.as_object_mut() else {
        return;
    };
    match object.get_mut("reasoning_content") {
        Some(Value::String(existing)) if !existing.is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(&reasoning);
        }
        _ => {
            object.insert("reasoning_content".to_string(), Value::String(reasoning));
        }
    }
}

fn append_pending_reasoning(pending_reasoning: &mut Option<String>, reasoning: Option<String>) {
    let Some(reasoning) = reasoning.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    match pending_reasoning {
        Some(existing) if !existing.is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(&reasoning);
        }
        _ => *pending_reasoning = Some(reasoning),
    }
}

fn reasoning_text(item: &Value) -> Option<String> {
    // Restored/inline plaintext reasoning (e.g. from the reasoning cache) wins:
    // it carries the model's actual chain-of-thought for this tool-call turn.
    for key in ["reasoning_content", "reasoning"] {
        if let Some(text) = item.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return Some(text.to_string());
            }
        }
    }
    let summary = item
        .get("summary")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| {
            matches!(
                part.get("type").and_then(Value::as_str),
                Some("summary_text" | "reasoning_text")
            )
            .then(|| part.get("text").and_then(Value::as_str))
            .flatten()
        })
        .collect::<Vec<_>>()
        .join("");
    if !summary.is_empty() {
        return Some(summary);
    }
    item.get("content")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn function_call_to_chat(
    object: &Map<String, Value>,
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Value, String> {
    let call_id = object
        .get("call_id")
        .or_else(|| object.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| "Responses function_call is missing call_id".to_string())?;
    let name = required_string(object, "name", "function_call")?;
    let namespace = object.get("namespace").and_then(Value::as_str);
    let name = namespace
        .filter(|value| !value.trim().is_empty())
        .map(|namespace| super::common::qualified_response_tool_name(namespace, name))
        .unwrap_or_else(|| {
            if tool_namespaces.contains_key(name) {
                name.to_string()
            } else {
                name.to_string()
            }
        });
    let arguments = object
        .get("arguments")
        .map(stringify_content)
        .transpose()?
        .unwrap_or_else(|| "{}".to_string());
    Ok(json!({
        "id": call_id,
        "type": "function",
        "function": {"name": name, "arguments": arguments}
    }))
}

fn synthetic_tool_call(object: &Map<String, Value>) -> Result<Value, String> {
    let call_id = object
        .get("call_id")
        .or_else(|| object.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| "Responses tool call is missing call_id".to_string())?;
    let item_type = object.get("type").and_then(Value::as_str).unwrap_or("");
    let name = if item_type == "tool_search_call" {
        "tool_search"
    } else {
        required_string(object, "name", "custom_tool_call")?
    };
    let arguments = if item_type == "custom_tool_call" {
        json!({"input": object.get("input").cloned().unwrap_or(Value::String(String::new()))})
    } else {
        object
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}))
    };
    Ok(json!({
        "id": call_id,
        "type": "function",
        "function": {"name": name, "arguments": serde_json::to_string(&arguments).unwrap_or_else(|_| "{}".to_string())}
    }))
}

fn tool_result_message(object: &Map<String, Value>, label: &str) -> Result<Value, String> {
    let call_id = required_string(object, "call_id", label)?;
    let output = object
        .get("output")
        .or_else(|| object.get("result"))
        .map(stringify_content)
        .transpose()?
        .unwrap_or_default();
    Ok(json!({"role": "tool", "tool_call_id": call_id, "content": output}))
}

fn chat_role(role: &str) -> &'static str {
    match role {
        "system" | "developer" => "system",
        "assistant" => "assistant",
        "tool" => "tool",
        _ => "user",
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
                        if let Some(text) = object.get("text").and_then(Value::as_str) {
                            if !text.is_empty() {
                                converted.push(json!({"type": "text", "text": text}));
                            }
                        }
                    }
                    Some("refusal") => {
                        if let Some(text) = object.get("refusal").and_then(Value::as_str) {
                            if !text.is_empty() {
                                converted.push(json!({"type": "text", "text": text}));
                            }
                        }
                    }
                    Some("input_image") => {
                        if let Some(image_url) = object.get("image_url") {
                            let image_url = if image_url.is_object() {
                                image_url.clone()
                            } else {
                                json!({ "url": image_url.as_str().unwrap_or_default() })
                            };
                            converted.push(json!({
                                "type": "image_url",
                                "image_url": image_url
                            }));
                        }
                    }
                    Some(_) | None => {}
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
            if converted.is_empty() {
                return Ok(Value::String(String::new()));
            }
            Ok(Value::Array(converted))
        }
        Value::Null => Ok(Value::String(String::new())),
        _ => Err("Responses message content must be a string or array".to_string()),
    }
}

fn convert_tools(tools: &Value) -> Result<Value, String> {
    let tools = flatten_responses_function_tools(tools)?;
    let mut converted = Vec::with_capacity(tools.len());
    for object in tools {
        let name = required_string(&object, "name", "function tool")?;
        let mut function = Map::new();
        function.insert("name".to_string(), Value::String(name.to_string()));
        if let Some(description) = object.get("description") {
            function.insert("description".to_string(), description.clone());
        }
        let parameters = if object.get("type").and_then(Value::as_str) == Some("custom") {
            json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Raw string input for the original Responses custom tool."
                    }
                },
                "required": ["input"]
            })
        } else {
            response_tool_parameters(&object)
        };
        function.insert("parameters".to_string(), parameters);
        if let Some(strict) = object.get("strict") {
            function.insert("strict".to_string(), strict.clone());
        }
        converted.push(json!({"type": "function", "function": function}));
    }
    Ok(Value::Array(converted))
}

fn add_agent_tool_continuation_instruction(result: &mut Map<String, Value>) {
    let Some(messages) = result.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    if messages.iter().any(|message| {
        message
            .get("content")
            .and_then(Value::as_str)
            .is_some_and(|content| content.contains(CHAT_AGENT_CONTINUATION_INSTRUCTION))
    }) {
        return;
    }
    if let Some(index) = messages.iter().position(|message| {
        message.get("role").and_then(Value::as_str) == Some("system")
            && message.get("content").and_then(Value::as_str).is_some()
    }) {
        let mut text = messages[index]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if !text.trim().is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(CHAT_AGENT_CONTINUATION_INSTRUCTION);
        messages[index]["content"] = Value::String(text);
        return;
    }
    messages.insert(
        0,
        json!({
            "role": "system",
            "content": CHAT_AGENT_CONTINUATION_INSTRUCTION
        }),
    );
}

fn convert_tool_choice(tool_choice: &Value, has_tools: bool) -> Result<Option<Value>, String> {
    let Some(object) = tool_choice.as_object() else {
        if !has_tools {
            return Ok(match tool_choice.as_str() {
                Some("required") => Some(json!("auto")),
                _ => None,
            });
        }
        return Ok(Some(tool_choice.clone()));
    };
    match object.get("type").and_then(Value::as_str) {
        Some("function" | "custom") => {
            let name = required_string(object, "name", "function tool choice")?;
            if has_tools {
                Ok(Some(
                    json!({"type": "function", "function": {"name": name}}),
                ))
            } else {
                Ok(Some(json!("auto")))
            }
        }
        Some(other) if is_responses_builtin_tool_type(other) => {
            Ok(has_tools.then(|| json!("auto")))
        }
        Some(other) => Err(format!("Unsupported Responses tool choice type: {other}")),
        None => Ok(has_tools.then(|| tool_choice.clone())),
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
