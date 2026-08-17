//! Cross-request reasoning/tool-call cache for the Codex → Chat Completions
//! bridge.
//!
//! Chat-format reasoning providers (DeepSeek, Xiaomi MiMo, …) require the
//! assistant message that carries `tool_calls` to also carry the *original*
//! `reasoning_content` from that same turn on every subsequent request, or they
//! reject the follow-up with HTTP 400 and — more importantly — lose the model's
//! working plan, which makes the agent narrate one line and stop.
//!
//! Codex speaks the Responses API, where reasoning travels as opaque
//! `encrypted_content` we cannot read, so the plaintext reasoning is only ever
//! visible in the *upstream Chat response*. This store captures the complete
//! tool-call group under the response ID and restores it before the matching
//! outputs on the next request. A unique `call_id` is only used as a fallback
//! when the response ID cannot resolve the requested call.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

/// Upper bound on retained Responses turns; oldest turns are evicted first.
const MAX_CACHED_RESPONSES: usize = 512;

#[derive(Clone, Default)]
pub struct CodexReasoningCache {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    responses: HashMap<String, CachedResponse>,
    response_order: VecDeque<String>,
    call_index: HashMap<String, VecDeque<String>>,
    anonymous_response_counter: u64,
}

#[derive(Clone, Default)]
struct CachedResponse {
    calls_by_id: HashMap<String, Value>,
    call_order: Vec<String>,
}

#[derive(Clone, Default)]
struct CachedLookup {
    previous: Option<CachedResponse>,
    fallback: CachedResponse,
}

/// One captured assistant turn from an upstream Chat response.
#[derive(Debug, Default, PartialEq)]
struct CapturedTurn {
    response_id: Option<String>,
    reasoning: String,
    /// (call_id, name, arguments)
    calls: Vec<(String, String, String)>,
}

impl CodexReasoningCache {
    /// Capture reasoning + tool calls from a raw upstream Chat Completions
    /// response (streaming SSE or single JSON). No-op when the response has no
    /// tool calls (nothing to key reasoning against).
    pub fn record_from_chat_response(&self, body: &[u8]) {
        let Some(turn) = parse_chat_response(body) else {
            return;
        };
        if turn.calls.is_empty() {
            return;
        }
        let mut inner = self.inner.lock().expect("codex reasoning cache lock");
        let response_id = turn
            .response_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| inner.next_anonymous_response_id());
        let calls = turn
            .calls
            .into_iter()
            .map(|(call_id, name, arguments)| {
                let mut item = json!({
                    "type": "function_call",
                    "call_id": call_id.clone(),
                    "name": name,
                    "arguments": arguments,
                });
                if !turn.reasoning.trim().is_empty() {
                    item["reasoning_content"] = Value::String(turn.reasoning.clone());
                }
                (call_id, item)
            })
            .collect();
        inner.insert_calls(&response_id, calls);
    }

    /// Restore reasoning (and, when needed, the whole function_call item) onto a
    /// Codex Responses request body before it is converted to Chat. Returns the
    /// number of items changed. Operates in place on `input`.
    pub fn enrich_responses_request(&self, body: &mut Value) -> usize {
        let previous_response_id = body
            .get("previous_response_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string);
        let Some(input) = body.get_mut("input") else {
            return 0;
        };
        let original_input = std::mem::take(input);
        let original_was_object = matches!(&original_input, Value::Object(_));
        let items = match original_input {
            Value::Array(items) => items,
            Value::Object(object) => vec![Value::Object(object)],
            other => {
                *input = other;
                return 0;
            }
        };

        let output_call_ids = items
            .iter()
            .filter(|item| item_type(item).is_some_and(is_call_output_item_type))
            .filter_map(call_id_of)
            .collect::<HashSet<_>>();
        let existing_call_ids = items
            .iter()
            .filter(|item| item_type(item).is_some_and(is_call_item_type))
            .filter_map(call_id_of)
            .collect::<HashSet<_>>();
        let requested_call_ids = output_call_ids
            .union(&existing_call_ids)
            .cloned()
            .collect::<HashSet<_>>();
        let lookup = self.lookup(previous_response_id.as_deref(), &requested_call_ids);
        let restore_group = lookup.restore_group(&output_call_ids, &existing_call_ids);
        let restore_group_ids = restore_group
            .iter()
            .map(|(call_id, _)| call_id.clone())
            .collect::<HashSet<_>>();
        let mut restore_group = Some(restore_group);
        let mut seen_call_ids = HashSet::new();
        let mut restored = 0usize;
        let mut enriched = 0usize;
        let mut rebuilt = Vec::with_capacity(items.len() + restore_group_ids.len());

        for mut item in items {
            match item_type(&item) {
                Some(item_type) if is_call_item_type(item_type) => {
                    if let Some(call_id) = call_id_of(&item) {
                        if let Some(cached) = lookup.call(&call_id) {
                            if enrich_call_item_from_cache(&mut item, cached) {
                                enriched += 1;
                            }
                        }
                        seen_call_ids.insert(call_id);
                    }
                    rebuilt.push(item);
                }
                Some(item_type) if is_call_output_item_type(item_type) => {
                    if let Some(group) = restore_group.take().filter(|group| !group.is_empty()) {
                        for (call_id, cached_item) in group {
                            seen_call_ids.insert(call_id);
                            rebuilt.push(cached_item);
                            restored += 1;
                        }
                    }

                    if let Some(call_id) = call_id_of(&item) {
                        if !seen_call_ids.contains(&call_id)
                            && !restore_group_ids.contains(&call_id)
                        {
                            if let Some(cached) = lookup.call(&call_id).cloned() {
                                seen_call_ids.insert(call_id);
                                rebuilt.push(cached);
                                restored += 1;
                            }
                        }
                    }
                    rebuilt.push(item);
                }
                _ => rebuilt.push(item),
            }
        }

        let changed = restored + enriched;
        if changed == 0 && original_was_object && rebuilt.len() == 1 {
            *input = rebuilt.into_iter().next().unwrap_or(Value::Null);
        } else {
            *input = Value::Array(rebuilt);
        }
        changed
    }

    fn lookup(
        &self,
        previous_response_id: Option<&str>,
        requested_call_ids: &HashSet<String>,
    ) -> CachedLookup {
        let inner = self.inner.lock().expect("codex reasoning cache lock");
        let previous = previous_response_id.and_then(|id| inner.responses.get(id).cloned());
        let fallback = inner.unique_fallback_calls(requested_call_ids, previous.as_ref());
        CachedLookup { previous, fallback }
    }
}

impl Inner {
    fn next_anonymous_response_id(&mut self) -> String {
        self.anonymous_response_counter = self.anonymous_response_counter.wrapping_add(1);
        format!(
            "ai-switch-anonymous-response-{}",
            self.anonymous_response_counter
        )
    }

    fn insert_calls(&mut self, response_id: &str, calls: Vec<(String, Value)>) -> usize {
        if !self.responses.contains_key(response_id) {
            self.response_order.push_back(response_id.to_string());
        }

        let cached_response = self.responses.entry(response_id.to_string()).or_default();
        let mut inserted_or_updated = 0usize;
        let mut indexed_call_ids = Vec::new();
        for (call_id, item) in calls {
            if !cached_response.calls_by_id.contains_key(&call_id) {
                cached_response.call_order.push(call_id.clone());
            }
            cached_response.calls_by_id.insert(call_id.clone(), item);
            indexed_call_ids.push(call_id);
            inserted_or_updated += 1;
        }
        for call_id in indexed_call_ids {
            self.index_call(&call_id, response_id);
        }
        self.prune();
        inserted_or_updated
    }

    fn prune(&mut self) {
        while self.response_order.len() > MAX_CACHED_RESPONSES {
            let Some(response_id) = self.response_order.pop_front() else {
                break;
            };
            self.responses.remove(&response_id);
            self.remove_response_from_call_index(&response_id);
        }
    }

    fn index_call(&mut self, call_id: &str, response_id: &str) {
        let response_ids = self.call_index.entry(call_id.to_string()).or_default();
        if !response_ids
            .iter()
            .any(|cached_id| cached_id == response_id)
        {
            response_ids.push_back(response_id.to_string());
        }
    }

    fn remove_response_from_call_index(&mut self, response_id: &str) {
        for response_ids in self.call_index.values_mut() {
            response_ids.retain(|cached_id| cached_id != response_id);
        }
        self.call_index
            .retain(|_, response_ids| !response_ids.is_empty());
    }

    fn unique_fallback_calls(
        &self,
        requested_call_ids: &HashSet<String>,
        previous: Option<&CachedResponse>,
    ) -> CachedResponse {
        let mut selected = HashMap::new();
        for call_id in requested_call_ids {
            if previous.is_some_and(|response| response.calls_by_id.contains_key(call_id)) {
                continue;
            }
            if let Some(item) = self.unique_call(call_id) {
                selected.insert(call_id.clone(), item.clone());
            }
        }

        let mut fallback = CachedResponse::default();
        for response_id in &self.response_order {
            let Some(response) = self.responses.get(response_id) else {
                continue;
            };
            for call_id in &response.call_order {
                if let Some(item) = selected.remove(call_id) {
                    fallback.call_order.push(call_id.clone());
                    fallback.calls_by_id.insert(call_id.clone(), item);
                }
            }
        }
        fallback
    }

    fn unique_call(&self, call_id: &str) -> Option<&Value> {
        let response_ids = self.call_index.get(call_id)?;
        let mut found = None;
        for response_id in response_ids {
            let Some(item) = self
                .responses
                .get(response_id)
                .and_then(|response| response.calls_by_id.get(call_id))
            else {
                continue;
            };
            if found.is_some() {
                return None;
            }
            found = Some(item);
        }
        found
    }
}

impl CachedLookup {
    fn call(&self, call_id: &str) -> Option<&Value> {
        self.previous
            .as_ref()
            .and_then(|previous| previous.calls_by_id.get(call_id))
            .or_else(|| self.fallback.calls_by_id.get(call_id))
    }

    fn restore_group(
        &self,
        output_call_ids: &HashSet<String>,
        existing_call_ids: &HashSet<String>,
    ) -> Vec<(String, Value)> {
        let mut group = Vec::new();
        let mut grouped_call_ids = HashSet::new();
        if let Some(previous) = &self.previous {
            append_restore_group(
                previous,
                output_call_ids,
                existing_call_ids,
                &mut grouped_call_ids,
                &mut group,
            );
        }
        append_restore_group(
            &self.fallback,
            output_call_ids,
            existing_call_ids,
            &mut grouped_call_ids,
            &mut group,
        );
        group
    }
}

fn append_restore_group(
    response: &CachedResponse,
    output_call_ids: &HashSet<String>,
    existing_call_ids: &HashSet<String>,
    grouped_call_ids: &mut HashSet<String>,
    group: &mut Vec<(String, Value)>,
) {
    for call_id in &response.call_order {
        if !output_call_ids.contains(call_id)
            || existing_call_ids.contains(call_id)
            || grouped_call_ids.contains(call_id)
        {
            continue;
        }
        if let Some(item) = response.calls_by_id.get(call_id).cloned() {
            grouped_call_ids.insert(call_id.clone());
            group.push((call_id.clone(), item));
        }
    }
}

fn item_type(item: &Value) -> Option<&str> {
    item.get("type").and_then(Value::as_str)
}

fn call_id_of(item: &Value) -> Option<String> {
    item.get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn is_call_item_type(item_type: &str) -> bool {
    matches!(
        item_type,
        "function_call" | "custom_tool_call" | "tool_search_call"
    )
}

fn is_call_output_item_type(item_type: &str) -> bool {
    matches!(
        item_type,
        "function_call_output" | "custom_tool_call_output" | "tool_search_output"
    )
}

fn enrich_call_item_from_cache(item: &mut Value, cached: &Value) -> bool {
    let mut changed = false;
    for key in [
        "name",
        "namespace",
        "arguments",
        "input",
        "status",
        "execution",
        "reasoning_content",
        "reasoning",
    ] {
        if item.get(key).is_some_and(|value| !is_empty_value(value)) {
            continue;
        }
        let Some(value) = cached.get(key).filter(|value| !is_empty_value(value)) else {
            continue;
        };
        if let Some(object) = item.as_object_mut() {
            object.insert(key.to_string(), value.clone());
            changed = true;
        }
    }
    changed
}

fn is_empty_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(value) => value.trim().is_empty(),
        Value::Array(values) => values.is_empty(),
        Value::Object(values) => values.is_empty(),
        _ => false,
    }
}

fn parse_chat_response(body: &[u8]) -> Option<CapturedTurn> {
    let text = String::from_utf8_lossy(body);
    if text
        .lines()
        .any(|line| line.trim_start().starts_with("data:"))
    {
        parse_chat_sse(&text)
    } else {
        parse_chat_json(body)
    }
}

fn parse_chat_json(body: &[u8]) -> Option<CapturedTurn> {
    let value = serde_json::from_slice::<Value>(body).ok()?;
    let response_id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string);
    let message = value.pointer("/choices/0/message")?;
    let reasoning = message
        .get("reasoning_content")
        .or_else(|| message.get("reasoning"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut calls = Vec::new();
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for tool_call in tool_calls {
            let Some(call_id) = call_id_of(tool_call) else {
                continue;
            };
            let function = tool_call.get("function");
            let name = function
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let arguments = function
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            calls.push((call_id, name, arguments));
        }
    }
    Some(CapturedTurn {
        response_id,
        reasoning,
        calls,
    })
}

fn parse_chat_sse(text: &str) -> Option<CapturedTurn> {
    use std::collections::BTreeMap;
    let normalized = text.replace("\r\n", "\n");
    let mut response_id = None;
    let mut reasoning = String::new();
    // index -> (call_id, name, accumulated arguments)
    let mut tools: BTreeMap<u64, (String, String, String)> = BTreeMap::new();

    for block in normalized.split("\n\n") {
        let data = block
            .lines()
            .filter_map(|line| line.trim().strip_prefix("data:").map(str::trim))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if response_id.is_none() {
            response_id = value
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(ToString::to_string);
        }
        let Some(delta) = value.pointer("/choices/0/delta") else {
            continue;
        };
        if let Some(text) = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(Value::as_str)
        {
            reasoning.push_str(text);
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for tool_call in tool_calls {
                let index = tool_call.get("index").and_then(Value::as_u64).unwrap_or(0);
                let entry = tools.entry(index).or_default();
                if let Some(id) = tool_call.get("id").and_then(Value::as_str) {
                    if !id.is_empty() {
                        entry.0 = id.to_string();
                    }
                }
                if let Some(function) = tool_call.get("function") {
                    if let Some(name) = function.get("name").and_then(Value::as_str) {
                        entry.1.push_str(name);
                    }
                    if let Some(args) = function.get("arguments").and_then(Value::as_str) {
                        entry.2.push_str(args);
                    }
                }
            }
        }
    }

    let calls = tools
        .into_values()
        .filter(|(call_id, _, _)| !call_id.is_empty())
        .collect::<Vec<_>>();
    Some(CapturedTurn {
        response_id,
        reasoning,
        calls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_json_call(
        cache: &CodexReasoningCache,
        response_id: &str,
        call_id: &str,
        name: &str,
        arguments: &str,
        reasoning: &str,
    ) {
        let response = serde_json::to_vec(&json!({
            "id": response_id,
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": reasoning,
                    "tool_calls": [{
                        "id": call_id,
                        "type": "function",
                        "function": {"name": name, "arguments": arguments}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .unwrap();
        cache.record_from_chat_response(&response);
    }

    #[test]
    fn restores_reasoning_onto_existing_function_call_from_json_response() {
        let cache = CodexReasoningCache::default();
        let response = serde_json::to_vec(&json!({
            "id": "chatcmpl-1",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "Plan: list files, then edit.",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "exec_command", "arguments": "{\"cmd\":\"ls\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .unwrap();
        cache.record_from_chat_response(&response);

        // Next turn replays the function_call with a lost reasoning placeholder.
        let mut body = json!({
            "input": [
                {"type": "function_call", "call_id": "call_1", "name": "exec_command", "arguments": "{\"cmd\":\"ls\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "ok"}
            ]
        });
        let changed = cache.enrich_responses_request(&mut body);
        assert_eq!(changed, 1);
        assert_eq!(
            body["input"][0]["reasoning_content"],
            "Plan: list files, then edit."
        );
    }

    #[test]
    fn synthesizes_missing_function_call_from_sse_response() {
        let cache = CodexReasoningCache::default();
        let sse = concat!(
            "data: {\"id\":\"chatcmpl-stream\",\"choices\":[{\"delta\":{\"reasoning_content\":\"Think.\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_9\",\"function\":{\"name\":\"exec_command\",\"arguments\":\"{\\\"cmd\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"pwd\\\"}\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        cache.record_from_chat_response(sse.as_bytes());

        // Codex replays only the output (previous_response_id mode).
        let mut body = json!({
            "previous_response_id": "chatcmpl-stream",
            "input": [
                {"type": "function_call_output", "call_id": "call_9", "output": "/repo"}
            ]
        });
        let changed = cache.enrich_responses_request(&mut body);
        assert_eq!(changed, 1);
        assert_eq!(body["input"][0]["type"], "function_call");
        assert_eq!(body["input"][0]["call_id"], "call_9");
        assert_eq!(body["input"][0]["name"], "exec_command");
        assert_eq!(body["input"][0]["arguments"], "{\"cmd\":\"pwd\"}");
        assert_eq!(body["input"][0]["reasoning_content"], "Think.");
        assert_eq!(body["input"][1]["type"], "function_call_output");
    }

    #[test]
    fn restores_parallel_function_calls_before_their_outputs() {
        let cache = CodexReasoningCache::default();
        let response = serde_json::to_vec(&json!({
            "id": "chatcmpl-parallel",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "Need both files.",
                    "tool_calls": [
                        {
                            "id": "call_1",
                            "type": "function",
                            "function": {"name": "read_file", "arguments": r#"{"path":"a"}"#}
                        },
                        {
                            "id": "call_2",
                            "type": "function",
                            "function": {"name": "read_file", "arguments": r#"{"path":"b"}"#}
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .unwrap();
        cache.record_from_chat_response(&response);

        let mut body = json!({
            "previous_response_id": "chatcmpl-parallel",
            "input": [
                {"type": "function_call_output", "call_id": "call_1", "output": "A"},
                {"type": "function_call_output", "call_id": "call_2", "output": "B"}
            ]
        });

        let changed = cache.enrich_responses_request(&mut body);
        assert_eq!(changed, 2);
        let input = body["input"].as_array().expect("input array");
        assert_eq!(input[0]["type"], "function_call");
        assert_eq!(input[0]["call_id"], "call_1");
        assert_eq!(input[1]["type"], "function_call");
        assert_eq!(input[1]["call_id"], "call_2");
        assert_eq!(input[2]["type"], "function_call_output");
        assert_eq!(input[3]["type"], "function_call_output");
    }

    #[test]
    fn uses_previous_response_id_to_disambiguate_reused_call_id() {
        let cache = CodexReasoningCache::default();
        record_json_call(
            &cache,
            "chatcmpl-first",
            "call_1",
            "first_tool",
            r#"{"source":"first"}"#,
            "First response reasoning.",
        );
        record_json_call(
            &cache,
            "chatcmpl-second",
            "call_1",
            "second_tool",
            r#"{"source":"second"}"#,
            "Second response reasoning.",
        );

        let mut body = json!({
            "previous_response_id": "chatcmpl-second",
            "input": [
                {"type": "function_call_output", "call_id": "call_1", "output": "ok"}
            ]
        });

        assert_eq!(cache.enrich_responses_request(&mut body), 1);
        assert_eq!(body["input"][0]["name"], "second_tool");
        assert_eq!(body["input"][0]["arguments"], r#"{"source":"second"}"#);
        assert_eq!(
            body["input"][0]["reasoning_content"],
            "Second response reasoning."
        );
    }

    #[test]
    fn does_not_restore_ambiguous_call_id_without_matching_previous_response() {
        let cache = CodexReasoningCache::default();
        record_json_call(
            &cache,
            "chatcmpl-first",
            "call_1",
            "first_tool",
            "{}",
            "First response reasoning.",
        );
        record_json_call(
            &cache,
            "chatcmpl-second",
            "call_1",
            "second_tool",
            "{}",
            "Second response reasoning.",
        );

        for previous_response_id in [None, Some("chatcmpl-missing")] {
            let mut body = json!({
                "input": [
                    {"type": "function_call_output", "call_id": "call_1", "output": "ok"}
                ]
            });
            if let Some(previous_response_id) = previous_response_id {
                body["previous_response_id"] = Value::String(previous_response_id.to_string());
            }

            assert_eq!(cache.enrich_responses_request(&mut body), 0);
            assert_eq!(body["input"].as_array().map(Vec::len), Some(1));
            assert_eq!(body["input"][0]["type"], "function_call_output");
        }
    }

    #[test]
    fn restores_single_object_input_from_previous_response() {
        let cache = CodexReasoningCache::default();
        record_json_call(
            &cache,
            "chatcmpl-single",
            "call_single",
            "read_file",
            r#"{"path":"README.md"}"#,
            "Read the file first.",
        );
        let mut body = json!({
            "previous_response_id": "chatcmpl-single",
            "input": {
                "type": "function_call_output",
                "call_id": "call_single",
                "output": "ok"
            }
        });

        assert_eq!(cache.enrich_responses_request(&mut body), 1);
        let input = body["input"].as_array().expect("expanded input array");
        assert_eq!(input[0]["type"], "function_call");
        assert_eq!(input[0]["call_id"], "call_single");
        assert_eq!(input[0]["reasoning_content"], "Read the file first.");
        assert_eq!(input[1]["type"], "function_call_output");
    }

    #[test]
    fn leaves_present_reasoning_and_unknown_calls_untouched() {
        let cache = CodexReasoningCache::default();
        let mut body = json!({
            "input": [
                {"type": "function_call", "call_id": "call_x", "name": "t", "arguments": "{}", "reasoning_content": "real"},
                {"type": "function_call_output", "call_id": "call_x", "output": "ok"}
            ]
        });
        let changed = cache.enrich_responses_request(&mut body);
        assert_eq!(changed, 0);
        assert_eq!(body["input"][0]["reasoning_content"], "real");
        assert_eq!(body["input"].as_array().unwrap().len(), 2);
    }
}
