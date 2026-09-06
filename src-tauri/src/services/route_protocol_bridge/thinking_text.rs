//! Recovers reasoning that an upstream inlined into the visible answer.
//!
//! Relays with a "thinking to content" switch — New API's `thinking_to_content`
//! and the one-api forks that copied it — splice the model's reasoning into
//! `content` wrapped in `<think>` / `<thinking>` tags, so that clients which only
//! read `content` still show something. Some models write the same tags
//! themselves when nothing wired up their reasoning channel. Either way the tags
//! arrive on the *visible* text channel, and a bridge that forwards text
//! verbatim makes Codex render `<thinking>…</thinking>` as part of the answer.
//!
//! Moving the block onto the reasoning channel puts it where the client already
//! knows how to render reasoning, and leaves `output_text` holding the answer
//! alone.
//!
//! # Why only a leading block
//!
//! A tag is recognised only at the very start of a turn's text, which is where
//! every producer above puts it. Rewriting tags found anywhere would eat the
//! answer whenever the tags are what the user is asking about: "strip the
//! `<think>` blocks out of this transcript" is a question that quotes them, and
//! the reply quotes them back.

/// Openers, longest first. `<think>` is not a prefix of `<thinking>` — the two
/// diverge at the `>` — so the order is defensive rather than load-bearing.
const OPEN_TAGS: [&str; 2] = ["<thinking>", "<think>"];
const CLOSE_TAGS: [&str; 2] = ["</thinking>", "</think>"];

/// How much undecided text to hold before giving up on finding an opening tag.
///
/// A producer that opens with a block writes the tag first, at most after a
/// newline or two. Anything longer is prose that happens to start blank, and
/// holding it back would stall the stream for no reason.
const MAX_UNDECIDED_BYTES: usize = 32;

/// Splits a leading `<think>` / `<thinking>` block off `text`.
///
/// Returns the block's contents and whatever the turn actually said. Text with
/// no leading block is handed back untouched, so callers can run every turn
/// through this unconditionally.
pub(super) fn split_leading_thinking(text: &str) -> (Option<&str>, &str) {
    let trimmed = text.trim_start();
    let Some(opener) = OPEN_TAGS
        .iter()
        .find(|tag| starts_with_ignore_ascii_case(trimmed, tag))
        .map(|tag| tag.len())
    else {
        return (None, text);
    };
    let body = &trimmed[opener..];
    match split_at_close_tag(body) {
        Some((reasoning, rest)) => (Some(reasoning.trim()), rest.trim_start()),
        // An unclosed block means the turn was cut off mid-reasoning, so there
        // is no answer to show. Hand the remainder over as reasoning rather than
        // leak a dangling tag into the visible text.
        None => (Some(body.trim()), ""),
    }
}

/// One piece of a streamed turn, tagged with the channel it belongs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TextSegment {
    Reasoning(String),
    Text(String),
}

/// Streaming counterpart to [`split_leading_thinking`].
///
/// Deltas break wherever the upstream flushed, routinely mid-tag, so the
/// decision cannot be made one delta at a time. This holds back exactly as much
/// as it needs to — a possible opening tag at the head, a possible closing tag
/// at the tail — and releases everything else immediately.
#[derive(Debug, Default)]
pub(super) struct InlineThinkingSplitter {
    state: SplitterState,
    buffer: String,
    opened_reasoning: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum SplitterState {
    /// Still deciding whether this turn opens with a tag.
    #[default]
    Undecided,
    /// Inside the block: everything up to the closing tag is reasoning.
    Thinking,
    /// Decided — either no leading block, or the block already closed.
    Text,
}

impl InlineThinkingSplitter {
    /// Feeds one text delta and takes whichever segments became decidable.
    pub(super) fn push(&mut self, delta: &str) -> Vec<TextSegment> {
        if delta.is_empty() {
            return Vec::new();
        }
        // Past the decision point nothing is buffered, so the common case costs
        // one allocation and no scanning.
        if self.state == SplitterState::Text {
            return vec![TextSegment::Text(delta.to_string())];
        }
        self.buffer.push_str(delta);
        let mut segments = Vec::new();
        self.drain(&mut segments, false);
        segments
    }

    /// Releases whatever is still held once the stream ends.
    pub(super) fn finish(&mut self) -> Vec<TextSegment> {
        let mut segments = Vec::new();
        self.drain(&mut segments, true);
        segments
    }

    fn drain(&mut self, segments: &mut Vec<TextSegment>, ended: bool) {
        loop {
            match self.state {
                SplitterState::Undecided => {
                    let leading = self.buffer.len() - self.buffer.trim_start().len();
                    let opener = OPEN_TAGS
                        .iter()
                        .find(|tag| starts_with_ignore_ascii_case(&self.buffer[leading..], tag))
                        .map(|tag| tag.len());
                    if let Some(length) = opener {
                        self.buffer.drain(..leading + length);
                        self.state = SplitterState::Thinking;
                        continue;
                    }
                    let undecided = {
                        let trimmed = &self.buffer[leading..];
                        trimmed.is_empty()
                            || OPEN_TAGS.iter().any(|tag| is_proper_prefix(trimmed, tag))
                    };
                    if !ended && undecided && self.buffer.len() <= MAX_UNDECIDED_BYTES {
                        return;
                    }
                    self.state = SplitterState::Text;
                    continue;
                }
                SplitterState::Thinking => {
                    // Whitespace between the opening tag and the reasoning is
                    // the producer's formatting, not part of the thought.
                    if !self.opened_reasoning {
                        let leading = self.buffer.len() - self.buffer.trim_start().len();
                        self.buffer.drain(..leading);
                    }
                    if let Some(close) = find_close_tag(&self.buffer) {
                        let rest = self.buffer.split_off(close.end);
                        self.buffer.truncate(close.start);
                        let reasoning = std::mem::take(&mut self.buffer);
                        push_non_empty(
                            segments,
                            TextSegment::Reasoning(reasoning.trim_end().to_string()),
                        );
                        self.buffer = rest.trim_start().to_string();
                        self.state = SplitterState::Text;
                        continue;
                    }
                    if ended {
                        let reasoning = std::mem::take(&mut self.buffer);
                        push_non_empty(
                            segments,
                            TextSegment::Reasoning(reasoning.trim_end().to_string()),
                        );
                        self.state = SplitterState::Text;
                        return;
                    }
                    let release = self.buffer.len() - partial_close_suffix(&self.buffer);
                    if release > 0 {
                        let released: String = self.buffer.drain(..release).collect();
                        push_non_empty(segments, TextSegment::Reasoning(released));
                        self.opened_reasoning = true;
                    }
                    return;
                }
                SplitterState::Text => {
                    if !self.buffer.is_empty() {
                        let text = std::mem::take(&mut self.buffer);
                        push_non_empty(segments, TextSegment::Text(text));
                    }
                    return;
                }
            }
        }
    }
}

fn push_non_empty(segments: &mut Vec<TextSegment>, segment: TextSegment) {
    let empty = match &segment {
        TextSegment::Reasoning(text) | TextSegment::Text(text) => text.is_empty(),
    };
    if !empty {
        segments.push(segment);
    }
}

struct CloseTag {
    start: usize,
    end: usize,
}

/// Locates the earliest closing tag. Either spelling closes either opener: the
/// producers that mismatch them are the same ones that inline the block at all.
fn find_close_tag(body: &str) -> Option<CloseTag> {
    let lowered = body.to_ascii_lowercase();
    CLOSE_TAGS
        .iter()
        .filter_map(|tag| {
            lowered.find(tag).map(|start| CloseTag {
                start,
                end: start + tag.len(),
            })
        })
        .min_by_key(|found| found.start)
}

fn split_at_close_tag(body: &str) -> Option<(&str, &str)> {
    let found = find_close_tag(body)?;
    Some((&body[..found.start], &body[found.end..]))
}

/// Length of the tail that could still grow into a closing tag, and so must not
/// be released yet.
fn partial_close_suffix(buffer: &str) -> usize {
    CLOSE_TAGS
        .iter()
        .flat_map(|tag| (1..tag.len()).map(move |length| (length, &tag[..length])))
        .filter(|(length, prefix)| {
            buffer.len() >= *length
                && buffer.is_char_boundary(buffer.len() - length)
                && buffer[buffer.len() - length..].eq_ignore_ascii_case(prefix)
        })
        .map(|(length, _)| length)
        .max()
        .unwrap_or(0)
}

fn starts_with_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack.len() >= needle.len()
        && haystack.as_bytes()[..needle.len()].eq_ignore_ascii_case(needle.as_bytes())
}

fn is_proper_prefix(candidate: &str, tag: &str) -> bool {
    candidate.len() < tag.len() && starts_with_ignore_ascii_case(tag, candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(deltas: &[&str]) -> Vec<TextSegment> {
        let mut splitter = InlineThinkingSplitter::default();
        let mut segments = Vec::new();
        for delta in deltas {
            segments.extend(splitter.push(delta));
        }
        segments.extend(splitter.finish());
        segments
    }

    fn reasoning(segments: &[TextSegment]) -> String {
        segments
            .iter()
            .filter_map(|segment| match segment {
                TextSegment::Reasoning(text) => Some(text.as_str()),
                TextSegment::Text(_) => None,
            })
            .collect()
    }

    fn text(segments: &[TextSegment]) -> String {
        segments
            .iter()
            .filter_map(|segment| match segment {
                TextSegment::Text(text) => Some(text.as_str()),
                TextSegment::Reasoning(_) => None,
            })
            .collect()
    }

    #[test]
    fn moves_a_leading_block_off_the_visible_text() {
        let (reasoning, text) = split_leading_thinking(
            "<thinking>\nWeighing the options.\n</thinking>\n\nUse the second one.",
        );
        assert_eq!(reasoning, Some("Weighing the options."));
        assert_eq!(text, "Use the second one.");
    }

    #[test]
    fn accepts_the_short_spelling_and_any_casing() {
        let (reasoning, text) = split_leading_thinking("<THINK>hm</Think>answer");
        assert_eq!(reasoning, Some("hm"));
        assert_eq!(text, "answer");
    }

    /// Text with no leading block has to come back byte-identical, because every
    /// turn is run through this — including the overwhelming majority that never
    /// saw a relay rewrite.
    #[test]
    fn leaves_ordinary_text_untouched() {
        let original = "  Sure — here is the patch.\n";
        let (reasoning, text) = split_leading_thinking(original);
        assert_eq!(reasoning, None);
        assert_eq!(text, original);
    }

    /// The guard that keeps this from eating answers: a turn *about* these tags
    /// quotes them, and quoting is not reasoning.
    #[test]
    fn ignores_tags_once_the_answer_has_started() {
        let original = "Strip the <thinking>…</thinking> wrapper like this.";
        let (reasoning, text) = split_leading_thinking(original);
        assert_eq!(reasoning, None);
        assert_eq!(text, original);
    }

    /// A stream cut mid-reasoning has no answer to show. Leaving the dangling
    /// open tag on the text channel is the exact symptom this module exists to
    /// remove, so the remainder goes to reasoning instead.
    #[test]
    fn treats_an_unclosed_block_as_reasoning() {
        let (reasoning, text) = split_leading_thinking("<thinking>Half a thou");
        assert_eq!(reasoning, Some("Half a thou"));
        assert_eq!(text, "");
    }

    #[test]
    fn streams_a_block_split_across_delta_boundaries() {
        let segments = stream(&[
            "<thin",
            "king>Look",
            "ing it up.</thin",
            "king>The answer is 4.",
        ]);
        assert_eq!(reasoning(&segments), "Looking it up.");
        assert_eq!(text(&segments), "The answer is 4.");
    }

    /// Nothing may be held back once the turn is known to be plain prose: a
    /// stalled first token is visible to the user as a hung request.
    #[test]
    fn releases_plain_text_on_the_first_delta() {
        let mut splitter = InlineThinkingSplitter::default();
        assert_eq!(
            splitter.push("Here is the plan."),
            vec![TextSegment::Text("Here is the plan.".to_string())]
        );
    }

    /// A `<` that could still become a closing tag must not be released as
    /// reasoning, or the tag leaks into the summary one character at a time.
    #[test]
    fn holds_back_a_partial_closing_tag() {
        let mut splitter = InlineThinkingSplitter::default();
        splitter.push("<thinking>done");
        assert!(
            splitter.push("</thin").is_empty(),
            "a tail that could still close the block must stay buffered"
        );
        let segments = splitter.push("king>answer");
        assert_eq!(reasoning(&segments), "");
        assert_eq!(text(&segments), "answer");
    }

    #[test]
    fn flushes_an_unclosed_stream_as_reasoning() {
        let segments = stream(&["<think>", "still going"]);
        assert_eq!(reasoning(&segments), "still going");
        assert_eq!(text(&segments), "");
    }

    /// Leading whitespace is the producer's formatting. It must not decide the
    /// question on its own, and it must not stall the stream forever either.
    #[test]
    fn tolerates_whitespace_before_the_tag_without_stalling_on_it() {
        let segments = stream(&["\n\n", "<think>a</think>b"]);
        assert_eq!(reasoning(&segments), "a");
        assert_eq!(text(&segments), "b");

        let long_blank = " ".repeat(MAX_UNDECIDED_BYTES + 8);
        let segments = stream(&[&long_blank, "plain"]);
        assert_eq!(reasoning(&segments), "");
        assert_eq!(text(&segments), format!("{long_blank}plain"));
    }

    /// Multi-byte text lands on the byte-level scans; a naive slice would panic
    /// rather than merely mis-detect.
    #[test]
    fn handles_multi_byte_text() {
        let segments = stream(&["<thinking>先查一下。</thinking>", "答案是四。"]);
        assert_eq!(reasoning(&segments), "先查一下。");
        assert_eq!(text(&segments), "答案是四。");

        let segments = stream(&["中文开头，没有标签。"]);
        assert_eq!(text(&segments), "中文开头，没有标签。");
    }
}
