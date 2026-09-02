use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::models::config_snapshot::ConfigSnapshotSummary;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct TargetApp {
    pub id: String,
    pub key: String,
    pub platform: Option<String>,
    pub display_name: String,
    pub enabled: i64,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct TargetAppState {
    pub id: String,
    pub target_app_id: String,
    pub active_item_type: Option<String>,
    pub active_item_id: Option<String>,
    pub last_write_status: Option<String>,
    pub last_error_code: Option<String>,
    pub last_written_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetAppStateUpdate {
    pub target_app_id: String,
    pub active_item_type: Option<String>,
    pub active_item_id: Option<String>,
    pub last_write_status: Option<String>,
    pub last_error_code: Option<String>,
    pub last_written_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetConfigStatus {
    pub target: TargetApp,
    pub support_level: Option<String>,
    pub adapter_available: bool,
    pub config_path: Option<String>,
    pub file_status: String,
    pub last_write_status: Option<String>,
    pub last_error_code: Option<String>,
    pub last_written_at: Option<String>,
    pub snapshot_count: i64,
    pub latest_snapshot: Option<ConfigSnapshotSummary>,
}

/// One client the user can write config for, plus the current state of that
/// client's config file. Narrower than `TargetConfigStatus`: it carries the
/// client identity the write dialog needs and skips snapshot bookkeeping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigWriteClientStatus {
    pub client_key: String,
    pub display_name: String,
    pub native: bool,
    pub restart_required: bool,
    pub target_key: String,
    pub platform: String,
    pub config_path: Option<String>,
    pub file_status: String,
    pub error_code: Option<String>,
}
