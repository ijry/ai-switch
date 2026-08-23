//! In-memory live request log for the route proxy.
//!
//! Captures the four stages of every proxied request (inbound client request,
//! transformed upstream request, raw upstream response, final client response)
//! so the UI can tail them live for troubleshooting — especially protocol
//! conversions where the four stages differ.
//!
//! This is ephemeral: a bounded ring buffer in memory, never persisted. Live
//! updates are pushed through the shared [`EventEmitter`] (Tauri `emit` on the
//! desktop, WebSocket broadcast on the headless server) only while at least one
//! viewer is subscribed.

use crate::web::event_bridge::EventEmitter;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const ROUTE_PROXY_LIVE_LOG_EVENT: &str = "route-proxy-live-log";

/// Newest N entries kept in memory (per whole proxy, not per platform).
const LIVE_LOG_CAPACITY: usize = 100;
/// Max bytes retained per stage before truncation.
pub(crate) const LIVE_LOG_STAGE_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteProxyLiveLogEntry {
    pub id: String,
    pub trace_id: Option<String>,
    pub platform: String,
    pub credential_id: String,
    pub credential_name: String,
    pub attempt: usize,
    pub path: String,
    pub target_url: Option<String>,
    /// Headers actually sent upstream, one `name: value` per line, sorted.
    ///
    /// Credential-bearing values are masked. The identity headers the proxy
    /// injects to look like an official CLI stay visible — they are what a
    /// fingerprinting gateway accepts or rejects on, so this is the field that
    /// makes an `unauthorized client detected` diagnosable. Defaulted so older
    /// payloads stay compatible.
    #[serde(default)]
    pub upstream_headers: Option<String>,
    pub requested_model: Option<String>,
    pub upstream_model: Option<String>,
    pub status: Option<u16>,
    pub success: bool,
    pub error_message: Option<String>,
    pub duration_ms: i64,
    /// Protocol bridge kind applied (e.g. codex responses->chat); None when the
    /// request was forwarded without conversion.
    pub bridge: Option<String>,
    pub client_request: Option<String>,
    pub upstream_request: Option<String>,
    pub upstream_response: Option<String>,
    pub final_response: Option<String>,
    /// Non-error diagnostics surfaced for troubleshooting, e.g. a bridged
    /// upstream that completed a turn without emitting any tool call. Defaulted
    /// so older payloads/consumers stay compatible.
    #[serde(default)]
    pub notes: Vec<String>,
    pub truncated: bool,
    pub created_at: String,
}

/// Truncate a captured body to the per-stage limit and lossily decode to text.
/// Returns `(text, truncated)`. `None` inputs pass through as `(None, false)`.
pub fn stage_preview(body: Option<&[u8]>) -> (Option<String>, bool) {
    let Some(body) = body else {
        return (None, false);
    };
    if body.is_empty() {
        return (None, false);
    }
    let truncated = body.len() > LIVE_LOG_STAGE_LIMIT;
    let slice = &body[..body.len().min(LIVE_LOG_STAGE_LIMIT)];
    (Some(String::from_utf8_lossy(slice).to_string()), truncated)
}

#[derive(Default)]
struct LiveLogState {
    buffer: VecDeque<RouteProxyLiveLogEntry>,
    subscribers: usize,
    emitter: Option<EventEmitter>,
}

#[derive(Clone, Default)]
pub struct RouteProxyLiveLog {
    inner: Arc<Mutex<LiveLogState>>,
}

impl RouteProxyLiveLog {
    pub fn set_emitter(&self, emitter: EventEmitter) {
        let mut state = self.inner.lock().expect("route proxy live log lock");
        state.emitter = Some(emitter);
    }

    /// Push an entry into the ring buffer, and emit it live when someone is
    /// watching. Recording always happens so a freshly opened viewer sees
    /// recent history; emission is gated to avoid pushing to idle webviews.
    pub fn record(&self, entry: RouteProxyLiveLogEntry) {
        let emitter = {
            let mut state = self.inner.lock().expect("route proxy live log lock");
            if state.buffer.len() >= LIVE_LOG_CAPACITY {
                state.buffer.pop_front();
            }
            state.buffer.push_back(entry.clone());
            if state.subscribers > 0 {
                state.emitter.clone()
            } else {
                None
            }
        };
        if let Some(emitter) = emitter {
            emitter.emit(ROUTE_PROXY_LIVE_LOG_EVENT, &entry);
        }
    }

    /// Register a viewer and return the current history for its platform
    /// (oldest first).
    pub fn subscribe(&self, platform: &str) -> Vec<RouteProxyLiveLogEntry> {
        let mut state = self.inner.lock().expect("route proxy live log lock");
        state.subscribers += 1;
        state
            .buffer
            .iter()
            .filter(|entry| entry.platform == platform)
            .cloned()
            .collect()
    }

    pub fn unsubscribe(&self) {
        let mut state = self.inner.lock().expect("route proxy live log lock");
        state.subscribers = state.subscribers.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::event_bridge::WebEventBroadcaster;

    fn entry(platform: &str, id: &str) -> RouteProxyLiveLogEntry {
        RouteProxyLiveLogEntry {
            id: id.to_string(),
            trace_id: None,
            platform: platform.to_string(),
            credential_id: "cred".to_string(),
            credential_name: "Cred".to_string(),
            attempt: 0,
            path: "/v1/messages".to_string(),
            target_url: None,
            upstream_headers: None,
            requested_model: None,
            upstream_model: None,
            status: Some(200),
            success: true,
            error_message: None,
            duration_ms: 1,
            bridge: None,
            client_request: Some("in".to_string()),
            upstream_request: Some("up".to_string()),
            upstream_response: Some("raw".to_string()),
            final_response: Some("final".to_string()),
            notes: Vec::new(),
            truncated: false,
            created_at: "now".to_string(),
        }
    }

    #[test]
    fn ring_buffer_drops_oldest_beyond_capacity() {
        let log = RouteProxyLiveLog::default();
        for index in 0..(LIVE_LOG_CAPACITY + 5) {
            log.record(entry("codex", &format!("e{index}")));
        }
        let snapshot = log.subscribe("codex");
        assert_eq!(snapshot.len(), LIVE_LOG_CAPACITY);
        assert_eq!(snapshot.first().map(|e| e.id.as_str()), Some("e5"));
        assert_eq!(
            snapshot.last().map(|e| e.id.as_str()),
            Some(format!("e{}", LIVE_LOG_CAPACITY + 4)).as_deref()
        );
    }

    #[test]
    fn snapshot_filters_by_platform() {
        let log = RouteProxyLiveLog::default();
        log.record(entry("codex", "a"));
        log.record(entry("claude", "b"));
        let codex = log.subscribe("codex");
        assert_eq!(codex.len(), 1);
        assert_eq!(codex[0].id, "a");
    }

    #[test]
    fn emits_only_when_subscribed() {
        let broadcaster = Arc::new(WebEventBroadcaster::new());
        let mut receiver = broadcaster.subscribe();
        let log = RouteProxyLiveLog::default();
        log.set_emitter(EventEmitter::Web(broadcaster));

        // No subscriber yet: recorded but not emitted.
        log.record(entry("codex", "silent"));
        assert!(receiver.try_recv().is_err());

        // Subscribing returns history and enables live emission.
        let history = log.subscribe("codex");
        assert_eq!(history.len(), 1);
        log.record(entry("codex", "live"));
        let event = receiver.try_recv().expect("live event");
        assert_eq!(event.channel, ROUTE_PROXY_LIVE_LOG_EVENT);
        assert_eq!(event.payload["id"], "live");

        // After the last viewer leaves, emission stops again.
        log.unsubscribe();
        log.record(entry("codex", "silent-again"));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn stage_preview_truncates_and_flags() {
        let big = vec![b'x'; LIVE_LOG_STAGE_LIMIT + 10];
        let (text, truncated) = stage_preview(Some(&big));
        assert_eq!(text.map(|t| t.len()), Some(LIVE_LOG_STAGE_LIMIT));
        assert!(truncated);
        assert_eq!(stage_preview(Some(b"")), (None, false));
        assert_eq!(stage_preview(None), (None, false));
    }
}
