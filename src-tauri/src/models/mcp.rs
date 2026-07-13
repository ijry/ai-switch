use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args_json: String,
    pub url: Option<String>,
    pub env_json: String,
    pub enabled: i64,
    pub notes: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewMcpServer {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args_json: String,
    pub url: Option<String>,
    pub env_json: String,
    pub enabled: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetMcpServerEnabledRequest {
    pub id: String,
    pub enabled: bool,
}
