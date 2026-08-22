//! Bounded priming for the incremental (non-buffering) proxy response path.
//!
//! The buffered path reads the whole upstream body before writing anything to
//! the client, which is what lets the proxy inspect a response and silently
//! retry it on another credential. Streaming clients pay for that: they see
//! nothing until the generation finishes.
//!
//! Priming is the middle ground, taken from cc-switch's
//! `proxy/forwarder.rs::validate_responses_stream_start`. Read a *bounded*
//! prefix, inspect that, then either fall back to the buffered path (if the
//! stream already ended) or commit: replay the prefix and chain the rest
//! straight through to the client.
//!
//! Once committed there is no failover for that request — the client already has
//! bytes. That trade-off is why the caller gates this behind a user setting.

use bytes::Bytes;
use futures_util::{stream, Stream, StreamExt};
use std::time::Duration;

/// Cap on how much of the body is pulled before committing. Matches cc-switch's
/// `MAX_PRIME_BYTES`: enough to carry an error envelope or the opening SSE
/// events, small enough that time-to-first-byte stays low.
pub(crate) const MAX_PRIME_BYTES: usize = 256 * 1024;

/// A response whose opening bytes have been read.
pub(crate) enum PrimedUpstream {
    /// The body ended during priming, so the whole thing is in hand. The caller
    /// should treat this exactly like the buffered path — including failover,
    /// which is still available because nothing has been written to the client.
    Complete(Vec<u8>),
    /// The body is still open. `prefix` is what was read; `rest` is the
    /// remainder. Inspect `prefix`, then either abandon the request or commit by
    /// streaming `into_stream()` to the client.
    Streaming {
        prefix: Vec<u8>,
        rest: Box<dyn Stream<Item = reqwest::Result<Bytes>> + Send + Unpin>,
        chunks: Vec<Bytes>,
    },
}

impl PrimedUpstream {
    /// The bytes read so far, for inspection in either state.
    pub(crate) fn prefix(&self) -> &[u8] {
        match self {
            Self::Complete(body) => body,
            Self::Streaming { prefix, .. } => prefix,
        }
    }

    /// Consumes this into a stream that replays the primed prefix and then
    /// continues with the live remainder, preserving byte order.
    pub(crate) fn into_stream(self) -> impl Stream<Item = reqwest::Result<Bytes>> + Send {
        match self {
            Self::Complete(body) => {
                let chunks = vec![Ok(Bytes::from(body))];
                stream::iter(chunks).chain(EmptyRest::new()).boxed()
            }
            Self::Streaming { chunks, rest, .. } => {
                stream::iter(chunks.into_iter().map(Ok)).chain(rest).boxed()
            }
        }
    }
}

/// An empty tail so both arms of `into_stream` have the same type.
struct EmptyRest;

impl EmptyRest {
    fn new() -> stream::Iter<std::vec::IntoIter<reqwest::Result<Bytes>>> {
        stream::iter(Vec::new())
    }
}

/// Reads from `body` until a terminal SSE block is seen, the stream ends, or
/// `max_prime_bytes` is reached.
///
/// `first_chunk_timeout` bounds only the wait for the *first* byte: an upstream
/// that accepts the request and then goes silent would otherwise hang the client
/// forever, since the outbound client is built with no request timeout.
pub(crate) async fn prime_upstream_stream<S>(
    mut body: S,
    max_prime_bytes: usize,
    first_chunk_timeout: Option<Duration>,
) -> Result<PrimedUpstream, String>
where
    S: Stream<Item = reqwest::Result<Bytes>> + Send + Unpin + 'static,
{
    let mut chunks: Vec<Bytes> = Vec::new();
    let mut prefix: Vec<u8> = Vec::new();
    // Accumulated text for delimiter scanning, with any partial UTF-8 sequence
    // held back so a character split across chunks is never corrupted.
    let mut text = String::new();
    let mut utf8_remainder: Vec<u8> = Vec::new();

    loop {
        let next = match first_chunk_timeout {
            Some(timeout) if chunks.is_empty() => tokio::time::timeout(timeout, body.next())
                .await
                .map_err(|_| {
                    format!(
                        "upstream produced no data within {}s",
                        timeout.as_secs_f32()
                    )
                })?,
            _ => body.next().await,
        };

        let Some(chunk) = next else {
            // Ended during priming: hand back the whole body so the caller keeps
            // every inspection and failover option it has today.
            return Ok(PrimedUpstream::Complete(prefix));
        };
        let chunk = chunk.map_err(|error| format!("could not read upstream response: {error}"))?;

        prefix.extend_from_slice(&chunk);
        append_utf8_safe(&mut text, &mut utf8_remainder, &chunk);
        chunks.push(chunk);

        // A terminal marker means the interesting part has arrived; commit now
        // rather than waiting for the cap.
        if contains_terminal_marker(&text) || prefix.len() >= max_prime_bytes {
            return Ok(PrimedUpstream::Streaming {
                prefix,
                rest: Box::new(body),
                chunks,
            });
        }
    }
}

/// True once the primed text carries a marker that a real response has begun or
/// finished. Deliberately generous: any of these means the upstream is producing
/// output rather than an error envelope.
fn contains_terminal_marker(text: &str) -> bool {
    // A completed SSE block is the strongest signal that framing is intact.
    if text.contains("\n\n") || text.contains("\r\n\r\n") {
        return true;
    }
    ["data: [DONE]", "message_stop", "response.completed"]
        .iter()
        .any(|marker| text.contains(marker))
}

/// Appends bytes to a `String`, holding back a trailing incomplete UTF-8
/// sequence so a multi-byte character split across chunks is not corrupted.
/// Ported from cc-switch's `proxy/sse.rs::append_utf8_safe`.
fn append_utf8_safe(buffer: &mut String, remainder: &mut Vec<u8>, new_bytes: &[u8]) {
    let combined: Vec<u8> = if remainder.is_empty() {
        new_bytes.to_vec()
    } else {
        let mut combined = std::mem::take(remainder);
        combined.extend_from_slice(new_bytes);
        combined
    };

    match std::str::from_utf8(&combined) {
        Ok(text) => buffer.push_str(text),
        Err(error) => {
            let valid_up_to = error.valid_up_to();
            // SAFETY-equivalent: from_utf8 told us this prefix is valid.
            if let Ok(text) = std::str::from_utf8(&combined[..valid_up_to]) {
                buffer.push_str(text);
            }
            let tail = &combined[valid_up_to..];
            // A well-formed stream leaves at most 3 bytes pending. More than
            // that means genuinely invalid input, so decode it lossily and move
            // on instead of accumulating forever.
            if tail.len() <= 3 {
                *remainder = tail.to_vec();
            } else {
                buffer.push_str(&String::from_utf8_lossy(tail));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk_stream(
        chunks: Vec<&'static [u8]>,
    ) -> impl Stream<Item = reqwest::Result<Bytes>> + Send + Unpin {
        stream::iter(
            chunks
                .into_iter()
                .map(|chunk| Ok(Bytes::from_static(chunk)))
                .collect::<Vec<_>>(),
        )
    }

    async fn collect(stream: impl Stream<Item = reqwest::Result<Bytes>> + Send) -> Vec<u8> {
        let mut out = Vec::new();
        let mut stream = Box::pin(stream);
        while let Some(chunk) = stream.next().await {
            out.extend_from_slice(&chunk.expect("chunk"));
        }
        out
    }

    /// A short response that ends while priming must come back Complete, so the
    /// caller keeps failover for it.
    #[tokio::test]
    async fn short_body_reports_complete() {
        let primed = prime_upstream_stream(chunk_stream(vec![b"{\"ok\":true}"]), 1024, None)
            .await
            .expect("primed");

        assert!(matches!(primed, PrimedUpstream::Complete(_)));
        assert_eq!(primed.prefix(), b"{\"ok\":true}");
    }

    /// The whole point: replaying the prefix then chaining the rest must
    /// reproduce the upstream bytes exactly, in order.
    #[tokio::test]
    async fn prefix_then_rest_preserves_byte_order() {
        let stream = chunk_stream(vec![
            b"data: {\"n\":1}\n\n",
            b"data: {\"n\":2}\n\n",
            b"data: [DONE]\n\n",
        ]);
        let primed = prime_upstream_stream(stream, MAX_PRIME_BYTES, None)
            .await
            .expect("primed");

        // The first chunk closes an SSE block, so priming commits immediately.
        assert!(matches!(primed, PrimedUpstream::Streaming { .. }));
        assert_eq!(
            collect(primed.into_stream()).await,
            b"data: {\"n\":1}\n\ndata: {\"n\":2}\n\ndata: [DONE]\n\n"
        );
    }

    /// A Complete body must also round-trip through into_stream, since the
    /// caller may still choose to stream it.
    #[tokio::test]
    async fn complete_body_round_trips_through_into_stream() {
        let primed = prime_upstream_stream(chunk_stream(vec![b"abc", b"def"]), 1024, None)
            .await
            .expect("primed");

        assert_eq!(collect(primed.into_stream()).await, b"abcdef");
    }

    /// Without a cap, an upstream that never emits a delimiter would be buffered
    /// in full — defeating the purpose.
    #[tokio::test]
    async fn commits_once_the_prime_cap_is_reached() {
        // No delimiter anywhere, so only the cap can stop priming.
        let stream = chunk_stream(vec![b"aaaa", b"bbbb", b"cccc"]);
        let primed = prime_upstream_stream(stream, 4, None)
            .await
            .expect("primed");

        match &primed {
            PrimedUpstream::Streaming { prefix, .. } => {
                assert_eq!(prefix, b"aaaa", "must stop at the cap, not read on");
            }
            PrimedUpstream::Complete(_) => panic!("should have committed at the cap"),
        }
        // The un-primed remainder still reaches the client.
        assert_eq!(collect(primed.into_stream()).await, b"aaaabbbbcccc");
    }

    /// A multi-byte character split across a chunk boundary must not be
    /// corrupted by the delimiter scan, and must survive intact on the wire.
    #[tokio::test]
    async fn utf8_character_split_across_chunks_is_not_corrupted() {
        // "世" is E4 B8 96; split it 1 byte / 2 bytes.
        let stream = chunk_stream(vec![b"data: \xe4", b"\xb8\x96\n\n"]);
        let primed = prime_upstream_stream(stream, MAX_PRIME_BYTES, None)
            .await
            .expect("primed");

        let bytes = collect(primed.into_stream()).await;
        assert_eq!(
            String::from_utf8(bytes).expect("valid utf8"),
            "data: 世\n\n"
        );
    }

    #[test]
    fn append_utf8_safe_holds_back_partial_sequences() {
        let mut buffer = String::new();
        let mut remainder = Vec::new();

        append_utf8_safe(&mut buffer, &mut remainder, b"ab\xe4");
        assert_eq!(buffer, "ab", "the partial character must be held back");
        assert_eq!(remainder, vec![0xe4]);

        append_utf8_safe(&mut buffer, &mut remainder, b"\xb8\x96c");
        assert_eq!(buffer, "ab世c");
        assert!(remainder.is_empty());
    }

    /// An upstream that accepts the request then goes silent must not hang the
    /// client, since the outbound client has no request timeout.
    #[tokio::test]
    async fn first_chunk_timeout_fires_on_a_silent_upstream() {
        let silent = stream::pending::<reqwest::Result<Bytes>>();
        let error = match prime_upstream_stream(silent, 1024, Some(Duration::from_millis(20))).await
        {
            Err(error) => error,
            Ok(_) => panic!("a silent upstream must time out rather than prime"),
        };

        assert!(error.contains("no data within"), "error={error}");
    }
}
