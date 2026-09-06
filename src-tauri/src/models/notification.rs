use serde::{Deserialize, Serialize};

/// Which notification channel to deliver through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    Feishu,
    Bark,
    Webhook,
}

impl std::fmt::Display for NotificationChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationChannel::Feishu => write!(f, "feishu"),
            NotificationChannel::Bark => write!(f, "bark"),
            NotificationChannel::Webhook => write!(f, "webhook"),
        }
    }
}

/// Configuration for one notification channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationChannelConfig {
    pub enabled: bool,
    #[serde(flatten)]
    pub kind: NotificationChannelKind,
}

/// Per-channel payload, flattened into NotificationChannelConfig.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum NotificationChannelKind {
    #[serde(rename = "feishu")]
    Feishu {
        /// Feishu bot webhook URL (e.g. https://open.feishu.cn/open-apis/bot/v2/hook/xxx)
        webhook_url: String,
    },
    #[serde(rename = "bark")]
    Bark {
        /// Bark server URL (e.g. https://api.day.app)
        server_url: String,
        /// Device key
        device_key: String,
    },
    #[serde(rename = "webhook")]
    Webhook {
        /// Arbitrary webhook endpoint URL
        url: String,
    },
}

/// Which event types to notify on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationEventFilter {
    #[serde(default = "default_true")]
    pub account_error: bool,
    #[serde(default = "default_true")]
    pub account_anomaly: bool,
    #[serde(default = "default_true")]
    pub health_check: bool,
}

fn default_true() -> bool {
    true
}

impl Default for NotificationEventFilter {
    fn default() -> Self {
        Self {
            account_error: true,
            account_anomaly: true,
            health_check: true,
        }
    }
}

/// Full notification configuration, stored as JSON string in AppSettings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationConfig {
    /// Master switch — all channels disabled when false.
    #[serde(default)]
    pub enabled: bool,
    /// Configured channels.
    #[serde(default)]
    pub channels: Vec<NotificationChannelConfig>,
    /// Which event types to deliver.
    #[serde(default)]
    pub events: NotificationEventFilter,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channels: Vec::new(),
            events: NotificationEventFilter::default(),
        }
    }
}

impl NotificationConfig {
    /// Parse from the opaque JSON string stored in settings.
    pub fn from_json(json: Option<&str>) -> Self {
        json.and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Serialize to the opaque JSON string stored in settings.
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    /// Returns enabled channels whose kind matches the given channel type.
    pub fn enabled_channels(&self, channel: NotificationChannel) -> Vec<&NotificationChannelConfig> {
        self.channels
            .iter()
            .filter(|c| c.enabled && match (&c.kind, channel) {
                (NotificationChannelKind::Feishu { .. }, NotificationChannel::Feishu) => true,
                (NotificationChannelKind::Bark { .. }, NotificationChannel::Bark) => true,
                (NotificationChannelKind::Webhook { .. }, NotificationChannel::Webhook) => true,
                _ => false,
            })
            .collect()
    }
}

/// A notification event to be sent through all matching channels.
#[derive(Debug, Clone)]
pub struct NotificationEvent {
    pub title: String,
    pub body: String,
    /// Optional structured fields shown as key-value pairs.
    pub fields: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled_with_no_channels() {
        let config = NotificationConfig::default();
        assert!(!config.enabled);
        assert!(config.channels.is_empty());
        assert!(config.events.account_error);
    }

    #[test]
    fn round_trips_through_json() {
        let config = NotificationConfig {
            enabled: true,
            channels: vec![NotificationChannelConfig {
                enabled: true,
                kind: NotificationChannelKind::Bark {
                    server_url: "https://api.day.app".into(),
                    device_key: "abc123".into(),
                },
            }],
            events: NotificationEventFilter::default(),
        };
        let json = config.to_json().unwrap();
        let parsed = NotificationConfig::from_json(Some(&json));
        assert_eq!(parsed, config);
    }

    #[test]
    fn from_json_returns_defaults_for_none() {
        let config = NotificationConfig::from_json(None);
        assert_eq!(config, NotificationConfig::default());
    }
}
