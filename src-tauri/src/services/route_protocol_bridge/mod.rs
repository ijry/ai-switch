mod anthropic_cache;
mod claude_chat;
mod claude_gemini;
mod claude_responses;
mod common;
mod gemini_schema;
mod responses_chat;
mod responses_claude;
mod responses_gemini;
mod responses_responses;
mod sse;

use crate::models::platform::{ApiDialect, PlatformId};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolBridgeKind {
    ResponsesToChat,
    ResponsesToResponses,
    ResponsesToAnthropic,
    ResponsesToGemini,
    ClaudeToChat,
    ClaudeToResponses,
    ClaudeToGemini,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBridgeRequest {
    pub kind: Option<ProtocolBridgeKind>,
    pub upstream_path: String,
    pub upstream_query: Option<String>,
    pub body: Vec<u8>,
    pub streaming: bool,
    pub tool_namespaces: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformedBridgeResponse {
    pub body: Vec<u8>,
    pub content_type: Option<String>,
}

/// True when the path is Anthropic's `/v1/messages/count_tokens` endpoint.
///
/// Claude Code calls this to size its context meter. It is not a chat
/// completion, so no bridge can convert it — and most third-party relays do not
/// implement it. Forwarding it upstream earns a 404 that the retry loop would
/// otherwise charge against every credential in the pool.
pub fn is_anthropic_count_tokens_path(path: &str) -> bool {
    common::is_create_subpath(path, "messages", "count_tokens")
}

pub fn prepare_request(
    platform: PlatformId,
    upstream_dialect: ApiDialect,
    path: &str,
    body: &[u8],
) -> Result<PreparedBridgeRequest, String> {
    let streaming = common::request_streaming(body);
    let normalized_path = common::normalize_path(path);
    let is_responses = common::is_create_path(&normalized_path, "responses");
    let is_messages = common::is_create_path(&normalized_path, "messages");

    if platform == PlatformId::Codex && is_responses {
        return match upstream_dialect {
            ApiDialect::OpenAi => Ok(PreparedBridgeRequest {
                kind: Some(ProtocolBridgeKind::ResponsesToChat),
                upstream_path: "/chat/completions".to_string(),
                upstream_query: None,
                body: responses_chat::responses_request_to_chat(body)?,
                streaming,
                tool_namespaces: common::responses_tool_namespaces_from_body(body)?,
            }),
            ApiDialect::OpenAiResponses => Ok(PreparedBridgeRequest {
                kind: Some(ProtocolBridgeKind::ResponsesToResponses),
                upstream_path: "/responses".to_string(),
                upstream_query: None,
                body: responses_responses::responses_request_to_responses(body)?,
                streaming,
                tool_namespaces: common::responses_tool_namespaces_from_body(body)?,
            }),
            ApiDialect::Anthropic => Ok(PreparedBridgeRequest {
                kind: Some(ProtocolBridgeKind::ResponsesToAnthropic),
                upstream_path: "/v1/messages".to_string(),
                upstream_query: None,
                body: responses_claude::responses_request_to_anthropic(body)?,
                streaming,
                tool_namespaces: common::responses_tool_namespaces_from_body(body)?,
            }),
            ApiDialect::Gemini => {
                let model = common::gemini_model_from_body(body)?;
                let (upstream_path, upstream_query) = common::gemini_endpoint(&model, streaming);
                Ok(PreparedBridgeRequest {
                    kind: Some(ProtocolBridgeKind::ResponsesToGemini),
                    upstream_path,
                    upstream_query,
                    body: responses_gemini::responses_request_to_gemini(body)?,
                    streaming,
                    tool_namespaces: common::responses_tool_namespaces_from_body(body)?,
                })
            }
        };
    }

    if platform == PlatformId::Claude && is_messages {
        return match upstream_dialect {
            ApiDialect::OpenAi => Ok(PreparedBridgeRequest {
                kind: Some(ProtocolBridgeKind::ClaudeToChat),
                upstream_path: "/chat/completions".to_string(),
                upstream_query: None,
                body: claude_chat::anthropic_request_to_chat(body)?,
                streaming,
                tool_namespaces: BTreeMap::new(),
            }),
            ApiDialect::OpenAiResponses => Ok(PreparedBridgeRequest {
                kind: Some(ProtocolBridgeKind::ClaudeToResponses),
                upstream_path: "/responses".to_string(),
                upstream_query: None,
                body: claude_responses::anthropic_request_to_responses(body)?,
                streaming,
                tool_namespaces: BTreeMap::new(),
            }),
            ApiDialect::Anthropic => Ok(passthrough_request("/v1/messages", body, streaming)),
            ApiDialect::Gemini => {
                let model = common::gemini_model_from_body(body)?;
                let (upstream_path, upstream_query) = common::gemini_endpoint(&model, streaming);
                Ok(PreparedBridgeRequest {
                    kind: Some(ProtocolBridgeKind::ClaudeToGemini),
                    upstream_path,
                    upstream_query,
                    body: claude_gemini::anthropic_request_to_gemini(body)?,
                    streaming,
                    tool_namespaces: BTreeMap::new(),
                })
            }
        };
    }

    Ok(passthrough_request(&normalized_path, body, streaming))
}

pub fn transform_response(
    kind: ProtocolBridgeKind,
    status: u16,
    content_type: Option<&str>,
    body: &[u8],
) -> Result<TransformedBridgeResponse, String> {
    transform_response_with_tool_namespaces(kind, status, content_type, body, &BTreeMap::new())
}

pub fn transform_response_with_tool_namespaces(
    kind: ProtocolBridgeKind,
    status: u16,
    content_type: Option<&str>,
    body: &[u8],
    tool_namespaces: &BTreeMap<String, String>,
) -> Result<TransformedBridgeResponse, String> {
    match kind {
        ProtocolBridgeKind::ResponsesToChat => {
            responses_chat::chat_response_to_responses(status, content_type, body, tool_namespaces)
        }
        ProtocolBridgeKind::ResponsesToResponses => {
            responses_responses::responses_response_to_responses(
                status,
                content_type,
                body,
                tool_namespaces,
            )
        }
        ProtocolBridgeKind::ResponsesToAnthropic => {
            responses_claude::anthropic_response_to_responses(
                status,
                content_type,
                body,
                tool_namespaces,
            )
        }
        ProtocolBridgeKind::ResponsesToGemini => responses_gemini::gemini_response_to_responses(
            status,
            content_type,
            body,
            tool_namespaces,
        ),
        ProtocolBridgeKind::ClaudeToChat => {
            claude_chat::chat_response_to_anthropic(status, content_type, body)
        }
        ProtocolBridgeKind::ClaudeToResponses => {
            claude_responses::responses_response_to_anthropic(status, content_type, body)
        }
        ProtocolBridgeKind::ClaudeToGemini => {
            claude_gemini::gemini_response_to_anthropic(status, content_type, body)
        }
    }
}
fn passthrough_request(path: &str, body: &[u8], streaming: bool) -> PreparedBridgeRequest {
    PreparedBridgeRequest {
        kind: None,
        upstream_path: common::normalize_path(path),
        upstream_query: None,
        body: body.to_vec(),
        streaming,
        tool_namespaces: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    /// The streaming flag must be read from the *client* body. The Gemini
    /// converter moves the flag into the `?alt=sse` query and emits no `stream`
    /// key, so a caller that recomputes it from the converted body always gets
    /// false — and a stream cut mid-generation is then reported as complete.
    #[test]
    fn gemini_bridge_reports_streaming_though_converted_body_has_no_stream_key() {
        let body = serde_json::to_vec(&json!({
            "model": "gemini-2.5-flash",
            "max_tokens": 64,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap();

        let prepared = prepare_request(
            PlatformId::Claude,
            ApiDialect::Gemini,
            "/v1/messages",
            &body,
        )
        .expect("converted request");

        assert!(
            prepared.streaming,
            "the bridge must report the client's streaming intent"
        );
        assert!(
            prepared.upstream_query.as_deref() == Some("alt=sse"),
            "streaming Gemini requests go to ?alt=sse: {:?}",
            prepared.upstream_query
        );

        // The converted body carries no `stream` key, which is exactly why the
        // flag cannot be recovered from it downstream.
        let converted: Value = serde_json::from_slice(&prepared.body).unwrap();
        assert!(
            converted.get("stream").is_none(),
            "precondition: Gemini body has no stream key: {converted}"
        );
    }

    use super::{
        prepare_request, transform_response, transform_response_with_tool_namespaces,
        ProtocolBridgeKind,
    };
    use crate::models::platform::{ApiDialect, PlatformId};
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    #[test]
    fn selects_responses_to_chat_only_for_codex_chat_upstreams() {
        for path in ["/responses", "/v1/responses", "/v1/v1/responses"] {
            let prepared = prepare_request(
                PlatformId::Codex,
                ApiDialect::OpenAi,
                path,
                br#"{"model":"gpt-5","input":"hello"}"#,
            )
            .expect("bridge request");

            assert_eq!(prepared.kind, Some(ProtocolBridgeKind::ResponsesToChat));
            assert_eq!(prepared.upstream_path, "/chat/completions");
            assert_eq!(prepared.upstream_query, None);
        }
    }

    #[test]
    fn leaves_other_protocol_pairs_unbridged() {
        let cases = [
            (
                PlatformId::Codex,
                ApiDialect::OpenAi,
                "/v1/chat/completions",
            ),
            (
                PlatformId::Codex,
                ApiDialect::OpenAiResponses,
                "/v1/responses",
            ),
            (
                PlatformId::Codex,
                ApiDialect::OpenAi,
                "/v1/responses/compact",
            ),
        ];

        for (platform, dialect, path) in cases {
            let body = br#"{"model":"gpt-5","input":"hello"}"#;
            let prepared = prepare_request(platform, dialect, path, body).expect("request");

            if platform == PlatformId::Codex && dialect == ApiDialect::OpenAiResponses {
                assert_eq!(
                    prepared.kind,
                    Some(ProtocolBridgeKind::ResponsesToResponses),
                    "expected native Responses sanitation for {path}"
                );
            } else {
                assert_eq!(prepared.kind, None, "unexpected bridge for {path}");
            }
            if platform == PlatformId::Codex && dialect == ApiDialect::OpenAiResponses {
                assert_eq!(
                    serde_json::from_slice::<Value>(&prepared.body).unwrap(),
                    serde_json::from_slice::<Value>(body).unwrap()
                );
            } else {
                assert_eq!(prepared.body, body);
            }
            assert_eq!(prepared.upstream_query, None);
        }
    }

    #[test]
    fn selects_codex_responses_bridge_matrix() {
        let body = br#"{"model":"gpt-5","input":"hello"}"#;
        let cases = [
            (
                ApiDialect::OpenAi,
                Some(ProtocolBridgeKind::ResponsesToChat),
                "/chat/completions",
                None,
            ),
            (
                ApiDialect::OpenAiResponses,
                Some(ProtocolBridgeKind::ResponsesToResponses),
                "/responses",
                None,
            ),
            (
                ApiDialect::Anthropic,
                Some(ProtocolBridgeKind::ResponsesToAnthropic),
                "/v1/messages",
                None,
            ),
            (
                ApiDialect::Gemini,
                Some(ProtocolBridgeKind::ResponsesToGemini),
                "/v1beta/models/gpt-5:generateContent",
                None,
            ),
        ];

        for (dialect, expected_kind, expected_path, expected_query) in cases {
            let prepared =
                prepare_request(PlatformId::Codex, dialect, "/v1/responses", body).unwrap();
            assert_eq!(prepared.kind, expected_kind);
            assert_eq!(prepared.upstream_path, expected_path);
            assert_eq!(prepared.upstream_query.as_deref(), expected_query);
        }
    }

    #[test]
    fn selects_claude_messages_bridge_matrix() {
        let body = br#"{"model":"claude-sonnet-4-20250514","messages":[{"role":"user","content":"hello"}],"max_tokens":16}"#;
        let cases = [
            (
                ApiDialect::OpenAi,
                Some(ProtocolBridgeKind::ClaudeToChat),
                "/chat/completions",
            ),
            (
                ApiDialect::OpenAiResponses,
                Some(ProtocolBridgeKind::ClaudeToResponses),
                "/responses",
            ),
            (ApiDialect::Anthropic, None, "/v1/messages"),
            (
                ApiDialect::Gemini,
                Some(ProtocolBridgeKind::ClaudeToGemini),
                "/v1beta/models/claude-sonnet-4-20250514:generateContent",
            ),
        ];

        for (dialect, expected_kind, expected_path) in cases {
            let prepared =
                prepare_request(PlatformId::Claude, dialect, "/v1/messages", body).unwrap();
            assert_eq!(prepared.kind, expected_kind);
            assert_eq!(prepared.upstream_path, expected_path);
        }
    }

    #[test]
    fn gemini_bridge_streaming_uses_alt_sse_query() {
        let prepared = prepare_request(
            PlatformId::Claude,
            ApiDialect::Gemini,
            "/v1/messages",
            br#"{"model":"gemini-2.5-flash","stream":true,"messages":[{"role":"user","content":"hello"}],"max_tokens":16}"#,
        )
        .unwrap();

        assert_eq!(
            prepared.upstream_path,
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent"
        );
        assert_eq!(prepared.upstream_query.as_deref(), Some("alt=sse"));
    }

    #[test]
    fn converts_responses_string_input_instructions_tools_and_controls() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "gpt-5",
                "instructions": "Be concise",
                "input": "Hello",
                "max_output_tokens": 128,
                "temperature": 0.2,
                "top_p": 0.9,
                "parallel_tool_calls": true,
                "stream": true,
                "tools": [{
                    "type": "function",
                    "name": "lookup",
                    "description": "Lookup a value",
                    "parameters": {
                        "type": "object",
                        "properties": {"key": {"type": "string"}},
                        "required": ["key"]
                    },
                    "strict": true
                }]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let body: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert_eq!(body["model"], "gpt-5");
        assert_eq!(body["messages"][0]["role"], "system");
        assert!(body["messages"][0]["content"]
            .as_str()
            .is_some_and(|content| content.starts_with("Be concise\n\n")));
        assert_eq!(
            body["messages"][1],
            json!({"role":"user","content":"Hello"})
        );
        assert_eq!(body["max_tokens"], 128);
        assert!(body.get("max_completion_tokens").is_none());
        assert_eq!(body["temperature"], 0.2);
        assert_eq!(body["top_p"], 0.9);
        assert_eq!(body["parallel_tool_calls"], true);
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"], json!({"include_usage": true}));
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "lookup");
        assert_eq!(body["tools"][0]["function"]["strict"], true);
        assert!(body["messages"][0]["content"]
            .as_str()
            .is_some_and(|content| content
                .to_ascii_lowercase()
                .contains("do not end the response with only a progress update")));
    }

    #[test]
    fn adds_chat_agent_continuation_instruction_only_when_tools_are_available() {
        let with_tools = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "v2ex-deepseek",
                "instructions": "Use tools to finish the task.",
                "input": "continue",
                "tools": [{
                    "type": "function",
                    "name": "exec_command",
                    "parameters": {"type": "object", "properties": {}}
                }]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request with tools");
        let with_tools: Value = serde_json::from_slice(&with_tools.body).expect("chat json");
        let system = with_tools["messages"][0]["content"]
            .as_str()
            .expect("system content");
        assert!(system.contains("include the tool call in the same assistant response"));
        assert!(system
            .to_ascii_lowercase()
            .contains("do not end the response with only a progress update"));

        let without_tools = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            br#"{"model":"v2ex-deepseek","instructions":"Answer normally.","input":"hello"}"#,
        )
        .expect("converted request without tools");
        let without_tools: Value = serde_json::from_slice(&without_tools.body).expect("chat json");
        assert_eq!(without_tools["messages"][0]["content"], "Answer normally.");
    }

    #[test]
    fn ignores_responses_reasoning_items_when_bridging_to_chat() {
        let body = serde_json::to_vec(&json!({
            "model": "deepseek-chat",
            "input": [
                {"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"opaque"},
                {"type":"message","role":"user","content":[{"type":"input_text","text":"继续"}]}
            ]
        }))
        .expect("responses json");
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            &body,
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert_eq!(converted["messages"].as_array().map(Vec::len), Some(1));
        assert_eq!(converted["messages"][0]["content"], "继续");
    }

    #[test]
    fn preserves_reasoning_and_tool_turns_when_bridging_to_chat() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "deepseek-reasoner",
                "input": [
                    {"type":"reasoning","summary":[{"type":"summary_text","text":"先查询。"}]},
                    {"type":"function_call","call_id":"call_1","name":"lookup","arguments":{"q":"rust"}},
                    {"type":"function_call_output","call_id":"call_1","output":{"ok":true}},
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"继续"}]}
                ],
                "tools": [{
                    "type":"function",
                    "name":"lookup",
                    "parameters":{"type":"object","properties":{"q":{"type":"string"}}}
                }]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        let messages = converted["messages"].as_array().expect("messages");
        let assistant = messages
            .iter()
            .find(|message| message["role"] == "assistant")
            .expect("assistant message");
        assert_eq!(assistant["reasoning_content"], "先查询。");
        assert_eq!(assistant["tool_calls"][0]["id"], "call_1");
        assert_eq!(assistant["tool_calls"][0]["function"]["name"], "lookup");
        let tool = messages
            .iter()
            .find(|message| message["role"] == "tool")
            .expect("tool message");
        assert_eq!(
            tool,
            &json!({
                "role":"tool",
                "tool_call_id":"call_1",
                "content":"{\"ok\":true}"
            })
        );
        let user = messages
            .iter()
            .find(|message| message["role"] == "user")
            .expect("user message");
        assert_eq!(user["content"], "继续");
    }

    #[test]
    fn carries_reasoning_content_from_function_call_item_into_chat_assistant() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "mimo-v2.5-pro",
                "input": [
                    {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{}","reasoning_content":"Restored plan."},
                    {"type":"function_call_output","call_id":"call_1","output":"ok"},
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]}
                ],
                "tools": [{"type":"function","name":"lookup","parameters":{"type":"object","properties":{}}}]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        let assistant = converted["messages"]
            .as_array()
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message["role"] == "assistant")
            })
            .expect("assistant message");
        assert_eq!(assistant["reasoning_content"], "Restored plan.");
    }

    #[test]
    fn groups_parallel_function_calls_without_duplicating_reasoning() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "mimo-v2.5-pro",
                "input": [
                    {"type":"function_call","call_id":"call_1","name":"first","arguments":"{}","reasoning_content":"Need both tools."},
                    {"type":"function_call","call_id":"call_2","name":"second","arguments":"{}","reasoning_content":"Need both tools."},
                    {"type":"function_call_output","call_id":"call_1","output":"one"},
                    {"type":"function_call_output","call_id":"call_2","output":"two"}
                ],
                "tools": [
                    {"type":"function","name":"first","parameters":{"type":"object","properties":{}}},
                    {"type":"function","name":"second","parameters":{"type":"object","properties":{}}}
                ]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");
        let messages = converted["messages"].as_array().expect("messages");
        let assistant_messages = messages
            .iter()
            .filter(|message| message["role"] == "assistant")
            .collect::<Vec<_>>();

        assert_eq!(assistant_messages.len(), 1);
        assert_eq!(
            assistant_messages[0]["tool_calls"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(
            assistant_messages[0]["reasoning_content"],
            "Need both tools."
        );
    }

    #[test]
    fn injects_placeholder_reasoning_for_tool_call_without_reasoning_item() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "mimo-v2.5-pro",
                "input": [
                    {"type":"function_call","call_id":"call_1","name":"lookup","arguments":{"q":"x"}},
                    {"type":"function_call_output","call_id":"call_1","output":{"ok":true}},
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"继续"}]}
                ],
                "tools": [{
                    "type":"function",
                    "name":"lookup",
                    "parameters":{"type":"object","properties":{"q":{"type":"string"}}}
                }]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        let assistant = converted["messages"]
            .as_array()
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message["role"] == "assistant")
            })
            .expect("assistant message");
        assert!(assistant["tool_calls"][0]["id"] == "call_1");
        let reasoning = assistant["reasoning_content"].as_str();
        assert!(
            reasoning.is_some_and(|text| !text.trim().is_empty()),
            "tool-call assistant message must carry non-empty reasoning_content for MiMo/DeepSeek"
        );
    }

    #[test]
    fn collapses_system_and_developer_messages_to_a_single_head() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "deepseek-chat",
                "instructions": "BASE",
                "input": [
                    {"type":"message","role":"developer","content":[{"type":"input_text","text":"DEV"}]},
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}
                ]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");
        let messages = converted["messages"].as_array().expect("messages");

        let system_count = messages.iter().filter(|m| m["role"] == "system").count();
        assert_eq!(system_count, 1, "system messages should collapse to one");
        assert_eq!(messages[0]["role"], "system");
        let head = messages[0]["content"].as_str().unwrap_or_default();
        assert!(
            head.contains("BASE") && head.contains("DEV"),
            "head: {head}"
        );
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "hi");
    }

    #[test]
    fn drops_empty_messages_and_pads_empty_tool_output_when_bridging_to_chat() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "v2ex-deepseek",
                "input": [
                    {"type":"message","role":"user","content":[{"type":"input_text","text":""}]},
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]},
                    {"type":"function_call","call_id":"c1","name":"exec","arguments":"{}"},
                    {"type":"function_call_output","call_id":"c1","output":""}
                ]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");
        let messages = converted["messages"].as_array().expect("messages");

        // No message may have empty/blank content unless it carries tool_calls.
        for message in messages {
            let has_tool_calls = message
                .get("tool_calls")
                .and_then(Value::as_array)
                .is_some_and(|calls| !calls.is_empty());
            if has_tool_calls {
                continue;
            }
            let content = message.get("content");
            let blank = match content {
                None | Some(Value::Null) => true,
                Some(Value::String(text)) => text.is_empty(),
                Some(Value::Array(parts)) => parts.is_empty(),
                _ => false,
            };
            assert!(!blank, "empty non-tool message survived: {message}");
        }
        // The empty user message was dropped; the "go" user message remains.
        assert!(messages
            .iter()
            .any(|m| m.get("role").and_then(Value::as_str) == Some("user")
                && m.get("content").and_then(Value::as_str) == Some("go")));
    }

    #[test]
    fn converts_custom_tools_to_chat_functions() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model":"gpt-5.6-sol",
                "input":"hello",
                "tools":[{"type":"custom","name":"apply_patch","description":"patch files"}],
                "tool_choice":{"type":"custom","name":"apply_patch"}
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert_eq!(converted["tools"][0]["function"]["name"], "apply_patch");
        assert_eq!(
            converted["tools"][0]["function"]["parameters"]["required"],
            json!(["input"])
        );
        assert_eq!(
            converted["tool_choice"],
            json!({
                "type":"function",
                "function":{"name":"apply_patch"}
            })
        );
    }

    #[test]
    fn skips_builtin_output_history_items_when_bridging_to_chat() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            br#"{
                "model":"deepseek-chat",
                "input":[
                    {"type":"web_search_call","id":"ws_1","status":"completed"},
                    {"type":"web_search_call_output","call_id":"ws_1","output":"result"},
                    {"type":"message","role":"user","content":"continue"}
                ]
            }"#,
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert_eq!(converted["messages"].as_array().map(Vec::len), Some(1));
        assert_eq!(converted["messages"][0]["content"], "continue");
    }

    #[test]
    fn ignores_responses_builtin_tools_when_bridging_to_chat() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "deepseek-chat",
                "input": "search this",
                "tools": [
                    {"type": "web_search"},
                    {
                        "type": "function",
                        "name": "lookup",
                        "parameters": {"type": "object", "properties": {}}
                    }
                ]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert_eq!(converted["tools"].as_array().map(Vec::len), Some(1));
        assert_eq!(converted["tools"][0]["function"]["name"], "lookup");
    }

    #[test]
    fn tolerates_empty_text_parts_when_bridging_to_chat() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "deepseek-chat",
                "input": [
                    {"type":"message","role":"assistant","content":[{"type":"output_text","text":""}]},
                    {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{}"},
                    {"type":"function_call_output","call_id":"call_1","output":"ok"},
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"继续"}]}
                ],
                "tools": [{
                    "type":"function",
                    "name":"lookup",
                    "parameters": {"type":"object","properties":{}}
                }]
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        let messages = converted["messages"].as_array().expect("messages");
        // The empty-text assistant message is dropped (empty content + no
        // tool_calls would trip a `content must not be empty` gateway 400); the
        // surviving assistant is the tool-call turn with null content.
        let assistant = messages
            .iter()
            .find(|message| message["role"] == "assistant")
            .expect("assistant message");
        assert!(assistant
            .get("tool_calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| !calls.is_empty()));
        let user = messages
            .iter()
            .find(|message| message["role"] == "user")
            .expect("user message");
        assert_eq!(user["content"], "继续");
    }

    #[test]
    fn omits_builtin_tools_and_downgrades_required_choice_when_no_chat_tools_remain() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "deepseek-chat",
                "input": "search this",
                "tools": [{"type": "web_search"}],
                "tool_choice": "required"
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert!(converted.get("tools").is_none());
        assert_eq!(converted["tool_choice"], "auto");
    }

    #[test]
    fn downgrades_builtin_tool_choice_when_bridging_to_chat() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "deepseek-chat",
                "input": "search this",
                "tools": [{"type": "web_search"}],
                "tool_choice": {"type": "web_search"}
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert!(converted.get("tool_choice").is_none());
    }
    #[test]
    fn converts_responses_reasoning_effort_to_chat_reasoning_effort() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            br#"{
                "model":"gpt-5.6-sol",
                "input":"hello",
                "reasoning":{"effort":"xhigh"}
            }"#,
        )
        .expect("converted request");
        let body: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert_eq!(body["reasoning_effort"], "high");
    }

    #[test]
    fn converts_responses_reasoning_effort_to_anthropic_thinking() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::Anthropic,
            "/v1/responses",
            br#"{
                "model":"claude-sonnet-4-20250514",
                "input":"hello",
                "max_output_tokens":65536,
                "temperature":0.2,
                "reasoning":{"effort":"high"}
            }"#,
        )
        .expect("converted request");
        let body: Value = serde_json::from_slice(&prepared.body).expect("anthropic json");

        assert_eq!(
            body["thinking"],
            json!({"type":"enabled","budget_tokens":16384})
        );
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn converts_responses_reasoning_effort_to_gemini_thinking_config() {
        let cases = [
            ("gemini-2.5-flash", json!({"thinkingBudget":4096})),
            ("gemini-3-pro", json!({"thinkingLevel":"high"})),
        ];

        for (model, expected) in cases {
            let body = serde_json::to_vec(&json!({
                "model": model,
                "input": "hello",
                "reasoning": {"effort": "medium"}
            }))
            .expect("responses json");
            let prepared = prepare_request(
                PlatformId::Codex,
                ApiDialect::Gemini,
                "/v1/responses",
                &body,
            )
            .expect("converted request");
            let converted: Value = serde_json::from_slice(&prepared.body).expect("gemini json");

            assert_eq!(converted["generationConfig"]["thinkingConfig"], expected);
        }
    }

    #[test]
    fn uses_max_completion_tokens_for_o_series_models() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/responses",
            br#"{"model":"o3-mini","input":"hello","max_output_tokens":64}"#,
        )
        .expect("converted request");
        let body: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert_eq!(body["max_completion_tokens"], 64);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn converts_responses_messages_function_calls_and_outputs() {
        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            serde_json::to_vec(&json!({
                "model": "gpt-5",
                "input": [
                    {
                        "role": "user",
                        "content": [{"type":"input_text","text":"Find x"}]
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
            }))
            .unwrap()
            .as_slice(),
        )
        .expect("converted request");
        let body: Value = serde_json::from_slice(&prepared.body).expect("chat json");

        assert_eq!(
            body["messages"][0],
            json!({"role":"user","content":"Find x"})
        );
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][1]["tool_calls"][0]["id"], "call_1");
        assert_eq!(
            body["messages"][1]["tool_calls"][0]["function"]["name"],
            "lookup"
        );
        assert_eq!(
            body["messages"][2],
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "42"
            })
        );
    }

    #[test]
    fn converts_chat_json_text_tool_calls_and_usage_to_responses() {
        let input = json!({
            "id": "chatcmpl-1",
            "model": "deepseek-chat",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "hello",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "lookup",
                            "arguments": "{\"key\":\"x\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 3,
                "completion_tokens": 5,
                "total_tokens": 8,
                "prompt_tokens_details": {"cached_tokens": 1},
                "completion_tokens_details": {"reasoning_tokens": 2}
            }
        });
        let converted = transform_response(
            ProtocolBridgeKind::ResponsesToChat,
            200,
            Some("application/json"),
            serde_json::to_vec(&input).unwrap().as_slice(),
        )
        .expect("converted response");
        let output: Value = serde_json::from_slice(&converted.body).expect("responses json");

        assert_eq!(output["object"], "response");
        assert_eq!(output["id"], "chatcmpl-1");
        assert_eq!(output["model"], "deepseek-chat");
        assert_eq!(output["status"], "completed");
        assert_eq!(output["output_text"], "hello");
        assert_eq!(output["output"][0]["type"], "message");
        assert_eq!(output["output"][0]["content"][0]["type"], "output_text");
        assert_eq!(output["output"][1]["type"], "function_call");
        assert_eq!(output["output"][1]["call_id"], "call_1");
        assert_eq!(output["output"][1]["arguments"], "{\"key\":\"x\"}");
        assert_eq!(output["usage"]["input_tokens"], 3);
        assert_eq!(output["usage"]["output_tokens"], 5);
        assert_eq!(output["usage"]["total_tokens"], 8);
        assert_eq!(output["usage"]["input_tokens_details"]["cached_tokens"], 1);
        assert_eq!(
            output["usage"]["output_tokens_details"]["reasoning_tokens"],
            2
        );
        assert_eq!(converted.content_type.as_deref(), Some("application/json"));
    }

    #[test]
    fn restores_namespace_for_chat_tool_calls() {
        let input = json!({
            "id": "chatcmpl-namespace",
            "model": "deepseek-chat",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "lookup",
                            "arguments": "{}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        let mut namespaces = BTreeMap::new();
        namespaces.insert("lookup".to_string(), "database".to_string());
        let converted = transform_response_with_tool_namespaces(
            ProtocolBridgeKind::ResponsesToChat,
            200,
            Some("application/json"),
            serde_json::to_vec(&input).unwrap().as_slice(),
            &namespaces,
        )
        .expect("converted response");
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(output["output"][0]["namespace"], "database");
    }

    #[test]
    fn restores_namespace_for_anthropic_tool_calls() {
        let input = json!({
            "id": "msg-namespace",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet",
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "lookup",
                "input": {"key": "x"}
            }],
            "stop_reason": "tool_use"
        });
        let mut namespaces = BTreeMap::new();
        namespaces.insert("lookup".to_string(), "database".to_string());
        let converted = transform_response_with_tool_namespaces(
            ProtocolBridgeKind::ResponsesToAnthropic,
            200,
            Some("application/json"),
            serde_json::to_vec(&input).unwrap().as_slice(),
            &namespaces,
        )
        .expect("converted response");
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(output["output"][0]["namespace"], "database");
    }

    #[test]
    fn attaches_reasoning_content_to_function_call_item_in_responses_output() {
        let namespaces = BTreeMap::new();
        let upstream = json!({
            "id": "chatcmpl-1",
            "model": "deepseek-chat",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "Plan: list then edit.",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "exec_command", "arguments": "{\"cmd\":\"ls\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        let converted = transform_response_with_tool_namespaces(
            ProtocolBridgeKind::ResponsesToChat,
            200,
            Some("application/json"),
            serde_json::to_vec(&upstream).unwrap().as_slice(),
            &namespaces,
        )
        .expect("converted response");
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        // The function_call item must carry reasoning_content so Codex echoes it
        // back on the next request (DeepSeek/MiMo tool-call reasoning round trip).
        let function_call = output["output"]
            .as_array()
            .expect("output")
            .iter()
            .find(|item| item["type"] == "function_call")
            .expect("function_call item");
        assert_eq!(function_call["reasoning_content"], "Plan: list then edit.");
    }

    #[test]
    fn restores_namespace_for_gemini_tool_calls() {
        let input = json!({
            "responseId": "resp-namespace",
            "modelVersion": "gemini-2.5-flash",
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "lookup",
                            "args": {"key": "x"}
                        }
                    }]
                },
                "finishReason": "STOP"
            }]
        });
        let mut namespaces = BTreeMap::new();
        namespaces.insert("lookup".to_string(), "database".to_string());
        let converted = transform_response_with_tool_namespaces(
            ProtocolBridgeKind::ResponsesToGemini,
            200,
            Some("application/json"),
            serde_json::to_vec(&input).unwrap().as_slice(),
            &namespaces,
        )
        .expect("converted response");
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(output["output"][0]["namespace"], "database");
    }

    #[test]
    fn flattens_codex_namespace_tools_for_all_response_bridges() {
        let body = serde_json::to_vec(&json!({
            "model": "gpt-5",
            "input": "hello",
            "tools": [{
                "type": "namespace",
                "name": "database",
                "description": "Database tools",
                "tools": [{
                    "type": "function",
                    "name": "lookup",
                    "description": "Lookup",
                    "parameters": {"type": "object", "properties": {}}
                }]
            }]
        }))
        .unwrap();

        for dialect in [
            ApiDialect::OpenAi,
            ApiDialect::Anthropic,
            ApiDialect::Gemini,
        ] {
            let prepared = prepare_request(PlatformId::Codex, dialect, "/v1/responses", &body)
                .expect("converted request");
            let converted: Value = serde_json::from_slice(&prepared.body).unwrap();
            assert_eq!(
                prepared.tool_namespaces.get("lookup"),
                Some(&"database".to_string())
            );
            match dialect {
                ApiDialect::OpenAi => {
                    assert_eq!(
                        converted["tools"][0]["function"]["name"],
                        "database__lookup"
                    );
                    assert_eq!(
                        converted["tools"][0]["function"]["parameters"]["type"],
                        "object"
                    );
                }
                ApiDialect::Anthropic => {
                    assert_eq!(converted["tools"][0]["name"], "database__lookup");
                    assert_eq!(converted["tools"][0]["input_schema"]["type"], "object");
                }
                ApiDialect::Gemini => assert_eq!(
                    converted["tools"][0]["functionDeclarations"][0]["name"],
                    "database__lookup"
                ),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn flattens_codex_namespace_tools_with_input_schema_and_mcp_name() {
        let body = serde_json::to_vec(&json!({
            "model": "gpt-5.6-sol",
            "input": "hello",
            "tools": [{
                "type": "namespace",
                "name": "mcp__playwright__",
                "tools": [{
                    "type": "function",
                    "name": "browser_navigate",
                    "description": "Navigate",
                    "inputSchema": {
                        "type": "object",
                        "properties": {"url": {"type": "string"}}
                    }
                }]
            }]
        }))
        .unwrap();

        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAi,
            "/v1/responses",
            &body,
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).unwrap();

        assert_eq!(
            converted["tools"][0]["function"]["name"],
            "mcp__playwright__browser_navigate"
        );
        assert_eq!(
            converted["tools"][0]["function"]["parameters"]["properties"]["url"]["type"],
            "string"
        );
        assert_eq!(
            prepared
                .tool_namespaces
                .get("mcp__playwright__browser_navigate"),
            Some(&"mcp__playwright__".to_string())
        );
    }

    #[test]
    fn flattens_namespace_tools_for_native_responses_upstreams() {
        let body = serde_json::to_vec(&json!({
            "model": "deepseek-reasoner",
            "input": "hello",
            "tools": [{
                "type": "namespace",
                "name": "mcp__browser__",
                "tools": [{
                    "type": "function",
                    "name": "open",
                    "inputSchema": {"type": "object", "properties": {}}
                }]
            }]
        }))
        .unwrap();

        let prepared = prepare_request(
            PlatformId::Codex,
            ApiDialect::OpenAiResponses,
            "/v1/responses",
            &body,
        )
        .expect("converted request");
        let converted: Value = serde_json::from_slice(&prepared.body).unwrap();

        assert_eq!(converted["tools"][0]["name"], "mcp__browser__open");
        assert_eq!(converted["tools"][0]["type"], "function");
        assert!(converted["tools"][0].get("inputSchema").is_none());
        assert_eq!(converted["tools"][0]["parameters"]["type"], "object");
        assert_eq!(
            prepared.tool_namespaces.get("mcp__browser__open"),
            Some(&"mcp__browser__".to_string())
        );
    }

    #[test]
    fn restores_namespace_for_native_responses_tool_calls() {
        let input = json!({
            "id": "resp-namespace",
            "object": "response",
            "output": [{
                "type": "function_call",
                "name": "mcp__browser__open",
                "call_id": "call_1",
                "arguments": "{}"
            }]
        });
        let mut namespaces = BTreeMap::new();
        namespaces.insert(
            "mcp__browser__open".to_string(),
            "mcp__browser__".to_string(),
        );

        let converted = transform_response_with_tool_namespaces(
            ProtocolBridgeKind::ResponsesToResponses,
            200,
            Some("application/json"),
            serde_json::to_vec(&input).unwrap().as_slice(),
            &namespaces,
        )
        .expect("converted response");
        let output: Value = serde_json::from_slice(&converted.body).unwrap();

        assert_eq!(output["output"][0]["name"], "open");
        assert_eq!(output["output"][0]["namespace"], "mcp__browser__");
    }

    #[test]
    fn passes_through_non_success_responses() {
        let body = br#"{"error":{"message":"bad request"}}"#;
        let converted = transform_response(
            ProtocolBridgeKind::ResponsesToChat,
            400,
            Some("application/json"),
            body,
        )
        .expect("error response");

        assert_eq!(converted.body, body);
        assert_eq!(converted.content_type.as_deref(), Some("application/json"));
    }

    #[test]
    fn converts_chat_sse_text_usage_and_done_to_responses_events() {
        let body = concat!(
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"deepseek-chat\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"deepseek-chat\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"deepseek-chat\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
            "data: [DONE]\n\n"
        );
        let converted = transform_response(
            ProtocolBridgeKind::ResponsesToChat,
            200,
            Some("text/event-stream"),
            body.as_bytes(),
        )
        .expect("converted stream");
        let output = String::from_utf8(converted.body).expect("utf8 stream");

        assert!(output.contains("event: response.created"));
        assert!(output.contains("event: response.in_progress"));
        assert!(output.contains("event: response.output_text.delta"));
        assert!(output.contains("\"delta\":\"hello\""));
        assert!(output.contains("event: response.output_text.done"));
        assert!(output.contains("event: response.completed"));
        assert!(output.contains("\"input_tokens\":3"));
        assert!(!output.contains("[DONE]"));
        assert_eq!(converted.content_type.as_deref(), Some("text/event-stream"));
    }

    #[test]
    fn converts_chat_sse_tool_call_deltas_to_responses_events() {
        let first = json!({
            "id": "chatcmpl-2",
            "model": "deepseek-chat",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{\"k\":\""}
                    }]
                },
                "finish_reason": Value::Null
            }]
        });
        let second = json!({
            "id": "chatcmpl-2",
            "model": "deepseek-chat",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": {"arguments": "x\"}"}
                    }]
                },
                "finish_reason": Value::Null
            }]
        });
        let final_chunk = json!({
            "id": "chatcmpl-2",
            "model": "deepseek-chat",
            "choices": [{"index":0,"delta":{},"finish_reason":"tool_calls"}]
        });
        let body =
            format!("data: {first}\n\ndata: {second}\n\ndata: {final_chunk}\n\ndata: [DONE]\n\n");
        let converted = transform_response(
            ProtocolBridgeKind::ResponsesToChat,
            200,
            Some("text/event-stream"),
            body.as_bytes(),
        )
        .expect("converted stream");
        let output = String::from_utf8(converted.body).expect("utf8 stream");

        assert!(output.contains("event: response.output_item.added"));
        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("event: response.function_call_arguments.delta"));
        assert!(output.contains("event: response.function_call_arguments.done"));
        assert!(output.contains("event: response.completed"));
    }

    #[test]
    fn maps_reasoning_content_deltas_to_responses_reasoning_events() {
        let body = concat!(
            "data: {\"id\":\"cc-1\",\"model\":\"mimo\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"reasoning_content\":null},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"cc-1\",\"model\":\"mimo\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"Let me think.\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"cc-1\",\"model\":\"mimo\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Answer.\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"cc-1\",\"model\":\"mimo\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
            "data: [DONE]\n\n"
        );
        let converted = transform_response(
            ProtocolBridgeKind::ResponsesToChat,
            200,
            Some("text/event-stream"),
            body.as_bytes(),
        )
        .expect("converted stream");
        let output = String::from_utf8(converted.body).expect("utf8 stream");

        assert!(output.contains("event: response.reasoning_summary_part.added"));
        assert!(output.contains("event: response.reasoning_summary_text.delta"));
        assert!(output.contains("\"delta\":\"Let me think.\""));
        assert!(output.contains("event: response.reasoning_summary_text.done"));
        assert!(output.contains("event: response.output_text.delta"));

        let completed = completed_response(&output);
        let items = completed["output"].as_array().expect("output array");
        assert_eq!(items[0]["type"], "reasoning");
        assert_eq!(items[0]["summary"][0]["text"], "Let me think.");
        assert_eq!(items[1]["type"], "message");
        assert_eq!(items[1]["content"][0]["text"], "Answer.");
    }

    #[test]
    fn emits_reasoning_only_stream_without_empty_output() {
        let body = concat!(
            "data: {\"id\":\"cc-2\",\"model\":\"mimo\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"Thinking only.\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"cc-2\",\"model\":\"mimo\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let converted = transform_response(
            ProtocolBridgeKind::ResponsesToChat,
            200,
            Some("text/event-stream"),
            body.as_bytes(),
        )
        .expect("converted stream");
        let output = String::from_utf8(converted.body).expect("utf8 stream");

        let completed = completed_response(&output);
        let items = completed["output"].as_array().expect("output array");
        assert!(
            !items.is_empty(),
            "reasoning-only stream must not yield empty output"
        );
        assert_eq!(items[0]["type"], "reasoning");
        assert_eq!(items[0]["summary"][0]["text"], "Thinking only.");
    }

    #[test]
    fn maps_reasoning_content_in_non_stream_response() {
        let body = json!({
            "id": "cc-3",
            "model": "mimo",
            "choices": [{
                "message": {"role": "assistant", "reasoning_content": "Because.", "content": "Done."},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 2, "completion_tokens": 2}
        });
        let converted = transform_response(
            ProtocolBridgeKind::ResponsesToChat,
            200,
            Some("application/json"),
            serde_json::to_vec(&body).unwrap().as_slice(),
        )
        .expect("converted json");
        let response: Value = serde_json::from_slice(&converted.body).expect("responses json");

        let items = response["output"].as_array().expect("output array");
        assert_eq!(items[0]["type"], "reasoning");
        assert_eq!(items[0]["summary"][0]["text"], "Because.");
        assert_eq!(items[1]["type"], "message");
        assert_eq!(items[1]["content"][0]["text"], "Done.");
    }

    fn completed_response(stream: &str) -> Value {
        for block in stream.split("\n\n") {
            let is_completed = block
                .lines()
                .any(|line| line.trim() == "event: response.completed");
            if !is_completed {
                continue;
            }
            let data = block
                .lines()
                .find_map(|line| line.trim().strip_prefix("data:"))
                .map(str::trim)
                .expect("completed event data");
            let value: Value = serde_json::from_str(data).expect("completed json");
            return value["response"].clone();
        }
        panic!("no response.completed event found in stream");
    }
}
