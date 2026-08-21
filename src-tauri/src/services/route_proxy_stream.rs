//! Incremental inspection of a streamed upstream response.
//!
//! When the proxy buffers a response it can parse the finished body with the
//! whole-body helpers. A streamed response never exists as one buffer, so the
//! same facts — token usage, the served model, whether the stream ended
//! properly — have to be accumulated as bytes pass through.
//!
//! Two pieces live here:
//!
//! - [`SseFramer`] splits a byte stream into complete SSE frames. Chunk
//!   boundaries fall wherever the network puts them, routinely mid-frame, so
//!   the tail of each chunk is carried until its terminator arrives.
//! - [`StreamObserver`] folds those frames into the values the request log and
//!   health bookkeeping need once the stream finishes.

use crate::models::route_pool::RouteUsageBreakdown;
use serde_json::Value;

/// Terminal markers that mean "this stream ended on purpose".
///
/// Kept byte-identical to the whole-body check in
/// [`crate::services::response_failure_service::stream_disconnected_before_completion`]
/// so a stream is judged the same way whether it was buffered or passed
/// through. Changing one without the other splits the two paths' verdicts.
const STREAM_TERMINAL_MARKERS: [&str; 5] = [
    "response.completed",
    "data: [DONE]",
    "message_stop",
    "\"finish_reason\":\"stop\"",
    "\"finishReason\":\"STOP\"",
];

/// The longest terminal marker, minus one byte: the most that can be pending
/// across a chunk boundary while still being able to complete into a match.
const MAX_MARKER_OVERLAP: usize = 24;

/// Splits a byte stream into complete SSE frames.
///
/// Feed it chunks; it returns whichever frames became complete. Anything after
/// the last `\n\n` stays buffered until more bytes arrive, so a frame split
/// across chunks is never handed out as two malformed halves.
#[derive(Debug, Default)]
pub struct SseFramer {
    buffer: String,
}

impl SseFramer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a chunk and take whatever frames are now complete.
    ///
    /// Frame payloads are the joined `data:` lines, matching how the buffered
    /// parser in `route_protocol_bridge::sse` reads a body. Empty frames and
    /// the `[DONE]` sentinel carry no JSON and are skipped.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        // Lossy is right here: a chunk can split a multi-byte UTF-8 sequence,
        // and the replacement char only ever lands inside a payload we then
        // fail to parse as JSON — never in the ASCII framing itself.
        self.buffer.push_str(&String::from_utf8_lossy(chunk));
        if self.buffer.contains('\r') {
            self.buffer = self.buffer.replace("\r\n", "\n");
        }

        let mut payloads = Vec::new();
        while let Some(index) = self.buffer.find("\n\n") {
            let block: String = self.buffer.drain(..index + 2).collect();
            if let Some(payload) = frame_payload(&block) {
                payloads.push(payload);
            }
        }
        payloads
    }

    /// Flush the trailing bytes as a final frame.
    ///
    /// An upstream that ends without a blank line after its last frame is
    /// common enough that dropping the remainder would lose the usage totals
    /// that often ride on exactly that frame.
    pub fn finish(&mut self) -> Option<String> {
        let block = std::mem::take(&mut self.buffer);
        frame_payload(&block)
    }
}

/// Extract the joined `data:` payload of one SSE block, if it carries one.
fn frame_payload(block: &str) -> Option<String> {
    let data = block
        .lines()
        .filter_map(|line| line.trim().strip_prefix("data:").map(str::trim))
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    Some(data)
}

/// What a finished stream turned out to contain.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamOutcome {
    pub usage: RouteUsageBreakdown,
    pub response_model: Option<String>,
    /// Payload bytes seen, for the request log's duration/size reporting.
    pub byte_count: usize,
    /// True when data frames arrived but no terminal marker ever did — the
    /// streaming counterpart of `stream_disconnected_before_completion`.
    pub disconnected_before_completion: bool,
}

/// Accumulates the facts a finished request needs, one chunk at a time.
///
/// Everything it keeps is bounded: a usage struct, an optional model name, a
/// capped prefix, two flags and a small carry buffer. A long response costs no
/// more memory than a short one, which is the point of streaming at all.
#[derive(Debug)]
pub struct StreamObserver {
    framer: SseFramer,
    usage: RouteUsageBreakdown,
    response_model: Option<String>,
    preview: Vec<u8>,
    preview_limit: usize,
    byte_count: usize,
    saw_data_frame: bool,
    saw_terminal_marker: bool,
    /// Tail of the previous chunk, so a terminal marker straddling a chunk
    /// boundary is still found.
    marker_carry: String,
    streaming_request: bool,
}

impl StreamObserver {
    pub fn new(preview_limit: usize, streaming_request: bool) -> Self {
        Self {
            framer: SseFramer::new(),
            usage: RouteUsageBreakdown::default(),
            response_model: None,
            preview: Vec::new(),
            preview_limit,
            byte_count: 0,
            saw_data_frame: false,
            saw_terminal_marker: false,
            marker_carry: String::new(),
            streaming_request,
        }
    }

    pub fn observe(&mut self, chunk: &[u8]) {
        self.byte_count += chunk.len();
        if self.preview.len() < self.preview_limit {
            let take = (self.preview_limit - self.preview.len()).min(chunk.len());
            self.preview.extend_from_slice(&chunk[..take]);
        }
        self.scan_for_terminal_marker(chunk);

        for payload in self.framer.push(chunk) {
            self.absorb_frame(&payload);
        }
    }

    /// Finish the stream and report what it contained.
    pub fn finish(mut self) -> StreamOutcome {
        if let Some(payload) = self.framer.finish() {
            self.absorb_frame(&payload);
        }
        StreamOutcome {
            usage: self.usage,
            response_model: self.response_model,
            byte_count: self.byte_count,
            // Only a streaming request can be truncated in this sense; a plain
            // JSON reply has no terminal event to miss.
            disconnected_before_completion: self.streaming_request
                && self.saw_data_frame
                && !self.saw_terminal_marker,
        }
    }

    /// The leading bytes of the response, for the live log's stage preview.
    pub fn preview(&self) -> &[u8] {
        &self.preview
    }

    fn absorb_frame(&mut self, payload: &str) {
        self.saw_data_frame = true;
        let Ok(value) = serde_json::from_str::<Value>(payload) else {
            // A frame that is not JSON still proves data flowed; only its
            // contents are unusable. Matches the lossy buffered parser.
            return;
        };
        self.usage
            .merge_from(super::route_proxy_service::usage_breakdown_from_value(
                &value,
            ));
        if self.response_model.is_none() {
            self.response_model = super::route_proxy_service::response_model_from_value(&value);
        }
    }

    fn scan_for_terminal_marker(&mut self, chunk: &[u8]) {
        if self.saw_terminal_marker {
            return;
        }
        let text = String::from_utf8_lossy(chunk);
        // Prepend the carry so a marker split across chunks is still matched.
        let haystack = if self.marker_carry.is_empty() {
            text.into_owned()
        } else {
            format!("{}{}", self.marker_carry, text)
        };
        if STREAM_TERMINAL_MARKERS
            .iter()
            .any(|marker| haystack.contains(marker))
        {
            self.saw_terminal_marker = true;
            self.marker_carry.clear();
            return;
        }
        // Keep only enough tail to complete the longest marker next time.
        let keep = haystack
            .char_indices()
            .rev()
            .take(MAX_MARKER_OVERLAP)
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.marker_carry = haystack[keep..].to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framer_joins_a_frame_split_across_chunks() {
        let mut framer = SseFramer::new();
        // The split lands inside the JSON payload, the worst case for a naive
        // splitter: neither half is valid on its own.
        assert!(framer.push(b"data: {\"usage\":{\"input_to").is_empty());
        let frames = framer.push(b"kens\":5}}\n\n");
        assert_eq!(frames, vec![r#"{"usage":{"input_tokens":5}}"#.to_string()]);
    }

    #[test]
    fn framer_handles_a_split_inside_the_blank_line_terminator() {
        let mut framer = SseFramer::new();
        assert!(framer.push(b"data: {\"a\":1}\n").is_empty());
        assert_eq!(framer.push(b"\n"), vec![r#"{"a":1}"#.to_string()]);
    }

    #[test]
    fn framer_skips_done_and_empty_frames() {
        let mut framer = SseFramer::new();
        let frames = framer.push(b"data: [DONE]\n\ndata:\n\ndata: {\"a\":1}\n\n");
        assert_eq!(frames, vec![r#"{"a":1}"#.to_string()]);
    }

    #[test]
    fn framer_normalizes_crlf_frames() {
        let mut framer = SseFramer::new();
        let frames = framer.push(b"event: message\r\ndata: {\"a\":1}\r\n\r\n");
        assert_eq!(frames, vec![r#"{"a":1}"#.to_string()]);
    }

    #[test]
    fn framer_flushes_a_trailing_frame_without_blank_line() {
        let mut framer = SseFramer::new();
        assert!(framer.push(b"data: {\"a\":1}").is_empty());
        assert_eq!(framer.finish(), Some(r#"{"a":1}"#.to_string()));
    }

    #[test]
    fn observer_merges_usage_across_frames_like_the_buffered_parser() {
        let mut observer = StreamObserver::new(1024, true);
        observer.observe(
            br#"data: {"type":"message_start","message":{"model":"claude-opus-4-8","usage":{"input_tokens":120,"cache_read_input_tokens":80}}}

"#,
        );
        observer.observe(
            br#"data: {"type":"message_delta","usage":{"output_tokens":30}}

data: {"type":"message_stop"}

"#,
        );
        let outcome = observer.finish();
        assert_eq!(outcome.usage.input_tokens, Some(120));
        assert_eq!(outcome.usage.output_tokens, Some(30));
        assert_eq!(outcome.usage.cache_tokens, Some(80));
        assert_eq!(outcome.response_model.as_deref(), Some("claude-opus-4-8"));
        assert!(!outcome.disconnected_before_completion);
    }

    #[test]
    fn observer_reads_usage_from_a_final_chunk_split_mid_frame() {
        let mut observer = StreamObserver::new(1024, true);
        observer.observe(b"data: {\"choices\":[],\"usage\":null}\n\ndata: {\"usage\":{\"prompt_to");
        observer.observe(b"kens\":7,\"completion_tokens\":9}}\n\ndata: [DONE]\n\n");
        let outcome = observer.finish();
        assert_eq!(outcome.usage.input_tokens, Some(7));
        assert_eq!(outcome.usage.output_tokens, Some(9));
        assert!(!outcome.disconnected_before_completion);
    }

    #[test]
    fn observer_flags_a_stream_that_never_terminated() {
        let mut observer = StreamObserver::new(1024, true);
        observer.observe(b"data: {\"type\":\"content_block_delta\"}\n\n");
        let outcome = observer.finish();
        assert!(outcome.disconnected_before_completion);
    }

    #[test]
    fn observer_finds_a_terminal_marker_split_across_chunks() {
        let mut observer = StreamObserver::new(1024, true);
        observer.observe(b"data: {\"type\":\"content_block_delta\"}\n\ndata: {\"type\":\"mess");
        observer.observe(b"age_stop\"}\n\n");
        let outcome = observer.finish();
        assert!(
            !outcome.disconnected_before_completion,
            "a marker split across chunks must still count as a clean end"
        );
    }

    #[test]
    fn observer_does_not_flag_a_non_streaming_response() {
        let mut observer = StreamObserver::new(1024, false);
        observer.observe(br#"{"usage":{"input_tokens":3}}"#);
        let outcome = observer.finish();
        assert!(!outcome.disconnected_before_completion);
    }

    #[test]
    fn observer_caps_the_preview_but_keeps_counting_bytes() {
        let mut observer = StreamObserver::new(8, true);
        observer.observe(b"data: {\"type\":\"message_stop\"}\n\n");
        assert_eq!(observer.preview().len(), 8);
        let outcome = observer.finish();
        assert_eq!(outcome.byte_count, 31);
    }
}
