mod claude_chat;
mod claude_gemini;
mod claude_responses;
mod common;
mod responses_chat;
mod responses_claude;
mod responses_gemini;
mod sse;

use crate::models::platform::{ApiDialect, PlatformId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolBridgeKind {
    ResponsesToChat,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformedBridgeResponse {
    pub body: Vec<u8>,
    pub content_type: Option<String>,
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
            }),
            ApiDialect::OpenAiResponses => Ok(passthrough_request("/responses", body, streaming)),
            ApiDialect::Anthropic => Ok(PreparedBridgeRequest {
                kind: Some(ProtocolBridgeKind::ResponsesToAnthropic),
                upstream_path: "/v1/messages".to_string(),
                upstream_query: None,
                body: responses_claude::responses_request_to_anthropic(body)?,
                streaming,
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
            }),
            ApiDialect::OpenAiResponses => Ok(PreparedBridgeRequest {
                kind: Some(ProtocolBridgeKind::ClaudeToResponses),
                upstream_path: "/responses".to_string(),
                upstream_query: None,
                body: claude_responses::anthropic_request_to_responses(body)?,
                streaming,
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
    match kind {
        ProtocolBridgeKind::ResponsesToChat => {
            responses_chat::chat_response_to_responses(status, content_type, body)
        }
        ProtocolBridgeKind::ResponsesToAnthropic => {
            responses_claude::anthropic_response_to_responses(status, content_type, body)
        }
        ProtocolBridgeKind::ResponsesToGemini => {
            responses_gemini::gemini_response_to_responses(status, content_type, body)
        }
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
    }
}

#[cfg(test)]
mod tests {
    use super::{prepare_request, transform_response, ProtocolBridgeKind};
    use crate::models::platform::{ApiDialect, PlatformId};
    use serde_json::{json, Value};

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

            assert_eq!(prepared.kind, None, "unexpected bridge for {path}");
            assert_eq!(prepared.body, body);
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
            (ApiDialect::OpenAiResponses, None, "/responses", None),
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
        assert_eq!(
            body["messages"][0],
            json!({"role":"system","content":"Be concise"})
        );
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
}
