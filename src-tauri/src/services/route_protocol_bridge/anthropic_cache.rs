//! Anthropic prompt-cache breakpoint injection.
//!
//! Anthropic caches on a prefix match, keyed at each `cache_control` marker.
//! Clients that speak Anthropic natively (Claude Code) place their own markers,
//! and those requests reach the upstream untouched. A Codex client has no
//! `cache_control` concept at all, so a converted Codex→Anthropic request
//! arrives with none — and every turn re-processes the whole system prompt and
//! tool array at full input price.
//!
//! This injects markers on the stable prefix so a multi-turn conversation reads
//! from cache instead. Adapted from cc-switch's `proxy/cache_injector.rs`.

use serde_json::{json, Value};

/// Anthropic accepts at most four `cache_control` breakpoints per request.
const MAX_BREAKPOINTS: usize = 4;

/// Adds `cache_control` markers to an Anthropic request body, in prefix order:
/// tools tail → system tail → newest cacheable message → one older user anchor.
///
/// Caller-supplied markers are counted and never moved or removed: they express
/// an intent this code cannot reconstruct, and rewriting them would change the
/// cache key the caller was aiming for.
pub(super) fn inject_cache_breakpoints(body: &mut Value) {
    let existing = count_existing_breakpoints(body);
    let mut budget = MAX_BREAKPOINTS.saturating_sub(existing);
    if budget == 0 {
        // Already at or over the cap — leave the caller's markers alone and let
        // the upstream validate the total.
        return;
    }

    // (a) Tools render first in the prefix, so a marker here covers the whole
    // tool array on every later turn.
    if mark_tools_tail(body) {
        budget -= 1;
    }

    // (b) System renders after tools; a marker on its tail covers tools+system.
    if budget > 0 && mark_system_tail(body) {
        budget -= 1;
    }

    // (c) The newest cacheable message extends the cached prefix through the
    // conversation so far. Tool loops usually end on a user/tool_result turn,
    // so scan backwards rather than assuming an assistant turn is last.
    if budget > 0 {
        if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
            let marked = messages.iter_mut().rev().any(mark_message_breakpoint);
            if marked {
                budget -= 1;
            }
        }
    }

    // (d) A second, older user anchor. Anthropic walks back at most 20 content
    // blocks from a breakpoint to find a prior cache entry, so a turn that adds
    // many tool_result blocks can push the stable prefix out of reach of the
    // newest marker alone.
    if budget > 0 {
        if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
            if messages.len() >= 4 {
                let mut user_turns = 0;
                for message in messages.iter_mut().rev() {
                    if message.get("role").and_then(Value::as_str) != Some("user") {
                        continue;
                    }
                    user_turns += 1;
                    if user_turns == 2 {
                        mark_message_breakpoint(message);
                        break;
                    }
                }
            }
        }
    }
}

fn mark_tools_tail(body: &mut Value) -> bool {
    body.get_mut("tools")
        .and_then(Value::as_array_mut)
        .and_then(|tools| tools.last_mut())
        .is_some_and(mark_value)
}

/// Marks the tail of `system`, promoting a plain string to block form first —
/// `cache_control` lives on a content block, not on a bare string.
fn mark_system_tail(body: &mut Value) -> bool {
    if let Some(text) = body.get("system").and_then(Value::as_str) {
        if text.is_empty() {
            return false;
        }
        body["system"] = json!([{"type": "text", "text": text}]);
    }
    body.get_mut("system")
        .and_then(Value::as_array_mut)
        .and_then(|system| system.last_mut())
        .is_some_and(mark_value)
}

/// Marks the last non-reasoning block of a message. A marker on a `thinking`
/// block is not a stable cache anchor: reasoning is regenerated per turn.
fn mark_message_breakpoint(message: &mut Value) -> bool {
    message
        .get_mut("content")
        .and_then(Value::as_array_mut)
        .and_then(|content| {
            content.iter_mut().rev().find(|block| {
                !matches!(
                    block.get("type").and_then(Value::as_str),
                    Some("thinking" | "redacted_thinking")
                )
            })
        })
        .is_some_and(mark_value)
}

/// Adds an ephemeral marker, leaving an already-marked value untouched.
fn mark_value(value: &mut Value) -> bool {
    if value.get("cache_control").is_some() {
        return false;
    }
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    object.insert("cache_control".to_string(), json!({"type": "ephemeral"}));
    true
}

fn count_existing_breakpoints(body: &Value) -> usize {
    fn marked(values: Option<&Vec<Value>>) -> usize {
        values.map_or(0, |values| {
            values
                .iter()
                .filter(|value| value.get("cache_control").is_some())
                .count()
        })
    }

    let tools = marked(body.get("tools").and_then(Value::as_array));
    let system = marked(body.get("system").and_then(Value::as_array));
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .map_or(0, |messages| {
            messages
                .iter()
                .map(|message| marked(message.get("content").and_then(Value::as_array)))
                .sum()
        });
    tools + system + messages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(body: &Value) -> usize {
        count_existing_breakpoints(body)
    }

    #[test]
    fn marks_tools_system_and_latest_message() {
        let mut body = json!({
            "model": "claude-sonnet-4",
            "system": "You are concise.",
            "tools": [{"name": "a", "input_schema": {}}, {"name": "b", "input_schema": {}}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]}
            ]
        });

        inject_cache_breakpoints(&mut body);

        // A string system is promoted to block form so it can carry a marker.
        assert!(body["system"].is_array(), "system={}", body["system"]);
        assert!(body["system"][0]["cache_control"].is_object());
        // Only the tail of the tool array is marked, not every tool.
        assert!(body["tools"][0].get("cache_control").is_none());
        assert!(body["tools"][1]["cache_control"].is_object());
        assert!(body["messages"][0]["content"][0]["cache_control"].is_object());
        assert_eq!(count(&body), 3);
    }

    #[test]
    fn never_exceeds_the_four_breakpoint_cap() {
        let mut body = json!({
            "system": [{"type": "text", "text": "s"}],
            "tools": [{"name": "a", "input_schema": {}}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "1"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "2"}]},
                {"role": "user", "content": [{"type": "text", "text": "3"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "4"}]},
                {"role": "user", "content": [{"type": "text", "text": "5"}]}
            ]
        });

        inject_cache_breakpoints(&mut body);
        assert!(count(&body) <= MAX_BREAKPOINTS, "count={}", count(&body));
    }

    /// Caller markers express an intent this code cannot reconstruct, so they
    /// are counted against the budget and never moved.
    #[test]
    fn preserves_caller_supplied_breakpoints() {
        let mut body = json!({
            "system": [{"type": "text", "text": "s", "cache_control": {"type": "ephemeral"}}],
            "tools": [
                {"name": "a", "input_schema": {}, "cache_control": {"type": "ephemeral"}}
            ],
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "hi", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        });
        let before = body.clone();

        inject_cache_breakpoints(&mut body);

        // Three caller markers leave budget for one more, but every original
        // marker stays exactly where it was.
        assert_eq!(body["system"][0], before["system"][0]);
        assert_eq!(body["tools"][0], before["tools"][0]);
        assert!(count(&body) <= MAX_BREAKPOINTS);
    }

    #[test]
    fn does_nothing_when_already_at_the_cap() {
        let marker = json!({"type": "ephemeral"});
        let mut body = json!({
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "1", "cache_control": marker},
                {"type": "text", "text": "2", "cache_control": marker},
                {"type": "text", "text": "3", "cache_control": marker},
                {"type": "text", "text": "4", "cache_control": marker}
            ]}],
            "system": [{"type": "text", "text": "s"}]
        });
        let before = body.clone();

        inject_cache_breakpoints(&mut body);

        assert_eq!(body, before, "a request at the cap must be left untouched");
    }

    /// Reasoning is regenerated each turn, so a marker there is not a stable
    /// anchor for the prefix.
    #[test]
    fn skips_thinking_blocks_when_anchoring_a_message() {
        let mut body = json!({
            "messages": [{"role": "assistant", "content": [
                {"type": "text", "text": "answer"},
                {"type": "thinking", "thinking": "reasoning", "signature": "sig"}
            ]}]
        });

        inject_cache_breakpoints(&mut body);

        assert!(body["messages"][0]["content"][0]["cache_control"].is_object());
        assert!(body["messages"][0]["content"][1]
            .get("cache_control")
            .is_none());
    }

    #[test]
    fn tolerates_missing_and_unmarkable_fields() {
        // No tools, no system, string message content (not a block array).
        let mut body = json!({
            "model": "claude-sonnet-4",
            "messages": [{"role": "user", "content": "plain string"}]
        });

        inject_cache_breakpoints(&mut body);

        // Nothing to anchor: the request must still be valid and unchanged.
        assert_eq!(count(&body), 0);
        assert_eq!(body["messages"][0]["content"], "plain string");

        // An entirely empty body must not panic either.
        let mut empty = json!({});
        inject_cache_breakpoints(&mut empty);
        assert_eq!(empty, json!({}));
    }
}
