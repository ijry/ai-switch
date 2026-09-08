use serde_json::Value;

const OPAQUE_ITEM_TYPES: &[&str] = &[
    "compaction",
    "compaction_summary",
    "context_compaction",
    "reasoning",
];

/// Removes replayed Responses state that is only verifiable by the upstream
/// which created it. Returns `None` when the request did not contain such
/// state, or when it was not valid JSON.
pub(crate) fn strip_replayed_encrypted_content_from_bytes(body: &[u8]) -> Option<Vec<u8>> {
    let mut value = serde_json::from_slice::<Value>(body).ok()?;
    let mut changed = false;
    strip_value(&mut value, &mut changed);
    if changed {
        serde_json::to_vec(&value).ok()
    } else {
        None
    }
}

fn strip_value(value: &mut Value, changed: &mut bool) -> bool {
    match value {
        Value::Array(items) => {
            let mut kept = Vec::with_capacity(items.len());
            for mut item in std::mem::take(items) {
                if strip_value(&mut item, changed) {
                    *changed = true;
                } else {
                    kept.push(item);
                }
            }
            *items = kept;
            false
        }
        Value::Object(object) => {
            let item_type = object.get("type").and_then(Value::as_str);
            if item_type == Some("encrypted_content") {
                return true;
            }
            if item_type.is_some_and(|value| {
                OPAQUE_ITEM_TYPES.contains(&value)
                    && (value == "compaction"
                        || value == "compaction_summary"
                        || object.contains_key("encrypted_content"))
            }) {
                return true;
            }

            let mut empty_content_after_removal = false;
            if let Some(content) = object.get_mut("content") {
                if let Value::Array(items) = content {
                    let original_len = items.len();
                    let mut kept = Vec::with_capacity(original_len);
                    for mut item in std::mem::take(items) {
                        if strip_value(&mut item, changed) {
                            *changed = true;
                        } else {
                            kept.push(item);
                        }
                    }
                    empty_content_after_removal = original_len > kept.len() && kept.is_empty();
                    *items = kept;
                }
            }
            if empty_content_after_removal {
                return true;
            }

            let keys = object.keys().cloned().collect::<Vec<_>>();
            let mut remove_keys = Vec::new();
            for key in keys {
                if key == "content" {
                    continue;
                }
                if let Some(child) = object.get_mut(&key) {
                    if matches!(child, Value::Array(_) | Value::Object(_))
                        && strip_value(child, changed)
                    {
                        remove_keys.push(key);
                        *changed = true;
                    }
                }
            }
            for key in remove_keys {
                object.remove(&key);
            }
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::strip_replayed_encrypted_content_from_bytes;
    use serde_json::{json, Value};

    #[test]
    fn removes_replayed_items_but_preserves_messages_tools_include_and_trigger() {
        let body = serde_json::to_vec(&json!({
            "model": "gpt-6-astra",
            "include": ["reasoning.encrypted_content"],
            "tools": [{"type": "function", "name": "lookup"}],
            "input": [
                {"type": "reasoning", "encrypted_content": "old-reasoning"},
                {"type": "compaction", "encrypted_content": "old-compaction"},
                {"type": "context_compaction", "encrypted_content": "old-context"},
                {"type": "compaction_trigger"},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "keep me"}]},
                {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "done"},
                {"type": "agent_message", "content": [
                    {"type": "input_text", "text": "keep this prose"},
                    {"type": "encrypted_content", "encrypted_content": "old-agent-state"}
                ]}
            ]
        }))
        .unwrap();

        let rewritten = strip_replayed_encrypted_content_from_bytes(&body).expect("rewritten");
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        let input = value["input"].as_array().unwrap();

        assert_eq!(input.len(), 5);
        assert_eq!(input[0]["type"], "compaction_trigger");
        assert_eq!(input[1]["content"][0]["text"], "keep me");
        assert_eq!(input[2]["type"], "function_call");
        assert_eq!(input[3]["type"], "function_call_output");
        assert_eq!(input[4]["content"][0]["text"], "keep this prose");
        assert_eq!(value["include"][0], "reasoning.encrypted_content");
        assert_eq!(value["tools"][0]["name"], "lookup");
        assert!(!rewritten
            .windows("old-reasoning".len())
            .any(|window| window == b"old-reasoning"));
    }

    #[test]
    fn returns_none_when_no_replayed_state_exists() {
        let body = br#"{"model":"gpt-6-astra","input":[{"type":"message","content":[{"type":"input_text","text":"hello"}]}]}"#;
        assert!(strip_replayed_encrypted_content_from_bytes(body).is_none());
        assert!(strip_replayed_encrypted_content_from_bytes(b"not json").is_none());
    }
}
