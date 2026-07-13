use crate::error::AppError;
use crate::models::quota_snapshot::{NewQuotaSnapshot, QuotaSnapshot};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct QuotaSnapshotRepository;

impl QuotaSnapshotRepository {
    pub async fn insert(
        pool: &SqlitePool,
        input: NewQuotaSnapshot,
    ) -> Result<QuotaSnapshot, AppError> {
        let id = Uuid::new_v4().to_string();
        let fetched_at = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO quota_snapshots (id, owner_type, owner_id, status, remaining_label, reset_at, summary_json, raw_excerpt_json, fetched_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.owner_type)
        .bind(&input.owner_id)
        .bind(&input.status)
        .bind(&input.remaining_label)
        .bind(&input.reset_at)
        .bind(&input.summary_json)
        .bind(&input.raw_excerpt_json)
        .bind(&fetched_at)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.quota_snapshot_insert",
            message: "Could not record quota snapshot".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<QuotaSnapshot, AppError> {
        sqlx::query_as::<_, QuotaSnapshot>("SELECT * FROM quota_snapshots WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.quota_snapshot_get",
                message: "Could not load quota snapshot".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn get_optional(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<Option<QuotaSnapshot>, AppError> {
        sqlx::query_as::<_, QuotaSnapshot>("SELECT * FROM quota_snapshots WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.quota_snapshot_get",
                message: "Could not load quota snapshot".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}
