use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct SyncProfile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub endpoint_url: Option<String>,
    pub auth_ref: Option<String>,
    pub scope_json: String,
    pub enabled: i64,
    pub notes: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewSyncProfile {
    pub name: String,
    pub provider: String,
    pub endpoint_url: Option<String>,
    pub auth_ref: Option<String>,
    pub scope_json: String,
    pub enabled: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct SyncSnapshot {
    pub id: String,
    pub profile_id: Option<String>,
    pub direction: String,
    pub status: String,
    pub item_counts_json: String,
    pub manifest_json: String,
    pub artifact_ref: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateSyncSnapshotRequest {
    pub profile_id: Option<String>,
    pub direction: String,
    pub artifact_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewSyncSnapshot {
    pub profile_id: Option<String>,
    pub direction: String,
    pub status: String,
    pub item_counts_json: String,
    pub manifest_json: String,
    pub artifact_ref: Option<String>,
}
