use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSwitchRequest {
    pub target_app_id: String,
    pub provider_id: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSwitchOutcome {
    pub target_app_id: String,
    pub target_key: String,
    pub provider_id: String,
    pub provider_name: String,
    pub mode: String,
    pub path: String,
    pub status: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub snapshot_id: String,
    pub state_id: String,
    pub written_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigRollbackOutcome {
    pub target_app_id: String,
    pub target_key: String,
    pub source_snapshot_id: String,
    pub rollback_snapshot_id: String,
    pub state_id: String,
    pub path: String,
    pub status: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub rolled_back_at: String,
}
