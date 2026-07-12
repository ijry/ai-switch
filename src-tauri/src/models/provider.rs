use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub model_config_json: String,
    pub target_options_json: String,
    pub secret_ref: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewProvider {
    pub name: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub model_config_json: String,
    pub target_options_json: String,
    pub secret_ref: Option<String>,
}
