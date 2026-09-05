use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticResponseFailure {
    pub code: Option<String>,
    /// The upstream's `error.type`. Gateways that omit `error.code` often still
    /// name the error family here, which is the only way to tell their own
    /// errors apart from the ones they relay.
    pub error_type: Option<String>,
    pub message: String,
}

/// Returns whether a semantic response failure is a temporary upstream
/// capacity issue rather than an account-specific failure.
pub fn is_transient_response_failure(message: &str) -> bool {
    message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
        .contains("our servers are currently overloaded")
}

/// The `error.type` new-api style gateways stamp on their own errors, as
/// opposed to the ones they relay from a real upstream.
const NEW_API_ERROR_TYPE: &str = "new_api_error";
/// How those gateways open the message once the account's balance is spent.
const NEW_API_INSUFFICIENT_BALANCE_PREFIX: &str = "用户额度不足";

/// Returns whether a new-api gateway is reporting a spent account balance.
///
/// These bodies carry no `error.code` at all — only `error.type` — so the
/// code-based rules cannot see them, and the message is too specific to match
/// on alone (the same gateway relays upstream 用户额度不足 text for other
/// accounts). Requiring both keeps the rule narrow. By the time this arrives
/// the remaining balance is already negative, which makes it every bit as
/// deterministic as a quota reset boundary.
fn is_new_api_insufficient_balance(failure: &SemanticResponseFailure) -> bool {
    failure
        .error_type
        .as_deref()
        .is_some_and(|error_type| error_type.trim().eq_ignore_ascii_case(NEW_API_ERROR_TYPE))
        && failure
            .message
            .trim_start()
            .starts_with(NEW_API_INSUFFICIENT_BALANCE_PREFIX)
}

/// Returns whether an upstream error is a definitive quota exhaustion signal.
/// These errors should mark the account as abnormal immediately instead of
/// spending the configured retry budget.
pub fn is_quota_exhaustion_failure(failure: &SemanticResponseFailure) -> bool {
    if is_new_api_insufficient_balance(failure) {
        return true;
    }

    let code = failure
        .code
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .replace(['_', '-', ':', ' '], "");
    if code.contains("insufficientquota")
        || code.contains("quotaexhausted")
        || code.contains("usageexhausted")
    {
        return true;
    }

    let message = failure.message.to_ascii_lowercase();
    let compact_message = message.split_whitespace().collect::<String>();
    compact_message.contains("额度已耗尽")
        || compact_message.contains("额度耗尽")
        || compact_message.contains("额度已用完")
        || compact_message.contains("配额已耗尽")
        || compact_message.contains("配额耗尽")
        || compact_message.contains("配额已用完")
        || message.contains("quota exhausted")
        || message.contains("quota has been exhausted")
        || message.contains("insufficient quota")
        || message.contains("used all the included free usage")
        || message.contains("free usage exhausted")
}

/// Returns whether an upstream rejected a `thinking` block that the client
/// replayed from an earlier turn.
///
/// A thinking block's `signature` only verifies against the upstream that minted
/// it. Claude Code replays assistant thinking blocks in every later turn, so a
/// pooled conversation breaks as soon as one turn lands on a different account
/// than the turn that produced the reasoning — and a relay that re-signs into
/// another platform's API (Bedrock validates strictly) rejects it outright.
/// Neither the credential nor the model is at fault, and no other account in the
/// pool can accept the same history, so this must not be charged to the account.
///
/// The rule needs both nouns plus an invalidity marker because relays word it
/// differently and wrap it in their own prose, which rules out matching one
/// phrase. Requiring `thinking` keeps it clear of request-signing and TLS errors,
/// which also say "signature". A *missing* signature lands here too: same cause,
/// same recovery.
pub fn is_thinking_signature_failure(text: &str) -> bool {
    // Upstreams quote the field names (`` `signature` ``), so drop the backticks
    // before matching rather than spelling both forms.
    let normalized = text.to_lowercase().replace('`', " ");
    let compact = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let names_a_replayed_block = compact.contains("thinking") && compact.contains("signature");
    let rejected = compact.contains("invalid")
        || compact.contains("无效")
        || compact.contains("verification failed")
        || compact.contains("does not match")
        || compact.contains("is required");
    names_a_replayed_block && rejected
}

/// Detects a stream that delivered data but never emitted a terminal marker.
/// A partial Responses stream is commonly surfaced to the caller as
/// `stream disconnected before completion`.
pub fn stream_disconnected_before_completion(
    body: &[u8],
    content_type: Option<&str>,
    streaming_request: bool,
) -> bool {
    if !streaming_request {
        return false;
    }
    let text = String::from_utf8_lossy(body);
    let looks_like_sse = content_type
        .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
        || text.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("data:") || line.starts_with("event:")
        });
    if !looks_like_sse {
        return false;
    }
    if text.contains("response.completed")
        || text.contains("data: [DONE]")
        || text.contains("message_stop")
        || text.contains("\"finish_reason\":\"stop\"")
        || text.contains("\"finishReason\":\"STOP\"")
    {
        return false;
    }
    text.lines().any(|line| {
        let line = line.trim_start();
        line.strip_prefix("data:")
            .map(str::trim)
            .is_some_and(|data| !data.is_empty() && data != "[DONE]")
    })
}

pub const STREAM_DISCONNECTED_FAILURE_MESSAGE: &str =
    "stream disconnected before completion: stream closed before response.completed";

pub fn detect_response_failed(body: &[u8]) -> Option<SemanticResponseFailure> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(failure) = detect_value(&value) {
            return Some(failure);
        }
    }
    let text = std::str::from_utf8(body).ok()?;
    for line in text.lines() {
        let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            if let Some(failure) = detect_value(&value) {
                return Some(failure);
            }
        }
    }
    let normalized = text.trim();
    let lower = normalized.to_ascii_lowercase();
    let is_known_error_text = lower.contains("stream disconnected before completion")
        || lower.contains("gateway timeout")
        || lower.contains("bad gateway")
        || lower.contains("service unavailable")
        || lower.contains("connection reset")
        || lower.contains("temporarily unavailable")
        || lower.contains("our servers are currently overloaded")
        || lower.contains("insufficient_quota")
        || lower.contains("quota exhausted")
        || normalized.contains("额度已耗尽")
        || normalized.contains("额度耗尽")
        || normalized.contains("配额已耗尽");
    if is_known_error_text && !normalized.is_empty() {
        return Some(SemanticResponseFailure {
            code: None,
            error_type: None,
            message: normalized.chars().take(512).collect(),
        });
    }
    None
}

fn detect_value(value: &Value) -> Option<SemanticResponseFailure> {
    let failed = value.get("type").and_then(Value::as_str) == Some("response.failed")
        || value.pointer("/response/status").and_then(Value::as_str) == Some("failed")
        || value.pointer("/status").and_then(Value::as_str) == Some("failed");
    let has_error = value
        .pointer("/response/error")
        .or_else(|| value.pointer("/error"))
        .is_some_and(|error| {
            error.is_object()
                && (error.get("message").and_then(Value::as_str).is_some()
                    || error.get("code").and_then(Value::as_str).is_some())
        })
        || (value.get("code").and_then(Value::as_str).is_some()
            && value.get("message").and_then(Value::as_str).is_some());
    if !failed && !has_error {
        return None;
    }
    let code = value
        .pointer("/response/error/code")
        .or_else(|| value.pointer("/error/code"))
        .or_else(|| value.get("code"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(str::to_string);
    // Only the error object's own `type` — a top-level one names the envelope
    // (`error`, `response.failed`), not the error family.
    let error_type = value
        .pointer("/response/error/type")
        .or_else(|| value.pointer("/error/type"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|error_type| !error_type.is_empty())
        .map(str::to_string);
    let message = value
        .pointer("/response/error/message")
        .or_else(|| value.pointer("/error/message"))
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .unwrap_or("Upstream response reported failure")
        .chars()
        .take(512)
        .collect();
    Some(SemanticResponseFailure {
        code,
        error_type,
        message,
    })
}

/// Upstream `error.code` values that mean the account should be auto-paused
/// (暂停): the gateway hit a hard, time-windowed quota that won't recover by
/// retrying the same account this window, so we stop routing to it.
pub const AUTO_PAUSE_ERROR_CODES: &[&str] = &["global_fixed_window_quota_exhausted"];

/// Returns the matched upstream error code when the response body carries one of
/// [`AUTO_PAUSE_ERROR_CODES`], inspecting both plain JSON and SSE `data:` frames.
pub fn detect_auto_pause_code(body: &[u8]) -> Option<String> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(code) = detect_pause_code_value(&value) {
            return Some(code);
        }
    }
    let text = std::str::from_utf8(body).ok()?;
    for line in text.lines() {
        let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            if let Some(code) = detect_pause_code_value(&value) {
                return Some(code);
            }
        }
    }
    None
}

fn detect_pause_code_value(value: &Value) -> Option<String> {
    let code = value
        .pointer("/error/code")
        .or_else(|| value.pointer("/code"))
        .and_then(Value::as_str)?;
    if AUTO_PAUSE_ERROR_CODES.contains(&code) {
        Some(code.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_response_failed_json_and_nested_status() {
        let body = br#"{"type":"response.failed","response":{"status":"failed","error":{"message":"maintenance"}}}"#;
        assert_eq!(detect_response_failed(body).unwrap().message, "maintenance");
        assert!(detect_response_failed(br#"{"response":{"status":"failed"}}"#).is_some());
    }

    #[test]
    fn detects_sse_and_ignores_done_and_success() {
        let body = br#": keepalive
data: [DONE]
data: {"type":"response.failed","error":{"message":"down"}}
"#;
        assert_eq!(detect_response_failed(body).unwrap().message, "down");
        assert!(detect_response_failed(br#"{"type":"response.completed"}"#).is_none());
    }

    #[test]
    fn identifies_overloaded_server_response_as_transient() {
        assert!(is_transient_response_failure(
            "Our servers are currently overloaded. Please try again later."
        ));
        assert!(is_transient_response_failure(
            " our   servers are currently overloaded "
        ));
        assert!(!is_transient_response_failure("invalid model"));
    }

    #[test]
    fn detects_generic_error_messages_and_quota_exhaustion() {
        let failure = detect_response_failed(
            r#"{"error":{"code":"insufficient_quota","message":"当日订阅额度已耗尽，请重置或购买新订阅。"}}"#.as_bytes(),
        )
        .expect("error message");
        assert_eq!(failure.code.as_deref(), Some("insufficient_quota"));
        assert!(is_quota_exhaustion_failure(&failure));

        let failure = detect_response_failed(
            br#"{"error":{"message":"stream disconnected before completion"}}"#,
        )
        .expect("generic error message");
        assert!(!is_quota_exhaustion_failure(&failure));

        let failure = detect_response_failed(
            "stream disconnected before completion: stream closed before response.completed"
                .as_bytes(),
        )
        .expect("plain text stream error");
        assert_eq!(failure.message, STREAM_DISCONNECTED_FAILURE_MESSAGE);
    }

    #[test]
    fn treats_new_api_insufficient_user_balance_as_quota_exhaustion() {
        let failure = detect_response_failed(
            r#"{"error":{"type":"new_api_error","message":"用户额度不足, 剩余额度: ＄-0.398052 (request id: 202609020218166141364498268d9d6A3V7Qkt0)"},"type":"error"}"#
                .as_bytes(),
        )
        .expect("semantic failure");
        assert!(is_quota_exhaustion_failure(&failure));
    }

    /// A relay whose shared budget pool ran dry answers with this body, and the
    /// generic `quota has been exhausted` message rule already catches it. The
    /// account list explains the difference to the user in a hover hint, and that
    /// copy only makes sense on an 异常 account — so pin the classification here.
    #[test]
    fn treats_an_exhausted_shared_budget_pool_as_quota_exhaustion() {
        let failure = detect_response_failed(
            r#"{"error":{"message":"Budget pool quota has been exhausted. Please ask an administrator to increase the limit or select another budget pool.","type":"bad_response_status_code","param":"","code":"bad_response_status_code"}}"#
                .as_bytes(),
        )
        .expect("semantic failure");
        assert!(is_quota_exhaustion_failure(&failure));
    }

    #[test]
    fn new_api_quota_rule_requires_the_error_type_and_the_message_prefix() {
        let other_new_api_error = detect_response_failed(
            r#"{"error":{"type":"new_api_error","message":"当前分组 default 下对于模型 gpt-5.5 无可用渠道"}}"#
                .as_bytes(),
        )
        .expect("semantic failure");
        assert!(!is_quota_exhaustion_failure(&other_new_api_error));

        let other_error_type = detect_response_failed(
            r#"{"error":{"type":"invalid_request_error","message":"用户额度不足, 剩余额度: ＄-0.398052"}}"#
                .as_bytes(),
        )
        .expect("semantic failure");
        assert!(!is_quota_exhaustion_failure(&other_error_type));

        let prefix_only_in_the_middle = detect_response_failed(
            r#"{"error":{"type":"new_api_error","message":"上游渠道报错：用户额度不足"}}"#
                .as_bytes(),
        )
        .expect("semantic failure");
        assert!(!is_quota_exhaustion_failure(&prefix_only_in_the_middle));
    }

    /// The body a Bedrock-backed relay returns once Claude Code replays a
    /// `thinking` block that a *different* upstream produced. Its `error.type` is
    /// the literal string `<nil>` — the gateway formatted a Go nil into JSON — so
    /// the type-based rules cannot see it and the message is all there is.
    #[test]
    fn detects_a_replayed_thinking_signature_rejection() {
        let failure = detect_response_failed(
            r#"{"error":{"message":"Anthropic Claude bad request: InvokeModelWithResponseStream: operation error Bedrock Runtime: InvokeModelWithResponseStream, https response error StatusCode: 400, , ValidationException: messages.5.content.0: Invalid `signature` in `thinking` block (request id: cf669ff644885c3d88c0febf9d08bc37)","type":"<nil>"},"type":"error"}"#
                .as_bytes(),
        )
        .expect("semantic failure");
        assert!(is_thinking_signature_failure(&failure.message));
        // Not the account's fault, so it must not read as spent quota.
        assert!(!is_quota_exhaustion_failure(&failure));
    }

    /// Relays word this differently and wrap it in their own prose, so the rule
    /// matches on the two nouns plus an invalidity marker rather than one phrase.
    #[test]
    fn accepts_other_wordings_of_the_same_rejection() {
        for message in [
            "Invalid signature for thinking block",
            "messages.3.content.0.thinking.signature: signature verification failed",
            "上游报错：thinking 块的 signature 无效",
        ] {
            assert!(
                is_thinking_signature_failure(message),
                "must match: {message}"
            );
        }
    }

    /// `signature` alone belongs to request signing and TLS; `thinking` alone
    /// belongs to every reasoning-parameter complaint. Only together do they name
    /// a replayed reasoning block.
    #[test]
    fn thinking_signature_rule_needs_both_nouns_and_an_invalidity_marker() {
        for message in [
            "SignatureDoesNotMatch: The request signature we calculated does not match the signature you provided",
            "thinking.budget_tokens must be less than max_tokens",
            "invalid_request_error: model not found",
            "thinking block signature accepted",
        ] {
            assert!(
                !is_thinking_signature_failure(message),
                "must not match: {message}"
            );
        }
    }

    #[test]
    fn detects_incomplete_streams_without_terminal_markers() {
        assert!(stream_disconnected_before_completion(
            b"event: response.output_text.delta\ndata: {\"delta\":\"hello\"}\n\n",
            Some("text/event-stream"),
            true,
        ));
        assert!(!stream_disconnected_before_completion(
            b"data: {\"delta\":\"hello\"}\n\ndata: [DONE]\n\n",
            Some("text/event-stream"),
            true,
        ));
        assert!(!stream_disconnected_before_completion(
            br#"{"error":{"message":"stream disconnected"}}"#,
            Some("application/json"),
            true,
        ));
    }

    #[test]
    fn detects_auto_pause_error_code_in_json() {
        let body = r#"{"error":{"message":"本时段全站额度已用完","type":"rate_limit_error","code":"global_fixed_window_quota_exhausted"}}"#;
        assert_eq!(
            detect_auto_pause_code(body.as_bytes()).as_deref(),
            Some("global_fixed_window_quota_exhausted")
        );
    }

    #[test]
    fn detects_auto_pause_error_code_in_sse_frames() {
        let body = br#": keepalive
data: {"error":{"code":"global_fixed_window_quota_exhausted"}}
"#;
        assert_eq!(
            detect_auto_pause_code(body).as_deref(),
            Some("global_fixed_window_quota_exhausted")
        );
    }

    #[test]
    fn ignores_unrelated_error_codes() {
        assert!(detect_auto_pause_code(br#"{"error":{"code":"rate_limit_error"}}"#).is_none());
        assert!(detect_auto_pause_code(br#"{"error":{"message":"boom"}}"#).is_none());
        assert!(detect_auto_pause_code(br#"{}"#).is_none());
    }
}
