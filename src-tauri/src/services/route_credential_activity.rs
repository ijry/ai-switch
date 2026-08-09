use crate::web::event_bridge::EventEmitter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const ROUTE_CREDENTIAL_ACTIVITY_EVENT: &str = "route-credential-activity";
pub const ROUTE_CREDENTIAL_STATUS_EVENT: &str = "route-credential-status";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialActivityEvent {
    pub platform: String,
    pub credential_id: String,
    pub active_request_count: i64,
    pub max_concurrency: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialStatusEvent {
    pub platform: String,
    pub credential_id: String,
}

#[derive(Clone, Default)]
pub struct RouteCredentialActivityRegistry {
    inner: Arc<Mutex<ActivityRegistryState>>,
}

#[derive(Default)]
struct ActivityRegistryState {
    accounts: HashMap<String, ActivityState>,
    emitter: Option<EventEmitter>,
}

struct ActivityState {
    platform: String,
    active_request_count: i64,
    max_concurrency: i64,
}

impl RouteCredentialActivityRegistry {
    pub fn set_emitter(&self, emitter: EventEmitter) {
        let mut state = self.inner.lock().expect("route credential activity lock");
        state.emitter = Some(emitter);
    }

    pub async fn try_acquire(
        &self,
        platform: &str,
        credential_id: &str,
        max_concurrency: i64,
    ) -> Option<RouteCredentialActivityLease> {
        if max_concurrency < 1 {
            return None;
        }

        let event = {
            let mut state = self.inner.lock().expect("route credential activity lock");
            let account = state
                .accounts
                .entry(credential_id.to_string())
                .or_insert_with(|| ActivityState {
                    platform: platform.to_string(),
                    active_request_count: 0,
                    max_concurrency,
                });
            account.platform = platform.to_string();
            account.max_concurrency = max_concurrency;
            if account.active_request_count >= max_concurrency {
                return None;
            }
            account.active_request_count += 1;
            activity_event(credential_id, account)
        };
        emit_activity_event(&self.inner, event);

        Some(RouteCredentialActivityLease {
            registry: self.clone(),
            credential_id: credential_id.to_string(),
        })
    }

    pub fn snapshot(&self, credential_id: &str) -> i64 {
        self.inner
            .lock()
            .expect("route credential activity lock")
            .accounts
            .get(credential_id)
            .map(|state| state.active_request_count)
            .unwrap_or(0)
    }

    pub fn notify_status_change(&self, platform: &str, credential_id: &str) {
        let emitter = self
            .inner
            .lock()
            .expect("route credential activity lock")
            .emitter
            .clone();
        if let Some(emitter) = emitter {
            emitter.emit(
                ROUTE_CREDENTIAL_STATUS_EVENT,
                &RouteCredentialStatusEvent {
                    platform: platform.to_string(),
                    credential_id: credential_id.to_string(),
                },
            );
        }
    }

    fn release(&self, credential_id: &str) {
        let event = {
            let mut state = self.inner.lock().expect("route credential activity lock");
            let Some(account) = state.accounts.get_mut(credential_id) else {
                return;
            };
            account.active_request_count = account.active_request_count.saturating_sub(1);
            let event = activity_event(credential_id, account);
            if account.active_request_count == 0 {
                state.accounts.remove(credential_id);
            }
            event
        };
        emit_activity_event(&self.inner, event);
    }
}

pub struct RouteCredentialActivityLease {
    registry: RouteCredentialActivityRegistry,
    credential_id: String,
}

impl Drop for RouteCredentialActivityLease {
    fn drop(&mut self) {
        self.registry.release(&self.credential_id);
    }
}

fn activity_event(credential_id: &str, state: &ActivityState) -> RouteCredentialActivityEvent {
    RouteCredentialActivityEvent {
        platform: state.platform.clone(),
        credential_id: credential_id.to_string(),
        active_request_count: state.active_request_count,
        max_concurrency: state.max_concurrency,
    }
}

fn emit_activity_event(
    registry: &Arc<Mutex<ActivityRegistryState>>,
    event: RouteCredentialActivityEvent,
) {
    let emitter = registry
        .lock()
        .expect("route credential activity lock")
        .emitter
        .clone();
    if let Some(emitter) = emitter {
        emitter.emit(ROUTE_CREDENTIAL_ACTIVITY_EVENT, &event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::event_bridge::WebEventBroadcaster;
    use std::sync::Arc;

    #[tokio::test]
    async fn enforces_concurrency_and_releases_on_drop() {
        let registry = RouteCredentialActivityRegistry::default();
        let first = registry.try_acquire("codex", "credential-a", 1).await;
        assert!(first.is_some());
        assert!(registry
            .try_acquire("codex", "credential-a", 1)
            .await
            .is_none());
        assert_eq!(registry.snapshot("credential-a"), 1);

        drop(first);
        assert_eq!(registry.snapshot("credential-a"), 0);
        assert!(registry
            .try_acquire("codex", "credential-a", 1)
            .await
            .is_some());
    }

    #[tokio::test]
    async fn accounts_are_independent_and_lowered_limits_are_respected() {
        let registry = RouteCredentialActivityRegistry::default();
        let first = registry
            .try_acquire("codex", "credential-a", 2)
            .await
            .expect("first lease");
        let second = registry
            .try_acquire("codex", "credential-a", 2)
            .await
            .expect("second lease");
        assert!(registry
            .try_acquire("codex", "credential-a", 1)
            .await
            .is_none());
        assert!(registry
            .try_acquire("codex", "credential-b", 1)
            .await
            .is_some());
        assert_eq!(registry.snapshot("credential-a"), 2);

        drop(first);
        assert_eq!(registry.snapshot("credential-a"), 1);
        assert!(registry
            .try_acquire("codex", "credential-a", 1)
            .await
            .is_none());
        drop(second);
        assert_eq!(registry.snapshot("credential-a"), 0);
    }

    #[test]
    fn emits_status_change_events() {
        let broadcaster = Arc::new(WebEventBroadcaster::new());
        let mut receiver = broadcaster.subscribe();
        let registry = RouteCredentialActivityRegistry::default();

        registry.set_emitter(EventEmitter::Web(broadcaster));
        registry.notify_status_change("codex", "credential-a");

        let event = receiver.try_recv().expect("status event");
        assert_eq!(event.channel, ROUTE_CREDENTIAL_STATUS_EVENT);
        assert_eq!(event.payload["platform"], "codex");
        assert_eq!(event.payload["credential_id"], "credential-a");
    }
}
