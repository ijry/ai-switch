use crate::error::AppError;
use crate::models::prompt_asset::{NewPromptAsset, PromptAsset};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct PromptAssetRepository;

impl PromptAssetRepository {
    pub async fn create(pool: &SqlitePool, input: NewPromptAsset) -> Result<PromptAsset, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        let enabled = i64::from(input.enabled);

        sqlx::query(
            "INSERT INTO prompt_assets (id, item_type, name, description, body, tags_json, metadata_json, enabled, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'draft', 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.item_type)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.body)
        .bind(&input.tags_json)
        .bind(&input.metadata_json)
        .bind(enabled)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.prompt_asset_create",
            message: "Could not create prompt asset".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<PromptAsset, AppError> {
        sqlx::query_as::<_, PromptAsset>("SELECT * FROM prompt_assets WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.prompt_asset_get",
                message: "Could not load prompt asset".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list(pool: &SqlitePool) -> Result<Vec<PromptAsset>, AppError> {
        sqlx::query_as::<_, PromptAsset>(
            "SELECT * FROM prompt_assets ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.prompt_asset_list",
            message: "Could not list prompt assets".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn set_enabled(
        pool: &SqlitePool,
        id: &str,
        enabled: bool,
    ) -> Result<PromptAsset, AppError> {
        let now = Utc::now().to_rfc3339();
        let enabled = i64::from(enabled);

        sqlx::query("UPDATE prompt_assets SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.prompt_asset_update_enabled",
                message: "Could not update prompt asset".to_string(),
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
    async fn list_returns_prompt_assets_ordered_by_created_at() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        PromptAssetRepository::create(
            &pool,
            NewPromptAsset {
                item_type: "prompt".to_string(),
                name: "Review Prompt".to_string(),
                description: Some("Review checklist".to_string()),
                body: "Review for regressions.".to_string(),
                tags_json: "[\"review\"]".to_string(),
                metadata_json: "{}".to_string(),
                enabled: true,
            },
        )
        .await
        .expect("first");
        PromptAssetRepository::create(
            &pool,
            NewPromptAsset {
                item_type: "skill".to_string(),
                name: "Release Notes".to_string(),
                description: None,
                body: "Summarize merged changes.".to_string(),
                tags_json: "[\"release\"]".to_string(),
                metadata_json: "{\"owner\":\"docs\"}".to_string(),
                enabled: false,
            },
        )
        .await
        .expect("second");

        let assets = PromptAssetRepository::list(&pool).await.expect("assets");

        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].name, "Release Notes");
        assert_eq!(assets[1].name, "Review Prompt");
    }

    #[tokio::test]
    async fn set_enabled_updates_prompt_asset_state() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let asset = PromptAssetRepository::create(
            &pool,
            NewPromptAsset {
                item_type: "prompt".to_string(),
                name: "Review Prompt".to_string(),
                description: None,
                body: "Review for regressions.".to_string(),
                tags_json: "[]".to_string(),
                metadata_json: "{}".to_string(),
                enabled: true,
            },
        )
        .await
        .expect("asset");

        let disabled = PromptAssetRepository::set_enabled(&pool, &asset.id, false)
            .await
            .expect("disabled");

        assert_eq!(disabled.enabled, 0);
    }
}
