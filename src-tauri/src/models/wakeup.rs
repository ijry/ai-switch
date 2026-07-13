use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct WakeupTask {
    pub id: String,
    pub name: String,
    pub managed_instance_id: Option<String>,
    pub target_app_id: Option<String>,
    pub provider_id: Option<String>,
    pub trigger_type: String,
    pub schedule_json: String,
    pub action_json: String,
    pub enabled: i64,
    pub status: String,
    pub last_run_at: Option<String>,
    pub notes: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewWakeupTask {
    pub name: String,
    pub managed_instance_id: Option<String>,
    pub target_app_id: Option<String>,
    pub provider_id: Option<String>,
    pub trigger_type: String,
    pub schedule_json: String,
    pub action_json: String,
    pub enabled: bool,
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetWakeupTaskEnabledRequest {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct WakeupRun {
    pub id: String,
    pub task_id: String,
    pub outcome: String,
    pub message: String,
    pub metadata_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewWakeupRun {
    pub task_id: String,
    pub outcome: String,
    pub message: String,
    pub metadata_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListWakeupRunsRequest {
    pub task_id: Option<String>,
}
