//! Inbound OpenAI Chat Completions bridged to a Gemini upstream.
//!
//! The Gemini twin of [`super::chat_claude`]. A client's wire shape is a
//! property of the client rather than of the pool it points at, so the
//! third-party clients that only speak chat completions still have to reach a
//! credential whose dialect is Gemini.
//!
//! The model and the endpoint belong to the caller: `prepare_request` reads the
//! model out of the inbound body and turns it into
//! `/v1beta/models/{model}:generateContent`, plus `?alt=sse` when the client
//! asked for a stream. So the converted body carries neither `model` nor
//! `stream` — `generateContent` knows neither field and rejects both.

use super::common::parse_base64_data_url;
use super::TransformedBridgeResponse;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(super) fn chat_request_to_gemini(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Chat request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Chat request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    let mut system = Vec::new();
    let mut contents = Vec::new();
    if let Some(items) = object.get("messages").and_then(Value::as_array) {
        // Gemini correlates a functionResponse to its call by function NAME,
        // while chat uses an opaque tool_call_id. Pre-scan the assistant turns
        // so every tool message can recover the name its id referred to.
        let function_names = collect_tool_call_names(items);
        for item in items {
            convert_message(item, &function_names, &mut system, &mut contents)?;
        }
    }
    if !system.is_empty() {
        result.insert("systemInstruction".to_string(), json!({"parts": system}));
    }
    result.insert("contents".to_string(), Value::Array(contents));

    let generation_config = convert_generation_config(object);
    if !generation_config.is_empty() {
        result.insert(
            "generationConfig".to_string(),
            Value::Object(generation_config),
        );
    }
    let declarations = match object.get("tools") {
        Some(tools) => convert_tools(tools)?,
        None => Vec::new(),
    };
    if !declarations.is_empty() {
        result.insert(
            "tools".to_string(),
            json!([{"functionDeclarations": declarations}]),
        );
        // A forced mode with nothing to call is a 400 on Gemini's side, so the
        // tool config only travels alongside the declarations.
        if let Some(config) = convert_tool_choice(object.get("tool_choice"))? {
            result.insert(
                "toolConfig".to_string(),
                json!({"functionCallingConfig": config}),
            );
        }
    }

    serde_json::to_vec(&Value::Object(result))
        .map_err(|error| format!("Could not serialize Gemini request: {error}"))
}

pub(super) fn gemini_response_to_chat(
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
            body: gemini_sse_to_chat(body)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }

    Ok(TransformedBridgeResponse {
        body: gemini_json_to_chat(body)?,
        content_type: Some("application/json".to_string()),
    })
}

/// Routes one chat message into either the Gemini `systemInstruction` parts or
/// the `contents` array.
///
/// Gemini keeps the system prompt outside the turn list, knows only the `user`
/// and `model` roles, and has no `tool` role: a tool result is a
/// `functionResponse` part on a *user* turn. All three are shape changes rather
/// than field renames, which is why this cannot be a straight copy.
fn convert_message(
    message: &Value,
    function_names: &BTreeMap<String, String>,
    system: &mut Vec<Value>,
    contents: &mut Vec<Value>,
) -> Result<(), String> {
    let object = message
        .as_object()
        .ok_or_else(|| "Chat messages entries must be objects".to_string())?;
    let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
    match role {
        "system" | "developer" => {
            let text = message_text(object.get("content"))?;
            if !text.is_empty() {
                system.push(json!({"text": text}));
            }
            Ok(())
        }
        "user" => {
            push_content(contents, "user", user_parts(object.get("content"))?);
            Ok(())
        }
        "assistant" => {
            push_content(contents, "model", assistant_parts(object)?);
            Ok(())
        }
        "tool" => {
            let id = object
                .get("tool_call_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Chat tool message is missing tool_call_id".to_string())?;
            // Gemini keys a functionResponse by the declared function name, so
            // the name comes from the assistant turn that made the call. Falling
            // back to the id keeps the turn alive when history is truncated.
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| function_names.get(id).map(String::as_str))
                .unwrap_or(id);
            push_content(
                contents,
                "user",
                vec![json!({"functionResponse": {
                    "name": name,
                    "response": {"output": message_text(object.get("content"))?}
                }})],
            );
            Ok(())
        }
        other => Err(format!("Unsupported Chat message role: {other}")),
    }
}

/// Gemini rejects a Content whose `parts` array is empty, so a message that
/// converted to nothing is dropped instead of sent.
fn push_content(contents: &mut Vec<Value>, role: &str, parts: Vec<Value>) {
    if parts.is_empty() {
        return;
    }
    contents.push(json!({"role": role, "parts": parts}));
}

/// Maps every assistant `tool_call` id to its function name so a later chat
/// `tool` message can be re-keyed for Gemini's name-based correlation.
fn collect_tool_call_names(messages: &[Value]) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for message in messages {
        let Some(calls) = message.get("tool_calls").and_then(Value::as_array) else {
            continue;
        };
        for call in calls {
            if let (Some(id), Some(name)) = (
                call.get("id").and_then(Value::as_str),
                call.pointer("/function/name").and_then(Value::as_str),
            ) {
                names.insert(id.to_string(), name.to_string());
            }
        }
    }
    names
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

fn user_parts(content: Option<&Value>) -> Result<Vec<Value>, String> {
    match content {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(text)) => Ok(if text.is_empty() {
            Vec::new()
        } else {
            vec![json!({"text": text})]
        }),
        Some(Value::Array(parts)) => {
            let mut converted = Vec::new();
            for part in parts {
                let kind = part.get("type").and_then(Value::as_str).unwrap_or("text");
                match kind {
                    "text" => {
                        let text = part.get("text").and_then(Value::as_str).unwrap_or_default();
                        if !text.is_empty() {
                            converted.push(json!({"text": text}));
                        }
                    }
                    "image_url" => converted.extend(convert_image_part(part)?),
                    other => return Err(format!("Unsupported Chat content part: {other}")),
                }
            }
            Ok(converted)
        }
        Some(other) => Err(format!("Unsupported Chat content shape: {other}")),
    }
}

/// `image_url` carries either a data URL or a remote URL: Gemini takes inline
/// bytes as `inlineData` and a URI it must fetch itself as `fileData`.
///
/// Returns `None` for a data URL that is not base64 — there are no bytes to
/// forward, and dropping one attachment beats failing a turn whose text still
/// answers the question.
fn convert_image_part(part: &Value) -> Result<Option<Value>, String> {
    let url = part
        .pointer("/image_url/url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Chat image part is missing image_url.url".to_string())?;
    if url.trim_start().starts_with("data:") {
        return Ok(parse_base64_data_url(url).map(
            |(media_type, data)| json!({"inlineData": {"mimeType": media_type, "data": data}}),
        ));
    }
    let mut file_data = Map::new();
    file_data.insert("fileUri".to_string(), json!(url));
    if let Some(media_type) = mime_type_from_url(url) {
        file_data.insert("mimeType".to_string(), json!(media_type));
    }
    Ok(Some(json!({"fileData": Value::Object(file_data)})))
}

/// Gemini's `fileData` pairs the URI with a media type that a chat image URL
/// does not carry. The extension is the only hint available, and an omitted
/// mimeType beats a wrong one.
fn mime_type_from_url(url: &str) -> Option<&'static str> {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    [
        (".png", "image/png"),
        (".jpg", "image/jpeg"),
        (".jpeg", "image/jpeg"),
        (".webp", "image/webp"),
        (".gif", "image/gif"),
        (".heic", "image/heic"),
        (".heif", "image/heif"),
        (".pdf", "application/pdf"),
    ]
    .into_iter()
    .find(|(extension, _)| path.ends_with(extension))
    .map(|(_, media_type)| media_type)
}

/// An assistant turn carries text and `tool_calls` side by side in chat; in
/// Gemini both are parts of one `model` turn.
///
/// The chat `tool_call.id` is deliberately not forwarded: Gemini correlates by
/// function name, and a `functionCall.id` with no matching id on the result
/// leaves the pair uncorrelated.
fn assistant_parts(object: &Map<String, Value>) -> Result<Vec<Value>, String> {
    let mut parts = Vec::new();
    let text = message_text(object.get("content"))?;
    if !text.is_empty() {
        parts.push(json!({"text": text}));
    }
    if let Some(calls) = object.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .ok_or_else(|| "Chat tool_call is missing function.name".to_string())?;
            parts.push(json!({"functionCall": {
                "name": name,
                "args": tool_call_arguments(call.pointer("/function/arguments"))
            }}));
        }
    }
    Ok(parts)
}

/// Tool arguments arrive as a JSON *string* in chat, while Gemini's
/// `functionCall.args` is a protobuf Struct where a string is a 400.
///
/// A partial or unparseable string becomes `{}` rather than failing the turn:
/// the upstream can still answer for a wrong argument set, but a rejected
/// request strands the client with nothing.
fn tool_call_arguments(arguments: Option<&Value>) -> Value {
    match arguments {
        Some(Value::String(raw)) => serde_json::from_str::<Value>(raw.trim())
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({})),
        Some(Value::Object(map)) => Value::Object(map.clone()),
        _ => json!({}),
    }
}

fn convert_generation_config(object: &Map<String, Value>) -> Map<String, Value> {
    let mut generation_config = Map::new();
    // `max_completion_tokens` is the newer spelling of the same cap; clients in
    // the wild send either one.
    if let Some(max_tokens) = object
        .get("max_tokens")
        .or_else(|| object.get("max_completion_tokens"))
    {
        generation_config.insert("maxOutputTokens".to_string(), max_tokens.clone());
    }
    if let Some(temperature) = object.get("temperature") {
        generation_config.insert("temperature".to_string(), temperature.clone());
    }
    if let Some(top_p) = object.get("top_p") {
        generation_config.insert("topP".to_string(), top_p.clone());
    }
    if let Some(stop) = object.get("stop") {
        generation_config.insert("stopSequences".to_string(), stop_sequences(stop));
    }
    generation_config
}

/// Chat allows a bare string; Gemini only takes a list.
fn stop_sequences(stop: &Value) -> Value {
    match stop {
        Value::String(text) => json!([text]),
        Value::Array(items) => Value::Array(items.clone()),
        _ => json!([]),
    }
}

fn convert_tools(tools: &Value) -> Result<Vec<Value>, String> {
    let items = tools
        .as_array()
        .ok_or_else(|| "Chat tools must be an array".to_string())?;
    let mut declarations = Vec::with_capacity(items.len());
    for tool in items {
        let function = tool
            .get("function")
            .and_then(Value::as_object)
            .ok_or_else(|| "Chat tool is missing function".to_string())?;
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "Chat tool is missing function.name".to_string())?;
        // Gemini's `parameters` is a restricted proto that rejects JSON Schema
        // keywords; the sanitizer picks a channel that can carry this schema.
        declarations.push(super::gemini_schema::build_gemini_function_declaration(
            name,
            function.get("description"),
            function.get("parameters"),
        ));
    }
    Ok(declarations)
}

/// Chat states forced tool use per request, Gemini in `toolConfig`. Unlike the
/// Anthropic bridge, `none` maps exactly here (`NONE`) instead of having to be
/// dropped, so a client that forbids tools is actually obeyed.
fn convert_tool_choice(tool_choice: Option<&Value>) -> Result<Option<Value>, String> {
    match tool_choice {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => match text.as_str() {
            "auto" => Ok(Some(json!({"mode": "AUTO"}))),
            "required" | "any" => Ok(Some(json!({"mode": "ANY"}))),
            "none" => Ok(Some(json!({"mode": "NONE"}))),
            other => Err(format!("Unsupported Chat tool_choice: {other}")),
        },
        Some(choice @ Value::Object(_)) => {
            let name = choice
                .pointer("/function/name")
                .and_then(Value::as_str)
                .ok_or_else(|| "Chat tool_choice object needs function.name".to_string())?;
            // Gemini has no single-tool mode: naming one function is `ANY`
            // narrowed to that name.
            Ok(Some(json!({"mode": "ANY", "allowedFunctionNames": [name]})))
        }
        Some(other) => Err(format!("Unsupported Chat tool_choice shape: {other}")),
    }
}

fn gemini_json_to_chat(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Gemini response JSON is invalid: {error}"))?;
    let candidate = value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|items| items.first());
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    // Gemini legitimately answers with a candidate that has no content at all
    // (MAX_TOKENS spent entirely on thinking, SAFETY, RECITATION), so a missing
    // parts array is an empty turn rather than a transform failure.
    for part in candidate
        .and_then(|candidate| candidate.pointer("/content/parts"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        // `thought: true` marks the model's internal reasoning summary. Emitting
        // it as content would leak thinking into the user-visible answer.
        if part.get("thought").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if let Some(found) = part.get("text").and_then(Value::as_str) {
            text.push_str(found);
            continue;
        }
        if let Some(function_call) = part.get("functionCall") {
            tool_calls.push(tool_call_from_part(function_call, tool_calls.len())?);
        }
    }

    // A prompt-level block comes back with no candidates at all. Saying so as
    // assistant text beats handing the client an empty answer it cannot explain.
    let block_reason = prompt_block_reason(&value);
    if let Some(reason) = block_reason {
        text = blocked_notice(reason);
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

    let finish = match block_reason {
        Some(_) => "content_filter",
        None => finish_reason(
            candidate
                .and_then(|candidate| candidate.get("finishReason"))
                .and_then(Value::as_str),
            !tool_calls.is_empty(),
        ),
    };
    let response = json!({
        "id": response_id(&value),
        "object": "chat.completion",
        "created": 0,
        "model": model_version(&value),
        "choices": [{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": finish,
        }],
        "usage": usage_to_chat(value.get("usageMetadata")),
    });
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Chat response: {error}"))
}

/// Builds one chat `tool_calls` entry from a Gemini `functionCall`. Shared with
/// the stream so an id and an argument string are derived exactly once.
fn tool_call_from_part(function_call: &Value, index: usize) -> Result<Value, String> {
    let name = function_call
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("tool");
    let arguments = function_call
        .get("args")
        .cloned()
        .unwrap_or_else(|| json!({}));
    Ok(json!({
        "index": index,
        // Gemini does not always send a call id, and a chat client needs one to
        // address the result back, so derive a stable one from name and position.
        "id": function_call
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("call_{name}_{index}")),
        "type": "function",
        "function": {
            "name": name,
            // Chat carries arguments as a JSON string, Gemini as a Struct.
            "arguments": serde_json::to_string(&arguments)
                .map_err(|error| format!("Could not serialize Gemini function args: {error}"))?,
        }
    }))
}

/// `MAX_TOKENS` outranks a tool call: a truncated call reported as a complete one
/// makes the client execute a partial argument set. A tool call in turn outranks
/// `STOP`, which is what Gemini says even when it asked for a tool — and a client
/// that sees `stop` next to `tool_calls` never runs the tool.
fn finish_reason(gemini_finish_reason: Option<&str>, has_tool_calls: bool) -> &'static str {
    if gemini_finish_reason == Some("MAX_TOKENS") {
        return "length";
    }
    if has_tool_calls {
        return "tool_calls";
    }
    match gemini_finish_reason {
        Some(
            "SAFETY" | "RECITATION" | "SPII" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "IMAGE_SAFETY",
        ) => "content_filter",
        _ => "stop",
    }
}

fn usage_to_chat(usage: Option<&Value>) -> Value {
    let field = |key: &str| {
        usage
            .and_then(|usage| usage.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let prompt = field("promptTokenCount");
    let total = field("totalTokenCount");
    // candidatesTokenCount omits thinking tokens while chat's completion_tokens
    // is defined to include them, so take whichever count is larger: deriving
    // from the total captures thinking even when thoughtsTokenCount is absent.
    let completion = (field("candidatesTokenCount") + field("thoughtsTokenCount"))
        .max(total.saturating_sub(prompt));
    json!({
        "prompt_tokens": prompt,
        "completion_tokens": completion,
        "total_tokens": total.max(prompt + completion),
    })
}

fn response_id(value: &Value) -> &str {
    value
        .get("responseId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .unwrap_or("chatcmpl-bridge")
}

fn model_version(value: &Value) -> &str {
    value
        .get("modelVersion")
        .and_then(Value::as_str)
        .or_else(|| value.get("model").and_then(Value::as_str))
        .unwrap_or_default()
}

/// Reads a prompt-level block reason, which Gemini reports instead of returning
/// any candidate.
fn prompt_block_reason(value: &Value) -> Option<&str> {
    value
        .pointer("/promptFeedback/blockReason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
}

fn blocked_notice(reason: &str) -> String {
    format!("Request blocked by Gemini safety filters: {reason}")
}

fn looks_like_sse(body: &[u8]) -> bool {
    std::str::from_utf8(body).ok().is_some_and(|text| {
        text.lines()
            .any(|line| line.trim_start().starts_with("data:"))
    })
}

/// One tool call already announced to the client, kept only to recognise a
/// repeat of it in a later chunk.
struct StreamToolCall {
    id: Option<String>,
    name: String,
    arguments: Value,
}

/// Translates the Gemini stream into chat chunks as they arrive.
///
/// Deliberately incremental rather than accumulate-then-emit: a chat client
/// streams to show progress, and replaying one whole message at the end would
/// make every turn appear to hang and then finish instantly.
fn gemini_sse_to_chat(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = String::new();
    let mut id = "chatcmpl-bridge".to_string();
    let mut model = String::new();
    let mut role_sent = false;
    // Text already handed to the client, so a cumulative chunk can be reduced to
    // the suffix it has not seen yet.
    let mut text = String::new();
    // Gemini part indices count text parts too, so tool calls need their own
    // contiguous numbering: a client keys its argument accumulator on this index.
    let mut tool_calls: Vec<StreamToolCall> = Vec::new();
    let mut gemini_finish_reason: Option<String> = None;
    let mut blocked = false;
    let mut usage: Option<Value> = None;

    for value in super::sse::parse_sse_data_records(body)? {
        if let Some(found) = value.get("responseId").and_then(Value::as_str) {
            id = found.to_string();
        }
        if let Some(found) = value
            .get("modelVersion")
            .and_then(Value::as_str)
            .or_else(|| value.get("model").and_then(Value::as_str))
        {
            model = found.to_string();
        }
        // Gemini repeats a cumulative usageMetadata on every chunk, so the last
        // one seen is the whole turn rather than an increment to add up.
        if let Some(found) = value.get("usageMetadata") {
            usage = Some(found.clone());
        }
        if !role_sent {
            push_chunk(&mut output, &id, &model, json!({"role": "assistant"}), None)?;
            role_sent = true;
        }
        if let Some(reason) = prompt_block_reason(&value) {
            // As in the non-streaming path: a blocked prompt yields no candidates,
            // and a stream that simply ends tells the client nothing.
            let notice = blocked_notice(reason);
            push_chunk(&mut output, &id, &model, json!({"content": notice}), None)?;
            blocked = true;
        }
        let Some(candidate) = value
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
        else {
            continue;
        };
        if let Some(reason) = candidate.get("finishReason").and_then(Value::as_str) {
            gemini_finish_reason = Some(reason.to_string());
        }
        for part in candidate
            .pointer("/content/parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if part.get("thought").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            if let Some(found) = part.get("text").and_then(Value::as_str) {
                // Gemini's alt=sse delivers either an incremental delta or a
                // cumulative snapshot of the text so far, and forwarding a
                // snapshot verbatim duplicates what the client already has
                // ("hel" + "hello" -> "helhello").
                let delta = found.strip_prefix(text.as_str()).unwrap_or(found);
                if !delta.is_empty() {
                    text.push_str(delta);
                    push_chunk(&mut output, &id, &model, json!({"content": delta}), None)?;
                }
                continue;
            }
            let Some(function_call) = part.get("functionCall") else {
                continue;
            };
            let name = function_call
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let call_id = function_call.get("id").and_then(Value::as_str);
            let arguments = function_call
                .get("args")
                .cloned()
                .unwrap_or_else(|| json!({}));
            // A cumulative chunk re-sends calls it already announced. Match on
            // Gemini's own call id when there is one and on the whole name+args
            // pair otherwise, so a resent snapshot collapses while two genuine
            // parallel calls to the same tool both survive.
            let already_sent = tool_calls
                .iter()
                .any(|call| match (call_id, call.id.as_deref()) {
                    (Some(incoming), Some(existing)) => incoming == existing,
                    _ => call.name == name && call.arguments == arguments,
                });
            if already_sent {
                continue;
            }
            let index = tool_calls.len();
            push_chunk(
                &mut output,
                &id,
                &model,
                json!({"tool_calls": [tool_call_from_part(function_call, index)?]}),
                None,
            )?;
            tool_calls.push(StreamToolCall {
                id: call_id.map(str::to_string),
                name: name.to_string(),
                arguments,
            });
        }
    }

    let finish = if blocked {
        "content_filter"
    } else {
        finish_reason(gemini_finish_reason.as_deref(), !tool_calls.is_empty())
    };
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
    fn the_system_prompt_leaves_messages_and_a_tool_result_becomes_a_function_response() {
        let body = json!({
            "model": "gemini-2.5-pro",
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
            "max_tokens": 64,
            "temperature": 0.2,
            "top_p": 0.9,
            "stop": "STOP",
            "stream": true
        });

        let converted = chat_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap();
        let value: Value = serde_json::from_slice(&converted).unwrap();

        // Gemini carries the system prompt outside the turn list.
        assert_eq!(value["systemInstruction"]["parts"][0]["text"], "be terse");
        let contents = value["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "weather?");
        // An assistant turn is role "model", and the arguments stop being a string.
        assert_eq!(contents[1]["role"], "model");
        let call = &contents[1]["parts"][0]["functionCall"];
        assert_eq!(call["name"], "get_weather");
        assert_eq!(call["args"]["city"], "SF");
        // There is no `tool` role: a result is a functionResponse on a user turn,
        // keyed by the function NAME the opaque id referred to.
        assert_eq!(contents[2]["role"], "user");
        let result = &contents[2]["parts"][0]["functionResponse"];
        assert_eq!(result["name"], "get_weather");
        assert_eq!(result["response"]["output"], "18C");
        assert_eq!(value["generationConfig"]["maxOutputTokens"], 64);
        assert_eq!(value["generationConfig"]["temperature"], 0.2);
        assert_eq!(value["generationConfig"]["topP"], 0.9);
        assert_eq!(value["generationConfig"]["stopSequences"], json!(["STOP"]));
        // The endpoint carries both of these; generateContent has no such fields.
        assert!(value.get("model").is_none());
        assert!(value.get("stream").is_none());
    }

    /// End-to-end guard through the real converter: a chat tool schema carrying
    /// JSON Schema keywords must never reach Gemini's restricted `parameters`
    /// channel, which 400s the whole request with `Cannot find field`.
    #[test]
    fn tool_schemas_are_sanitized_before_reaching_gemini() {
        let body = json!({
            "model": "gemini-2.5-pro",
            "messages": [{"role": "user", "content": "read it"}],
            "tools": [
                {"type": "function", "function": {
                    "name": "Read",
                    "description": "Read a file",
                    "parameters": {
                        "$schema": "https://json-schema.org/draft/2020-12/schema",
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {"file_path": {"type": "string"}},
                        "required": ["file_path"]
                    }
                }},
                // No-argument tool: must still get an explicit object schema.
                {"type": "function", "function": {"name": "TodoRead"}}
            ]
        });

        let converted = chat_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap();
        let value: Value = serde_json::from_slice(&converted).unwrap();
        let declarations = value["tools"][0]["functionDeclarations"]
            .as_array()
            .expect("functionDeclarations");

        // The rich schema routes to the JSON Schema channel instead.
        let read = &declarations[0];
        assert_eq!(read["name"], "Read");
        assert!(
            read.get("parameters").is_none(),
            "rich schema must not use the restricted channel: {read}"
        );
        assert_eq!(read["parametersJsonSchema"]["additionalProperties"], false);
        assert!(read["parametersJsonSchema"].get("$schema").is_none());
        // The no-argument tool still gets a schema Vertex accepts.
        assert_eq!(declarations[1]["parameters"]["type"], "object");
        assert!(declarations[1]["parameters"]["properties"].is_object());
        // Belt and braces: no document metadata anywhere in the payload.
        let rendered = serde_json::to_string(&value).unwrap();
        assert!(
            !rendered.contains("$schema"),
            "no JSON Schema metadata may reach Gemini: {rendered}"
        );
    }

    #[test]
    fn tool_choice_becomes_a_function_calling_config_mode() {
        let cases = [
            (json!("auto"), "AUTO", None),
            (json!("required"), "ANY", None),
            (json!("none"), "NONE", None),
            (
                json!({"type": "function", "function": {"name": "get_weather"}}),
                "ANY",
                Some("get_weather"),
            ),
        ];

        for (tool_choice, mode, allowed) in cases {
            let body = json!({
                "model": "gemini-2.5-pro",
                "messages": [{"role": "user", "content": "weather?"}],
                "tools": [{"type": "function", "function": {
                    "name": "get_weather",
                    "parameters": {"type": "object", "properties": {}}
                }}],
                "tool_choice": tool_choice
            });
            let converted = chat_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap();
            let value: Value = serde_json::from_slice(&converted).unwrap();
            let config = &value["toolConfig"]["functionCallingConfig"];

            assert_eq!(
                config["mode"], mode,
                "tool_choice {tool_choice} -> {config}"
            );
            match allowed {
                // Gemini has no single-tool mode: naming a function is ANY
                // narrowed to that name.
                Some(name) => assert_eq!(config["allowedFunctionNames"], json!([name])),
                None => assert!(config.get("allowedFunctionNames").is_none(), "{config}"),
            }
        }

        // A forced mode with nothing declared is a 400 on Gemini's side, so the
        // config never travels alone.
        let no_tools =
            json!({"model": "gemini-2.5-pro", "messages": [], "tool_choice": "required"});
        let converted = chat_request_to_gemini(&serde_json::to_vec(&no_tools).unwrap()).unwrap();
        let value: Value = serde_json::from_slice(&converted).unwrap();
        assert!(value.get("toolConfig").is_none(), "{value}");
    }

    #[test]
    fn a_tool_using_answer_becomes_chat_tool_calls_with_a_matching_finish_reason() {
        let upstream = json!({
            "responseId": "resp_1",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "content": {"role": "model", "parts": [
                    {"text": "checking"},
                    {"functionCall": {"name": "get_weather", "args": {"city": "SF"}}}
                ]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 12, "candidatesTokenCount": 7}
        });

        let converted = gemini_response_to_chat(
            200,
            Some("application/json"),
            &serde_json::to_vec(&upstream).unwrap(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(value["object"], "chat.completion");
        assert_eq!(value["id"], "resp_1");
        assert_eq!(value["model"], "gemini-2.5-pro");
        assert_eq!(value["choices"][0]["message"]["content"], "checking");
        let call = &value["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(call["function"]["name"], "get_weather");
        // Chat carries arguments as a string, Gemini as a Struct.
        assert_eq!(call["function"]["arguments"], "{\"city\":\"SF\"}");
        // Gemini reports STOP even when it asked for a tool, and a client that
        // sees "stop" next to tool_calls never runs the tool.
        assert_eq!(value["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(value["usage"]["prompt_tokens"], 12);
        assert_eq!(value["usage"]["completion_tokens"], 7);
        assert_eq!(value["usage"]["total_tokens"], 19);

        // MAX_TOKENS outranks the tool call: a truncated call must not be handed
        // over as a complete one. A tool-only turn also gets null content, since
        // several clients render "" as an empty assistant bubble.
        let truncated = json!({
            "candidates": [{
                "content": {"parts": [{"functionCall": {"name": "get_weather", "args": {}}}]},
                "finishReason": "MAX_TOKENS"
            }]
        });
        let converted = gemini_response_to_chat(
            200,
            Some("application/json"),
            &serde_json::to_vec(&truncated).unwrap(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&converted.body).unwrap();
        assert_eq!(value["choices"][0]["finish_reason"], "length");
        assert!(value["choices"][0]["message"]["content"].is_null());
        assert_eq!(
            value["choices"][0]["message"]["tool_calls"][0]["id"],
            "call_get_weather_0"
        );
    }

    #[test]
    fn an_upstream_error_is_passed_through_untouched() {
        let body = br#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED"}}"#;
        let converted = gemini_response_to_chat(429, Some("application/json"), body).unwrap();
        assert_eq!(converted.body, body.to_vec());
        assert_eq!(converted.content_type.as_deref(), Some("application/json"));
    }

    /// Chat sends image bytes inline and links by URL; Gemini has a separate
    /// channel for each, and putting one in the other drops the attachment.
    #[test]
    fn images_split_between_inline_data_and_file_data() {
        let body = json!({
            "model": "gemini-2.5-pro",
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "compare"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,aGVsbG8="}},
                {"type": "image_url", "image_url": {"url": "https://example.com/a.jpg?v=2"}}
            ]}]
        });

        let converted = chat_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap();
        let value: Value = serde_json::from_slice(&converted).unwrap();
        let parts = value["contents"][0]["parts"].as_array().unwrap();

        assert_eq!(parts[0]["text"], "compare");
        assert_eq!(parts[1]["inlineData"]["mimeType"], "image/png");
        assert_eq!(parts[1]["inlineData"]["data"], "aGVsbG8=");
        assert_eq!(
            parts[2]["fileData"]["fileUri"],
            "https://example.com/a.jpg?v=2"
        );
        // The query string must not be mistaken for part of the extension.
        assert_eq!(parts[2]["fileData"]["mimeType"], "image/jpeg");
    }

    #[test]
    fn the_event_stream_is_translated_incrementally_with_its_own_tool_call_indices() {
        // Part 0 is text, so the call sits at Gemini part 1 but chat tool_call
        // index 0: a client keys its argument accumulator on that index, and
        // reusing the part index would leave a gap it never fills. The last chunk
        // repeats the call as a cumulative snapshot, which must not open a second.
        let upstream = concat!(
            "data: {\"responseId\":\"resp_1\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"hel\"}]}}],\"usageMetadata\":{\"promptTokenCount\":4}}\n\n",
            "data: {\"responseId\":\"resp_1\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"lo\"},{\"functionCall\":{\"id\":\"fc1\",\"name\":\"get_weather\",\"args\":{\"city\":\"SF\"}}}]}}]}\n\n",
            "data: {\"responseId\":\"resp_1\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"functionCall\":{\"id\":\"fc1\",\"name\":\"get_weather\",\"args\":{\"city\":\"SF\"}}}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":9}}\n\n",
            "data: [DONE]\n\n"
        );

        let converted =
            gemini_response_to_chat(200, Some("text/event-stream"), upstream.as_bytes()).unwrap();
        assert_eq!(converted.content_type.as_deref(), Some("text/event-stream"));
        let output = String::from_utf8(converted.body).unwrap();
        let chunks: Vec<Value> = output
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter(|payload| *payload != "[DONE]")
            .map(|payload| serde_json::from_str(payload).unwrap())
            .collect();

        assert_eq!(chunks[0]["choices"][0]["delta"]["role"], "assistant");
        // Text arrives as deltas, not as one replay at the end.
        assert_eq!(chunks[1]["choices"][0]["delta"]["content"], "hel");
        assert_eq!(chunks[2]["choices"][0]["delta"]["content"], "lo");
        let call = &chunks[3]["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(
            call["index"], 0,
            "chat numbering, not the Gemini part index"
        );
        assert_eq!(call["id"], "fc1");
        assert_eq!(call["function"]["name"], "get_weather");
        assert_eq!(call["function"]["arguments"], "{\"city\":\"SF\"}");
        let announced = chunks
            .iter()
            .filter(|chunk| chunk["choices"][0]["delta"].get("tool_calls").is_some())
            .count();
        assert_eq!(
            announced, 1,
            "a resent snapshot must not become a second tool call: {output}"
        );
        // Terminal chunk carries the finish reason, then a usage-only chunk.
        let terminal = chunks.iter().rev().nth(1).unwrap();
        assert_eq!(terminal["choices"][0]["finish_reason"], "tool_calls");
        let usage = chunks.last().unwrap();
        assert!(usage["choices"].as_array().unwrap().is_empty());
        assert_eq!(usage["usage"]["prompt_tokens"], 4);
        assert_eq!(usage["usage"]["completion_tokens"], 9);
        assert_eq!(usage["usage"]["total_tokens"], 13);
        assert!(output.trim_end().ends_with("data: [DONE]"));
    }
}
