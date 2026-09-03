//! Inbound OpenAI Chat Completions bridged to an OpenAI Responses upstream.
//!
//! The mirror of [`super::responses_chat`], which carries Responses inbound to a
//! chat upstream. This direction exists because a client's wire shape is a
//! property of the client, not of the pool: the third-party clients that only
//! speak chat completions are configured for codex-platform pools too, and a
//! Responses-only relay rejects a chat body outright.

use super::TransformedBridgeResponse;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Stand-in id for a response whose body or stream never named one. Chat clients
/// key their accumulator on `id`, so it cannot be empty.
const DEFAULT_RESPONSE_ID: &str = "chatcmpl-bridge";

pub(super) fn chat_request_to_responses(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Chat request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Chat request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    if let Some(model) = object.get("model") {
        result.insert("model".to_string(), model.clone());
    }

    let mut instructions = Vec::new();
    let mut input = Vec::new();
    if let Some(items) = object.get("messages").and_then(Value::as_array) {
        for item in items {
            convert_message(item, &mut instructions, &mut input)?;
        }
    }
    // Responses keeps the system prompt in `instructions`, outside the item list a
    // client replays every turn. Several prompts are joined instead of kept in
    // place: chat lets a `system` message sit anywhere in the conversation, and
    // Responses has no slot for a mid-conversation one.
    if !instructions.is_empty() {
        result.insert(
            "instructions".to_string(),
            Value::String(instructions.join("\n\n")),
        );
    }
    result.insert("input".to_string(), Value::Array(input));

    // Unlike Anthropic, Responses does not require a cap, so an absent one stays
    // absent: an invented ceiling would truncate an answer the upstream would
    // otherwise have finished.
    if let Some(limit) = object
        .get("max_tokens")
        .or_else(|| object.get("max_completion_tokens"))
    {
        result.insert("max_output_tokens".to_string(), limit.clone());
    }

    for field in ["temperature", "top_p", "stream", "parallel_tool_calls"] {
        if let Some(found) = object.get(field) {
            result.insert(field.to_string(), found.clone());
        }
    }
    // `stop`, `n`, the penalty knobs, and `stream_options` are dropped rather than
    // forwarded: Responses has no equivalent parameter and strict upstreams reject
    // an unknown body field outright, which would turn a serviceable request into
    // a 400. Streamed usage needs no opt-in either — it rides on the terminal
    // `response.completed` event.

    if let Some(tools) = object.get("tools") {
        result.insert("tools".to_string(), convert_tools(tools)?);
    }
    if let Some(tool_choice) = object.get("tool_choice") {
        if let Some(converted) = convert_tool_choice(tool_choice)? {
            result.insert("tool_choice".to_string(), converted);
        }
    }

    serde_json::to_vec(&Value::Object(result))
        .map_err(|error| format!("Could not serialize Responses request: {error}"))
}

pub(super) fn responses_response_to_chat(
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
            body: responses_sse_to_chat(body)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }

    Ok(TransformedBridgeResponse {
        body: responses_json_to_chat(body)?,
        content_type: Some("application/json".to_string()),
    })
}

/// Routes one chat message into either the Responses `instructions` text or the
/// `input` item list.
///
/// Responses has no array of role/content pairs: the system prompt lives in
/// `instructions`, a tool call is a top-level `function_call` item rather than a
/// field on the assistant message, and a tool result is a `function_call_output`
/// item rather than a `tool` role. All three are shape changes, not renames.
fn convert_message(
    message: &Value,
    instructions: &mut Vec<String>,
    input: &mut Vec<Value>,
) -> Result<(), String> {
    let object = message
        .as_object()
        .ok_or_else(|| "Chat messages entries must be objects".to_string())?;
    let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
    match role {
        "system" | "developer" => {
            let text = message_text(object.get("content"))?;
            if !text.is_empty() {
                instructions.push(text);
            }
            Ok(())
        }
        "user" => {
            let content = user_content(object.get("content"))?;
            if !content.is_empty() {
                input.push(json!({"type": "message", "role": "user", "content": content}));
            }
            Ok(())
        }
        "assistant" => push_assistant_items(object, input),
        "tool" => {
            let call_id = object
                .get("tool_call_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Chat tool message is missing tool_call_id".to_string())?;
            input.push(json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": message_text(object.get("content"))?
            }));
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

/// A user turn keeps its parts, but Responses names them by direction:
/// `input_text` and `input_image` rather than chat's `text` and `image_url`.
fn user_content(content: Option<&Value>) -> Result<Vec<Value>, String> {
    match content {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(text)) => Ok(if text.is_empty() {
            Vec::new()
        } else {
            vec![json!({"type": "input_text", "text": text})]
        }),
        Some(Value::Array(parts)) => {
            let mut converted = Vec::new();
            for part in parts {
                match part.get("type").and_then(Value::as_str).unwrap_or("text") {
                    "text" => {
                        let text = part.get("text").and_then(Value::as_str).unwrap_or_default();
                        if !text.is_empty() {
                            converted.push(json!({"type": "input_text", "text": text}));
                        }
                    }
                    "image_url" => converted.push(convert_image_part(part)?),
                    other => return Err(format!("Unsupported Chat content part: {other}")),
                }
            }
            Ok(converted)
        }
        Some(other) => Err(format!("Unsupported Chat content shape: {other}")),
    }
}

/// Responses takes the image as a plain URL string, so a data URL survives
/// untouched — unlike the Anthropic bridge, which has to split it into a media
/// type and a base64 payload.
fn convert_image_part(part: &Value) -> Result<Value, String> {
    let url = part
        .pointer("/image_url/url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Chat image part is missing image_url.url".to_string())?;
    Ok(json!({"type": "input_image", "image_url": url}))
}

/// An assistant turn carries text and `tool_calls` side by side in chat, so one
/// message fans out into a `message` item followed by one `function_call` item per
/// call.
///
/// A replayed `reasoning_content` is dropped: Responses reasoning items are opaque
/// objects the model itself issued, and a synthesized one is rejected.
fn push_assistant_items(object: &Map<String, Value>, input: &mut Vec<Value>) -> Result<(), String> {
    let text = message_text(object.get("content"))?;
    if !text.is_empty() {
        input.push(json!({
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": text}]
        }));
    }
    for call in object
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let call_id = call
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Chat tool_call is missing id".to_string())?;
        let name = call
            .pointer("/function/name")
            .and_then(Value::as_str)
            .ok_or_else(|| "Chat tool_call is missing function.name".to_string())?;
        // Both dialects carry the arguments as a JSON *string*, so this is the one
        // tool field that needs no reshaping at all.
        let arguments = call
            .pointer("/function/arguments")
            .and_then(Value::as_str)
            .unwrap_or("{}");
        input.push(json!({
            "type": "function_call",
            "call_id": call_id,
            "name": name,
            "arguments": arguments
        }));
    }
    Ok(())
}

/// Chat nests a declaration under `function`; Responses keeps the same fields flat
/// on the tool object.
fn convert_tools(tools: &Value) -> Result<Value, String> {
    let items = tools
        .as_array()
        .ok_or_else(|| "Chat tools must be an array".to_string())?;
    let mut converted = Vec::with_capacity(items.len());
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
        entry.insert("type".to_string(), json!("function"));
        entry.insert("name".to_string(), json!(name));
        if let Some(description) = function.get("description") {
            entry.insert("description".to_string(), description.clone());
        }
        entry.insert(
            "parameters".to_string(),
            function
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
        );
        if let Some(strict) = function.get("strict") {
            entry.insert("strict".to_string(), strict.clone());
        }
        converted.push(Value::Object(entry));
    }
    Ok(Value::Array(converted))
}

/// Responses has a real `none`, so — unlike the Anthropic bridge — nothing has to
/// be dropped to keep a client's "do not call tools" honest.
fn convert_tool_choice(tool_choice: &Value) -> Result<Option<Value>, String> {
    match tool_choice {
        Value::String(text) => match text.as_str() {
            "auto" => Ok(Some(json!("auto"))),
            "required" | "any" => Ok(Some(json!("required"))),
            "none" => Ok(Some(json!("none"))),
            other => Err(format!("Unsupported Chat tool_choice: {other}")),
        },
        Value::Object(_) => {
            let name = tool_choice
                .pointer("/function/name")
                .and_then(Value::as_str)
                .ok_or_else(|| "Chat tool_choice object needs function.name".to_string())?;
            Ok(Some(json!({"type": "function", "name": name})))
        }
        Value::Null => Ok(None),
        other => Err(format!("Unsupported Chat tool_choice shape: {other}")),
    }
}

fn responses_json_to_chat(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Responses response JSON is invalid: {error}"))?;
    // A failed turn is forwarded as a chat error document rather than as assistant
    // text: the proxy keys credential failover on `error`, so dressing the message
    // up as model output would report a dead upstream as a finished answer.
    if value.get("status").and_then(Value::as_str) == Some("failed")
        || value.get("error").and_then(Value::as_object).is_some()
    {
        return serde_json::to_vec(&chat_error(&value))
            .map_err(|error| format!("Could not serialize Chat error: {error}"));
    }
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut tool_calls = Vec::new();
    for item in value
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                for part in item
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    text.push_str(output_text(part));
                }
            }
            // Some relays hoist the text part to the top of `output` instead of
            // wrapping it in a message item.
            Some("output_text") => text.push_str(output_text(item)),
            Some("reasoning") => reasoning.push_str(&reasoning_text(item)),
            Some("function_call") => tool_calls.push(tool_call_from_item(item, tool_calls.len())),
            _ => {}
        }
    }
    // The flattened convenience field is the only content some relays report.
    if text.is_empty() {
        text.push_str(
            value
                .get("output_text")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
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
    // DeepSeek's spelling, which is what the chat side of this codebase already
    // reads and writes; a client that does not know the field ignores it.
    if !reasoning.is_empty() {
        message.insert("reasoning_content".to_string(), json!(reasoning));
    }
    if !tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(tool_calls.clone()));
    }

    let response = json!({
        "id": value.get("id").and_then(Value::as_str).unwrap_or(DEFAULT_RESPONSE_ID),
        "object": "chat.completion",
        "created": 0,
        "model": value.get("model").and_then(Value::as_str).unwrap_or_default(),
        "choices": [{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": finish_reason(
                value.get("status").and_then(Value::as_str),
                value.pointer("/incomplete_details/reason").and_then(Value::as_str),
                !tool_calls.is_empty(),
            ),
        }],
        "usage": usage_to_chat(value.get("usage")),
    });
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Chat response: {error}"))
}
fn output_text(part: &Value) -> &str {
    match part.get("type").and_then(Value::as_str) {
        Some("output_text" | "text") => {
            part.get("text").and_then(Value::as_str).unwrap_or_default()
        }
        _ => "",
    }
}

/// Reasoning normally arrives as summary parts; the plaintext fields cover relays
/// that inline it and the reasoning cache that restores it.
fn reasoning_text(item: &Value) -> String {
    for key in ["reasoning_content", "reasoning"] {
        if let Some(text) = item.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return text.to_string();
            }
        }
    }
    item.get("summary")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

fn tool_call_from_item(item: &Value, index: usize) -> Value {
    json!({
        "index": index,
        // `call_id` is what the client must echo back on its `tool` message; the
        // item `id` only addresses the item inside the Responses object.
        "id": item
            .get("call_id")
            .or_else(|| item.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "type": "function",
        "function": {
            "name": item.get("name").and_then(Value::as_str).unwrap_or_default(),
            "arguments": arguments_string(item.get("arguments")),
        }
    })
}
/// Chat requires the arguments as a JSON *string*. Responses already uses one, so
/// this only has to cover a relay that answered with the parsed object instead —
/// stringifying it keeps the call runnable where dropping it would not.
fn arguments_string(arguments: Option<&Value>) -> String {
    match arguments {
        Some(Value::String(text)) => text.clone(),
        None | Some(Value::Null) => "{}".to_string(),
        Some(other) => other.to_string(),
    }
}

/// Truncation outranks `tool_calls`: arguments cut off at the token cap are not
/// parseable JSON, and a client told `tool_calls` would run the broken call rather
/// than report that the answer was cut short.
fn finish_reason(
    status: Option<&str>,
    incomplete_reason: Option<&str>,
    has_tool_calls: bool,
) -> &'static str {
    match incomplete_reason {
        Some("max_output_tokens") => return "length",
        Some("content_filter") => return "content_filter",
        _ => {}
    }
    // The token cap is the usual reason Responses stops early, so an `incomplete`
    // that states none is still reported as a truncation rather than a clean stop.
    if status == Some("incomplete") {
        return "length";
    }
    if has_tool_calls {
        return "tool_calls";
    }
    "stop"
}
fn usage_to_chat(usage: Option<&Value>) -> Value {
    let field = |name: &str| {
        usage
            .and_then(|value| value.get(name))
            .and_then(Value::as_u64)
    };
    let input = field("input_tokens").unwrap_or(0);
    let output = field("output_tokens").unwrap_or(0);
    let mut result = json!({
        "prompt_tokens": input,
        "completion_tokens": output,
        "total_tokens": field("total_tokens").unwrap_or(input + output),
    });
    // The proxy reads token usage off this translated body rather than off the
    // upstream one, so the detail counters have to survive the rename or the stats
    // view loses its cache-hit and reasoning columns. Absent details stay absent: a
    // zero would record "no cache hit" where the upstream reported nothing at all.
    if let Some(cached) = usage
        .and_then(|value| value.pointer("/input_tokens_details/cached_tokens"))
        .and_then(Value::as_u64)
    {
        result["prompt_tokens_details"] = json!({"cached_tokens": cached});
    }
    if let Some(reasoning) = usage
        .and_then(|value| value.pointer("/output_tokens_details/reasoning_tokens"))
        .and_then(Value::as_u64)
    {
        result["completion_tokens_details"] = json!({"reasoning_tokens": reasoning});
    }
    result
}

/// Renders an upstream failure as the chat error document that both a client and
/// the proxy's failure detector recognize.
fn chat_error(response: &Value) -> Value {
    let mut error = Map::new();
    error.insert(
        "message".to_string(),
        json!(response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("Responses upstream reported a failed response")),
    );
    error.insert("type".to_string(), json!("upstream_error"));
    if let Some(code) = response
        .pointer("/error/code")
        .filter(|code| !code.is_null())
    {
        error.insert("code".to_string(), code.clone());
    }
    json!({"error": Value::Object(error)})
}
fn looks_like_sse(body: &[u8]) -> bool {
    std::str::from_utf8(body).ok().is_some_and(|text| {
        text.lines()
            .any(|line| line.trim_start().starts_with("data:"))
    })
}

/// Translates the Responses event stream into chat chunks as they arrive.
///
/// Deliberately incremental rather than accumulate-then-emit: a chat client
/// streams to show progress, and replaying one whole message at the end would make
/// every turn appear to hang and then finish instantly.
fn responses_sse_to_chat(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = String::new();
    let mut id = DEFAULT_RESPONSE_ID.to_string();
    let mut model = String::new();
    let mut role_sent = false;
    // Responses numbers every output item — reasoning, message, and calls alike —
    // while a chat client keys its tool-call accumulator on a contiguous, tool-only
    // index. Reusing the output index would leave a gap the client never fills, so
    // calls are renumbered in the order they appear.
    let mut tools: BTreeMap<u64, StreamedToolCall> = BTreeMap::new();
    let mut status: Option<String> = None;
    let mut incomplete_reason: Option<String> = None;
    let mut usage: Option<Value> = None;

    for value in super::sse::parse_sse_data_records(body)? {
        if let Some(response) = value.get("response") {
            if let Some(found) = response.get("id").and_then(Value::as_str) {
                id = found.to_string();
            }
            if let Some(found) = response.get("model").and_then(Value::as_str) {
                model = found.to_string();
            }
            if let Some(found) = response.get("status").and_then(Value::as_str) {
                status = Some(found.to_string());
            }
            if let Some(found) = response
                .pointer("/incomplete_details/reason")
                .and_then(Value::as_str)
            {
                incomplete_reason = Some(found.to_string());
            }
            // `response.created` carries `usage: null`, which must not clear counts
            // a later event reported.
            if let Some(found) = response.get("usage").filter(|usage| !usage.is_null()) {
                usage = Some(found.clone());
            }
        }
        // A relay can also drop a bare error frame into the stream with no event
        // type at all, so both spellings end the turn the same way.
        if value.get("type").and_then(Value::as_str) == Some("response.failed")
            || value.get("error").and_then(Value::as_object).is_some()
        {
            let failure = chat_error(value.get("response").unwrap_or(&value));
            output.push_str(&format!("data: {failure}\n\n"));
            // Clients that block on the sentinel would otherwise hang on a stream
            // that ends with an error frame.
            output.push_str("data: [DONE]\n\n");
            return Ok(output.into_bytes());
        }

        match value.get("type").and_then(Value::as_str) {
            Some("response.created") => {
                ensure_role_chunk(&mut output, &mut role_sent, &id, &model)?
            }
            Some("response.output_text.delta") => {
                let delta = value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !delta.is_empty() {
                    ensure_role_chunk(&mut output, &mut role_sent, &id, &model)?;
                    push_chunk(&mut output, &id, &model, json!({"content": delta}), None)?;
                }
            }
            // Reasoning summaries are output the caller was billed for, so they are
            // forwarded under the field the chat side of this codebase uses.
            Some("response.reasoning_summary_text.delta" | "response.reasoning_text.delta") => {
                let delta = value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !delta.is_empty() {
                    ensure_role_chunk(&mut output, &mut role_sent, &id, &model)?;
                    push_chunk(
                        &mut output,
                        &id,
                        &model,
                        json!({"reasoning_content": delta}),
                        None,
                    )?;
                }
            }
            Some("response.output_item.added" | "response.output_item.done") => {
                let Some(item) = value.get("item") else {
                    continue;
                };
                if item.get("type").and_then(Value::as_str) != Some("function_call") {
                    continue;
                }
                ensure_role_chunk(&mut output, &mut role_sent, &id, &model)?;
                let call_id = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
                let tool = tool_entry(&mut tools, output_index(&value));
                open_tool_call(&mut output, &id, &model, tool, call_id, name)?;
                if let Some(arguments) = item.get("arguments").and_then(Value::as_str) {
                    push_tool_arguments(&mut output, &id, &model, tool, arguments, true)?;
                }
            }
            Some("response.function_call_arguments.delta")
            | Some("response.function_call_arguments.done") => {
                ensure_role_chunk(&mut output, &mut role_sent, &id, &model)?;
                let cumulative = value.get("type").and_then(Value::as_str)
                    == Some("response.function_call_arguments.done");
                let arguments = value
                    .get(if cumulative { "arguments" } else { "delta" })
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let tool = tool_entry(&mut tools, output_index(&value));
                // A relay that streams argument fragments without announcing the
                // item first leaves `item_id` as the only identifier to hand the
                // client; a call with no id at all would be unanswerable.
                let call_id = value
                    .get("item_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                open_tool_call(&mut output, &id, &model, tool, call_id, "")?;
                push_tool_arguments(&mut output, &id, &model, tool, arguments, cumulative)?;
            }
            _ => {}
        }
    }

    let finish = finish_reason(
        status.as_deref(),
        incomplete_reason.as_deref(),
        !tools.is_empty(),
    );
    ensure_role_chunk(&mut output, &mut role_sent, &id, &model)?;
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

#[derive(Debug, Default)]
struct StreamedToolCall {
    /// Contiguous chat-side index, assigned in first-seen order.
    chat_index: usize,
    /// Everything already forwarded as an `arguments` delta, so a terminal event
    /// that repeats the finished string cannot double it up.
    streamed_arguments: String,
    opened: bool,
}

fn output_index(value: &Value) -> u64 {
    value
        .get("output_index")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn tool_entry(
    tools: &mut BTreeMap<u64, StreamedToolCall>,
    output_index: u64,
) -> &mut StreamedToolCall {
    let chat_index = tools.len();
    tools
        .entry(output_index)
        .or_insert_with(|| StreamedToolCall {
            chat_index,
            ..StreamedToolCall::default()
        })
}
/// Announces the call once, with the id and name a chat client needs before any
/// argument fragment means anything.
fn open_tool_call(
    output: &mut String,
    id: &str,
    model: &str,
    tool: &mut StreamedToolCall,
    call_id: &str,
    name: &str,
) -> Result<(), String> {
    if tool.opened {
        return Ok(());
    }
    tool.opened = true;
    push_chunk(
        output,
        id,
        model,
        json!({"tool_calls": [{
            "index": tool.chat_index,
            "id": call_id,
            "type": "function",
            "function": {"name": name, "arguments": ""}
        }]}),
        None,
    )
}

/// Forwards only the part of the arguments that has not been streamed yet.
///
/// The delta events carry fragments while the `.done` events repeat the whole
/// finished string, and both feed this one path: a relay that emits only the
/// terminal event still produces a complete call, and one that emits both does not
/// deliver the arguments twice.
fn push_tool_arguments(
    output: &mut String,
    id: &str,
    model: &str,
    tool: &mut StreamedToolCall,
    arguments: &str,
    cumulative: bool,
) -> Result<(), String> {
    let fragment = if cumulative {
        // A terminal string that does not extend what was streamed means the relay
        // re-chunked it; trust the fragments already delivered.
        match arguments.strip_prefix(tool.streamed_arguments.as_str()) {
            Some(fragment) => fragment,
            None => return Ok(()),
        }
    } else {
        arguments
    };
    if fragment.is_empty() {
        return Ok(());
    }
    let chunk = json!({"tool_calls": [{
        "index": tool.chat_index,
        "function": {"arguments": fragment}
    }]});
    tool.streamed_arguments.push_str(fragment);
    push_chunk(output, id, model, chunk, None)
}
fn ensure_role_chunk(
    output: &mut String,
    role_sent: &mut bool,
    id: &str,
    model: &str,
) -> Result<(), String> {
    if *role_sent {
        return Ok(());
    }
    *role_sent = true;
    push_chunk(output, id, model, json!({"role": "assistant"}), None)
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
    fn the_system_prompt_becomes_instructions_and_tool_history_becomes_input_items() {
        let body = json!({
            "model": "gpt-5.6-sol",
            "messages": [
                {"role": "system", "content": "be terse"},
                {"role": "developer", "content": "no emoji"},
                {"role": "user", "content": "weather?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
                }]},
                {"role": "tool", "tool_call_id": "call_1", "content": "18C"}
            ],
            "max_tokens": 256,
            "stream": true,
            "tools": [{"type": "function", "function": {
                "name": "get_weather",
                "description": "look up weather",
                "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
            }}]
        });

        let converted = chat_request_to_responses(&serde_json::to_vec(&body).unwrap()).unwrap();
        let value: Value = serde_json::from_slice(&converted).unwrap();

        // Both prompt roles leave `input` for `instructions`, joined in order.
        assert_eq!(value["instructions"], "be terse\n\nno emoji");
        let input = value["input"].as_array().unwrap();
        assert_eq!(input.len(), 3, "converted={value}");
        assert_eq!(input[0]["type"], "message");
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[0]["content"][0]["type"], "input_text");
        assert_eq!(input[0]["content"][0]["text"], "weather?");
        // A tool call is its own item, not a field on the assistant turn.
        assert_eq!(input[1]["type"], "function_call");
        assert_eq!(input[1]["call_id"], "call_1");
        assert_eq!(input[1]["name"], "get_weather");
        // Both dialects carry arguments as a JSON string, so it is copied verbatim.
        assert_eq!(input[1]["arguments"], "{\"city\":\"SF\"}");
        // There is no `tool` role: a result is a function_call_output item.
        assert_eq!(input[2]["type"], "function_call_output");
        assert_eq!(input[2]["call_id"], "call_1");
        assert_eq!(input[2]["output"], "18C"); // Tool declarations lose the `function` wrapper and keep `parameters`.
        assert_eq!(value["tools"][0]["type"], "function");
        assert_eq!(value["tools"][0]["name"], "get_weather");
        assert_eq!(value["tools"][0]["description"], "look up weather");
        assert_eq!(
            value["tools"][0]["parameters"]["properties"]["city"]["type"],
            "string"
        );
        assert!(value["tools"][0].get("function").is_none());
        assert_eq!(value["max_output_tokens"], 256);
        assert!(value.get("max_tokens").is_none());
        assert_eq!(value["stream"], true);
    }

    #[test]
    fn tool_choice_keeps_its_meaning_including_none() {
        let cases = [
            (json!("auto"), json!("auto")),
            (json!("required"), json!("required")),
            // Responses has a real `none`, so unlike the Anthropic bridge nothing
            // has to be dropped to honor "do not call tools".
            (json!("none"), json!("none")),
            (
                json!({"type": "function", "function": {"name": "get_weather"}}),
                json!({"type": "function", "name": "get_weather"}),
            ),
        ];

        for (chat_choice, expected) in cases {
            let body = json!({"model": "m", "messages": [], "tool_choice": chat_choice});
            let converted = chat_request_to_responses(&serde_json::to_vec(&body).unwrap()).unwrap();
            let value: Value = serde_json::from_slice(&converted).unwrap();
            assert_eq!(value["tool_choice"], expected, "converted={value}");
        }
    }

    #[test]
    fn a_tool_using_answer_becomes_chat_tool_calls_with_a_matching_finish_reason() {
        let upstream = json!({
            "id": "resp_1",
            "model": "gpt-5.6-sol",
            "status": "completed",
            "output": [
                {"type": "message", "role": "assistant", "status": "completed",
                 "content": [{"type": "output_text", "text": "checking"}]},
                {"type": "function_call", "id": "fc_1", "call_id": "call_1",
                 "name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
            ],
            "usage": {
                "input_tokens": 12,
                "input_tokens_details": {"cached_tokens": 8},
                "output_tokens": 7,
                "output_tokens_details": {"reasoning_tokens": 3},
                "total_tokens": 19
            }
        });
        let converted = responses_response_to_chat(
            200,
            Some("application/json"),
            &serde_json::to_vec(&upstream).unwrap(),
        )
        .unwrap();
        assert_eq!(converted.content_type.as_deref(), Some("application/json"));
        let value: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(value["object"], "chat.completion");
        assert_eq!(value["id"], "resp_1");
        assert_eq!(value["choices"][0]["message"]["content"], "checking");
        let call = &value["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(call["index"], 0);
        // `call_id` is what pairs with the client's next `tool` message, so the
        // item id must not be what goes back.
        assert_eq!(call["id"], "call_1");
        assert_eq!(call["function"]["name"], "get_weather");
        assert_eq!(call["function"]["arguments"], "{\"city\":\"SF\"}");
        assert_eq!(value["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(value["usage"]["prompt_tokens"], 12);
        assert_eq!(value["usage"]["completion_tokens"], 7);
        assert_eq!(value["usage"]["total_tokens"], 19);
        // Usage is read off this body, so the detail counters have to survive.
        assert_eq!(value["usage"]["prompt_tokens_details"]["cached_tokens"], 8);
        assert_eq!(
            value["usage"]["completion_tokens_details"]["reasoning_tokens"],
            3
        );
    }

    #[test]
    fn a_turn_truncated_at_the_token_cap_reports_length_not_tool_calls() {
        let upstream = json!({
            "id": "resp_2",
            "model": "gpt-5.6-sol",
            "status": "incomplete",
            "incomplete_details": {"reason": "max_output_tokens"},
            "output": [{
                "type": "function_call", "call_id": "call_1", "name": "get_weather",
                "arguments": "{\"city\":\"S"
            }]
        });

        let converted = responses_response_to_chat(
            200,
            Some("application/json"),
            &serde_json::to_vec(&upstream).unwrap(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&converted.body).unwrap();
        // The arguments are cut mid-string; reporting `tool_calls` would make the
        // client run a call whose JSON does not parse.
        assert_eq!(value["choices"][0]["finish_reason"], "length");
        assert_eq!(
            value["choices"][0]["message"]["tool_calls"][0]["id"],
            "call_1"
        );
        // A tool-only turn sends null content rather than "", which some clients
        // render as an empty assistant bubble.
        assert!(value["choices"][0]["message"]["content"].is_null());
        // No usage was reported, so the counts are zero rather than absent.
        assert_eq!(value["usage"]["total_tokens"], 0);
    }

    #[test]
    fn an_upstream_error_is_passed_through_untouched() {
        let body = br#"{"error":{"message":"model not found","type":"invalid_request_error"}}"#;
        let converted = responses_response_to_chat(404, Some("application/json"), body).unwrap();
        assert_eq!(converted.body, body.to_vec());
        assert_eq!(converted.content_type.as_deref(), Some("application/json"));
    }

    #[test]
    fn the_event_stream_is_translated_incrementally_with_its_own_tool_call_indices() {
        // Output item 0 is the message, so the call is Responses output_index 1 but
        // chat tool_call index 0: a client keys its argument accumulator on that
        // index, and reusing the output index would leave a gap it never fills.
        let upstream = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5.6-sol\",\"status\":\"in_progress\",\"usage\":null}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[]}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"msg_1\",\"delta\":\"hi\"}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"id\":\"fc_1\",\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"get_weather\",\"arguments\":\"\"}}\n\n",
            "event: response.function_call_arguments.delta\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":1,\"item_id\":\"fc_1\",\"delta\":\"{\\\"city\\\":\"}\n\n",
            "event: response.function_call_arguments.delta\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":1,\"item_id\":\"fc_1\",\"delta\":\"\\\"SF\\\"}\"}\n\n",
            "event: response.function_call_arguments.done\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"output_index\":1,\"item_id\":\"fc_1\",\"arguments\":\"{\\\"city\\\":\\\"SF\\\"}\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5.6-sol\",\"status\":\"completed\",\"usage\":{\"input_tokens\":4,\"output_tokens\":9,\"total_tokens\":13}}}\n\n"
        );

        let converted =
            responses_response_to_chat(200, Some("text/event-stream"), upstream.as_bytes())
                .unwrap();
        assert_eq!(converted.content_type.as_deref(), Some("text/event-stream"));
        let output = String::from_utf8(converted.body).unwrap();
        let chunks: Vec<Value> = output
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter(|payload| *payload != "[DONE]")
            .map(|payload| serde_json::from_str(payload).unwrap())
            .collect();

        assert_eq!(chunks[0]["id"], "resp_1");
        assert_eq!(chunks[0]["object"], "chat.completion.chunk");
        assert_eq!(chunks[0]["choices"][0]["delta"]["role"], "assistant");
        assert_eq!(chunks[1]["choices"][0]["delta"]["content"], "hi");
        let opening = &chunks[2]["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(opening["index"], 0, "chat numbering, not the output index");
        assert_eq!(opening["id"], "call_1");
        assert_eq!(opening["function"]["name"], "get_weather");
        assert_eq!(opening["function"]["arguments"], "");
        // Argument fragments arrive as deltas, still on chat index 0.
        assert_eq!(
            chunks[3]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            "{\"city\":"
        );
        assert_eq!(
            chunks[4]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            "\"SF\"}"
        );
        assert_eq!(
            chunks[4]["choices"][0]["delta"]["tool_calls"][0]["index"],
            0
        );
        // The terminal `.done` repeats the finished argument string, and none of it
        // is sent a second time.
        assert_eq!(chunks.len(), 7, "unexpected chunk sequence: {output}");
        let terminal = chunks.iter().rev().nth(1).unwrap();
        assert_eq!(terminal["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(terminal["choices"][0]["delta"], json!({}));
        let usage = chunks.last().unwrap();
        assert_eq!(usage["choices"].as_array().map(Vec::len), Some(0));
        assert_eq!(usage["usage"]["prompt_tokens"], 4);
        assert_eq!(usage["usage"]["completion_tokens"], 9);
        assert_eq!(usage["usage"]["total_tokens"], 13);
        assert!(output.trim_end().ends_with("data: [DONE]"));
    }
    /// A mid-stream failure arrives on an HTTP 200, so the passthrough above never
    /// sees it. Ending the stream with a clean `stop` would report a dead upstream
    /// as a finished answer and skip failover.
    #[test]
    fn a_failed_stream_ends_with_an_error_frame_rather_than_a_clean_stop() {
        let upstream = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_9\",\"model\":\"gpt-5.6-sol\",\"status\":\"in_progress\"}}\n\n",
            "event: response.failed\n",
            "data: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp_9\",\"status\":\"failed\",\"error\":{\"code\":\"server_error\",\"message\":\"upstream exploded\"}}}\n\n"
        );

        let converted =
            responses_response_to_chat(200, Some("text/event-stream"), upstream.as_bytes())
                .unwrap();
        let output = String::from_utf8(converted.body).unwrap();

        assert!(output.contains("upstream exploded"), "output={output}");
        assert!(output.contains("\"code\":\"server_error\""));
        assert!(!output.contains("\"finish_reason\":\"stop\""));
        // Clients that block on the sentinel must not hang on the error frame.
        assert!(output.trim_end().ends_with("data: [DONE]"));
        // The proxy's semantic-failure detector has to recognize the frame, or the
        // credential that failed keeps serving traffic.
        assert!(
            crate::services::response_failure_service::detect_response_failed(output.as_bytes())
                .is_some(),
            "output={output}"
        );
    }
}
