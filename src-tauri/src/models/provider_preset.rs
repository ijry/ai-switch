use crate::models::provider::Provider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub model_config_json: String,
    pub target_options_json: String,
    pub secret_env_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateProviderFromPresetRequest {
    pub preset_id: String,
    pub batch_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateProviderFromPresetOutcome {
    pub provider: Provider,
    pub batch_id: Option<String>,
}
