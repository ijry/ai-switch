use crate::database::repositories::account_repository::AccountRepository;
use crate::database::repositories::batch_repository::BatchRepository;
use crate::database::repositories::provider_repository::ProviderRepository;
use crate::error::AppError;
use crate::models::account::{NewOfficialAccount, OfficialAccount, UpdateOfficialAccount};
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

    pub async fn get_official_account(
        pool: &SqlitePool,
        id: String,
    ) -> Result<OfficialAccount, AppError> {
        AccountRepository::get(pool, id.trim()).await
    }

    pub async fn update_official_account(
        pool: &SqlitePool,
        id: String,
        input: UpdateOfficialAccount,
    ) -> Result<OfficialAccount, AppError> {
        let id = id.trim().to_string();
        if id.is_empty() {
            return Err(AppError::Validation {
                code: "validation.account_id_required",
                message: "Account id is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        if input.display_name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.account_name_required",
                message: "Account display name is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        if !matches!(input.status.as_str(), "ok" | "warning" | "error") {
            return Err(AppError::Validation {
                code: "validation.account_status",
                message: "Account status must be ok, warning, or error".to_string(),
                details: Some(input.status),
                recoverable: true,
            });
        }

        let metadata_json = normalize_object_json(
            &input.account_metadata_json,
            "validation.account_metadata_json",
            "Account metadata JSON must be an object",
        )?;

        AccountRepository::update(
            pool,
            &id,
            UpdateOfficialAccount {
                display_name: input.display_name.trim().to_string(),
                email: trim_optional(input.email),
                plan: trim_optional(input.plan),
                account_metadata_json: metadata_json,
                secret_ref: trim_optional(input.secret_ref),
                status: input.status,
            },
        )
        .await
    }

    pub async fn list_groups(
        pool: &SqlitePool,
        search: Option<String>,
    ) -> Result<Vec<BatchGroup>, AppError> {
        BatchRepository::list_groups(pool, search.as_deref()).await
    }
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_object_json(
    raw: &str,
    code: &'static str,
    message: &'static str,
) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok("{}".to_string());
    }

    let value: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|err| AppError::Validation {
            code,
            message: "JSON data is invalid".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

    if !value.is_object() {
        return Err(AppError::Validation {
            code,
            message: message.to_string(),
            details: Some(trimmed.to_string()),
            recoverable: true,
        });
    }

    Ok(value.to_string())
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

    #[tokio::test]
    async fn update_official_account_trims_fields_and_validates_metadata() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let account = BatchService::create_official_account(
            &pool,
            NewOfficialAccount {
                platform: "codex".to_string(),
                display_name: "Original".to_string(),
                email: Some("old@example.com".to_string()),
                plan: Some("Plus".to_string()),
                account_metadata_json: "{}".to_string(),
                secret_ref: None,
            },
            None,
        )
        .await
        .expect("account");

        let updated = BatchService::update_official_account(
            &pool,
            account.id,
            UpdateOfficialAccount {
                display_name: "  Updated  ".to_string(),
                email: Some("  new@example.com  ".to_string()),
                plan: Some("  Team  ".to_string()),
                account_metadata_json: r#"{"source":"edit"}"#.to_string(),
                secret_ref: Some(" ".to_string()),
                status: "warning".to_string(),
            },
        )
        .await
        .expect("updated");

        assert_eq!(updated.display_name, "Updated");
        assert_eq!(updated.email.as_deref(), Some("new@example.com"));
        assert_eq!(updated.plan.as_deref(), Some("Team"));
        assert_eq!(updated.account_metadata_json, r#"{"source":"edit"}"#);
        assert_eq!(updated.secret_ref, None);
        assert_eq!(updated.status, "warning");
    }

    #[tokio::test]
    async fn update_official_account_rejects_non_object_metadata() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = BatchService::update_official_account(
            &pool,
            "missing".to_string(),
            UpdateOfficialAccount {
                display_name: "Updated".to_string(),
                email: None,
                plan: None,
                account_metadata_json: "[]".to_string(),
                secret_ref: None,
                status: "ok".to_string(),
            },
        )
        .await
        .expect_err("metadata error");

        match error {
            AppError::Validation { code, .. } => {
                assert_eq!(code, "validation.account_metadata_json");
            }
            _ => panic!("expected validation error"),
        }
    }
}
