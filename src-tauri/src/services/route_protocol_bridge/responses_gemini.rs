#[cfg(test)]
mod tests {
    use super::{gemini_response_to_responses, responses_request_to_gemini};
    use serde_json::Value;
    use std::collections::BTreeMap;

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

    /// Responses states forced tool use per request, Gemini in `toolConfig`.
    /// Dropping it downgrades a forced call to optional, which stalls the agent
    /// loops that depend on the call and makes a tool-call capability probe report
    /// a perfectly capable model as text-only.
    #[test]
    fn tool_choice_becomes_a_function_calling_config_mode() {
        let cases = [
            (serde_json::json!("auto"), "AUTO", None),
            (serde_json::json!("required"), "ANY", None),
            (serde_json::json!("none"), "NONE", None),
            (
                serde_json::json!({"type": "function", "name": "lookup"}),
                "ANY",
                Some("lookup"),
            ),
            // A `custom` tool is still a named function once flattened.
            (
                serde_json::json!({"type": "custom", "name": "lookup"}),
                "ANY",
                Some("lookup"),
            ),
            // "any of the listed tools" has no narrower Gemini equivalent.
            (
                serde_json::json!({"type": "allowed_tools", "tools": [{"type": "function", "name": "lookup"}]}),
                "ANY",
                None,
            ),
        ];

        for (tool_choice, mode, allowed) in cases {
            let body = serde_json::json!({
                "model": "gemini-2.5-flash",
                "input": [{"role":"user","content":[{"type":"input_text","text":"go"}]}],
                "tools": [{"type":"function","name":"lookup","parameters":{"type":"object","properties":{}}}],
                "tool_choice": tool_choice
            });

            let converted: Value = serde_json::from_slice(
                &responses_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
            )
            .unwrap();
            let config = &converted["toolConfig"]["functionCallingConfig"];

            assert_eq!(
                config["mode"], mode,
                "tool_choice {tool_choice} -> {config}"
            );
            match allowed {
                // Gemini has no single-tool mode: naming a function is ANY
                // narrowed to that name.
                Some(name) => {
                    assert_eq!(config["allowedFunctionNames"], serde_json::json!([name]))
                }
                None => assert!(config.get("allowedFunctionNames").is_none(), "{config}"),
            }
        }

        // A forced mode with nothing declared is a 400 on Gemini's side, so the
        // config never travels alone.
        let no_tools = serde_json::json!({
            "model": "gemini-2.5-flash",
            "input": "go",
            "tool_choice": "required"
        });
        let converted: Value = serde_json::from_slice(
            &responses_request_to_gemini(&serde_json::to_vec(&no_tools).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(converted.get("toolConfig").is_none(), "{converted}");
    }

    /// `allowedFunctionNames` has to match the declaration, and a namespaced tool
    /// is declared under its flattened `namespace__name`. Forwarding the name the
    /// client wrote would force a function Gemini was never given.
    #[test]
    fn a_forced_namespaced_tool_names_the_flattened_declaration() {
        let body = serde_json::json!({
            "model": "gemini-2.5-flash",
            "input": "go",
            "tools": [{
                "type": "namespace",
                "name": "database",
                "tools": [{
                    "type": "function",
                    "name": "lookup",
                    "parameters": {"type": "object", "properties": {}}
                }]
            }],
            "tool_choice": {"type": "function", "name": "lookup"}
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(
            converted["tools"][0]["functionDeclarations"][0]["name"],
            "database__lookup"
        );
        assert_eq!(
            converted["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"],
            serde_json::json!(["database__lookup"])
        );
    }

    /// Hosted tools are dropped on the way to Gemini, so a request that asked only
    /// for those has nothing left to declare — and an empty declaration list plus a
    /// forced mode is exactly the 400 the gate above avoids.
    #[test]
    fn builtin_only_tools_leave_no_declarations_and_no_forced_mode() {
        let body = serde_json::json!({
            "model": "gemini-2.5-flash",
            "input": "go",
            "tools": [{"type": "web_search"}],
            "tool_choice": {"type": "web_search"}
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert!(converted.get("tools").is_none(), "{converted}");
        assert!(converted.get("toolConfig").is_none(), "{converted}");
    }

    /// Same guard as the Claude direction: Codex/MCP tool schemas carry JSON
    /// Schema keywords that Gemini's restricted `parameters` channel rejects.
    #[test]
    fn tool_schemas_are_sanitized_before_reaching_gemini() {
        let body = serde_json::json!({
            "model": "gemini-2.5-flash",
            "input": [{"role":"user","content":[{"type":"input_text","text":"go"}]}],
            "tools": [{
                "type": "function",
                "name": "apply_patch",
                "parameters": {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {"patch": {"type": "string"}}
                }
            }]
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_gemini(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        let declaration = &converted["tools"][0]["functionDeclarations"][0];
        assert_eq!(declaration["name"], "apply_patch");
        assert!(
            declaration.get("parameters").is_none(),
            "rich schema must use parametersJsonSchema: {declaration}"
        );

        let rendered = serde_json::to_string(&converted).unwrap();
        assert!(
            !rendered.contains("$schema"),
            "no JSON Schema metadata may reach Gemini: {rendered}"
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
    fn codex_only_item_types_convert_without_failing_the_request() {
        let body = serde_json::json!({
            "model": "gemini-2.5-flash",
            "instructions": "You are a coding agent.",
            "input": [
                {"type": "message", "role": "user",
                 "content": [{"type": "input_text", "text": "fix the typo"}]},
                {"type": "custom_tool_call", "id": "ctc_1", "call_id": "call_1",
                 "name": "apply_patch", "input": "*** Begin Patch\n*** End Patch\n"},
                {"type": "custom_tool_call_output", "call_id": "call_1", "output": "Success"},
                {"type": "local_shell_call", "id": "lsc_1", "call_id": "call_2",
                 "action": {"type": "exec", "command": ["ls"]}},
                {"type": "local_shell_call_output", "call_id": "call_2", "output": "ok"},
                {"type": "function_call", "id": "fc_1", "call_id": "call_3",
                 "name": "shell", "arguments": "{\"command\":[\"ls\"]}"},
                {"type": "function_call_output", "call_id": "call_3", "output": "ok"},
                {"type": "input_text", "text": "this too"}
            ],
            "tools": [
                {"type": "function", "name": "shell", "description": "run a command",
                 "parameters": {"type": "object", "properties": {"command": {"type": "array"}}}},
                {"type": "custom", "name": "apply_patch", "description": "edit files",
                 "format": {"type": "grammar", "syntax": "lark", "definition": "start: TEXT"}}
            ],
            "max_output_tokens": 16
        });
        let gemini_body: Value = serde_json::from_slice(
            &responses_request_to_gemini(&serde_json::to_vec(&body).unwrap())
                .expect("codex items convert cleanly"),
        )
        .unwrap();
        let contents = gemini_body["contents"].as_array().expect("contents array");

        // Both tool call/result pairs survived, `local_shell_*` was dropped rather
        // than failing the request, and the bare content part became its own turn.
        assert_eq!(
            contents.len(),
            6,
            "user → apply_patch pair → shell pair → bare text: {gemini_body}"
        );
        assert_eq!(
            contents[1]["parts"][0]["functionCall"]["name"],
            "apply_patch"
        );
        assert_eq!(
            contents[1]["parts"][0]["functionCall"]["args"]["input"],
            "*** Begin Patch\n*** End Patch\n",
            "the freeform payload keeps the key its schema declares: {gemini_body}"
        );
        assert_eq!(
            contents[2]["parts"][0]["functionResponse"]["name"],
            "apply_patch"
        );
        assert_eq!(contents[3]["parts"][0]["functionCall"]["name"], "shell");
        assert_eq!(contents[4]["parts"][0]["functionResponse"]["name"], "shell");
        assert_eq!(contents[5]["parts"][0]["text"], "this too");
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
            &BTreeMap::new(),
        )
        .unwrap();
        let output = String::from_utf8(converted.body).unwrap();

        assert!(output.contains("event: response.created"));
        assert!(output.contains("event: response.output_text.delta"));
        assert!(output.contains("event: response.function_call_arguments.delta"));
        assert!(output.contains("event: response.completed"));
    }
}

use super::common::{
    flatten_responses_function_tools, gemini_thinking_config, is_reasoning_input_item,
    response_tool_name, response_tool_namespace, responses_reasoning_effort,
    ResponsesToolNamespaces,
};
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
    if let Some(thinking_config) = responses_reasoning_effort(object).and_then(|effort| {
        let model = object
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default();
        gemini_thinking_config(&effort, model)
    }) {
        result
            .entry("generationConfig".to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| "generationConfig must be an object".to_string())?
            .insert("thinkingConfig".to_string(), thinking_config);
    }
    if let Some(tools) = object.get("tools") {
        let declarations = convert_tools(tools)?;
        // Hosted tools are dropped on the way here, so a request can arrive with
        // tools and leave with nothing to declare. A forced mode with nothing to
        // call is a 400 on Gemini's side, so both keys hang off the declarations.
        if !declarations.is_empty() {
            let forced = convert_tool_choice(object.get("tool_choice"), &declarations);
            result.insert(
                "tools".to_string(),
                json!([{"functionDeclarations": declarations}]),
            );
            if let Some(config) = forced {
                result.insert(
                    "toolConfig".to_string(),
                    json!({"functionCallingConfig": config}),
                );
            }
        }
    }

    serde_json::to_vec(&Value::Object(result))
        .map_err(|error| format!("Could not serialize Gemini request: {error}"))
}

pub(super) fn gemini_response_to_responses(
    _status: u16,
    content_type: Option<&str>,
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
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
            body: gemini_sse_to_responses(body, tool_namespaces)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }
    Ok(TransformedBridgeResponse {
        body: gemini_json_to_responses(body, tool_namespaces)?,
        content_type: Some("application/json".to_string()),
    })
}

fn gemini_json_to_responses(
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Vec<u8>, String> {
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
        .cloned()
        .unwrap_or_else(|| json!({}));
    // Gemini legitimately returns a candidate with no content (MAX_TOKENS spent
    // entirely on thinking, SAFETY, RECITATION), and a prompt-level block omits
    // candidates altogether. Neither is a transform failure.
    let parts = candidate
        .get("content")
        .and_then(Value::as_object)
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (output, text) = gemini_parts_to_responses_output(response_id, &parts, tool_namespaces)?;
    let block_reason = prompt_block_reason(&value);
    let finish_reason =
        block_reason.or_else(|| candidate.get("finishReason").and_then(Value::as_str));
    let response = json!({
        "id": response_id,
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": responses_status(finish_reason),
        "model": model,
        "output": output,
        "output_text": text,
        "error": gemini_error_payload(block_reason, finish_reason),
        "incomplete_details": incomplete_details(finish_reason),
        "usage": gemini_usage_to_responses(value.get("usageMetadata")),
    });
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Responses response: {error}"))
}

fn gemini_sse_to_responses(
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Vec<u8>, String> {
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
        tool_namespaces,
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
                if is_reasoning_input_item(item) {
                    continue;
                }
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
                    // Codex-only item types. They appear from the second turn of
                    // any session that used a freeform tool (`apply_patch`) or the
                    // sandboxed shell, and rejecting one fails the whole request —
                    // so the account looks broken while the same relay works from a
                    // client that speaks this upstream's dialect natively. Mirrors
                    // `responses_chat::convert_input_item`.
                    Some("custom_tool_call") | Some("tool_search_call") => {
                        contents.push(convert_function_call(
                            &codex_tool_call_as_function_call(object),
                            &mut state,
                        )?);
                    }
                    Some("custom_tool_call_output") | Some("tool_search_output") => {
                        contents.push(convert_function_result(
                            &codex_tool_output_as_function_output(object),
                            &state,
                        )?);
                    }
                    // Hosted tools are filtered out of the declaration Gemini
                    // receives, so replaying their calls would name a function it
                    // was never told about.
                    Some(
                        "web_search_call"
                        | "web_search_call_output"
                        | "file_search_call"
                        | "file_search_call_output"
                        | "computer_call"
                        | "computer_call_output"
                        | "local_shell_call"
                        | "local_shell_call_output",
                    ) => {}
                    // A bare content part used as an input item rather than
                    // wrapped in a message.
                    Some("input_text" | "input_image" | "input_file" | "input_audio") => {
                        let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
                        contents.push(json!({
                            "role": if role == "assistant" { "model" } else { "user" },
                            "parts": convert_message_content(&Value::Array(vec![item.clone()]))?
                        }));
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

/// Restate a Codex freeform / tool-search call in `function_call` terms.
///
/// A freeform call's whole payload is the single `input` string, and
/// `custom_tool_to_function_tool` declares that as `{"input": string}` — so the
/// replayed call has to use the same key or it contradicts the schema the tool was
/// declared with. `tool_search_call` carries no name of its own.
fn codex_tool_call_as_function_call(object: &Map<String, Value>) -> Map<String, Value> {
    let mut normalized = object.clone();
    if object.get("type").and_then(Value::as_str) == Some("tool_search_call") {
        normalized.insert("name".to_string(), json!("tool_search"));
    } else {
        normalized.insert(
            "arguments".to_string(),
            json!({"input": object.get("input").cloned().unwrap_or_else(|| json!(""))}),
        );
    }
    if !normalized.contains_key("call_id") {
        if let Some(id) = object.get("id").cloned() {
            normalized.insert("call_id".to_string(), id);
        }
    }
    normalized
}

/// The matching output item, so `convert_function_result` can resolve the name
/// from the call it recorded.
fn codex_tool_output_as_function_output(object: &Map<String, Value>) -> Map<String, Value> {
    let mut normalized = object.clone();
    if !normalized.contains_key("output") {
        if let Some(result) = object.get("result").cloned() {
            normalized.insert("output".to_string(), result);
        }
    }
    if object.get("type").and_then(Value::as_str) == Some("tool_search_output")
        && !normalized.contains_key("name")
    {
        normalized.insert("name".to_string(), json!("tool_search"));
    }
    normalized
}

fn convert_message(object: &Map<String, Value>) -> Result<Value, String> {
    // Gemini accepts only "user" and "model"; forwarding "assistant",
    // "developer", or "system" verbatim is a 400 on contents[N].role.
    let role = match object.get("role").and_then(Value::as_str).unwrap_or("user") {
        "assistant" => "model",
        _ => "user",
    };
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
        Value::Array(parts) => parts
            .iter()
            .map(convert_content_part)
            .filter_map(|result| match result {
                Ok(Some(value)) => Some(Ok(value)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err("Responses message content must be a string or array".to_string()),
    }
}

fn convert_content_part(part: &Value) -> Result<Option<Value>, String> {
    let object = part
        .as_object()
        .ok_or_else(|| "Responses content parts must be objects".to_string())?;
    match object.get("type").and_then(Value::as_str) {
        Some("input_text" | "output_text" | "text") => Ok(object
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(|text| json!({"text": text}))),
        Some("input_image") => {
            let image_url = required_string(object, "image_url", "input_image")?;
            // A remote URL has no bytes to inline; skip it rather than fail the
            // whole request over one attachment.
            let Some((mime_type, data)) = parse_base64_data_url(image_url) else {
                return Ok(None);
            };
            Ok(Some(json!({
                "inlineData": {
                    "mimeType": mime_type,
                    "data": data
                }
            })))
        }
        Some("input_file") => {
            let Some((mime_type, data)) = object
                .get("file_data")
                .and_then(Value::as_str)
                .and_then(parse_base64_data_url)
            else {
                return Ok(None);
            };
            Ok(Some(json!({
                "inlineData": {"mimeType": mime_type, "data": data}
            })))
        }
        // Unknown/newer part types are skipped: a degraded turn beats a 502 that
        // also marks every credential in the pool as failed.
        Some(_) | None => Ok(None),
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
    // Responses carries `arguments` as a serialized JSON string, but Gemini's
    // FunctionCall.args is a protobuf Struct — a string there is a 400.
    let arguments = match object.get("arguments") {
        Some(Value::String(raw)) => serde_json::from_str::<Value>(raw.trim())
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({})),
        Some(Value::Object(map)) => Value::Object(map.clone()),
        _ => json!({}),
    };
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
        .or_else(|| {
            state
                .function_names
                .get(call_id)
                .map(|value: &String| value.as_str())
        })
        .ok_or_else(|| {
            format!("Gemini bridge cannot resolve function name for call_id `{call_id}`")
        })?;
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

fn convert_tools(tools: &Value) -> Result<Vec<Value>, String> {
    let tools = flatten_responses_function_tools(tools)?;
    let mut declarations = Vec::with_capacity(tools.len());
    for object in tools {
        let name = required_string(&object, "name", "function tool")?;
        // Gemini's `parameters` is a restricted proto that rejects JSON Schema
        // keywords; the sanitizer picks a channel that can carry this schema.
        // `parameters` / `inputSchema` are both spellings Responses tools use.
        let schema = object
            .get("parameters")
            .or_else(|| object.get("inputSchema"));
        declarations.push(super::gemini_schema::build_gemini_function_declaration(
            name,
            object.get("description"),
            schema,
        ));
    }
    Ok(declarations)
}

/// Responses states forced tool use per request, Gemini in `toolConfig`.
///
/// Unrecognized choices are dropped rather than rejected: `tool_choice` also
/// names hosted tools that never reach Gemini, and failing the whole request over
/// one Gemini cannot express is worse than letting the model decide.
fn convert_tool_choice(tool_choice: Option<&Value>, declarations: &[Value]) -> Option<Value> {
    let forced_name = |value: &Map<String, Value>| {
        value
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| match declared_function_name(declarations, name) {
                // Gemini has no single-tool mode: naming a function is `ANY`
                // narrowed to that name.
                Some(declared) => json!({"mode": "ANY", "allowedFunctionNames": [declared]}),
                // Forcing a name Gemini was never given is a 400, so an
                // unresolvable name keeps the "must call something" intent only.
                None => json!({"mode": "ANY"}),
            })
    };
    match tool_choice? {
        Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(json!({"mode": "AUTO"})),
            "required" | "any" => Some(json!({"mode": "ANY"})),
            "none" => Some(json!({"mode": "NONE"})),
            _ => None,
        },
        Value::Object(value) => match value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            // Responses names a forced tool via {type:"function",name:"x"}.
            "function" | "tool" | "custom" => forced_name(value),
            "allowed_tools" => Some(json!({"mode": "ANY"})),
            "auto" => Some(json!({"mode": "AUTO"})),
            "required" | "any" => Some(json!({"mode": "ANY"})),
            "none" => Some(json!({"mode": "NONE"})),
            _ => None,
        },
        _ => None,
    }
}

/// Matches a `tool_choice` name against the declarations actually sent.
///
/// A namespaced Responses tool is declared under its flattened
/// `namespace__name`, while `tool_choice` carries the bare name the client wrote,
/// so the two only line up after this lookup.
fn declared_function_name<'a>(declarations: &'a [Value], name: &str) -> Option<&'a str> {
    let declared = |declaration: &'a Value| declaration.get("name").and_then(Value::as_str);
    if let Some(exact) = declarations
        .iter()
        .filter_map(declared)
        .find(|candidate| *candidate == name)
    {
        return Some(exact);
    }
    let suffix = format!("__{name}");
    let mut namespaced = declarations
        .iter()
        .filter_map(declared)
        .filter(|candidate| candidate.ends_with(&suffix));
    let first = namespaced.next()?;
    // Two namespaces exposing the same tool name cannot be told apart from the
    // bare name alone; forcing the wrong one is worse than not narrowing.
    namespaced.next().is_none().then_some(first)
}

fn gemini_parts_to_responses_output(
    response_id: &str,
    parts: &[Value],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<(Vec<Value>, String), String> {
    let mut output = Vec::new();
    let mut text = String::new();
    let mut message_parts = Vec::new();
    for part in parts {
        let object = part
            .as_object()
            .ok_or_else(|| "Gemini content parts must be objects".to_string())?;
        // `thought: true` marks internal reasoning. Emitting it as output_text
        // would leak the model's thinking into the user-visible answer.
        if object.get("thought").and_then(Value::as_bool) == Some(true) {
            continue;
        }
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
            let response_name = response_tool_name(name, tool_namespaces);
            let arguments = function_call
                .get("args")
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new()));
            let mut function_call = json!({
                "id": gemini_call_id(response_id, output.len()),
                "type": "function_call",
                "status": "completed",
                "call_id": gemini_call_id(response_id, output.len()),
                "name": response_name,
                "arguments": serde_json::to_string(&arguments)
                    .map_err(|error| format!("Could not serialize Gemini function args: {error}"))?
            });
            if let Some(namespace) = response_tool_namespace(name, tool_namespaces) {
                function_call["namespace"] = Value::String(namespace.to_string());
            }
            output.push(function_call);
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
    let field = |key: &str| usage.get(key).and_then(Value::as_i64).unwrap_or(0);
    let prompt_tokens = field("promptTokenCount");
    let cached_tokens = field("cachedContentTokenCount");
    let thoughts_tokens = field("thoughtsTokenCount");
    let candidates_tokens = field("candidatesTokenCount");
    let total_tokens = usage
        .get("totalTokenCount")
        .and_then(Value::as_i64)
        .unwrap_or(prompt_tokens + candidates_tokens + thoughts_tokens);
    // candidatesTokenCount omits thinking tokens, so derive output from the total
    // to capture them even when thoughtsTokenCount is absent.
    let output_tokens = total_tokens
        .saturating_sub(prompt_tokens)
        .max(candidates_tokens + thoughts_tokens);
    json!({
        "input_tokens": prompt_tokens,
        "input_tokens_details": {"cached_tokens": cached_tokens},
        "output_tokens": output_tokens,
        "output_tokens_details": {"reasoning_tokens": thoughts_tokens},
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
                let function_object = function_entry
                    .as_object_mut()
                    .ok_or_else(|| "Gemini functionCall entry must be an object".to_string())?;
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
        // A safety stop is a completed turn the model refused, not an upstream
        // failure. Reporting "failed" makes the proxy's own failure detector
        // retry the request across every credential and mark each one failed.
        Some(
            "SAFETY" | "RECITATION" | "SPII" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "IMAGE_SAFETY",
        ) => "incomplete",
        _ => "completed",
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

/// Surfaces a content-filter stop as a readable reason instead of a bare
/// `status` with `error: null`, which tells the client nothing.
fn gemini_error_payload(block_reason: Option<&str>, finish_reason: Option<&str>) -> Value {
    let reason = block_reason.or(match finish_reason {
        Some(
            reason @ ("SAFETY" | "RECITATION" | "SPII" | "BLOCKLIST" | "PROHIBITED_CONTENT"
            | "IMAGE_SAFETY"),
        ) => Some(reason),
        _ => None,
    });
    match reason {
        Some(reason) => json!({
            "code": "content_filter",
            "message": format!("Gemini stopped generating: {reason}")
        }),
        None => Value::Null,
    }
}

fn incomplete_details(finish_reason: Option<&str>) -> Value {
    match finish_reason {
        Some("MAX_TOKENS") => json!({"reason": "max_output_tokens"}),
        Some(
            "SAFETY" | "RECITATION" | "SPII" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "IMAGE_SAFETY",
        ) => json!({"reason": "content_filter"}),
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
