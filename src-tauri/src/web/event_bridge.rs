use serde::{Serialize, Serializer};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
pub struct WebEvent {
    pub channel: String,
    pub payload: Arc<serde_json::Value>,
}

impl Serialize for WebEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("WebEvent", 2)?;
        state.serialize_field("channel", &self.channel)?;
        state.serialize_field("payload", self.payload.as_ref())?;
        state.end()
    }
}

pub struct WebEventBroadcaster {
    sender: broadcast::Sender<WebEvent>,
}

impl Default for WebEventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl WebEventBroadcaster {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(4096);
        Self { sender }
    }

    pub fn send(&self, channel: &str, payload: &impl Serialize) {
        let Ok(value) = serde_json::to_value(payload) else {
            return;
        };

        if self.sender.receiver_count() == 0 {
            return;
        }

        let _ = self.sender.send(WebEvent {
            channel: channel.to_string(),
            payload: Arc::new(value),
        });
    }

    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<WebEvent> {
        self.sender.subscribe()
    }
}

#[derive(Clone)]
pub enum EventEmitter {
    Tauri(tauri::AppHandle),
    #[allow(dead_code)]
    Web(Arc<WebEventBroadcaster>),
    #[allow(dead_code)]
    Noop,
}

impl EventEmitter {
    pub fn emit(&self, channel: &str, payload: &impl Serialize) {
        match self {
            EventEmitter::Tauri(app) => {
                let _ = app.emit(channel, payload);
            }
            EventEmitter::Web(broadcaster) => {
                broadcaster.send(channel, payload);
            }
            EventEmitter::Noop => {}
        }
    }
}
