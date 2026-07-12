use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub language: String,
    pub theme: String,
    pub copy_import_sources: bool,
    pub logging_enabled: bool,
    pub secret_storage: String,
    pub data_dir: String,
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
        }
    }
}
