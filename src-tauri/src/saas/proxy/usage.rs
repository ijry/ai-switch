use crate::saas::billing::BillableUsage;
use serde_json::Value;

const MAX_OBSERVED_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct UsageObserver {
    streaming: bool,
    anthropic: bool,
    buffer: Vec<u8>,
    frame: Vec<u8>,
    input: Option<i64>,
    output: Option<i64>,
    cache_read: i64,
    cache_write: i64,
    terminal: bool,
    invalid: bool,
}

impl UsageObserver {
    pub(super) fn new(streaming: bool, anthropic: bool) -> Self {
        Self {
            streaming,
            anthropic,
            buffer: Vec::new(),
            frame: Vec::new(),
            input: None,
            output: None,
            cache_read: 0,
            cache_write: 0,
            terminal: false,
            invalid: false,
        }
    }

    pub(super) fn observe(&mut self, bytes: &[u8]) {
        if self.invalid {
            return;
        }
        if self.buffer.len().saturating_add(bytes.len()) > MAX_OBSERVED_BYTES {
            self.invalid = true;
            self.buffer.clear();
            return;
        }
        self.buffer.extend_from_slice(bytes);
        if !self.streaming {
            return;
        }
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let remainder = self.buffer.split_off(end + 1);
            let mut line = std::mem::replace(&mut self.buffer, remainder);
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                let frame = std::mem::take(&mut self.frame);
                if frame == b"[DONE]" {
                    self.terminal = true;
                } else if let Ok(value) = serde_json::from_slice::<Value>(&frame) {
                    self.read_json(&value);
                }
            } else if let Some(data) = line.strip_prefix(b"data:") {
                let data = data.strip_prefix(b" ").unwrap_or(data);
                if self.frame.len().saturating_add(data.len() + 1) > MAX_OBSERVED_BYTES {
                    self.invalid = true;
                    self.frame.clear();
                    return;
                }
                if !self.frame.is_empty() {
                    self.frame.push(b'\n');
                }
                self.frame.extend_from_slice(data);
            }
        }
    }

    fn read_json(&mut self, value: &Value) {
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(kind, "response.completed" | "message_stop") {
            self.terminal = true;
        }
        if matches!(kind, "response.failed" | "error")
            || value.get("error").is_some_and(|error| !error.is_null())
        {
            self.invalid = true;
            return;
        }
        let usage = value
            .get("usage")
            .or_else(|| {
                value
                    .get("response")
                    .and_then(|response| response.get("usage"))
            })
            .or_else(|| {
                value
                    .get("message")
                    .and_then(|message| message.get("usage"))
            });
        let Some(usage) = usage else {
            return;
        };
        for (field, target) in [
            (
                usage
                    .get("input_tokens")
                    .or_else(|| usage.get("prompt_tokens")),
                &mut self.input,
            ),
            (
                usage
                    .get("output_tokens")
                    .or_else(|| usage.get("completion_tokens")),
                &mut self.output,
            ),
        ] {
            if let Some(field) = field {
                match field.as_i64().filter(|tokens| *tokens >= 0) {
                    Some(tokens) => *target = Some(tokens),
                    None => self.invalid = true,
                }
            }
        }
        let cache = if self.anthropic {
            usage.get("cache_read_input_tokens")
        } else {
            usage
                .get("input_tokens_details")
                .or_else(|| usage.get("prompt_tokens_details"))
                .and_then(|details| details.get("cached_tokens"))
        };
        for (field, target) in [
            (cache, &mut self.cache_read),
            (
                usage.get("cache_creation_input_tokens"),
                &mut self.cache_write,
            ),
        ] {
            if let Some(field) = field {
                match field.as_i64().filter(|tokens| *tokens >= 0) {
                    Some(tokens) => *target = tokens,
                    None => self.invalid = true,
                }
            }
        }
    }

    pub(super) fn finish(&mut self, eof: bool) -> Option<BillableUsage> {
        if !self.streaming && eof {
            let buffer = std::mem::take(&mut self.buffer);
            let value: Value = serde_json::from_slice(&buffer).ok()?;
            self.read_json(&value);
            self.terminal = true;
        }
        if self.invalid || !self.terminal {
            return None;
        }
        let mut input = self.input?;
        if !self.anthropic {
            input = input
                .checked_sub(self.cache_read)
                .filter(|tokens| *tokens >= 0)?;
        }
        Some(BillableUsage {
            input_tokens: input,
            cache_read_tokens: self.cache_read,
            cache_write_tokens: self.cache_write,
            output_tokens: self.output?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_usage_separates_openai_cached_input_and_anthropic_cache_writes() {
        let mut observer = UsageObserver::new(false, false);
        observer.observe(br#"{"usage":{"prompt_tokens":120,"completion_tokens":30,"prompt_tokens_details":{"cached_tokens":50}}}"#);
        let usage = observer.finish(true).unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.cache_read_tokens,
                usage.cache_write_tokens,
                usage.output_tokens
            ),
            (70, 50, 0, 30)
        );
        let mut observer = UsageObserver::new(false, true);
        observer.observe(br#"{"usage":{"input_tokens":10,"output_tokens":8,"cache_read_input_tokens":20,"cache_creation_input_tokens":30}}"#);
        let usage = observer.finish(true).unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.cache_read_tokens,
                usage.cache_write_tokens,
                usage.output_tokens
            ),
            (10, 20, 30, 8)
        );
    }

    #[test]
    fn fragmented_sse_merges_anthropic_usage_and_requires_completion() {
        let payload = b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":0,\"cache_read_input_tokens\":4}}}\r\n\r\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":7}}\n\ndata: {\"type\":\"message_stop\"}\n\n";
        let mut observer = UsageObserver::new(true, true);
        for chunk in payload.chunks(7) {
            observer.observe(chunk);
        }
        let usage = observer.finish(true).unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_read_tokens
            ),
            (10, 7, 4)
        );
        let mut partial = UsageObserver::new(true, true);
        partial.observe(&payload[..payload.len() - 39]);
        assert!(partial.finish(false).is_none());
    }

    #[test]
    fn responses_final_usage_missing_or_invalid_usage_stays_unconfirmed() {
        let mut observer = UsageObserver::new(true, false);
        observer.observe(b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":9,\"output_tokens\":6,\"input_tokens_details\":{\"cached_tokens\":3}}}}\n\n");
        let usage = observer.finish(true).unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_read_tokens
            ),
            (6, 6, 3)
        );
        let mut observer = UsageObserver::new(false, false);
        observer.observe(br#"{"usage":{"input_tokens":1,"output_tokens":2,"input_tokens_details":{"cached_tokens":5}}}"#);
        assert!(observer.finish(true).is_none());
        let mut observer = UsageObserver::new(false, false);
        observer.observe(br#"{"choices":[{"message":{"content":"unknown usage"}}]}"#);
        assert!(observer.finish(true).is_none());
    }
}
