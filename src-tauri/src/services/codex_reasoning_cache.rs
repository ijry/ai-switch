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
//! visible in the *upstream Chat response*. This store captures it there (keyed
//! by tool `call_id`) and restores it onto the matching `function_call` item on
//! the next request — the same mechanism cc-switch uses. Without it the bridge
//! can only inject a placeholder, and reasoning models stall.

use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// Upper bound on distinct `call_id`s retained; oldest are evicted first.
const MAX_CACHED_CALLS: usize = 2048;

#[derive(Clone, Default)]
pub struct CodexReasoningCache {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    /// call_id -> cached reasoning text for that tool-call turn.
    reasoning_by_call: HashMap<String, String>,
    /// call_id -> (name, arguments) so a bare function_call_output can be
    /// paired with a synthesized function_call when the client omits it.
    call_by_id: HashMap<String, (String, String)>,
    order: VecDeque<String>,
}

impl Inner {
    fn insert(&mut self, call_id: String, name: String, arguments: String, reasoning: &str) {
        if !self.call_by_id.contains_key(&call_id)
            && !self.reasoning_by_call.contains_key(&call_id)
        {
            self.order.push_back(call_id.clone());
            while self.order.len() > MAX_CACHED_CALLS {
                if let Some(evicted) = self.order.pop_front() {
                    self.reasoning_by_call.remove(&evicted);
                    self.call_by_id.remove(&evicted);
                }
            }
        }
        self.call_by_id.insert(call_id.clone(), (name, arguments));
        if !reasoning.trim().is_empty() {
            self.reasoning_by_call.insert(call_id, reasoning.to_string());
        }
    }
}

/// One captured assistant turn from an upstream Chat response.
#[derive(Debug, Default, PartialEq)]
struct CapturedTurn {
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
        for (call_id, name, arguments) in turn.calls {
            inner.insert(call_id, name, arguments, &turn.reasoning);
        }
    }

    /// Restore reasoning (and, when needed, the whole function_call item) onto a
    /// Codex Responses request body before it is converted to Chat. Returns the
    /// number of items changed. Operates in place on `input`.
    pub fn enrich_responses_request(&self, body: &mut Value) -> usize {
        let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) else {
            return 0;
        };
        let inner = self.inner.lock().expect("codex reasoning cache lock");

        // call_ids that already have a function_call item present in this input.
        let present_calls: std::collections::HashSet<String> = input
            .iter()
            .filter(|item| item_type(item) == Some("function_call"))
            .filter_map(call_id_of)
            .collect();

        let mut changed = 0usize;
        let mut rebuilt: Vec<Value> = Vec::with_capacity(input.len());
        for item in input.drain(..) {
            match item_type(&item) {
                Some("function_call") => {
                    let mut item = item;
                    if let Some(call_id) = call_id_of(&item) {
                        if fill_reasoning(&mut item, inner.reasoning_by_call.get(&call_id)) {
                            changed += 1;
                        }
                    }
                    rebuilt.push(item);
                }
                Some("function_call_output") => {
                    if let Some(call_id) = call_id_of(&item) {
                        if !present_calls.contains(&call_id) {
                            if let Some(synth) = inner.synthesize_call(&call_id) {
                                rebuilt.push(synth);
                                changed += 1;
                            }
                        }
                    }
                    rebuilt.push(item);
                }
                _ => rebuilt.push(item),
            }
        }
        *input = rebuilt;
        changed
    }
}

impl Inner {
    fn synthesize_call(&self, call_id: &str) -> Option<Value> {
        let (name, arguments) = self.call_by_id.get(call_id)?;
        let mut item = json!({
            "type": "function_call",
            "call_id": call_id,
            "name": name,
            "arguments": arguments,
        });
        if let Some(reasoning) = self.reasoning_by_call.get(call_id) {
            item["reasoning_content"] = Value::String(reasoning.clone());
        }
        Some(item)
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

/// Set `reasoning_content` on a function_call item when it is missing/empty and
/// a cached value exists. Returns whether the item changed.
fn fill_reasoning(item: &mut Value, cached: Option<&String>) -> bool {
    let Some(reasoning) = cached.filter(|value| !value.trim().is_empty()) else {
        return false;
    };
    let already = item
        .get("reasoning_content")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    if already {
        return false;
    }
    if let Some(object) = item.as_object_mut() {
        object.insert(
            "reasoning_content".to_string(),
            Value::String(reasoning.clone()),
        );
        return true;
    }
    false
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
    Some(CapturedTurn { reasoning, calls })
}

fn parse_chat_sse(text: &str) -> Option<CapturedTurn> {
    use std::collections::BTreeMap;
    let normalized = text.replace("\r\n", "\n");
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
    Some(CapturedTurn { reasoning, calls })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Think.\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_9\",\"function\":{\"name\":\"exec_command\",\"arguments\":\"{\\\"cmd\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"pwd\\\"}\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        cache.record_from_chat_response(sse.as_bytes());

        // Codex replays only the output (previous_response_id mode).
        let mut body = json!({
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
