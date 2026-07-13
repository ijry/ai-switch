use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub title: String,
    pub target_app_id: Option<String>,
    pub provider_id: Option<String>,
    pub official_account_id: Option<String>,
    pub prompt_asset_id: Option<String>,
    pub mcp_server_ids_json: String,
    pub tags_json: String,
    pub status: String,
    pub notes: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewSessionRecord {
    pub title: String,
    pub target_app_id: Option<String>,
    pub provider_id: Option<String>,
    pub official_account_id: Option<String>,
    pub prompt_asset_id: Option<String>,
    pub mcp_server_ids_json: String,
    pub tags_json: String,
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetSessionStatusRequest {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct SessionEvent {
    pub id: String,
    pub session_id: String,
    pub event_type: String,
    pub message: String,
    pub metadata_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewSessionEvent {
    pub session_id: String,
    pub event_type: String,
    pub message: String,
    pub metadata_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListSessionEventsRequest {
    pub session_id: Option<String>,
}
