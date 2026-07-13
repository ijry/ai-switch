use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ManagedInstance {
    pub id: String,
    pub name: String,
    pub target_app_id: Option<String>,
    pub provider_id: Option<String>,
    pub launch_args_json: String,
    pub env_json: String,
    pub profile_json: String,
    pub status: String,
    pub notes: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewManagedInstance {
    pub name: String,
    pub target_app_id: Option<String>,
    pub provider_id: Option<String>,
    pub launch_args_json: String,
    pub env_json: String,
    pub profile_json: String,
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetInstanceStatusRequest {
    pub id: String,
    pub status: String,
}
