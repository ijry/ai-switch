use super::common::stringify_tool_result_content;
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
    // Gemini correlates a functionResponse to its call by function NAME, while
    // Anthropic uses an opaque tool_use_id. Pre-scan assistant turns so every
    // tool_result can recover the name its id referred to.
    let function_names = collect_tool_use_names(messages);
    let mut contents = Vec::with_capacity(messages.len());
    for message in messages {
        let object = message
            .as_object()
            .ok_or_else(|| "Anthropic messages entries must be objects".to_string())?;
        // Gemini accepts only "user" and "model"; anything else (including a
        // client-injected "system" turn) folds into "user".
        let role = match object.get("role").and_then(Value::as_str).unwrap_or("user") {
            "assistant" => "model",
            _ => "user",
        };
        let parts = convert_content_blocks(
            object.get("content").unwrap_or(&Value::Null),
            &function_names,
        )?;
        // Gemini rejects a Content whose parts array is empty.
        if parts.is_empty() {
            continue;
        }
        contents.push(json!({"role": role, "parts": parts}));
    }
    Ok(Value::Array(contents))
}

/// Maps every assistant `tool_use` id to its function name so a later
/// `tool_result` can be re-keyed for Gemini's name-based correlation.
fn collect_tool_use_names(messages: &[Value]) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for message in messages {
        let Some(content) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for block in content {
            let Some(object) = block.as_object() else {
                continue;
            };
            if object.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            if let (Some(id), Some(name)) = (
                object.get("id").and_then(Value::as_str),
                object.get("name").and_then(Value::as_str),
            ) {
                names.insert(id.to_string(), name.to_string());
            }
        }
    }
    names
}

fn convert_content_blocks(
    content: &Value,
    function_names: &BTreeMap<String, String>,
) -> Result<Vec<Value>, String> {
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
            Some("image") => {
                // A non-base64 source has no bytes to forward; skip it rather
                // than fail the whole request over one unfetchable attachment.
                if let Some(part) = convert_image_block(object) {
                    parts.push(part);
                }
            }
            Some("document") => {
                if let Some(part) = convert_document_block(object) {
                    parts.push(part);
                }
            }
            Some("tool_use") => {
                let name = required_string(object, "name", "tool_use")?;
                let input = object.get("input").cloned().unwrap_or_else(|| json!({}));
                parts.push(json!({"functionCall": {"name": name, "args": input}}));
            }
            Some("tool_result") => {
                // Gemini keys functionResponse by the declared function name, not
                // by an opaque id, so the caller resolves it from history first.
                let tool_use_id = required_string(object, "tool_use_id", "tool_result")?;
                let name = function_names
                    .get(tool_use_id)
                    .map(String::as_str)
                    .unwrap_or(tool_use_id);
                let output = object
                    .get("content")
                    .map(stringify_tool_result_content)
                    .transpose()?
                    .unwrap_or_default();
                // Gemini's functionResponse has no error flag, so an Anthropic
                // `is_error` result has to say so in the payload.
                let response = if object.get("is_error").and_then(Value::as_bool) == Some(true) {
                    json!({"error": output})
                } else {
                    json!({"output": output})
                };
                parts.push(json!({
                    "functionResponse": {
                        "name": name,
                        "response": response
                    }
                }));
            }
            // Anthropic-only reasoning blocks. Claude Code replays these in
            // assistant history, and Gemini has no inbound equivalent, so drop
            // them instead of failing every turn after the first.
            Some("thinking") | Some("redacted_thinking") => {}
            // Unknown/newer block types (server_tool_use, mcp_tool_use, …) are
            // dropped: a degraded turn beats a 502 that also fails the pool.
            Some(_) | None => {}
        }
    }
    Ok(parts)
}

/// Returns `None` when the block carries no inlinable bytes (a `url`/`file`
/// source, or a missing media_type/data), so the caller can skip it.
fn convert_image_block(object: &Map<String, Value>) -> Option<Value> {
    inline_data_part(object, None)
}

/// Anthropic `document` blocks (PDF attachments) map onto the same Gemini
/// inlineData channel; media_type defaults to PDF when omitted.
fn convert_document_block(object: &Map<String, Value>) -> Option<Value> {
    inline_data_part(object, Some("application/pdf"))
}

fn inline_data_part(object: &Map<String, Value>, default_mime: Option<&str>) -> Option<Value> {
    let source = object.get("source").and_then(Value::as_object)?;
    let source_type = source
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("base64");
    if source_type != "base64" {
        return None;
    }
    let mime_type = source
        .get("media_type")
        .and_then(Value::as_str)
        .or(default_mime)?;
    let data = source.get("data").and_then(Value::as_str)?;
    Some(json!({
        "inlineData": {"mimeType": mime_type, "data": data}
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
        // Gemini's `parameters` is a restricted proto that rejects JSON Schema
        // keywords; the sanitizer picks a channel that can carry this schema.
        declarations.push(super::gemini_schema::build_gemini_function_declaration(
            name,
            object.get("description"),
            object.get("input_schema"),
        ));
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
    // A prompt-level safety block arrives with no candidates at all. Surface it
    // as a refusal the client can render rather than failing the transform.
    if let Some(reason) = prompt_block_reason(&value) {
        let response = anthropic_message(
            response_id,
            model,
            vec![json!({
                "type": "text",
                "text": format!("Request blocked by Gemini safety filters: {reason}")
            })],
            "refusal",
            gemini_usage_to_anthropic(value.get("usageMetadata")),
        );
        return serde_json::to_vec(&response)
            .map_err(|error| format!("Could not serialize Anthropic response: {error}"));
    }
    let candidate = value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    // Gemini legitimately returns a candidate with no content (MAX_TOKENS spent
    // entirely on thinking, SAFETY, RECITATION). Treat that as empty content.
    let parts = candidate
        .get("content")
        .and_then(Value::as_object)
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let content = gemini_parts_to_anthropic_content(&parts)?;
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
    for tool in &aggregate.tools {
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
    // Insertion-ordered: a sorted map would reorder parallel tool calls, and the
    // client executes them in the order it receives them.
    tools: Vec<StreamToolCall>,
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
            // Thought summaries must not reach the visible text block.
            if object.get("thought").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            if let Some(text) = object.get("text").and_then(Value::as_str) {
                // Gemini's alt=sse may deliver either incremental deltas or a
                // cumulative snapshot of the text so far. Appending a snapshot
                // would duplicate ("hel" + "hello" -> "helhello"), so detect the
                // cumulative case and append only the new suffix.
                if let Some(suffix) = text.strip_prefix(self.text.as_str()) {
                    self.text.push_str(suffix);
                } else {
                    self.text.push_str(text);
                }
            }
            if let Some(function_call) = object.get("functionCall").and_then(Value::as_object) {
                let name = function_call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                let input = function_call
                    .get("args")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                // Prefer Gemini's own call id; it is the only stable key when a
                // cumulative chunk re-sends calls that were already seen. Without
                // one, fall back to matching on name so a re-sent snapshot
                // overwrites instead of duplicating.
                let call_id = function_call.get("id").and_then(Value::as_str);
                let existing = self.tools.iter_mut().find(|tool| match call_id {
                    Some(id) => tool.id == id,
                    None => tool.name == name,
                });
                // Re-sent snapshots overwrite rather than duplicate, so one call
                // does not become three tool_use blocks the client executes.
                match existing {
                    Some(tool) => {
                        tool.name = name.to_string();
                        tool.input = input;
                    }
                    None => {
                        let id = call_id
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("{name}_{}", self.tools.len()));
                        self.tools.push(StreamToolCall {
                            id,
                            name: name.to_string(),
                            input,
                        });
                    }
                }
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
        // `thought: true` marks the model's internal reasoning summary. Emitting
        // it as text would leak thinking into the user-visible answer.
        if object.get("thought").and_then(Value::as_bool) == Some(true) {
            continue;
        }
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
    // MAX_TOKENS wins over tool_use: a truncated tool call must not be reported
    // as a complete one, or the client will try to execute a partial call.
    if finish_reason == Some("MAX_TOKENS") {
        return "max_tokens";
    }
    if content
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
    {
        return "tool_use";
    }
    match finish_reason {
        // Safety/policy stops are refusals, not custom stop-sequence matches.
        Some(
            "SAFETY" | "RECITATION" | "SPII" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "IMAGE_SAFETY",
        ) => "refusal",
        _ => "end_turn",
    }
}

/// Reads a prompt-level block reason, which Gemini reports instead of returning
/// any candidate.
fn prompt_block_reason(value: &Value) -> Option<&str> {
    value
        .get("promptFeedback")
        .and_then(Value::as_object)
        .and_then(|feedback| feedback.get("blockReason"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
}

fn gemini_usage_to_anthropic(usage: Option<&Value>) -> Value {
    let Some(usage) = usage else {
        return json!({"input_tokens": 0, "output_tokens": 0});
    };
    let field = |key: &str| usage.get(key).and_then(Value::as_i64).unwrap_or(0);
    let prompt_tokens = field("promptTokenCount");
    let cached_tokens = field("cachedContentTokenCount");
    let total_tokens = field("totalTokenCount");
    // Gemini's promptTokenCount INCLUDES cache reads, while Anthropic's
    // input_tokens excludes them, so subtract to avoid double-billing.
    let input_tokens = prompt_tokens.saturating_sub(cached_tokens);
    // candidatesTokenCount omits thinking tokens; deriving output from the total
    // captures thoughtsTokenCount without depending on that field being present.
    let output_tokens = total_tokens
        .saturating_sub(prompt_tokens)
        .max(field("candidatesTokenCount"));
    let mut result = json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens
    });
    if cached_tokens > 0 {
        result["cache_read_input_tokens"] = json!(cached_tokens);
    }
    result
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
            // Blocks this bridge does not emit are skipped rather than failing a
            // stream that already has valid content.
            Some(_) | None => {}
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

    /// End-to-end guard through the real request converter: a Claude Code tool
    /// schema must never reach Gemini's restricted `parameters` channel with
    /// JSON Schema keywords intact, which 400s the whole request.
    #[test]
    fn tool_schemas_are_sanitized_before_reaching_gemini() {
        let body = json!({
            "model": "gemini-2.5-flash",
            "max_tokens": 64,
            "messages": [{"role":"user","content":[{"type":"text","text":"read it"}]}],
            "tools": [
                {
                    "name": "Read",
                    "description": "Read a file",
                    "input_schema": {
                        "$schema": "https://json-schema.org/draft/2020-12/schema",
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {"file_path": {"type": "string"}},
                        "required": ["file_path"]
                    }
                },
                {
                    // No-argument tool: must still get an explicit object schema.
                    "name": "TodoRead",
                    "description": "Read todos"
                }
            ]
        });

        let converted: Value = serde_json::from_slice(
            &anthropic_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        let declarations = converted["tools"][0]["functionDeclarations"]
            .as_array()
            .expect("functionDeclarations");

        // The rich schema routes to the JSON Schema channel, with document
        // metadata stripped.
        let read = &declarations[0];
        assert_eq!(read["name"], "Read");
        assert!(
            read.get("parameters").is_none(),
            "rich schema must not use the restricted channel: {read}"
        );
        assert!(read["parametersJsonSchema"].get("$schema").is_none());

        // The no-argument tool gets a valid object schema Vertex will accept.
        let todo = &declarations[1];
        assert_eq!(todo["parameters"]["type"], "object");
        assert!(todo["parameters"]["properties"].is_object());

        // Belt-and-braces: `$schema` must not appear anywhere in the payload.
        let rendered = serde_json::to_string(&converted).unwrap();
        assert!(
            !rendered.contains("$schema"),
            "no JSON Schema metadata may reach Gemini: {rendered}"
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

    #[test]
    fn drops_thinking_blocks_instead_of_failing_the_turn() {
        // Claude Code replays thinking blocks in assistant history once extended
        // thinking is on, so this is every turn after the first.
        let body = json!({
            "model": "gemini-2.5-flash",
            "max_tokens": 64,
            "messages": [
                {"role":"user","content":[{"type":"text","text":"Find x"}]},
                {"role":"assistant","content":[
                    {"type":"thinking","thinking":"Let me look.","signature":"sig"},
                    {"type":"redacted_thinking","data":"opaque"},
                    {"type":"text","text":"Found it."}
                ]}
            ]
        });

        let converted: Value = serde_json::from_slice(
            &anthropic_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        let parts = converted["contents"][1]["parts"].as_array().unwrap();
        assert_eq!(
            parts.len(),
            1,
            "reasoning blocks must not survive: {parts:?}"
        );
        assert_eq!(parts[0]["text"], "Found it.");
    }

    #[test]
    fn resolves_function_response_name_from_tool_use_id() {
        // Gemini correlates functionResponse by function NAME; forwarding the
        // opaque Anthropic id leaves the result uncorrelated.
        let body = json!({
            "model": "gemini-2.5-flash",
            "max_tokens": 64,
            "messages": [
                {"role":"user","content":[{"type":"text","text":"run it"}]},
                {"role":"assistant","content":[
                    {"type":"tool_use","id":"toolu_01ABC","name":"lookup","input":{"q":"x"}}
                ]},
                {"role":"user","content":[
                    {"type":"tool_result","tool_use_id":"toolu_01ABC","content":"42"}
                ]}
            ]
        });

        let converted: Value = serde_json::from_slice(
            &anthropic_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(
            converted["contents"][2]["parts"][0]["functionResponse"]["name"],
            "lookup"
        );
    }

    #[test]
    fn tolerates_candidate_without_content() {
        // MAX_TOKENS spent entirely on thinking yields a candidate with no
        // content. That is a normal Gemini reply, not a transform failure.
        let upstream = json!({
            "candidates": [{"finishReason": "MAX_TOKENS"}],
            "usageMetadata": {"promptTokenCount": 10, "totalTokenCount": 4010}
        });

        let output = gemini_response_to_anthropic(
            200,
            Some("application/json"),
            &serde_json::to_vec(&upstream).unwrap(),
        )
        .expect("a contentless candidate must not fail the transform");
        let value: Value = serde_json::from_slice(&output.body).unwrap();

        assert_eq!(value["content"].as_array().unwrap().len(), 0);
        assert_eq!(value["stop_reason"], "max_tokens");
    }

    #[test]
    fn surfaces_prompt_block_as_refusal() {
        let upstream = json!({
            "promptFeedback": {"blockReason": "SAFETY"},
            "usageMetadata": {"promptTokenCount": 8, "totalTokenCount": 8}
        });

        let output = gemini_response_to_anthropic(
            200,
            Some("application/json"),
            &serde_json::to_vec(&upstream).unwrap(),
        )
        .expect("a prompt-level block must not fail the transform");
        let value: Value = serde_json::from_slice(&output.body).unwrap();

        assert_eq!(value["stop_reason"], "refusal");
        assert!(value["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("SAFETY"));
    }

    #[test]
    fn usage_captures_thinking_and_separates_cache_reads() {
        let upstream = json!({
            "candidates": [{
                "content": {"role":"model","parts":[{"text":"hi"}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10000,
                "candidatesTokenCount": 50,
                "thoughtsTokenCount": 4000,
                "cachedContentTokenCount": 9000,
                "totalTokenCount": 14050
            }
        });

        let output = gemini_response_to_anthropic(
            200,
            Some("application/json"),
            &serde_json::to_vec(&upstream).unwrap(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output.body).unwrap();

        // promptTokenCount includes cache reads; Anthropic's input_tokens does not.
        assert_eq!(value["usage"]["input_tokens"], 1000);
        assert_eq!(value["usage"]["cache_read_input_tokens"], 9000);
        // total - prompt captures thinking tokens candidatesTokenCount omits.
        assert_eq!(value["usage"]["output_tokens"], 4050);
    }

    #[test]
    fn skips_thought_parts_so_reasoning_does_not_leak() {
        let upstream = json!({
            "candidates": [{
                "content": {"role":"model","parts":[
                    {"text":"Internal deliberation.","thought":true},
                    {"text":"The answer is 42."}
                ]},
                "finishReason": "STOP"
            }]
        });

        let output = gemini_response_to_anthropic(
            200,
            Some("application/json"),
            &serde_json::to_vec(&upstream).unwrap(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output.body).unwrap();

        let content = value["content"].as_array().unwrap();
        assert_eq!(
            content.len(),
            1,
            "thought parts must be filtered: {content:?}"
        );
        assert_eq!(content[0]["text"], "The answer is 42.");
    }

    #[test]
    fn cumulative_stream_chunks_do_not_duplicate_text_or_tools() {
        // Gemini's alt=sse may resend the whole content as a snapshot per chunk.
        let upstream = concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hel\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"fc1\",\"name\":\"lookup\",\"args\":{\"q\":\"x\"}}}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"fc1\",\"name\":\"lookup\",\"args\":{\"q\":\"x\"}}}],\"role\":\"model\"},\"finishReason\":\"STOP\"}]}\n\n"
        );

        let output =
            gemini_response_to_anthropic(200, Some("text/event-stream"), upstream.as_bytes())
                .unwrap();
        let text = String::from_utf8(output.body).unwrap();

        assert!(
            !text.contains("helhello"),
            "cumulative snapshots must not be concatenated: {text}"
        );
        assert_eq!(
            text.matches("\"type\":\"tool_use\"").count(),
            1,
            "a resent tool call must not become multiple tool_use blocks: {text}"
        );
    }
}
