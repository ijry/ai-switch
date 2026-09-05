//! Replayed-reasoning removal for Anthropic request bodies.
//!
//! A `thinking` block's `signature` only verifies against the upstream that
//! minted it, and Claude Code replays assistant thinking blocks in every later
//! turn. A pooled conversation therefore breaks the first time a turn lands on an
//! account other than the one that produced the reasoning: a strict upstream — a
//! relay re-signing into Bedrock, say — answers 400, and every other account in
//! the pool rejects the same history identically, so switching accounts cannot
//! recover it. Only rewriting the body can.
//!
//! Dropping the replayed blocks is what the inbound bridges already do with this
//! same history (`route_protocol_bridge::claude_responses`), and it leaves the
//! turn servable by any account. Anthropic ignores reasoning from earlier turns;
//! the one place it does not is a final assistant turn carrying `tool_use`, which
//! has to open with a thinking block while thinking is enabled — so that case
//! also relaxes `thinking`, rather than sending a body the upstream is bound to
//! reject for a second reason.

use serde_json::{json, Value};

/// Removes replayed `thinking` / `redacted_thinking` blocks from the assistant
/// history of an Anthropic request body, and relaxes `thinking` when the stripped
/// history would otherwise be invalid. Returns whether the body changed.
///
/// `false` means there was nothing to remove: the caller must not spend a retry
/// on a byte-identical body.
pub(crate) fn strip_replayed_thinking(body: &mut Value) -> bool {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };

    let mut stripped = false;
    for message in messages.iter_mut() {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
            // String content carries no blocks, so there is nothing to remove.
            continue;
        };
        let before = content.len();
        content.retain(|block| !is_reasoning_block(block));
        stripped |= content.len() != before;
    }

    if !stripped {
        return false;
    }

    // An empty `content` is not a valid message. Consecutive same-role messages
    // are, so dropping the husk of a turn that held nothing but reasoning is
    // safe.
    messages.retain(|message| {
        message
            .get("content")
            .and_then(Value::as_array)
            .is_none_or(|content| !content.is_empty())
    });

    if final_assistant_turn_calls_a_tool(body) {
        relax_thinking(body);
    }
    true
}

/// Byte-level [`strip_replayed_thinking`], for callers that hold a raw request
/// body rather than a parsed one.
///
/// `None` means the body is unchanged: either it is not JSON this understands, or
/// it carries no replayed reasoning. Callers use that to avoid spending a retry
/// on a body the upstream has already refused.
pub(crate) fn strip_replayed_thinking_from_bytes(body: &[u8]) -> Option<Vec<u8>> {
    let mut value = serde_json::from_slice::<Value>(body).ok()?;
    if !strip_replayed_thinking(&mut value) {
        return None;
    }
    serde_json::to_vec(&value).ok()
}

fn is_reasoning_block(block: &Value) -> bool {
    matches!(
        block.get("type").and_then(Value::as_str),
        Some("thinking" | "redacted_thinking")
    )
}

/// Whether the newest assistant turn asks for a tool. Anthropic requires such a
/// turn to open with a thinking block while thinking is enabled, which the strip
/// above has just made impossible.
fn final_assistant_turn_calls_a_tool(body: &Value) -> bool {
    body.get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|message| message.get("role").and_then(Value::as_str) == Some("assistant"))
        })
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .is_some_and(|content| {
            content
                .iter()
                .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        })
}

/// Turns thinking off for this one attempt. Left alone when the caller never
/// asked for thinking, or already disabled it, so the body stays as close to the
/// client's as the upstream allows.
fn relax_thinking(body: &mut Value) {
    let asked_for_thinking = body
        .get("thinking")
        .and_then(|thinking| thinking.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|kind| kind != "disabled");
    if asked_for_thinking {
        body["thinking"] = json!({"type": "disabled"});
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_replayed_reasoning_and_keeps_the_rest() {
        let mut body = json!({
            "model": "claude-sonnet-4",
            "thinking": {"type": "enabled", "budget_tokens": 1024},
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Find x"}]},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "Let me look.", "signature": "foreign"},
                    {"type": "redacted_thinking", "data": "opaque"},
                    {"type": "text", "text": "Found it."}
                ]},
                {"role": "user", "content": [{"type": "text", "text": "thanks"}]}
            ]
        });

        assert!(strip_replayed_thinking(&mut body));

        assert_eq!(body["messages"][1]["content"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][1]["content"][0]["type"], "text");
        let rendered = serde_json::to_string(&body).unwrap();
        assert!(!rendered.contains("foreign"), "signature must be gone");
        assert!(!rendered.contains("opaque"), "redacted data must be gone");
        // The user turns are the client's own words; they must survive untouched.
        assert_eq!(body["messages"][0]["content"][0]["text"], "Find x");
        assert_eq!(body["messages"][2]["content"][0]["text"], "thanks");
        // Nothing forced thinking off: the newest assistant turn calls no tool.
        assert_eq!(body["thinking"]["type"], "enabled");
    }

    /// A retry only helps if the body actually differs, so a history with no
    /// replayed reasoning has to report that plainly.
    #[test]
    fn reports_no_change_when_there_is_nothing_to_strip() {
        let mut body = json!({
            "thinking": {"type": "enabled", "budget_tokens": 1024},
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "hello"}]}
            ]
        });
        let before = body.clone();

        assert!(!strip_replayed_thinking(&mut body));
        assert_eq!(body, before);
    }

    /// Anthropic rejects a tool-calling final assistant turn that does not open
    /// with a thinking block, so stripping one there has to turn thinking off or
    /// the retry earns a second 400 for a different reason.
    #[test]
    fn relaxes_thinking_when_the_final_assistant_turn_calls_a_tool() {
        let mut body = json!({
            "thinking": {"type": "enabled", "budget_tokens": 1024},
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "read a.txt"}]},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "Read it.", "signature": "foreign"},
                    {"type": "tool_use", "id": "toolu_1", "name": "read", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_1", "content": "hi"}
                ]}
            ]
        });

        assert!(strip_replayed_thinking(&mut body));

        assert_eq!(body["thinking"], json!({"type": "disabled"}));
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
    }

    /// A client that never asked for thinking must not have the field invented for
    /// it — the body should differ from the client's by as little as possible.
    #[test]
    fn leaves_thinking_absent_when_the_client_never_asked_for_it() {
        let mut body = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "hm", "signature": "foreign"},
                    {"type": "tool_use", "id": "toolu_1", "name": "read", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_1", "content": "hi"}
                ]}
            ]
        });

        assert!(strip_replayed_thinking(&mut body));
        assert!(body.get("thinking").is_none());
    }

    /// An interrupted turn can hold reasoning and nothing else. An empty `content`
    /// is not a valid message, so the husk has to go; two consecutive user turns
    /// are fine.
    #[test]
    fn drops_a_turn_that_held_nothing_but_reasoning() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "one"}]},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "…", "signature": "foreign"}
                ]},
                {"role": "user", "content": [{"type": "text", "text": "two"}]}
            ]
        });

        assert!(strip_replayed_thinking(&mut body));

        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert!(messages.iter().all(|message| message["role"] == "user"));
    }

    #[test]
    fn tolerates_string_content_and_missing_fields() {
        // String content, a bodiless request, and a roleless message must all be
        // reported as unchanged rather than panicking.
        let mut string_content = json!({
            "messages": [{"role": "assistant", "content": "plain string"}]
        });
        assert!(!strip_replayed_thinking(&mut string_content));
        assert_eq!(string_content["messages"][0]["content"], "plain string");

        let mut empty = json!({});
        assert!(!strip_replayed_thinking(&mut empty));
        assert_eq!(empty, json!({}));

        let mut roleless = json!({"messages": [{"content": []}]});
        assert!(!strip_replayed_thinking(&mut roleless));
    }

    #[test]
    fn byte_level_wrapper_only_answers_when_the_body_changed() {
        let with_reasoning = serde_json::to_vec(&json!({
            "messages": [{"role": "assistant", "content": [
                {"type": "thinking", "thinking": "…", "signature": "foreign"},
                {"type": "text", "text": "done"}
            ]}]
        }))
        .unwrap();
        let rewritten = strip_replayed_thinking_from_bytes(&with_reasoning)
            .expect("a body with replayed reasoning must be rewritten");
        let parsed: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(
            parsed["messages"][0]["content"].as_array().unwrap().len(),
            1
        );

        let without_reasoning =
            serde_json::to_vec(&json!({"messages": [{"role": "user", "content": "hi"}]})).unwrap();
        assert!(strip_replayed_thinking_from_bytes(&without_reasoning).is_none());
        // A body this cannot parse is not one it can safely rewrite either.
        assert!(strip_replayed_thinking_from_bytes(b"not json").is_none());
    }
}
