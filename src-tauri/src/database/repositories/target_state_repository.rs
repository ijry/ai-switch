use crate::error::AppError;
use crate::models::target_app::{TargetAppState, TargetAppStateUpdate};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct TargetStateRepository;

impl TargetStateRepository {
    pub async fn get(
        pool: &SqlitePool,
        target_app_id: &str,
    ) -> Result<Option<TargetAppState>, AppError> {
        sqlx::query_as::<_, TargetAppState>(
            "SELECT * FROM target_app_states WHERE target_app_id = ?",
        )
        .bind(target_app_id)
        .fetch_optional(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_state_get",
            message: "Could not load target app state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn record(
        pool: &SqlitePool,
        input: TargetAppStateUpdate,
    ) -> Result<TargetAppState, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO target_app_states (
                id, target_app_id, active_item_type, active_item_id, last_write_status,
                last_error_code, last_written_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(target_app_id) DO UPDATE SET
                active_item_type = excluded.active_item_type,
                active_item_id = excluded.active_item_id,
                last_write_status = excluded.last_write_status,
                last_error_code = excluded.last_error_code,
                last_written_at = excluded.last_written_at,
                updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(&input.target_app_id)
        .bind(input.active_item_type)
        .bind(input.active_item_id)
        .bind(input.last_write_status)
        .bind(input.last_error_code)
        .bind(input.last_written_at)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_state_record",
            message: "Could not record target app state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &input.target_app_id)
            .await?
            .ok_or_else(|| AppError::Database {
                code: "database.target_state_missing",
                message: "Target app state was not persisted".to_string(),
                details: Some(input.target_app_id),
                recoverable: true,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::TargetStateRepository;
    use crate::database::repositories::target_repository::TargetRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::target_app::TargetAppStateUpdate;

    #[tokio::test]
    async fn record_replaces_prior_state_for_target() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        TargetRepository::ensure_defaults(&pool)
            .await
            .expect("target defaults");
        let target = TargetRepository::get_by_key(&pool, "codex")
            .await
            .expect("Codex target");

        for status in ["failed", "succeeded"] {
            TargetStateRepository::record(
                &pool,
                TargetAppStateUpdate {
                    target_app_id: target.id.clone(),
                    active_item_type: Some("route_proxy".to_string()),
                    active_item_id: None,
                    last_write_status: Some(status.to_string()),
                    last_error_code: if status == "failed" {
                        Some("config.write_failed".to_string())
                    } else {
                        None
                    },
                    last_written_at: Some("2026-08-01T00:00:00Z".to_string()),
                },
            )
            .await
            .expect("record state");
        }

        let state = TargetStateRepository::get(&pool, &target.id)
            .await
            .expect("load state")
            .expect("state");
        assert_eq!(state.last_write_status.as_deref(), Some("succeeded"));
        assert_eq!(state.last_error_code, None);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM target_app_states WHERE target_app_id = ?")
                .bind(&target.id)
                .fetch_one(&pool)
                .await
                .expect("state count");
        assert_eq!(count, 1);
    }
}
