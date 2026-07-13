use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct PromptAsset {
    pub id: String,
    pub item_type: String,
    pub name: String,
    pub description: Option<String>,
    pub body: String,
    pub tags_json: String,
    pub metadata_json: String,
    pub enabled: i64,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewPromptAsset {
    pub item_type: String,
    pub name: String,
    pub description: Option<String>,
    pub body: String,
    pub tags_json: String,
    pub metadata_json: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetPromptAssetEnabledRequest {
    pub id: String,
    pub enabled: bool,
}
