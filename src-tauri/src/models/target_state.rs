use crate::models::provider::Provider;
use crate::models::target_app::TargetApp;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetSwitchStatus {
    pub target: TargetApp,
    pub active_provider: Option<Provider>,
    pub last_write_status: Option<String>,
    pub last_error_code: Option<String>,
    pub last_written_at: Option<String>,
    pub last_snapshot_path: Option<String>,
    pub last_snapshot_id: Option<String>,
    pub last_snapshot_operation: Option<String>,
    pub can_rollback: bool,
}
