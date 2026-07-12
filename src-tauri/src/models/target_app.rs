use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct TargetApp {
    pub id: String,
    pub key: String,
    pub display_name: String,
    pub enabled: i64,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}
