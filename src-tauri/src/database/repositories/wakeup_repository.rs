use crate::error::AppError;
use crate::models::wakeup::{NewWakeupRun, NewWakeupTask, WakeupRun, WakeupTask};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct WakeupRepository;

impl WakeupRepository {
    pub async fn create_task(
        pool: &SqlitePool,
        input: NewWakeupTask,
    ) -> Result<WakeupTask, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let enabled = if input.enabled { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO wakeup_tasks (id, name, managed_instance_id, target_app_id, provider_id, trigger_type, schedule_json, action_json, enabled, status, last_run_at, notes, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.managed_instance_id)
        .bind(&input.target_app_id)
        .bind(&input.provider_id)
        .bind(&input.trigger_type)
        .bind(&input.schedule_json)
        .bind(&input.action_json)
        .bind(enabled)
        .bind(&input.status)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.wakeup_task_create",
            message: "Could not create wakeup task".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_task(pool, &id).await
    }

    pub async fn get_task(pool: &SqlitePool, id: &str) -> Result<WakeupTask, AppError> {
        sqlx::query_as::<_, WakeupTask>("SELECT * FROM wakeup_tasks WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.wakeup_task_get",
                message: "Could not load wakeup task".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_tasks(pool: &SqlitePool) -> Result<Vec<WakeupTask>, AppError> {
        sqlx::query_as::<_, WakeupTask>(
            "SELECT * FROM wakeup_tasks ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.wakeup_task_list",
            message: "Could not list wakeup tasks".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn set_task_enabled(
        pool: &SqlitePool,
        id: &str,
        enabled: bool,
    ) -> Result<WakeupTask, AppError> {
        let now = Utc::now().to_rfc3339();
        let enabled_value = if enabled { 1 } else { 0 };
        let status = if enabled { "configured" } else { "paused" };

        sqlx::query("UPDATE wakeup_tasks SET enabled = ?, status = ?, updated_at = ? WHERE id = ?")
            .bind(enabled_value)
            .bind(status)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.wakeup_task_enabled",
                message: "Could not update wakeup task enabled state".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Self::get_task(pool, id).await
    }

    pub async fn create_run(pool: &SqlitePool, input: NewWakeupRun) -> Result<WakeupRun, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO wakeup_runs (id, task_id, outcome, message, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.task_id)
        .bind(&input.outcome)
        .bind(&input.message)
        .bind(&input.metadata_json)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.wakeup_run_create",
            message: "Could not create wakeup run record".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        sqlx::query(
            "UPDATE wakeup_tasks
             SET last_run_at = ?,
                 status = CASE WHEN ? = 'failed' THEN 'error' ELSE status END,
                 updated_at = ?
             WHERE id = ?",
        )
        .bind(&now)
        .bind(&input.outcome)
        .bind(&now)
        .bind(&input.task_id)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.wakeup_task_last_run",
            message: "Could not update wakeup task run metadata".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_run(pool, &id).await
    }

    pub async fn get_run(pool: &SqlitePool, id: &str) -> Result<WakeupRun, AppError> {
        sqlx::query_as::<_, WakeupRun>("SELECT * FROM wakeup_runs WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.wakeup_run_get",
                message: "Could not load wakeup run record".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_runs(
        pool: &SqlitePool,
        task_id: Option<&str>,
    ) -> Result<Vec<WakeupRun>, AppError> {
        let query = if let Some(task_id) = task_id {
            sqlx::query_as::<_, WakeupRun>(
                "SELECT * FROM wakeup_runs WHERE task_id = ? ORDER BY created_at DESC",
            )
            .bind(task_id)
        } else {
            sqlx::query_as::<_, WakeupRun>("SELECT * FROM wakeup_runs ORDER BY created_at DESC")
        };

        query
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.wakeup_run_list",
                message: "Could not list wakeup run records".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn creates_tasks_and_run_records() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let task = WakeupRepository::create_task(
            &pool,
            NewWakeupTask {
                name: "Morning review".to_string(),
                managed_instance_id: None,
                target_app_id: None,
                provider_id: None,
                trigger_type: "manual".to_string(),
                schedule_json: "{}".to_string(),
                action_json: "{\"kind\":\"status_record\"}".to_string(),
                enabled: true,
                status: "configured".to_string(),
                notes: None,
            },
        )
        .await
        .expect("task");

        let paused = WakeupRepository::set_task_enabled(&pool, &task.id, false)
            .await
            .expect("paused");
        assert_eq!(paused.enabled, 0);
        assert_eq!(paused.status, "paused");

        let run = WakeupRepository::create_run(
            &pool,
            NewWakeupRun {
                task_id: task.id.clone(),
                outcome: "recorded".to_string(),
                message: "Manual readiness note".to_string(),
                metadata_json: "{}".to_string(),
            },
        )
        .await
        .expect("run");
        assert_eq!(run.outcome, "recorded");

        assert_eq!(
            WakeupRepository::list_tasks(&pool)
                .await
                .expect("tasks")
                .len(),
            1
        );
        assert_eq!(
            WakeupRepository::list_runs(&pool, Some(&task.id))
                .await
                .expect("runs")
                .len(),
            1
        );
    }
}
