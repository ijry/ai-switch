use crate::error::AppError;
use crate::models::instance::{ManagedInstance, NewManagedInstance};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct InstanceRepository;

impl InstanceRepository {
    pub async fn create(
        pool: &SqlitePool,
        input: NewManagedInstance,
    ) -> Result<ManagedInstance, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO managed_instances (id, name, target_app_id, provider_id, launch_args_json, env_json, profile_json, status, notes, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.target_app_id)
        .bind(&input.provider_id)
        .bind(&input.launch_args_json)
        .bind(&input.env_json)
        .bind(&input.profile_json)
        .bind(&input.status)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.instance_create",
            message: "Could not create managed instance".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<ManagedInstance, AppError> {
        sqlx::query_as::<_, ManagedInstance>("SELECT * FROM managed_instances WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.instance_get",
                message: "Could not load managed instance".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list(pool: &SqlitePool) -> Result<Vec<ManagedInstance>, AppError> {
        sqlx::query_as::<_, ManagedInstance>(
            "SELECT * FROM managed_instances ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.instance_list",
            message: "Could not list managed instances".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn set_status(
        pool: &SqlitePool,
        id: &str,
        status: &str,
    ) -> Result<ManagedInstance, AppError> {
        let now = Utc::now().to_rfc3339();

        sqlx::query("UPDATE managed_instances SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.instance_status",
                message: "Could not update managed instance status".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Self::get(pool, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn creates_and_updates_instances() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let instance = InstanceRepository::create(
            &pool,
            NewManagedInstance {
                name: "Codex Review".to_string(),
                target_app_id: None,
                provider_id: None,
                launch_args_json: "[\"--profile\",\"review\"]".to_string(),
                env_json: "{\"API_KEY\":\"env://API_KEY\"}".to_string(),
                profile_json: "{\"workspace\":\"review\"}".to_string(),
                status: "configured".to_string(),
                notes: None,
            },
        )
        .await
        .expect("instance");

        let running = InstanceRepository::set_status(&pool, &instance.id, "running")
            .await
            .expect("running");
        assert_eq!(running.status, "running");
        assert_eq!(
            InstanceRepository::list(&pool)
                .await
                .expect("instances")
                .len(),
            1
        );
    }
}
