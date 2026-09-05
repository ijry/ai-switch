#[cfg(test)]
mod tests {
    /// A Codex client never sends cache_control, so without injection every
    /// turn re-bills the whole system prompt and tool array at full price.
    #[test]
    fn codex_requests_get_cache_breakpoints() {
        let body = serde_json::json!({
            "model": "claude-sonnet-4",
            "max_output_tokens": 1024,
            "instructions": "You are a careful engineer.",
            "input": [
                {"role": "user", "content": [{"type": "input_text", "text": "first"}]},
                {"type": "function_call", "call_id": "c1", "name": "read", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "c1", "output": "ok"},
                {"role": "user", "content": [{"type": "input_text", "text": "second"}]}
            ],
            "tools": [{
                "type": "function",
                "name": "read",
                "parameters": {"type": "object", "properties": {}}
            }]
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_anthropic(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        // Tools and system carry the stable prefix markers.
        let tools = converted["tools"].as_array().expect("tools");
        assert!(
            tools.last().unwrap()["cache_control"].is_object(),
            "tools tail must be marked: {converted}"
        );
        assert!(
            converted["system"][0]["cache_control"].is_object(),
            "system tail must be marked: {converted}"
        );

        // At least one message anchor extends the cached prefix, and the total
        // stays within Anthropic's limit of four.
        let rendered = serde_json::to_string(&converted).unwrap();
        let total = rendered.matches("\"cache_control\"").count();
        assert!(total >= 3, "expected several breakpoints, got {total}");
        assert!(total <= 4, "must not exceed 4 breakpoints, got {total}");
    }

    use super::{anthropic_response_to_responses, responses_request_to_anthropic};
    use serde_json::Value;
    use std::collections::BTreeMap;

    #[test]
    fn converts_responses_request_to_claude_messages() {
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "instructions": "Be concise",
            "input": [
                {"role": "user", "content": [{"type": "input_text", "text": "Find x"}]},
                {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{\"key\":\"x\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "42"}
            ],
            "max_output_tokens": 64,
            "temperature": 0.2,
            "tools": [{
                "type": "function",
                "name": "lookup",
                "description": "Lookup value",
                "parameters": {"type":"object","properties":{"key":{"type":"string"}}}
            }]
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_anthropic(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        // `system` is promoted to Anthropic's block form so it can carry a
        // cache_control breakpoint; the text itself is unchanged.
        assert_eq!(converted["system"][0]["type"], "text");
        assert_eq!(converted["system"][0]["text"], "Be concise");
        assert_eq!(converted["messages"][0]["role"], "user");
        assert_eq!(converted["messages"][0]["content"][0]["type"], "text");
        assert_eq!(converted["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(
            converted["messages"][2]["content"][0]["type"],
            "tool_result"
        );
        assert_eq!(converted["max_tokens"], 64);
        assert_eq!(converted["tools"][0]["input_schema"]["type"], "object");
    }

    #[test]
    fn forwards_metadata_so_claude_code_gated_relays_accept_the_request() {
        // Relays gating on the Claude Code signature parse `metadata.user_id` and
        // reject the request when it is missing, so dropping the field during
        // conversion breaks Codex against those upstreams.
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "input": "hi",
            "metadata": {"user_id": "{\"device_id\":\"abc\",\"session_id\":\"s\"}"}
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_anthropic(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(
            converted["metadata"]["user_id"],
            "{\"device_id\":\"abc\",\"session_id\":\"s\"}"
        );
    }

    #[test]
    fn converts_responses_input_image_to_anthropic_image_block() {
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "input": [
                {
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": "See image"},
                        {"type": "input_image", "image_url": "data:image/png;base64,aGVsbG8="}
                    ]
                }
            ]
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_anthropic(&serde_json::to_vec(&body).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(converted["messages"][0]["content"][0]["type"], "text");
        assert_eq!(converted["messages"][0]["content"][1]["type"], "image");
        assert_eq!(
            converted["messages"][0]["content"][1]["source"]["type"],
            "base64"
        );
        assert_eq!(
            converted["messages"][0]["content"][1]["source"]["media_type"],
            "image/png"
        );
        assert_eq!(
            converted["messages"][0]["content"][1]["source"]["data"],
            "aGVsbG8="
        );
    }

    /// Codex replays its own transcript on every turn, and after the first
    /// `apply_patch` that transcript contains item types no Anthropic client ever
    /// sends. Rejecting one fails the whole request, so the relay looks dead
    /// under the codex platform while the same base_url and key keep working
    /// under the claude platform.
    #[test]
    fn converts_codex_only_input_items_instead_of_failing_the_turn() {
        let body = serde_json::json!({
            "model": "claude-sonnet-4-5",
            "instructions": "You are a coding agent running in the Codex CLI.",
            "input": [
                {"type": "message", "role": "user",
                 "content": [{"type": "input_text", "text": "fix the typo"}]},
                {"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "gAAAA"},
                {"type": "custom_tool_call", "id": "ctc_1", "call_id": "call_1",
                 "name": "apply_patch", "input": "*** Begin Patch\n*** End Patch\n"},
                {"type": "custom_tool_call_output", "call_id": "call_1", "output": "Success"},
                {"type": "local_shell_call", "id": "lsc_1", "call_id": "call_2",
                 "action": {"type": "exec", "command": ["ls"]}},
                {"type": "local_shell_call_output", "call_id": "call_2", "output": "README.md"},
                {"type": "input_text", "text": "and run the tests"}
            ]
        });

        let converted: Value = serde_json::from_slice(
            &responses_request_to_anthropic(&serde_json::to_vec(&body).unwrap())
                .expect("Codex transcript must convert"),
        )
        .unwrap();
        let messages = converted["messages"].as_array().expect("messages");

        // The freeform call keeps the `{"input": …}` spelling the tool was
        // declared with, so the replayed turn agrees with its own schema.
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"][0]["type"], "tool_use");
        assert_eq!(messages[1]["content"][0]["name"], "apply_patch");
        assert_eq!(messages[1]["content"][0]["id"], "call_1");
        assert_eq!(
            messages[1]["content"][0]["input"]["input"],
            "*** Begin Patch\n*** End Patch\n"
        );
        assert_eq!(messages[2]["content"][0]["type"], "tool_result");
        assert_eq!(messages[2]["content"][0]["tool_use_id"], "call_1");
        assert_eq!(messages[2]["content"][0]["content"], "Success");

        // `local_shell` is filtered out of the tool array, so replaying its call
        // would be a tool_use Claude cannot match to any declared tool.
        let rendered = serde_json::to_string(&converted).unwrap();
        assert!(
            !rendered.contains("call_2"),
            "hosted-tool calls must be dropped, not forwarded: {rendered}"
        );
        assert_eq!(
            messages.last().unwrap()["content"][0]["text"],
            "and run the tests"
        );
    }

    #[test]
    fn converts_anthropic_response_to_responses_json() {
        let upstream = serde_json::json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [
                {"type": "text", "text": "hello"},
                {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {"key":"x"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 3, "output_tokens": 5}
        });

        let converted = anthropic_response_to_responses(
            200,
            Some("application/json"),
            serde_json::to_vec(&upstream).unwrap().as_slice(),
            &BTreeMap::new(),
        )
        .unwrap();
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(output["object"], "response");
        assert_eq!(output["id"], "msg_1");
        assert_eq!(output["output_text"], "hello");
        assert_eq!(output["output"][1]["type"], "function_call");
        assert_eq!(output["output"][1]["call_id"], "toolu_1");
        assert_eq!(output["usage"]["input_tokens"], 3);
        assert_eq!(output["usage"]["output_tokens"], 5);
    }

    #[test]
    fn converts_anthropic_sse_to_responses_events() {
        let upstream = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-sonnet-4\",\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );

        let converted = anthropic_response_to_responses(
            200,
            Some("text/event-stream"),
            upstream.as_bytes(),
            &BTreeMap::new(),
        )
        .unwrap();
        let output = String::from_utf8(converted.body).unwrap();

        assert!(output.contains("event: response.created"));
        assert!(output.contains("event: response.output_text.delta"));
        assert!(output.contains("\"delta\":\"hello\""));
        assert!(output.contains("event: response.completed"));
    }

    /// This bridge asks the upstream for thinking, so it must accept the thinking
    /// blocks that request produces. Failing here 502s and fails every credential.
    #[test]
    fn accepts_the_thinking_it_enables() {
        let request = serde_json::json!({
            "model": "claude-sonnet-4",
            "max_output_tokens": 8192,
            "reasoning": {"effort": "medium"},
            "input": [{"type": "message", "role": "user", "content": "hi"}]
        });
        let prepared: Value = serde_json::from_slice(
            &responses_request_to_anthropic(&serde_json::to_vec(&request).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            prepared["thinking"]["type"], "enabled",
            "precondition: the bridge enables thinking"
        );

        let upstream = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-sonnet-4\",\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Let me think.\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"Erf1\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"42\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );

        let converted = anthropic_response_to_responses(
            200,
            Some("text/event-stream"),
            upstream.as_bytes(),
            &BTreeMap::new(),
        )
        .expect("thinking deltas must not fail the transform");
        let output = String::from_utf8(converted.body).unwrap();

        assert!(output.contains("event: response.completed"));
        assert!(
            output.contains("\"delta\":\"42\""),
            "visible text must survive: {output}"
        );
        assert!(
            !output.contains("Let me think."),
            "reasoning must not leak into visible output: {output}"
        );
    }

    #[test]
    fn thinking_content_blocks_do_not_become_output_text() {
        let upstream = serde_json::json!({
            "id": "msg_1",
            "model": "claude-sonnet-4",
            "stop_reason": "end_turn",
            "content": [
                {"type": "thinking", "thinking": "Internal reasoning.", "signature": "sig"},
                {"type": "redacted_thinking", "data": "opaque"},
                {"type": "text", "text": "The answer is 42."}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });

        let converted = anthropic_response_to_responses(
            200,
            Some("application/json"),
            &serde_json::to_vec(&upstream).unwrap(),
            &BTreeMap::new(),
        )
        .expect("thinking blocks must not fail the transform");
        let value: Value = serde_json::from_slice(&converted.body).unwrap();

        let rendered = serde_json::to_string(&value).unwrap();
        assert!(
            !rendered.contains("Internal reasoning."),
            "reasoning must not leak into output: {rendered}"
        );
        assert!(rendered.contains("The answer is 42."));
    }
}

use super::common::{
    anthropic_thinking_budget, flatten_responses_function_tools, is_reasoning_input_item,
    response_tool_name, response_tool_namespace, response_tool_parameters,
    responses_reasoning_effort, ResponsesToolNamespaces,
};
use super::{common::parse_base64_data_url, sse, TransformedBridgeResponse};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Anthropic requires `max_tokens`; the Responses API does not, and Codex never
/// sends `max_output_tokens`. Large enough not to truncate real answers.
const DEFAULT_ANTHROPIC_MAX_TOKENS: i64 = 8_192;

pub(super) fn responses_request_to_anthropic(body: &[u8]) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Responses request JSON is invalid: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Responses request body must be a JSON object".to_string())?;
    let mut result = Map::new();

    if let Some(model) = object.get("model") {
        result.insert("model".to_string(), model.clone());
    }
    if let Some(instructions) = object.get("instructions") {
        let system = text_value(instructions, "instructions")?;
        if !system.is_empty() {
            result.insert("system".to_string(), Value::String(system));
        }
    }
    let mut messages = Vec::new();
    if let Some(input) = object.get("input") {
        messages.extend(convert_input(input)?);
    }
    result.insert("messages".to_string(), Value::Array(messages));
    // Anthropic requires max_tokens, but the Responses API treats
    // max_output_tokens as optional and Codex omits it entirely. Without a
    // default every Codex request would 400.
    let max_tokens = object
        .get("max_output_tokens")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0);
    result.insert(
        "max_tokens".to_string(),
        json!(max_tokens.unwrap_or(DEFAULT_ANTHROPIC_MAX_TOKENS)),
    );
    if let Some(effort) = responses_reasoning_effort(object)
        .and_then(|effort| anthropic_thinking_budget(&effort, max_tokens))
    {
        result.insert(
            "thinking".to_string(),
            json!({"type": "enabled", "budget_tokens": effort}),
        );
    }
    // `metadata` rides along because relays gating on the Claude Code signature
    // parse `metadata.user_id` and reject the request when it is absent.
    copy_fields(
        object,
        &mut result,
        &["temperature", "top_p", "stream", "metadata"],
    );
    if result.get("thinking").is_some() {
        // Extended thinking rejects temperature outright and constrains top_p to
        // 0.95-1, so drop both rather than risk a 400 on the caller's value.
        result.remove("temperature");
        result.remove("top_p");
    }
    if let Some(stop) = object.get("stop") {
        result.insert("stop_sequences".to_string(), stop.clone());
    }
    if let Some(tools) = object.get("tools") {
        let converted_tools = convert_tools(tools)?;
        if converted_tools
            .as_array()
            .is_some_and(|tools| !tools.is_empty())
        {
            result.insert("tools".to_string(), converted_tools);
        }
    }
    // Only meaningful alongside tools; a dangling tool_choice is a 400.
    if result.contains_key("tools") {
        if let Some(tool_choice) = responses_tool_choice_to_anthropic(object) {
            result.insert("tool_choice".to_string(), tool_choice);
        }
    }

    // A Codex client has no cache_control concept, so without this the whole
    // system prompt and tool array is re-billed at full input price every turn.
    let mut result = Value::Object(result);
    super::anthropic_cache::inject_cache_breakpoints(&mut result);

    serde_json::to_vec(&result)
        .map_err(|error| format!("Could not serialize Anthropic request: {error}"))
}

/// Maps a Responses `tool_choice` (plus `parallel_tool_calls`) onto Anthropic's
/// shape. Dropping this silently downgrades a forced tool call to optional,
/// which stalls agent loops that depend on it.
fn responses_tool_choice_to_anthropic(object: &Map<String, Value>) -> Option<Value> {
    let disable_parallel = object
        .get("parallel_tool_calls")
        .and_then(Value::as_bool)
        .is_some_and(|parallel| !parallel);
    let mut choice = match object.get("tool_choice") {
        Some(Value::String(value)) => match value.trim().to_ascii_lowercase().as_str() {
            "auto" => json!({"type": "auto"}),
            "required" | "any" => json!({"type": "any"}),
            "none" => json!({"type": "none"}),
            _ => return None,
        },
        Some(Value::Object(value)) => {
            let choice_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match choice_type {
                // Responses names a forced tool via {type:"function",name:"x"}.
                "function" | "tool" | "custom" => {
                    let name = value
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|name| !name.is_empty())?;
                    json!({"type": "tool", "name": name})
                }
                "allowed_tools" => json!({"type": "any"}),
                "auto" => json!({"type": "auto"}),
                "required" | "any" => json!({"type": "any"}),
                "none" => json!({"type": "none"}),
                _ => return None,
            }
        }
        // No explicit choice: still surface a parallel-tool-use opt-out.
        _ if disable_parallel => json!({"type": "auto"}),
        _ => return None,
    };
    // Anthropic expresses "no parallel calls" as a flag on tool_choice, and
    // rejects it on {type:"none"}.
    if disable_parallel && choice.get("type").and_then(Value::as_str) != Some("none") {
        choice["disable_parallel_tool_use"] = Value::Bool(true);
    }
    Some(choice)
}

pub(super) fn anthropic_response_to_responses(
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
        let response = anthropic_sse_to_responses_json(body, tool_namespaces)?;
        return Ok(TransformedBridgeResponse {
            body: sse::responses_events_from_completed_response(&response)?,
            content_type: Some("text/event-stream".to_string()),
        });
    }
    Ok(TransformedBridgeResponse {
        body: anthropic_json_to_responses(body, tool_namespaces)?,
        content_type: Some("application/json".to_string()),
    })
}

fn anthropic_json_to_responses(
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Vec<u8>, String> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("Anthropic response JSON is invalid: {error}"))?;
    if value.get("error").is_some() {
        return Ok(body.to_vec());
    }
    anthropic_value_to_responses_json(&value, tool_namespaces)
}

fn anthropic_value_to_responses_json(
    value: &Value,
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Vec<u8>, String> {
    let response_id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_ai_switch");
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let stop_reason = value.get("stop_reason").and_then(Value::as_str);
    let (output, output_text) = anthropic_content_to_responses_output(
        response_id,
        value.get("content").and_then(Value::as_array),
        tool_namespaces,
    )?;
    let response = json!({
        "id": response_id,
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": responses_status(stop_reason),
        "model": model,
        "output": output,
        "output_text": output_text,
        "error": Value::Null,
        "incomplete_details": incomplete_details(stop_reason),
        "usage": anthropic_usage_to_responses(value.get("usage")),
    });
    serde_json::to_vec(&response)
        .map_err(|error| format!("Could not serialize Responses response: {error}"))
}

fn anthropic_sse_to_responses_json(
    body: &[u8],
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<Value, String> {
    let mut state = AnthropicSseState::default();
    for value in sse::parse_sse_data_records(body)? {
        match value.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                if let Some(message) = value.get("message") {
                    state.capture_message(message);
                }
            }
            Some("content_block_start") => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let block = value
                    .get("content_block")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "text", "text": ""}));
                state.blocks.insert(index, block);
            }
            Some("content_block_delta") => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                state.apply_delta(index, value.get("delta").unwrap_or(&Value::Null))?;
            }
            Some("message_delta") => {
                if let Some(stop_reason) =
                    value.pointer("/delta/stop_reason").and_then(Value::as_str)
                {
                    state.stop_reason = stop_reason.to_string();
                }
                if let Some(output_tokens) = value
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_i64)
                {
                    state.output_tokens = output_tokens;
                }
            }
            Some("message_stop") => {}
            _ => {}
        }
    }

    let message = json!({
        "id": state.response_id(),
        "model": state.model(),
        "content": state.blocks.into_values().collect::<Vec<_>>(),
        "stop_reason": if state.stop_reason.is_empty() { "end_turn" } else { &state.stop_reason },
        "usage": {
            "input_tokens": state.input_tokens,
            "output_tokens": state.output_tokens
        }
    });
    let bytes = anthropic_value_to_responses_json(&message, tool_namespaces)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Could not parse buffered Responses JSON: {error}"))
}

#[derive(Debug, Default)]
struct AnthropicSseState {
    id: String,
    model: String,
    input_tokens: i64,
    output_tokens: i64,
    stop_reason: String,
    blocks: BTreeMap<usize, Value>,
}

impl AnthropicSseState {
    fn capture_message(&mut self, message: &Value) {
        if self.id.is_empty() {
            self.id = message
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("resp_ai_switch")
                .to_string();
        }
        if self.model.is_empty() {
            self.model = message
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
        }
        self.input_tokens = message
            .pointer("/usage/input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(self.input_tokens);
    }

    fn apply_delta(&mut self, index: usize, delta: &Value) -> Result<(), String> {
        let block = self
            .blocks
            .entry(index)
            .or_insert_with(|| json!({"type": "text", "text": ""}));
        match delta.get("type").and_then(Value::as_str) {
            Some("text_delta") => {
                let text = delta.get("text").and_then(Value::as_str).unwrap_or("");
                let current = block.get("text").and_then(Value::as_str).unwrap_or("");
                block["text"] = Value::String(format!("{current}{text}"));
            }
            Some("input_json_delta") => {
                let partial_json = delta
                    .get("partial_json")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let current = block
                    .get("_partial_json")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                block["_partial_json"] = Value::String(format!("{current}{partial_json}"));
                if let Some(input) = serde_json::from_str::<Value>(
                    block
                        .get("_partial_json")
                        .and_then(Value::as_str)
                        .unwrap_or("{}"),
                )
                .ok()
                {
                    block["input"] = input;
                }
            }
            // thinking_delta / signature_delta arrive whenever this bridge asked
            // for thinking (see the request path). Reasoning has no Responses
            // input slot here, so absorb the deltas instead of failing.
            Some(_) | None => {}
        }
        Ok(())
    }

    fn response_id(&self) -> &str {
        if self.id.is_empty() {
            "resp_ai_switch"
        } else {
            &self.id
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

fn convert_input(input: &Value) -> Result<Vec<Value>, String> {
    match input {
        Value::String(text) => Ok(vec![json!({"role": "user", "content": [text_block(text)]})]),
        Value::Array(items) => items
            .iter()
            .filter(|item| !is_reasoning_input_item(item))
            .filter_map(|item| convert_input_item(item).transpose())
            .collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err("Responses input must be a string or array".to_string()),
    }
}

/// One Anthropic message per Responses input item, or `None` for items that have
/// no Anthropic equivalent and must be dropped rather than rejected.
///
/// The tail of this match is the part worth being careful with. Codex's own
/// transcript carries item types no other client produces — `custom_tool_call`
/// for freeform tools like `apply_patch`, `local_shell_call` for the sandboxed
/// shell — and they appear from the second turn of any session that edited a
/// file. Failing on an unknown type kills the whole request, so the account
/// looks broken while the same relay works under the claude platform (which
/// speaks `/v1/messages` and never reaches this converter). Mirrors
/// `responses_chat::convert_input_item`, which has handled these since the Chat
/// bridge shipped.
fn convert_input_item(item: &Value) -> Result<Option<Value>, String> {
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
            let input = serde_json::from_str::<Value>(arguments)
                .unwrap_or_else(|_| Value::String(arguments.to_string()));
            Ok(Some(json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": call_id,
                    "name": name,
                    "input": input
                }]
            })))
        }
        Some("function_call_output") => {
            let call_id = required_string(object, "call_id", "function_call_output")?;
            let output = object
                .get("output")
                .map(stringify_content)
                .transpose()?
                .unwrap_or_default();
            Ok(Some(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": output
                }]
            })))
        }
        // A freeform tool's whole payload is one string. `{"input": "…"}` is the
        // shape `custom_tool_to_function_tool` advertises to the model, so a call
        // replayed from history has to be spelled the same way or the next turn
        // contradicts the schema the tool was declared with.
        Some(item_type @ ("custom_tool_call" | "tool_search_call")) => {
            let call_id = object
                .get("call_id")
                .or_else(|| object.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|call_id| !call_id.is_empty())
                .ok_or_else(|| format!("Responses {item_type} is missing call_id"))?;
            let (name, input) = if item_type == "tool_search_call" {
                (
                    "tool_search",
                    object
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                )
            } else {
                (
                    required_string(object, "name", "custom_tool_call")?,
                    json!({"input": object.get("input").cloned().unwrap_or_else(|| json!(""))}),
                )
            };
            Ok(Some(json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": call_id,
                    "name": name,
                    "input": input
                }]
            })))
        }
        Some(item_type @ ("custom_tool_call_output" | "tool_search_output")) => {
            let call_id = required_string(object, "call_id", item_type)?;
            let output = object
                .get("output")
                .or_else(|| object.get("result"))
                .map(stringify_content)
                .transpose()?
                .unwrap_or_default();
            Ok(Some(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": output
                }]
            })))
        }
        // Tools the upstream never saw declared (they are filtered out of the
        // tool array by `is_responses_builtin_tool_type`), so replaying their
        // calls would be a tool_use Claude cannot match to a tool.
        Some(
            "web_search_call"
            | "web_search_call_output"
            | "file_search_call"
            | "file_search_call_output"
            | "computer_call"
            | "computer_call_output"
            | "local_shell_call"
            | "local_shell_call_output",
        ) => Ok(None),
        // A bare content part used as an input item, rather than wrapped in a
        // message. Codex sends this shape for pasted images.
        Some("input_text" | "input_image" | "input_file" | "input_audio") => {
            let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = convert_message_content(&Value::Array(vec![item.clone()]))?;
            Ok(Some(json!({"role": role, "content": content})))
        }
        Some("message") | None if object.contains_key("role") => {
            let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = object
                .get("content")
                .map(convert_message_content)
                .transpose()?
                .unwrap_or_else(Vec::new);
            Ok(Some(json!({"role": role, "content": content})))
        }
        Some(other) => Err(format!("Unsupported Responses input item type: {other}")),
        None => Err("Responses input item is missing role or type".to_string()),
    }
}

fn convert_message_content(content: &Value) -> Result<Vec<Value>, String> {
    match content {
        Value::String(text) => Ok(vec![text_block(text)]),
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
            .map(|text| text_block(text))),
        Some("input_image") => {
            let image_url = required_string(object, "image_url", "input_image")?;
            let Some((media_type, data)) = parse_base64_data_url(image_url) else {
                return Err("Anthropic bridge only supports base64 data URL images".to_string());
            };
            Ok(Some(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": media_type,
                    "data": data
                }
            })))
        }
        Some(other) => Err(format!("Unsupported Responses content type: {other}")),
        None => Err("Responses content part is missing type".to_string()),
    }
}

fn convert_tools(tools: &Value) -> Result<Value, String> {
    let tools = flatten_responses_function_tools(tools)?;
    let mut converted = Vec::with_capacity(tools.len());
    for object in tools {
        let name = required_string(&object, "name", "function tool")?;
        let mut converted_tool = Map::new();
        converted_tool.insert("name".to_string(), Value::String(name.to_string()));
        if let Some(description) = object.get("description") {
            converted_tool.insert("description".to_string(), description.clone());
        }
        converted_tool.insert(
            "input_schema".to_string(),
            response_tool_parameters(&object),
        );
        converted.push(Value::Object(converted_tool));
    }
    Ok(Value::Array(converted))
}

fn anthropic_content_to_responses_output(
    response_id: &str,
    content: Option<&Vec<Value>>,
    tool_namespaces: &ResponsesToolNamespaces,
) -> Result<(Vec<Value>, String), String> {
    let mut output = Vec::new();
    let mut text = String::new();
    let mut message_content = Vec::new();
    let Some(content) = content else {
        return Ok((output, text));
    };
    for item in content {
        match item.get("type").and_then(Value::as_str) {
            Some("text") => {
                let item_text = item.get("text").and_then(Value::as_str).unwrap_or("");
                text.push_str(item_text);
                message_content.push(json!({
                    "type": "output_text",
                    "text": item_text,
                    "annotations": [],
                    "logprobs": []
                }));
            }
            Some("tool_use") => {
                let call_id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_ai_switch");
                let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
                let response_name = response_tool_name(name, tool_namespaces);
                let arguments =
                    serde_json::to_string(item.get("input").unwrap_or(&Value::Object(Map::new())))
                        .map_err(|error| {
                            format!("Could not serialize Anthropic tool input: {error}")
                        })?;
                let mut function_call = json!({
                    "id": format!("fc_{}_{}", sanitize_id(response_id), output.len()),
                    "type": "function_call",
                    "status": "completed",
                    "call_id": call_id,
                    "name": response_name,
                    "arguments": arguments
                });
                if let Some(namespace) = response_tool_namespace(name, tool_namespaces) {
                    function_call["namespace"] = Value::String(namespace.to_string());
                }
                output.push(function_call);
            }
            // Thinking blocks arrive because the request path enables thinking.
            // They are reasoning, not visible output, so they must not be pushed
            // into output_text — drop them rather than failing the transform.
            Some("thinking") | Some("redacted_thinking") => {}
            Some(_) | None => {}
        }
    }
    if !message_content.is_empty() {
        output.insert(
            0,
            json!({
                "id": format!("msg_{}", sanitize_id(response_id)),
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": message_content
            }),
        );
    }
    Ok((output, text))
}

fn anthropic_usage_to_responses(usage: Option<&Value>) -> Value {
    let Some(usage) = usage else {
        return Value::Null;
    };
    let input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let field = |key: &str| usage.get(key).and_then(Value::as_i64).unwrap_or(0);
    // Anthropic reports cache reads/writes outside input_tokens, so the Responses
    // total has to add them back or a cached turn undercounts by the whole prefix.
    let cache_read = field("cache_read_input_tokens");
    let cache_creation = field("cache_creation_input_tokens");
    json!({
        "input_tokens": input_tokens + cache_read + cache_creation,
        "input_tokens_details": {"cached_tokens": cache_read},
        "output_tokens": output_tokens,
        "output_tokens_details": {"reasoning_tokens": 0},
        "total_tokens": input_tokens + cache_read + cache_creation + output_tokens
    })
}

fn responses_status(stop_reason: Option<&str>) -> &'static str {
    match stop_reason {
        Some("max_tokens") => "incomplete",
        _ => "completed",
    }
}

fn incomplete_details(stop_reason: Option<&str>) -> Value {
    match stop_reason {
        Some("max_tokens") => json!({"reason": "max_output_tokens"}),
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

fn text_block(text: &str) -> Value {
    json!({"type": "text", "text": text})
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
