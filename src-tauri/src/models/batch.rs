use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct Batch {
    pub id: String,
    pub name: String,
    pub source: String,
    pub notes: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct BatchItem {
    pub id: String,
    pub batch_id: String,
    pub item_type: String,
    pub item_id: String,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchChild {
    pub item_type: String,
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub platform: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchGroup {
    pub batch: Batch,
    pub health: String,
    pub children: Vec<BatchChild>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewBatch {
    pub name: String,
    pub source: String,
    pub notes: Option<String>,
}
