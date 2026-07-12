use crate::error::AppError;
use crate::models::provider::{NewProvider, Provider};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ProviderRepository;

impl ProviderRepository {
    pub async fn create(pool: &SqlitePool, input: NewProvider) -> Result<Provider, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO providers (id, name, kind, base_url, model_config_json, target_options_json, secret_ref, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'ok', 0, ?, ?)"
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.kind)
        .bind(&input.base_url)
        .bind(&input.model_config_json)
        .bind(&input.target_options_json)
        .bind(&input.secret_ref)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.provider_create",
            message: "Could not create provider".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Provider, AppError> {
        sqlx::query_as::<_, Provider>("SELECT * FROM providers WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.provider_get",
                message: "Could not load provider".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}
