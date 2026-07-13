use crate::error::AppError;
use crate::models::automation::{
    BulkOperation, ItemTag, NewBulkOperation, NewItemTag, NewPluginLink, NewTagRecord, PluginLink,
    TagRecord,
};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct AutomationRepository;

impl AutomationRepository {
    pub async fn create_tag(pool: &SqlitePool, input: NewTagRecord) -> Result<TagRecord, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO tags (id, name, color, description, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.color)
        .bind(&input.description)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.tag_create",
            message: "Could not create tag".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_tag(pool, &id).await
    }

    pub async fn get_tag(pool: &SqlitePool, id: &str) -> Result<TagRecord, AppError> {
        sqlx::query_as::<_, TagRecord>("SELECT * FROM tags WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.tag_get",
                message: "Could not load tag".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_tags(pool: &SqlitePool) -> Result<Vec<TagRecord>, AppError> {
        sqlx::query_as::<_, TagRecord>("SELECT * FROM tags ORDER BY sort_order ASC, name ASC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.tag_list",
                message: "Could not list tags".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn create_item_tag(
        pool: &SqlitePool,
        input: NewItemTag,
    ) -> Result<ItemTag, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO item_tags (id, tag_id, item_type, item_id, created_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(tag_id, item_type, item_id) DO UPDATE SET tag_id = excluded.tag_id",
        )
        .bind(&id)
        .bind(&input.tag_id)
        .bind(&input.item_type)
        .bind(&input.item_id)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.item_tag_create",
            message: "Could not assign tag".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        sqlx::query_as::<_, ItemTag>(
            "SELECT * FROM item_tags WHERE tag_id = ? AND item_type = ? AND item_id = ?",
        )
        .bind(&input.tag_id)
        .bind(&input.item_type)
        .bind(&input.item_id)
        .fetch_one(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.item_tag_get",
            message: "Could not load tag assignment".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn list_item_tags(pool: &SqlitePool) -> Result<Vec<ItemTag>, AppError> {
        sqlx::query_as::<_, ItemTag>("SELECT * FROM item_tags ORDER BY created_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.item_tag_list",
                message: "Could not list tag assignments".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn create_plugin_link(
        pool: &SqlitePool,
        input: NewPluginLink,
    ) -> Result<PluginLink, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let enabled = if input.enabled { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO plugin_links (id, name, plugin_key, item_type, item_id, config_json, enabled, status, notes, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.plugin_key)
        .bind(&input.item_type)
        .bind(&input.item_id)
        .bind(&input.config_json)
        .bind(enabled)
        .bind(&input.status)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.plugin_link_create",
            message: "Could not create plugin link".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_plugin_link(pool, &id).await
    }

    pub async fn get_plugin_link(pool: &SqlitePool, id: &str) -> Result<PluginLink, AppError> {
        sqlx::query_as::<_, PluginLink>("SELECT * FROM plugin_links WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.plugin_link_get",
                message: "Could not load plugin link".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_plugin_links(pool: &SqlitePool) -> Result<Vec<PluginLink>, AppError> {
        sqlx::query_as::<_, PluginLink>(
            "SELECT * FROM plugin_links ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.plugin_link_list",
            message: "Could not list plugin links".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn set_plugin_link_enabled(
        pool: &SqlitePool,
        id: &str,
        enabled: bool,
    ) -> Result<PluginLink, AppError> {
        let now = Utc::now().to_rfc3339();
        let enabled_value = if enabled { 1 } else { 0 };
        let status = if enabled { "configured" } else { "paused" };

        sqlx::query("UPDATE plugin_links SET enabled = ?, status = ?, updated_at = ? WHERE id = ?")
            .bind(enabled_value)
            .bind(status)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.plugin_link_enabled",
                message: "Could not update plugin link enabled state".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Self::get_plugin_link(pool, id).await
    }

    pub async fn create_bulk_operation(
        pool: &SqlitePool,
        input: NewBulkOperation,
    ) -> Result<BulkOperation, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let dry_run = if input.dry_run { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO bulk_operations (id, name, operation_type, target_type, item_ids_json, parameters_json, dry_run, status, summary_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.operation_type)
        .bind(&input.target_type)
        .bind(&input.item_ids_json)
        .bind(&input.parameters_json)
        .bind(dry_run)
        .bind(&input.status)
        .bind(&input.summary_json)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.bulk_operation_create",
            message: "Could not create bulk operation record".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_bulk_operation(pool, &id).await
    }

    pub async fn get_bulk_operation(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<BulkOperation, AppError> {
        sqlx::query_as::<_, BulkOperation>("SELECT * FROM bulk_operations WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.bulk_operation_get",
                message: "Could not load bulk operation record".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_bulk_operations(pool: &SqlitePool) -> Result<Vec<BulkOperation>, AppError> {
        sqlx::query_as::<_, BulkOperation>("SELECT * FROM bulk_operations ORDER BY created_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.bulk_operation_list",
                message: "Could not list bulk operation records".to_string(),
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
    async fn creates_tags_plugin_links_and_bulk_operations() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let tag = AutomationRepository::create_tag(
            &pool,
            NewTagRecord {
                name: "review".to_string(),
                color: Some("#3f6f5f".to_string()),
                description: None,
            },
        )
        .await
        .expect("tag");

        let item_tag = AutomationRepository::create_item_tag(
            &pool,
            NewItemTag {
                tag_id: tag.id.clone(),
                item_type: "provider".to_string(),
                item_id: "provider-1".to_string(),
            },
        )
        .await
        .expect("item tag");
        assert_eq!(item_tag.item_id, "provider-1");

        let plugin = AutomationRepository::create_plugin_link(
            &pool,
            NewPluginLink {
                name: "Review bridge".to_string(),
                plugin_key: "review.bridge".to_string(),
                item_type: "provider".to_string(),
                item_id: "provider-1".to_string(),
                config_json: "{\"mode\":\"metadata\"}".to_string(),
                enabled: true,
                status: "configured".to_string(),
                notes: None,
            },
        )
        .await
        .expect("plugin link");

        let paused = AutomationRepository::set_plugin_link_enabled(&pool, &plugin.id, false)
            .await
            .expect("paused");
        assert_eq!(paused.enabled, 0);
        assert_eq!(paused.status, "paused");

        let operation = AutomationRepository::create_bulk_operation(
            &pool,
            NewBulkOperation {
                name: "Apply review tag".to_string(),
                operation_type: "tag_apply".to_string(),
                target_type: "provider".to_string(),
                item_ids_json: "[\"provider-1\"]".to_string(),
                parameters_json: format!("{{\"tag_id\":\"{}\"}}", tag.id),
                dry_run: true,
                status: "planned".to_string(),
                summary_json: "{}".to_string(),
            },
        )
        .await
        .expect("operation");
        assert_eq!(operation.dry_run, 1);

        assert_eq!(
            AutomationRepository::list_tags(&pool).await.unwrap().len(),
            1
        );
        assert_eq!(
            AutomationRepository::list_item_tags(&pool)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            AutomationRepository::list_plugin_links(&pool)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            AutomationRepository::list_bulk_operations(&pool)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
