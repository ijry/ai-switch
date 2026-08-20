use serde::{Deserialize, Serialize};

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
            claude_client_config_json: None,
        }
    }
}
