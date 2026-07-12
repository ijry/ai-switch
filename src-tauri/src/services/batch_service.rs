use crate::database::repositories::account_repository::AccountRepository;
use crate::database::repositories::batch_repository::BatchRepository;
use crate::database::repositories::provider_repository::ProviderRepository;
use crate::error::AppError;
use crate::models::account::{NewOfficialAccount, OfficialAccount};
use crate::models::batch::{Batch, BatchGroup, NewBatch};
use crate::models::provider::{NewProvider, Provider};
use sqlx::SqlitePool;

pub struct BatchService;

impl BatchService {
    pub async fn create_batch(pool: &SqlitePool, input: NewBatch) -> Result<Batch, AppError> {
        if input.name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.batch_name_required",
                message: "Batch name is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        BatchRepository::create(
            pool,
            NewBatch {
                name: input.name.trim().to_string(),
                source: input.source,
                notes: input.notes,
            },
        )
        .await
    }

    pub async fn create_provider(
        pool: &SqlitePool,
        input: NewProvider,
        batch_id: Option<String>,
    ) -> Result<Provider, AppError> {
        if input.name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.provider_name_required",
                message: "Provider name is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let provider = ProviderRepository::create(pool, input).await?;
        if let Some(batch_id) = batch_id {
            BatchRepository::add_item(pool, &batch_id, "provider", &provider.id).await?;
        }
        Ok(provider)
    }

    pub async fn create_official_account(
        pool: &SqlitePool,
        input: NewOfficialAccount,
        batch_id: Option<String>,
    ) -> Result<OfficialAccount, AppError> {
        if input.display_name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.account_name_required",
                message: "Account display name is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let account = AccountRepository::create(pool, input).await?;
        if let Some(batch_id) = batch_id {
            BatchRepository::add_item(pool, &batch_id, "official_account", &account.id).await?;
        }
        Ok(account)
    }

    pub async fn list_groups(
        pool: &SqlitePool,
        search: Option<String>,
    ) -> Result<Vec<BatchGroup>, AppError> {
        BatchRepository::list_groups(pool, search.as_deref()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::batch::NewBatch;

    #[tokio::test]
    async fn create_batch_rejects_empty_name() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let result = BatchService::create_batch(
            &pool,
            NewBatch {
                name: " ".to_string(),
                source: "manual".to_string(),
                notes: None,
            },
        )
        .await;

        assert!(result.is_err());
    }
}
