use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ImageSession {
    pub id: String,
    pub title: String,
    pub platform: String,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ImageMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub prompt: String,
    pub request_json: String,
    pub status: String,
    pub model: Option<String>,
    pub upstream_model: Option<String>,
    pub member_id: Option<String>,
    pub upstream_response_id: Option<String>,
    pub error_message: Option<String>,
    pub image_count: i64,
    pub cost_micros: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ImageAsset {
    pub id: String,
    pub session_id: String,
    pub message_id: String,
    pub relative_path: String,
    pub mime_type: String,
    pub sha256: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub byte_size: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageConversation {
    pub session: ImageSession,
    pub messages: Vec<ImageMessage>,
    pub assets: Vec<ImageAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateImageSessionInput {
    pub title: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateImageSessionInput {
    pub id: String,
    pub title: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewImageMessage {
    pub session_id: String,
    pub role: String,
    pub prompt: String,
    pub request_json: String,
    pub status: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewImageAsset {
    pub session_id: String,
    pub message_id: String,
    pub relative_path: String,
    pub mime_type: String,
    pub sha256: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub byte_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageModelOption {
    pub id: String,
    pub upstream_model: String,
    pub capabilities: Vec<String>,
}
