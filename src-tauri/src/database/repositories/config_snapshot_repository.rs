use crate::error::AppError;
use crate::models::config_snapshot::{
    ConfigSnapshotRecord, ConfigSnapshotSummary, NewConfigSnapshot,
};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ConfigSnapshotRepository;

impl ConfigSnapshotRepository {
    pub async fn prepare(
        pool: &SqlitePool,
        input: NewConfigSnapshot,
    ) -> Result<ConfigSnapshotRecord, AppError> {
        let id = Uuid::new_v4().to_string();
        Self::prepare_with_id(pool, &id, input).await
    }

    pub async fn prepare_with_id(
        pool: &SqlitePool,
        id: &str,
        input: NewConfigSnapshot,
    ) -> Result<ConfigSnapshotRecord, AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO config_snapshots (
                id, target_app_id, platform, operation, operation_group_id, source_snapshot_id,
                path, before_hash, after_hash, backup_path, original_file_existed, metadata_json,
                status, error_code, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'prepared', NULL, ?, ?)",
        )
        .bind(id)
        .bind(input.target_app_id)
        .bind(input.platform)
        .bind(input.operation)
        .bind(input.operation_group_id)
        .bind(input.source_snapshot_id)
        .bind(input.path)
        .bind(input.before_hash)
        .bind(input.after_hash)
        .bind(input.backup_path)
        .bind(input.original_file_existed)
        .bind(input.metadata_json)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.config_snapshot_prepare",
                "Could not prepare config snapshot",
                err,
            )
        })?;

        Self::get(pool, id).await
    }

    pub async fn mark_status(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        after_hash: Option<&str>,
        error_code: Option<&str>,
    ) -> Result<(), AppError> {
        let result = sqlx::query(
            "UPDATE config_snapshots
             SET status = ?, after_hash = COALESCE(?, after_hash), error_code = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(after_hash)
        .bind(error_code)
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.config_snapshot_status",
                "Could not update config snapshot status",
                err,
            )
        })?;

        if result.rows_affected() == 0 {
            return Err(snapshot_not_found(id));
        }
        Ok(())
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<ConfigSnapshotRecord, AppError> {
        sqlx::query_as::<_, ConfigSnapshotRecord>(
            "SELECT id, target_app_id, platform, operation, operation_group_id,
                    source_snapshot_id, path, before_hash, after_hash, backup_path,
                    original_file_existed, metadata_json, status, error_code, created_at, updated_at
             FROM config_snapshots WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.config_snapshot_get",
                "Could not load config snapshot",
                err,
            )
        })?
        .ok_or_else(|| snapshot_not_found(id))
    }

    pub async fn list(
        pool: &SqlitePool,
        target_app_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ConfigSnapshotSummary>, AppError> {
        let limit = limit.clamp(1, 200);
        let rows = if let Some(target_app_id) = target_app_id {
            sqlx::query_as::<_, ConfigSnapshotSummary>(&format!(
                "{} WHERE target_app_id = ? ORDER BY created_at DESC LIMIT ?",
                SUMMARY_SELECT
            ))
            .bind(target_app_id)
            .bind(limit)
            .fetch_all(pool)
            .await
        } else {
            sqlx::query_as::<_, ConfigSnapshotSummary>(&format!(
                "{} ORDER BY created_at DESC LIMIT ?",
                SUMMARY_SELECT
            ))
            .bind(limit)
            .fetch_all(pool)
            .await
        };

        rows.map_err(|err| {
            database_error(
                "database.config_snapshot_list",
                "Could not list config snapshots",
                err,
            )
        })
    }

    pub async fn latest_for_target(
        pool: &SqlitePool,
        target_app_id: &str,
    ) -> Result<Option<ConfigSnapshotSummary>, AppError> {
        sqlx::query_as::<_, ConfigSnapshotSummary>(&format!(
            "{} WHERE target_app_id = ? ORDER BY created_at DESC LIMIT 1",
            SUMMARY_SELECT
        ))
        .bind(target_app_id)
        .fetch_optional(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.config_snapshot_latest",
                "Could not load latest config snapshot",
                err,
            )
        })
    }

    pub async fn count_for_target(pool: &SqlitePool, target_app_id: &str) -> Result<i64, AppError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM config_snapshots WHERE target_app_id = ?")
            .bind(target_app_id)
            .fetch_one(pool)
            .await
            .map_err(|err| {
                database_error(
                    "database.config_snapshot_count",
                    "Could not count config snapshots",
                    err,
                )
            })
    }

    pub async fn list_prepared_before(
        pool: &SqlitePool,
        cutoff: &str,
    ) -> Result<Vec<ConfigSnapshotRecord>, AppError> {
        sqlx::query_as::<_, ConfigSnapshotRecord>(
            "SELECT id, target_app_id, platform, operation, operation_group_id,
                    source_snapshot_id, path, before_hash, after_hash, backup_path,
                    original_file_existed, metadata_json, status, error_code, created_at, updated_at
             FROM config_snapshots
             WHERE status = 'prepared' AND created_at < ?
             ORDER BY created_at ASC",
        )
        .bind(cutoff)
        .fetch_all(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.config_snapshot_prepared",
                "Could not list prepared config snapshots",
                err,
            )
        })
    }
}

const SUMMARY_SELECT: &str =
    "SELECT id, target_app_id, platform, operation, operation_group_id, source_snapshot_id,
            path, before_hash, after_hash, original_file_existed, status, error_code,
            created_at, updated_at
     FROM config_snapshots";

fn snapshot_not_found(id: &str) -> AppError {
    AppError::Validation {
        code: "config.snapshot_not_found",
        message: "Config snapshot does not exist".to_string(),
        details: Some(id.to_string()),
        recoverable: true,
    }
}

fn database_error(code: &'static str, message: &str, error: sqlx::Error) -> AppError {
    AppError::Database {
        code,
        message: message.to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigSnapshotRepository;
    use crate::database::repositories::target_repository::TargetRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::config_snapshot::NewConfigSnapshot;

    #[tokio::test]
    async fn prepared_snapshot_can_succeed_and_be_listed_without_private_fields() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        TargetRepository::ensure_defaults(&pool)
            .await
            .expect("target defaults");
        let target = TargetRepository::get_by_key(&pool, "codex")
            .await
            .expect("Codex target");

        let snapshot = ConfigSnapshotRepository::prepare(
            &pool,
            NewConfigSnapshot {
                target_app_id: Some(target.id.clone()),
                platform: Some("codex".to_string()),
                operation: "write".to_string(),
                operation_group_id: Some("group-1".to_string()),
                source_snapshot_id: None,
                path: "C:/Users/test/.codex/config.toml".to_string(),
                before_hash: Some("before".to_string()),
                after_hash: Some("expected".to_string()),
                backup_path: Some("private/backup.bin".to_string()),
                original_file_existed: true,
                metadata_json: r#"{"non_secret":true}"#.to_string(),
            },
        )
        .await
        .expect("prepare snapshot");
        assert_eq!(snapshot.status, "prepared");

        ConfigSnapshotRepository::mark_status(
            &pool,
            &snapshot.id,
            "succeeded",
            Some("after"),
            None,
        )
        .await
        .expect("mark succeeded");

        let listed = ConfigSnapshotRepository::list(&pool, Some(&target.id), 20)
            .await
            .expect("list snapshots");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, "succeeded");
        assert_eq!(listed[0].after_hash.as_deref(), Some("after"));
        assert_eq!(
            ConfigSnapshotRepository::count_for_target(&pool, &target.id)
                .await
                .expect("count"),
            1
        );
        assert_eq!(
            ConfigSnapshotRepository::latest_for_target(&pool, &target.id)
                .await
                .expect("latest")
                .expect("snapshot")
                .id,
            snapshot.id
        );
        let serialized = serde_json::to_value(&listed[0]).expect("serialize summary");
        assert!(serialized.get("backup_path").is_none());
        assert!(serialized.get("metadata_json").is_none());
    }
}
