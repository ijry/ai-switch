use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct TagRecord {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewTagRecord {
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ItemTag {
    pub id: String,
    pub tag_id: String,
    pub item_type: String,
    pub item_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewItemTag {
    pub tag_id: String,
    pub item_type: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct PluginLink {
    pub id: String,
    pub name: String,
    pub plugin_key: String,
    pub item_type: String,
    pub item_id: String,
    pub config_json: String,
    pub enabled: i64,
    pub status: String,
    pub notes: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewPluginLink {
    pub name: String,
    pub plugin_key: String,
    pub item_type: String,
    pub item_id: String,
    pub config_json: String,
    pub enabled: bool,
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetPluginLinkEnabledRequest {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct BulkOperation {
    pub id: String,
    pub name: String,
    pub operation_type: String,
    pub target_type: String,
    pub item_ids_json: String,
    pub parameters_json: String,
    pub dry_run: i64,
    pub status: String,
    pub summary_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewBulkOperation {
    pub name: String,
    pub operation_type: String,
    pub target_type: String,
    pub item_ids_json: String,
    pub parameters_json: String,
    pub dry_run: bool,
    pub status: String,
    pub summary_json: String,
}
