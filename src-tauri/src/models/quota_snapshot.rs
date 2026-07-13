use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct QuotaSnapshot {
    pub id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub status: String,
    pub remaining_label: Option<String>,
    pub reset_at: Option<String>,
    pub summary_json: String,
    pub raw_excerpt_json: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewQuotaSnapshot {
    pub owner_type: String,
    pub owner_id: String,
    pub status: String,
    pub remaining_label: Option<String>,
    pub reset_at: Option<String>,
    pub summary_json: String,
    pub raw_excerpt_json: String,
}
