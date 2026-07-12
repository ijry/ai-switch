use crate::error::AppError;
use crate::models::import_job::ImportJob;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ImportRepository;

impl ImportRepository {
    pub async fn create_job(
        pool: &SqlitePool,
        source_type: &str,
        source_label: &str,
        batch_id: Option<&str>,
        strategy: &str,
    ) -> Result<ImportJob, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO import_jobs (id, source_type, source_label, batch_id, strategy, status, summary_json, created_at) VALUES (?, ?, ?, ?, ?, 'running', '{}', ?)",
        )
        .bind(&id)
        .bind(source_type)
        .bind(source_label)
        .bind(batch_id)
        .bind(strategy)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.import_job_create",
            message: "Could not create import job".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn complete_job(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        success_count: i64,
        failure_count: i64,
        conflict_count: i64,
        summary_json: &str,
    ) -> Result<ImportJob, AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE import_jobs SET status = ?, success_count = ?, failure_count = ?, conflict_count = ?, summary_json = ?, completed_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(success_count)
        .bind(failure_count)
        .bind(conflict_count)
        .bind(summary_json)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.import_job_complete",
            message: "Could not complete import job".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<ImportJob, AppError> {
        sqlx::query_as::<_, ImportJob>("SELECT * FROM import_jobs WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.import_job_get",
                message: "Could not load import job".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}
