use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub language: String,
    pub theme: String,
    pub copy_import_sources: bool,
    pub logging_enabled: bool,
    pub secret_storage: String,
    pub data_dir: String,
    #[serde(default)]
    pub ccswitch_deeplink_compat_enabled: bool,
    /// Close-button behavior: hide to the tray (true) or quit the app (false).
    /// On macOS the Dock icon follows the main window while this is enabled.
    /// Legacy settings files without this field default to tray behavior.
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    /// Pool-wide Claude Code client behavior switches (`includeCoAuthoredBy`,
    /// `permissions`, …), as a JSON object string. These are read by Claude Code
    /// from its own settings file, which the whole pool shares — so unlike model
    /// mappings they cannot be per-account. Merged into the settings file's root
    /// on every config write.
    #[serde(default)]
    pub claude_client_config_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettingsView {
    pub language: String,
    pub theme: String,
    pub copy_import_sources: bool,
    pub logging_enabled: bool,
    pub secret_storage: String,
    pub data_dir: String,
    pub ccswitch_deeplink_compat_enabled: bool,
    pub ccswitch_deeplink_compat_supported: bool,
    pub close_to_tray: bool,
    pub claude_client_config_json: Option<String>,
}

impl AppSettingsView {
    pub fn from_settings(settings: AppSettings, supported: bool) -> Self {
        Self {
            language: settings.language,
            theme: settings.theme,
            copy_import_sources: settings.copy_import_sources,
            logging_enabled: settings.logging_enabled,
            secret_storage: settings.secret_storage,
            data_dir: settings.data_dir,
            ccswitch_deeplink_compat_enabled: settings.ccswitch_deeplink_compat_enabled,
            ccswitch_deeplink_compat_supported: supported,
            close_to_tray: settings.close_to_tray,
            claude_client_config_json: settings.claude_client_config_json,
        }
    }
}

impl AppSettings {
    pub fn defaults_for_data_dir(data_dir: String) -> Self {
        Self {
            language: "zh-CN".to_string(),
            theme: "system".to_string(),
            copy_import_sources: false,
            logging_enabled: true,
            secret_storage: "keyring".to_string(),
            data_dir,
            ccswitch_deeplink_compat_enabled: false,
            close_to_tray: true,
            claude_client_config_json: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppSettings;

    #[test]
    fn deserializes_legacy_settings_without_close_to_tray() {
        let json = r#"{
            "language": "zh-CN",
            "theme": "system",
            "copy_import_sources": false,
            "logging_enabled": true,
            "secret_storage": "keyring",
            "data_dir": "/tmp/ai-switch",
            "ccswitch_deeplink_compat_enabled": false
        }"#;
        let settings: AppSettings = serde_json::from_str(json).expect("parse legacy settings");
        assert!(settings.close_to_tray);
    }
}
