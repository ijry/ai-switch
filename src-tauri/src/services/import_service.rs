use crate::database::repositories::batch_repository::BatchRepository;
use crate::database::repositories::import_repository::ImportRepository;
use crate::error::AppError;
use crate::importers::example_json::parse_example_json;
use crate::models::batch::NewBatch;
use crate::models::import_job::ImportJob;
use crate::services::batch_service::BatchService;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleJsonImportRequest {
    pub batch_name: String,
    pub source_label: String,
    pub strategy: String,
    pub json: String,
}

pub struct ImportService;

impl ImportService {
    pub async fn import_example_json(
        pool: &SqlitePool,
        request: ExampleJsonImportRequest,
    ) -> Result<ImportJob, AppError> {
        if request.batch_name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.import_batch_name_required",
                message: "Batch name is required for import".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let payload = parse_example_json(&request.json)?;
        let batch = BatchRepository::create(
            pool,
            NewBatch {
                name: request.batch_name.trim().to_string(),
                source: "example_json".to_string(),
                notes: Some(request.source_label.clone()),
            },
        )
        .await?;

        let job = ImportRepository::create_job(
            pool,
            "example_json",
            &request.source_label,
            Some(&batch.id),
            &request.strategy,
        )
        .await?;
        let mut success_count = 0_i64;

        for provider in payload.providers {
            let created =
                BatchService::create_provider(pool, provider, Some(batch.id.clone())).await?;
            if !created.id.is_empty() {
                success_count += 1;
            }
        }

        for account in payload.accounts {
            let created =
                BatchService::create_official_account(pool, account, Some(batch.id.clone()))
                    .await?;
            if !created.id.is_empty() {
                success_count += 1;
            }
        }

        let summary_json = serde_json::json!({
            "batch_id": batch.id,
            "created": success_count
        })
        .to_string();

        ImportRepository::complete_job(
            pool,
            &job.id,
            "completed",
            success_count,
            0,
            0,
            &summary_json,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn import_example_json_creates_batch_items_and_job() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let request = ExampleJsonImportRequest {
            batch_name: "Batch 2026-07".to_string(),
            source_label: "inline fixture".to_string(),
            strategy: "skip".to_string(),
            json: r#"{
              "providers": [{"name":"Acme Claude","kind":"openai_compatible","base_url":"https://api.example.com/v1","model_config_json":"{}","target_options_json":"{}","secret_ref":"secret://provider/acme"}],
              "accounts": [{"platform":"codex","display_name":"Team Account","email":"team@example.com","plan":"team","account_metadata_json":"{}","secret_ref":"secret://account/team"}]
            }"#
            .to_string(),
        };

        let job = ImportService::import_example_json(&pool, request)
            .await
            .expect("import");

        assert_eq!(job.status, "completed");
        assert_eq!(job.success_count, 2);
        assert_eq!(job.failure_count, 0);
    }
}
