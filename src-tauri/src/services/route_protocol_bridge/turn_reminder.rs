//! Per-turn reminder injection.
//!
//! A compact model drifts over a long session — it starts answering in English
//! when the user wants Chinese, or slides off whatever constraint the system
//! prompt set. A system-prompt instruction is read once at the start of the
//! prefix and grows weaker the further the conversation runs; a line appended to
//! the newest turn sits immediately before generation, where it still carries
//! weight.
//!
//! Appending at the tail also keeps Anthropic's prompt cache intact. Caching
//! matches on a prefix keyed at each `cache_control` marker, so rewriting
//! `system` every turn would break the prefix and re-bill the whole system +
//! tools block on every request. Everything this module writes lands after the
//! markers [`super::anthropic_cache`] places, which is exactly right: the
//! reminder moves every turn and was never a cache candidate.
//!
//! # Why this runs after protocol bridging
//!
//! The four writers below are keyed on the *upstream* dialect, not the client's
//! platform. A Codex client speaks Responses, but against an Anthropic upstream
//! [`super::prepare_request`] converts that body into `messages` — so the shape
//! to append to is only known once bridging is done. Writing an Anthropic
//! `messages` entry into a still-Responses body would reach
//! `responses_claude::convert_input_item` and fail the whole request.

use crate::models::platform::ApiDialect;
use serde_json::{json, Value};

/// Sent when an account enables the reminder without supplying its own text.
pub(crate) const DEFAULT_TURN_REMINDER: &str = "请用简体中文回复。";

/// Appends `reminder` to the newest user turn, in `dialect`'s own schema.
///
/// Returns the body unchanged when there is nothing safe to do: a body that is
/// not JSON, a conversation whose newest turn is not the user's, or one that
/// already carries this exact reminder. A reminder is a nicety; it must never be
/// the reason an otherwise valid request fails.
pub(crate) fn append_turn_reminder(dialect: ApiDialect, body: &[u8], reminder: &str) -> Vec<u8> {
    let reminder = reminder.trim();
    if reminder.is_empty() {
        return body.to_vec();
    }
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    if body_already_mentions(&value, reminder) {
        return body.to_vec();
    }

    let appended = match dialect {
        ApiDialect::Anthropic => append_anthropic(&mut value, reminder),
        ApiDialect::OpenAi => append_chat(&mut value, reminder),
        ApiDialect::OpenAiResponses => append_responses(&mut value, reminder),
        ApiDialect::Gemini => append_gemini(&mut value, reminder),
    };
    if !appended {
        return body.to_vec();
    }

    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

/// Cheap idempotence guard, mirroring `responses_chat`'s continuation
/// instruction. The retry loop rebuilds from the original body so this should
/// never trigger, but a double reminder is silly enough to be worth one scan.
fn body_already_mentions(value: &Value, reminder: &str) -> bool {
    serde_json::to_string(value).is_ok_and(|text| text.contains(reminder))
}

/// The newest turn must be the user's.
///
/// Anthropic requires alternating roles, so a fresh user message after an
/// assistant one would be rejected. And when the newest turn *is* the
/// assistant's, that is a prefill the caller wants continued — appending after it
/// would break the continuation. Skipping is the only correct move.
fn last_role_is_user(messages: &[Value], role_key: &str, user_role: &str) -> bool {
    messages
        .last()
        .and_then(|message| message.get(role_key))
        .and_then(Value::as_str)
        .is_some_and(|role| role == user_role)
}

/// Anthropic: a text block on the last `messages` entry.
///
/// A tool loop ends on a user turn whose content is `tool_result` blocks; a text
/// block may follow those (only the leading position is constrained), so the
/// reminder lands after them.
fn append_anthropic(value: &mut Value, reminder: &str) -> bool {
    let Some(messages) = value.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };
    if !last_role_is_user(messages, "role", "user") {
        return false;
    }
    let Some(last) = messages.last_mut() else {
        return false;
    };
    // `content` is a bare string on hand-written requests; promote it so the
    // reminder is a sibling block rather than concatenated prose.
    if let Some(text) = last.get("content").and_then(Value::as_str) {
        last["content"] = json!([{"type": "text", "text": text}]);
    }
    match last.get_mut("content").and_then(Value::as_array_mut) {
        Some(content) => {
            content.push(json!({"type": "text", "text": reminder}));
            true
        }
        None => false,
    }
}

/// OpenAI Chat Completions: `messages` with string-or-parts content.
fn append_chat(value: &mut Value, reminder: &str) -> bool {
    let Some(messages) = value.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };
    if !last_role_is_user(messages, "role", "user") {
        return false;
    }
    let Some(last) = messages.last_mut() else {
        return false;
    };
    match last.get_mut("content") {
        Some(Value::String(text)) => {
            text.push_str("\n\n");
            text.push_str(reminder);
            true
        }
        Some(Value::Array(parts)) => {
            parts.push(json!({"type": "text", "text": reminder}));
            true
        }
        _ => false,
    }
}

/// OpenAI Responses: an `input_text` part on the last `input` message.
///
/// The part type is `input_text`, not `text` — Responses names request-side text
/// differently from response-side, and `text` would be rejected.
fn append_responses(value: &mut Value, reminder: &str) -> bool {
    // `input` may be a bare string for a single-shot request.
    if let Some(text) = value.get("input").and_then(Value::as_str) {
        value["input"] = Value::String(format!("{text}\n\n{reminder}"));
        return true;
    }
    let Some(input) = value.get_mut("input").and_then(Value::as_array_mut) else {
        return false;
    };
    if !last_role_is_user(input, "role", "user") {
        return false;
    }
    let Some(last) = input.last_mut() else {
        return false;
    };
    if let Some(text) = last.get("content").and_then(Value::as_str) {
        last["content"] = json!([{"type": "input_text", "text": text}]);
    }
    match last.get_mut("content").and_then(Value::as_array_mut) {
        Some(content) => {
            content.push(json!({"type": "input_text", "text": reminder}));
            true
        }
        None => false,
    }
}

/// Gemini: a `parts` entry on the last `contents` element.
///
/// Gemini calls the user role `user` and the assistant role `model`.
fn append_gemini(value: &mut Value, reminder: &str) -> bool {
    let Some(contents) = value.get_mut("contents").and_then(Value::as_array_mut) else {
        return false;
    };
    if !last_role_is_user(contents, "role", "user") {
        return false;
    }
    let Some(last) = contents.last_mut() else {
        return false;
    };
    match last.get_mut("parts").and_then(Value::as_array_mut) {
        Some(parts) => {
            parts.push(json!({"text": reminder}));
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REMINDER: &str = "请用简体中文回复。";

    fn append(dialect: ApiDialect, body: &str) -> Value {
        let out = append_turn_reminder(dialect, body.as_bytes(), REMINDER);
        serde_json::from_slice(&out).expect("valid json")
    }

    #[test]
    fn anthropic_gets_a_text_block_on_the_last_user_message() {
        let value = append(
            ApiDialect::Anthropic,
            r#"{"model":"x","messages":[
                {"role":"user","content":[{"type":"text","text":"hi"}]},
                {"role":"assistant","content":[{"type":"text","text":"hello"}]},
                {"role":"user","content":[{"type":"text","text":"again"}]}
            ]}"#,
        );

        assert_eq!(
            value.pointer("/messages/2/content/1/type"),
            Some(&json!("text"))
        );
        assert_eq!(
            value.pointer("/messages/2/content/1/text"),
            Some(&json!(REMINDER))
        );
        // Earlier turns are untouched, so the cached prefix still matches.
        assert_eq!(
            value
                .pointer("/messages/0/content")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn anthropic_promotes_a_string_content_before_appending() {
        let value = append(
            ApiDialect::Anthropic,
            r#"{"messages":[{"role":"user","content":"hi"}]}"#,
        );

        assert_eq!(
            value.pointer("/messages/0/content/0/text"),
            Some(&json!("hi"))
        );
        assert_eq!(
            value.pointer("/messages/0/content/1/text"),
            Some(&json!(REMINDER))
        );
    }

    #[test]
    fn anthropic_appends_after_tool_result_blocks() {
        // A tool loop ends on a user turn carrying tool_result. Anthropic only
        // constrains tool_result to come first, so a trailing text block is legal
        // — this is the shape an agent session actually hits most often.
        let value = append(
            ApiDialect::Anthropic,
            r#"{"messages":[
                {"role":"user","content":[{"type":"text","text":"go"}]},
                {"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"ls","input":{}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}
            ]}"#,
        );

        let last = value
            .pointer("/messages/2/content")
            .and_then(Value::as_array)
            .expect("content array");
        assert_eq!(last.len(), 2);
        assert_eq!(last[0].get("type"), Some(&json!("tool_result")));
        assert_eq!(last[1].get("text"), Some(&json!(REMINDER)));
    }

    #[test]
    fn chat_appends_to_a_string_content() {
        let value = append(
            ApiDialect::OpenAi,
            r#"{"messages":[{"role":"system","content":"sys"},{"role":"user","content":"hi"}]}"#,
        );

        assert_eq!(
            value.pointer("/messages/1/content"),
            Some(&json!(format!("hi\n\n{REMINDER}")))
        );
        assert_eq!(value.pointer("/messages/0/content"), Some(&json!("sys")));
    }

    #[test]
    fn chat_appends_a_part_to_an_array_content() {
        let value = append(
            ApiDialect::OpenAi,
            r#"{"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#,
        );

        assert_eq!(
            value.pointer("/messages/0/content/1/text"),
            Some(&json!(REMINDER))
        );
    }

    #[test]
    fn responses_uses_input_text_not_text() {
        let value = append(
            ApiDialect::OpenAiResponses,
            r#"{"input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}]}"#,
        );

        // `text` would be rejected on the request side.
        assert_eq!(
            value.pointer("/input/0/content/1/type"),
            Some(&json!("input_text"))
        );
        assert_eq!(
            value.pointer("/input/0/content/1/text"),
            Some(&json!(REMINDER))
        );
    }

    #[test]
    fn responses_handles_a_bare_string_input() {
        let value = append(ApiDialect::OpenAiResponses, r#"{"input":"hi"}"#);

        assert_eq!(
            value.get("input"),
            Some(&json!(format!("hi\n\n{REMINDER}")))
        );
    }

    #[test]
    fn gemini_appends_a_part_to_the_last_user_content() {
        let value = append(
            ApiDialect::Gemini,
            r#"{"contents":[
                {"role":"user","parts":[{"text":"hi"}]},
                {"role":"model","parts":[{"text":"hello"}]},
                {"role":"user","parts":[{"text":"again"}]}
            ]}"#,
        );

        assert_eq!(
            value.pointer("/contents/2/parts/1/text"),
            Some(&json!(REMINDER))
        );
        assert_eq!(
            value
                .pointer("/contents/0/parts")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn an_assistant_final_turn_is_left_alone() {
        // Prefill: the caller wants this continued, and Anthropic rejects two
        // user messages in a row anyway.
        for (dialect, body) in [
            (
                ApiDialect::Anthropic,
                r#"{"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"par"}]}"#,
            ),
            (
                ApiDialect::OpenAi,
                r#"{"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"par"}]}"#,
            ),
            (
                ApiDialect::Gemini,
                r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]},{"role":"model","parts":[{"text":"par"}]}]}"#,
            ),
        ] {
            let out = append_turn_reminder(dialect, body.as_bytes(), REMINDER);
            assert_eq!(
                String::from_utf8_lossy(&out),
                body,
                "dialect={dialect:?} must be byte-identical"
            );
        }
    }

    #[test]
    fn unusable_bodies_pass_through_byte_for_byte() {
        for body in [
            "not json at all",
            // No conversation field to append to.
            r#"{"model":"x"}"#,
            // Empty conversation: nothing is the newest user turn.
            r#"{"messages":[]}"#,
        ] {
            let out = append_turn_reminder(ApiDialect::Anthropic, body.as_bytes(), REMINDER);
            assert_eq!(String::from_utf8_lossy(&out), body, "body={body}");
        }
    }

    #[test]
    fn an_empty_reminder_is_a_no_op() {
        let body = r#"{"messages":[{"role":"user","content":"hi"}]}"#;
        for reminder in ["", "   "] {
            let out = append_turn_reminder(ApiDialect::Anthropic, body.as_bytes(), reminder);
            assert_eq!(String::from_utf8_lossy(&out), body);
        }
    }

    #[test]
    fn a_reminder_is_never_added_twice() {
        let once = append_turn_reminder(
            ApiDialect::Anthropic,
            br#"{"messages":[{"role":"user","content":"hi"}]}"#,
            REMINDER,
        );
        let twice = append_turn_reminder(ApiDialect::Anthropic, &once, REMINDER);

        assert_eq!(once, twice);
        let value: Value = serde_json::from_slice(&twice).expect("json");
        assert_eq!(
            value
                .pointer("/messages/0/content")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
    }
}
