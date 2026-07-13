use crate::models::quota_snapshot::QuotaSnapshot;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct OfficialAccount {
    pub id: String,
    pub platform: String,
    pub display_name: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub account_metadata_json: String,
    pub secret_ref: Option<String>,
    pub quota_snapshot_id: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewOfficialAccount {
    pub platform: String,
    pub display_name: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub account_metadata_json: String,
    pub secret_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfficialAccountStatus {
    pub account: OfficialAccount,
    pub quota_snapshot: Option<QuotaSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordAccountQuotaSnapshotRequest {
    pub account_id: String,
    pub status: String,
    pub remaining_label: Option<String>,
    pub reset_at: Option<String>,
    pub summary_json: String,
    pub raw_excerpt_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefreshAccountQuotaSnapshotRequest {
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordAccountQuotaSnapshotOutcome {
    pub account: OfficialAccount,
    pub quota_snapshot: QuotaSnapshot,
}
