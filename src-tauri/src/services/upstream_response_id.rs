//! Extract the upstream response id from a response body.
//!
//! This id is the join key between a proxied request and the CLI transcript
//! entry for the same request: Claude Code records `message.id` per assistant
//! message, and Codex embeds the Responses id in its `rs_` / `msg_` / `fc_`
//! item ids. On a real corpus the two sides matched 2905/2933 (99.0%).

use serde_json::Value;

pub fn extract_upstream_response_id(body: &[u8]) -> Option<String> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        return response_id_from_value(&value);
    }
    crate::services::route_protocol_bridge::sse::parse_sse_data_records_lossy(body)
        .iter()
        .find_map(response_id_from_value)
}

/// Nested paths win over the top-level `id`: an Anthropic `message_start` frame
/// and an OpenAI `response.created` frame both wrap the real id one level down,
/// and when a provider sends both, the response's own id is the authoritative
/// one rather than the envelope's.
fn response_id_from_value(value: &Value) -> Option<String> {
    [
        value.pointer("/message/id"),
        value.pointer("/response/id"),
        value.get("id"),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .map(str::trim)
    .find(|id| !id.is_empty())
    .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_anthropic_streaming_message_start() {
        let body = b"event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_abc123\",\"model\":\"claude-opus-5\",\"usage\":{\"input_tokens\":10}}}\n\n";
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("msg_abc123")
        );
    }

    #[test]
    fn reads_anthropic_non_streaming_top_level_id() {
        let body =
            br#"{"id":"msg_def456","type":"message","content":[],"usage":{"input_tokens":5}}"#;
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("msg_def456")
        );
    }

    #[test]
    fn reads_openai_responses_created_frame() {
        let body = b"event: response.created\n\
data: {\"response\":{\"id\":\"5d76e101-2615-4e87-8455-72061b36392c\",\"object\":\"response\",\"model\":\"deepseek-v4-flash\"}}\n\n";
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("5d76e101-2615-4e87-8455-72061b36392c")
        );
    }

    #[test]
    fn reads_chat_completions_top_level_id() {
        let body = br#"{"id":"chatcmpl-route","object":"chat.completion","choices":[]}"#;
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("chatcmpl-route")
        );
    }

    #[test]
    fn returns_none_when_no_id_is_present() {
        // A truncated preview can cut off before the id, and an error body has
        // none at all. Both must read as "unknown" rather than as a bogus key,
        // because a wrong key would merge two unrelated requests.
        assert_eq!(extract_upstream_response_id(b""), None);
        assert_eq!(extract_upstream_response_id(b"not json at all"), None);
        assert_eq!(
            extract_upstream_response_id(br#"{"error":{"message":"expired"}}"#),
            None
        );
    }

    #[test]
    fn ignores_blank_and_non_string_ids() {
        assert_eq!(extract_upstream_response_id(br#"{"id":"   "}"#), None);
        assert_eq!(extract_upstream_response_id(br#"{"id":123}"#), None);
    }

    #[test]
    fn prefers_the_first_frame_that_carries_an_id() {
        // `message_start` comes first in a real Anthropic stream; later frames
        // (content_block_delta) carry no id, so scanning must not stop at the
        // first frame unconditionally, nor overwrite with a later empty value.
        let body = b"event: ping\n\
data: {\"type\":\"ping\"}\n\n\
event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_second_frame\"}}\n\n";
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("msg_second_frame")
        );
    }
}
