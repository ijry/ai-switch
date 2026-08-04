use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ConfigSnapshotRecord {
    pub id: String,
    pub target_app_id: Option<String>,
    pub platform: Option<String>,
    pub operation: String,
    pub operation_group_id: Option<String>,
    pub source_snapshot_id: Option<String>,
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub backup_path: Option<String>,
    pub original_file_existed: i64,
    pub metadata_json: String,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConfigSnapshot {
    pub target_app_id: Option<String>,
    pub platform: Option<String>,
    pub operation: String,
    pub operation_group_id: Option<String>,
    pub source_snapshot_id: Option<String>,
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub backup_path: Option<String>,
    pub original_file_existed: bool,
    pub metadata_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ConfigSnapshotSummary {
    pub id: String,
    pub target_app_id: Option<String>,
    pub platform: Option<String>,
    pub operation: String,
    pub operation_group_id: Option<String>,
    pub source_snapshot_id: Option<String>,
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub original_file_existed: i64,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigWriteOutcome {
    pub operation_id: String,
    pub snapshot_id: Option<String>,
    pub target_app_id: Option<String>,
    pub target_key: String,
    pub platform: String,
    pub path: String,
    pub status: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub error_code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::ConfigSnapshotSummary;

    #[test]
    fn public_snapshot_summary_omits_private_backup_fields() {
        let summary = ConfigSnapshotSummary {
            id: "snapshot-1".to_string(),
            target_app_id: Some("target-1".to_string()),
            platform: Some("codex".to_string()),
            operation: "write".to_string(),
            operation_group_id: None,
            source_snapshot_id: None,
            path: "config.toml".to_string(),
            before_hash: Some("before".to_string()),
            after_hash: Some("after".to_string()),
            original_file_existed: 1,
            status: "succeeded".to_string(),
            error_code: None,
            created_at: "2026-08-01T00:00:00Z".to_string(),
            updated_at: "2026-08-01T00:00:01Z".to_string(),
        };

        let value = serde_json::to_value(summary).expect("serialize summary");
        assert!(value.get("backup_path").is_none());
        assert!(value.get("metadata_json").is_none());
    }
}
