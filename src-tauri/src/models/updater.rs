use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct UpdateChannel {
    pub id: String,
    pub name: String,
    pub channel: String,
    pub feed_url: Option<String>,
    pub enabled: i64,
    pub notes: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewUpdateChannel {
    pub name: String,
    pub channel: String,
    pub feed_url: Option<String>,
    pub enabled: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct UpdateCheck {
    pub id: String,
    pub channel_id: Option<String>,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub status: String,
    pub release_notes_url: Option<String>,
    pub details_json: String,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewUpdateCheck {
    pub channel_id: Option<String>,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub status: String,
    pub release_notes_url: Option<String>,
    pub details_json: String,
}
