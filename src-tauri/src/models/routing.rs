use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ProxyProfile {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub auth_ref: Option<String>,
    pub enabled: i64,
    pub notes: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewProxyProfile {
    pub name: String,
    pub endpoint_url: String,
    pub auth_ref: Option<String>,
    pub enabled: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct FailoverPolicy {
    pub id: String,
    pub name: String,
    pub strategy: String,
    pub provider_ids_json: String,
    pub enabled: i64,
    pub notes: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewFailoverPolicy {
    pub name: String,
    pub strategy: String,
    pub provider_ids_json: String,
    pub enabled: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct UsageEvent {
    pub id: String,
    pub provider_id: Option<String>,
    pub official_account_id: Option<String>,
    pub source_label: String,
    pub metric_type: String,
    pub amount: i64,
    pub unit: String,
    pub metadata_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewUsageEvent {
    pub provider_id: Option<String>,
    pub official_account_id: Option<String>,
    pub source_label: String,
    pub metric_type: String,
    pub amount: i64,
    pub unit: String,
    pub metadata_json: String,
}
