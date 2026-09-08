use serde_json::Value;

pub(crate) fn strip_replayed_encrypted_content_from_bytes(body: &[u8]) -> Option<Vec<u8>> {
    let mut value = serde_json::from_slice::<Value>(body).ok()?;
    let input = value.get_mut("input")?.as_array_mut()?;
    let mut changed = false;
    input.retain_mut(|item| {
        let item_type = item.get("type").and_then(Value::as_str);
        let has_ciphertext = item
            .get("encrypted_content")
            .and_then(Value::as_str)
            .is_some_and(|content| !content.is_empty());
        if matches!(item_type, Some("compaction" | "compaction_summary"))
            || (matches!(item_type, Some("reasoning" | "context_compaction")) && has_ciphertext)
        {
            changed = true;
            return false;
        }
        if !matches!(item_type, Some("message" | "agent_message") | None) {
            return true;
        }
        let Some(content) = item.get_mut("content").and_then(Value::as_array_mut) else {
            return true;
        };
        let previous_len = content.len();
        content
            .retain(|part| part.get("type").and_then(Value::as_str) != Some("encrypted_content"));
        let removed = content.len() != previous_len;
        changed |= removed;
        !removed || !content.is_empty()
    });
    if changed {
        serde_json::to_vec(&value).ok()
    } else {
        None
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

    #[test]
    fn leaves_tool_data_and_non_input_fields_unchanged() {
        let business_data = json!({"type": "compaction", "encrypted_content": "business-data"});
        let request = json!({
            "input": [
                {"type": "reasoning", "summary": [], "encrypted_content": "foreign-state"},
                {"type": "function_call_output", "call_id": "call_1", "output": business_data},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "continue"}]}
            ],
            "tools": [{"type": "function", "name": "lookup", "parameters": {
                "type": "object", "properties": {"payload": {"type": "object", "default": business_data}}
            }}],
            "metadata": {"payload": business_data}
        });
        let mut expected = request.clone();
        expected["input"].as_array_mut().unwrap().remove(0);
        let rewritten =
            strip_replayed_encrypted_content_from_bytes(&serde_json::to_vec(&request).unwrap())
                .expect("replayed reasoning was removed");
        assert_eq!(
            serde_json::from_slice::<Value>(&rewritten).unwrap(),
            expected
        );
        assert!(strip_replayed_encrypted_content_from_bytes(
            &serde_json::to_vec(&expected).unwrap()
        )
        .is_none());
    }

    #[test]
    fn preserves_plaintext_reasoning_and_removes_only_newly_empty_messages() {
        let expected = json!([
            {"type": "reasoning", "summary": [{"type": "summary_text", "text": "public summary"}], "encrypted_content": null},
            {"type": "context_compaction"},
            {"type": "reasoning", "summary": [], "encrypted_content": ""},
            {"type": "message", "role": "assistant", "content": []},
            {"type": "compaction_trigger"}
        ]);
        let mut request = json!({"input": expected});
        request["input"].as_array_mut().unwrap().push(json!({
            "type": "agent_message", "author": "planner", "recipient": "coder",
            "content": [{"type": "encrypted_content", "encrypted_content": "foreign-state"}]
        }));
        let rewritten =
            strip_replayed_encrypted_content_from_bytes(&serde_json::to_vec(&request).unwrap())
                .expect("opaque message was removed");
        assert_eq!(
            serde_json::from_slice::<Value>(&rewritten).unwrap()["input"],
            expected
        );
    }
}
