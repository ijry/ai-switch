use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ImportJob {
    pub id: String,
    pub source_type: String,
    pub source_label: String,
    pub batch_id: Option<String>,
    pub strategy: String,
    pub status: String,
    pub success_count: i64,
    pub failure_count: i64,
    pub conflict_count: i64,
    pub summary_json: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}
