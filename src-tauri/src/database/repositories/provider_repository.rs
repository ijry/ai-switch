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

    pub async fn list(pool: &SqlitePool) -> Result<Vec<Provider>, AppError> {
        sqlx::query_as::<_, Provider>(
            "SELECT * FROM providers ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.provider_list",
            message: "Could not list providers".to_string(),
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
    async fn list_returns_providers_ordered_by_sort_and_created_at() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        ProviderRepository::create(
            &pool,
            NewProvider {
                name: "First Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://first.example.com/v1".to_string()),
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("first provider");
        ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Second Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://second.example.com/v1".to_string()),
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("second provider");

        let providers = ProviderRepository::list(&pool).await.expect("providers");

        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].name, "Second Provider");
        assert_eq!(providers[1].name, "First Provider");
    }
}
