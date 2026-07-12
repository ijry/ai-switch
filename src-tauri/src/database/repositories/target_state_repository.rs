use crate::error::AppError;
use crate::models::provider::Provider;
use crate::models::target_app::TargetApp;
use crate::models::target_state::{TargetAppState, TargetSwitchStatus};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct TargetStateRepository;

impl TargetStateRepository {
    pub async fn upsert_provider_state(
        pool: &SqlitePool,
        target_app_id: &str,
        provider_id: &str,
        status: &str,
        error_code: Option<&str>,
        written_at: &str,
    ) -> Result<TargetAppState, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO target_app_states (id, target_app_id, active_item_type, active_item_id, last_write_status, last_error_code, last_written_at, updated_at)
             VALUES (?, ?, 'provider', ?, ?, ?, ?, ?)
             ON CONFLICT(target_app_id) DO UPDATE SET
               active_item_type = 'provider',
               active_item_id = excluded.active_item_id,
               last_write_status = excluded.last_write_status,
               last_error_code = excluded.last_error_code,
               last_written_at = excluded.last_written_at,
               updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(target_app_id)
        .bind(provider_id)
        .bind(status)
        .bind(error_code)
        .bind(written_at)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_state_upsert",
            message: "Could not update target switch state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_for_target(pool, target_app_id).await
    }

    pub async fn record_failure(
        pool: &SqlitePool,
        target_app_id: &str,
        error_code: &str,
        written_at: &str,
    ) -> Result<TargetAppState, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO target_app_states (id, target_app_id, active_item_type, active_item_id, last_write_status, last_error_code, last_written_at, updated_at)
             VALUES (?, ?, NULL, NULL, 'failed', ?, ?, ?)
             ON CONFLICT(target_app_id) DO UPDATE SET
               last_write_status = excluded.last_write_status,
               last_error_code = excluded.last_error_code,
               last_written_at = excluded.last_written_at,
               updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(target_app_id)
        .bind(error_code)
        .bind(written_at)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_state_upsert",
            message: "Could not update target failure state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get_for_target(pool, target_app_id).await
    }

    pub async fn get_for_target(
        pool: &SqlitePool,
        target_app_id: &str,
    ) -> Result<TargetAppState, AppError> {
        sqlx::query_as::<_, TargetAppState>(
            "SELECT * FROM target_app_states WHERE target_app_id = ?",
        )
        .bind(target_app_id)
        .fetch_one(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_state_get",
            message: "Could not load target switch state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn list_switch_statuses(
        pool: &SqlitePool,
    ) -> Result<Vec<TargetSwitchStatus>, AppError> {
        let rows = sqlx::query(
            "SELECT
                t.id AS target_id,
                t.key AS target_key,
                t.display_name AS target_display_name,
                t.enabled AS target_enabled,
                t.sort_order AS target_sort_order,
                t.created_at AS target_created_at,
                t.updated_at AS target_updated_at,
                s.last_write_status,
                s.last_error_code,
                s.last_written_at,
                p.id AS provider_id,
                p.name AS provider_name,
                p.kind AS provider_kind,
                p.base_url AS provider_base_url,
                p.model_config_json AS provider_model_config_json,
                p.target_options_json AS provider_target_options_json,
                p.secret_ref AS provider_secret_ref,
                p.status AS provider_status,
                p.sort_order AS provider_sort_order,
                p.created_at AS provider_created_at,
                p.updated_at AS provider_updated_at,
                cs.id AS snapshot_id,
                cs.path AS snapshot_path
             FROM target_apps t
             LEFT JOIN target_app_states s ON s.target_app_id = t.id
             LEFT JOIN providers p ON s.active_item_type = 'provider' AND p.id = s.active_item_id
             LEFT JOIN config_snapshots cs ON cs.id = (
                SELECT id FROM config_snapshots
                WHERE target_app_id = t.id
                ORDER BY created_at DESC
                LIMIT 1
             )
             ORDER BY t.sort_order ASC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.target_status_list",
            message: "Could not list target switch statuses".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let provider_id: Option<String> = row.get("provider_id");
                let active_provider = provider_id.map(|id| Provider {
                    id,
                    name: row
                        .get::<Option<String>, _>("provider_name")
                        .unwrap_or_default(),
                    kind: row
                        .get::<Option<String>, _>("provider_kind")
                        .unwrap_or_default(),
                    base_url: row.get("provider_base_url"),
                    model_config_json: row
                        .get::<Option<String>, _>("provider_model_config_json")
                        .unwrap_or_else(|| "{}".to_string()),
                    target_options_json: row
                        .get::<Option<String>, _>("provider_target_options_json")
                        .unwrap_or_else(|| "{}".to_string()),
                    secret_ref: row.get("provider_secret_ref"),
                    status: row
                        .get::<Option<String>, _>("provider_status")
                        .unwrap_or_else(|| "ok".to_string()),
                    sort_order: row
                        .get::<Option<i64>, _>("provider_sort_order")
                        .unwrap_or(0),
                    created_at: row
                        .get::<Option<String>, _>("provider_created_at")
                        .unwrap_or_default(),
                    updated_at: row
                        .get::<Option<String>, _>("provider_updated_at")
                        .unwrap_or_default(),
                });

                TargetSwitchStatus {
                    target: TargetApp {
                        id: row.get("target_id"),
                        key: row.get("target_key"),
                        display_name: row.get("target_display_name"),
                        enabled: row.get("target_enabled"),
                        sort_order: row.get("target_sort_order"),
                        created_at: row.get("target_created_at"),
                        updated_at: row.get("target_updated_at"),
                    },
                    active_provider,
                    last_write_status: row.get("last_write_status"),
                    last_error_code: row.get("last_error_code"),
                    last_written_at: row.get("last_written_at"),
                    last_snapshot_path: row.get("snapshot_path"),
                    last_snapshot_id: row.get("snapshot_id"),
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::provider_repository::ProviderRepository;
    use crate::database::repositories::target_repository::TargetRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::provider::NewProvider;

    #[tokio::test]
    async fn upsert_provider_state_and_list_statuses_return_active_provider() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let provider = ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Acme Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://api.example.com/v1".to_string()),
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: Some("secret://provider/acme".to_string()),
            },
        )
        .await
        .expect("provider");

        let state = TargetStateRepository::upsert_provider_state(
            &pool,
            &targets[2].id,
            &provider.id,
            "written",
            None,
            "2026-07-13T00:00:00Z",
        )
        .await
        .expect("state");
        let statuses = TargetStateRepository::list_switch_statuses(&pool)
            .await
            .expect("statuses");
        let codex = statuses
            .iter()
            .find(|status| status.target.key == "codex")
            .expect("codex status");

        assert_eq!(state.active_item_type.as_deref(), Some("provider"));
        assert_eq!(state.active_item_id.as_deref(), Some(provider.id.as_str()));
        assert_eq!(
            codex
                .active_provider
                .as_ref()
                .map(|item| item.name.as_str()),
            Some("Acme Provider")
        );
        assert_eq!(codex.last_write_status.as_deref(), Some("written"));
    }
}
