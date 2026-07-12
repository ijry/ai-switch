use crate::error::AppError;
use crate::models::batch::{Batch, BatchChild, BatchGroup, BatchItem, NewBatch};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct BatchRepository;

impl BatchRepository {
    pub async fn create(pool: &SqlitePool, input: NewBatch) -> Result<Batch, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO batches (id, name, source, notes, sort_order, created_at, updated_at) VALUES (?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.source)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.batch_create",
            message: "Could not create batch".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Batch, AppError> {
        sqlx::query_as::<_, Batch>("SELECT * FROM batches WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.batch_get",
                message: "Could not load batch".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn add_item(
        pool: &SqlitePool,
        batch_id: &str,
        item_type: &str,
        item_id: &str,
    ) -> Result<BatchItem, AppError> {
        if item_type != "provider" && item_type != "official_account" {
            return Err(AppError::Validation {
                code: "validation.batch_item_type",
                message: "Batch item type must be provider or official_account".to_string(),
                details: Some(item_type.to_string()),
                recoverable: true,
            });
        }

        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO batch_items (id, batch_id, item_type, item_id, sort_order, created_at) VALUES (?, ?, ?, ?, 0, ?)",
        )
        .bind(&id)
        .bind(batch_id)
        .bind(item_type)
        .bind(item_id)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.batch_item_create",
            message: "Could not attach item to batch".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        sqlx::query_as::<_, BatchItem>("SELECT * FROM batch_items WHERE id = ?")
            .bind(&id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.batch_item_get",
                message: "Could not load batch item".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_groups(
        pool: &SqlitePool,
        search: Option<&str>,
    ) -> Result<Vec<BatchGroup>, AppError> {
        let batches =
            sqlx::query_as::<_, Batch>("SELECT * FROM batches ORDER BY sort_order ASC, created_at DESC")
                .fetch_all(pool)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.batch_list",
                    message: "Could not list batches".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;

        let mut groups = Vec::new();
        let needle = search.map(|value| value.to_lowercase());

        for batch in batches {
            let children = Self::children_for_batch(pool, &batch.id).await?;
            let filtered_children: Vec<BatchChild> = match &needle {
                Some(value) => children
                    .into_iter()
                    .filter(|child| {
                        batch.name.to_lowercase().contains(value)
                            || child.title.to_lowercase().contains(value)
                            || child
                                .subtitle
                                .clone()
                                .unwrap_or_default()
                                .to_lowercase()
                                .contains(value)
                    })
                    .collect(),
                None => children,
            };

            let batch_matches = needle
                .as_ref()
                .map(|value| batch.name.to_lowercase().contains(value))
                .unwrap_or(true);

            if batch_matches || !filtered_children.is_empty() {
                let health = if filtered_children
                    .iter()
                    .any(|child| child.status == "error")
                {
                    "error"
                } else if filtered_children
                    .iter()
                    .any(|child| child.status == "warning")
                {
                    "warning"
                } else {
                    "ok"
                };
                groups.push(BatchGroup {
                    batch,
                    health: health.to_string(),
                    children: filtered_children,
                });
            }
        }

        Ok(groups)
    }

    async fn children_for_batch(
        pool: &SqlitePool,
        batch_id: &str,
    ) -> Result<Vec<BatchChild>, AppError> {
        let rows = sqlx::query(
            "SELECT bi.item_type, bi.item_id, p.name as provider_name, p.kind as provider_kind, p.status as provider_status,
                    a.display_name as account_name, a.platform as account_platform, a.email as account_email, a.status as account_status
             FROM batch_items bi
             LEFT JOIN providers p ON bi.item_type = 'provider' AND bi.item_id = p.id
             LEFT JOIN official_accounts a ON bi.item_type = 'official_account' AND bi.item_id = a.id
             WHERE bi.batch_id = ?
             ORDER BY bi.sort_order ASC, bi.created_at ASC",
        )
        .bind(batch_id)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.batch_children",
            message: "Could not load batch children".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let item_type: String = row.get("item_type");
                let id: String = row.get("item_id");
                if item_type == "provider" {
                    BatchChild {
                        item_type,
                        id,
                        title: row
                            .get::<Option<String>, _>("provider_name")
                            .unwrap_or_default(),
                        subtitle: row.get::<Option<String>, _>("provider_kind"),
                        status: row
                            .get::<Option<String>, _>("provider_status")
                            .unwrap_or_else(|| "error".to_string()),
                    }
                } else {
                    let email: Option<String> = row.get("account_email");
                    BatchChild {
                        item_type,
                        id,
                        title: row
                            .get::<Option<String>, _>("account_name")
                            .unwrap_or_default(),
                        subtitle: email.or_else(|| row.get::<Option<String>, _>("account_platform")),
                        status: row
                            .get::<Option<String>, _>("account_status")
                            .unwrap_or_else(|| "error".to_string()),
                    }
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::account_repository::AccountRepository;
    use crate::database::repositories::provider_repository::ProviderRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::account::NewOfficialAccount;
    use crate::models::batch::NewBatch;
    use crate::models::provider::NewProvider;

    #[tokio::test]
    async fn list_groups_returns_batch_with_provider_and_account_children() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let batch = BatchRepository::create(
            &pool,
            NewBatch {
                name: "July imports".to_string(),
                source: "example_json".to_string(),
                notes: None,
            },
        )
        .await
        .expect("batch");

        let provider = ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Acme Claude".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://api.example.com/v1".to_string()),
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: Some("secret://provider/acme".to_string()),
            },
        )
        .await
        .expect("provider");

        let account = AccountRepository::create(
            &pool,
            NewOfficialAccount {
                platform: "codex".to_string(),
                display_name: "Team Account".to_string(),
                email: Some("team@example.com".to_string()),
                plan: Some("team".to_string()),
                account_metadata_json: "{}".to_string(),
                secret_ref: Some("secret://account/team".to_string()),
            },
        )
        .await
        .expect("account");

        BatchRepository::add_item(&pool, &batch.id, "provider", &provider.id)
            .await
            .expect("provider link");
        BatchRepository::add_item(&pool, &batch.id, "official_account", &account.id)
            .await
            .expect("account link");

        let groups = BatchRepository::list_groups(&pool, None)
            .await
            .expect("groups");

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].batch.name, "July imports");
        assert_eq!(groups[0].health, "ok");
        assert_eq!(groups[0].children.len(), 2);
    }
}
