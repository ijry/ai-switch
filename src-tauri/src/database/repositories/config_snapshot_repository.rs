use crate::error::AppError;
use crate::models::config_snapshot::{ConfigSnapshot, NewConfigSnapshot};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ConfigSnapshotRepository;

impl ConfigSnapshotRepository {
    pub async fn insert(
        pool: &SqlitePool,
        input: NewConfigSnapshot,
    ) -> Result<ConfigSnapshot, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO config_snapshots (id, target_app_id, operation, path, before_hash, after_hash, backup_path, status, error_code, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.target_app_id)
        .bind(&input.operation)
        .bind(&input.path)
        .bind(&input.before_hash)
        .bind(&input.after_hash)
        .bind(&input.backup_path)
        .bind(&input.status)
        .bind(&input.error_code)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.config_snapshot_insert",
            message: "Could not record config snapshot".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        sqlx::query_as::<_, ConfigSnapshot>("SELECT * FROM config_snapshots WHERE id = ?")
            .bind(&id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.config_snapshot_get",
                message: "Could not load config snapshot".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    #[cfg(test)]
    pub async fn latest_for_target(
        pool: &SqlitePool,
        target_app_id: &str,
    ) -> Result<Option<ConfigSnapshot>, AppError> {
        sqlx::query_as::<_, ConfigSnapshot>(
            "SELECT * FROM config_snapshots WHERE target_app_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(target_app_id)
        .fetch_optional(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.config_snapshot_latest",
            message: "Could not load latest config snapshot".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<ConfigSnapshot, AppError> {
        sqlx::query_as::<_, ConfigSnapshot>("SELECT * FROM config_snapshots WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.config_snapshot_get",
                message: "Could not load config snapshot".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::target_repository::TargetRepository;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn insert_and_latest_for_target_round_trip_snapshot() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");

        let inserted = ConfigSnapshotRepository::insert(
            &pool,
            NewConfigSnapshot {
                target_app_id: Some(targets[2].id.clone()),
                operation: "switch_provider:sandbox".to_string(),
                path: "C:/Users/example/.ai-switch/targets/codex/provider.json".to_string(),
                before_hash: None,
                after_hash: Some("after".to_string()),
                backup_path: None,
                status: "written".to_string(),
                error_code: None,
            },
        )
        .await
        .expect("insert");

        let latest = ConfigSnapshotRepository::latest_for_target(&pool, &targets[2].id)
            .await
            .expect("latest")
            .expect("snapshot");

        assert_eq!(latest.id, inserted.id);
        assert_eq!(latest.operation, "switch_provider:sandbox");
        assert_eq!(latest.status, "written");
    }
}
