use crate::error::AppError;
use crate::models::route_pool::{RoutePoolMemberAccount, RoutePoolStats, RoutePoolUsageLog};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct RoutePoolRepository;

impl RoutePoolRepository {
    pub async fn list_member_ids(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query(
            "SELECT route_credential_id
             FROM route_pool_members
             WHERE platform = ? AND enabled = 1
             ORDER BY sort_order ASC, created_at ASC",
        )
        .bind(platform)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_members",
            message: "Could not load route pool members".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("route_credential_id"))
            .collect())
    }

    pub async fn replace_members(
        pool: &SqlitePool,
        platform: &str,
        account_ids: &[String],
    ) -> Result<Vec<String>, AppError> {
        let mut tx = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_pool_tx",
            message: "Could not start route pool update".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        sqlx::query("DELETE FROM route_pool_members WHERE platform = ?")
            .bind(platform)
            .execute(&mut *tx)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_delete",
                message: "Could not clear route pool members".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let now = Utc::now().to_rfc3339();
        for (index, account_id) in account_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO route_pool_members
                 (id, platform, route_credential_id, enabled, sort_order, created_at, updated_at)
                 VALUES (?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(platform)
            .bind(account_id)
            .bind(index as i64)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_insert",
                message: "Could not add route pool member".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;
        }

        tx.commit().await.map_err(|err| AppError::Database {
            code: "database.route_pool_commit",
            message: "Could not save route pool members".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::list_member_ids(pool, platform).await
    }

    pub async fn member_accounts(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Vec<RoutePoolMemberAccount>, AppError> {
        let rows = sqlx::query(
            "SELECT a.id, a.display_name
             FROM route_pool_members rpm
             INNER JOIN route_credentials a ON a.id = rpm.route_credential_id
             WHERE rpm.platform = ? AND rpm.enabled = 1 AND a.status = 'ok'
             ORDER BY rpm.sort_order ASC, rpm.created_at ASC",
        )
        .bind(platform)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_member_accounts",
            message: "Could not load route pool account records".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .map(|row| RoutePoolMemberAccount {
                id: row.get("id"),
                display_name: row.get("display_name"),
            })
            .collect())
    }

    pub async fn next_cursor_index(pool: &SqlitePool, platform: &str) -> Result<i64, AppError> {
        let row = sqlx::query("SELECT next_index FROM route_pool_cursors WHERE platform = ?")
            .bind(platform)
            .fetch_optional(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_cursor_get",
                message: "Could not load route pool cursor".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(row.map(|row| row.get("next_index")).unwrap_or(0))
    }

    pub async fn save_cursor_index(
        pool: &SqlitePool,
        platform: &str,
        next_index: i64,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO route_pool_cursors (platform, next_index, updated_at)
             VALUES (?, ?, ?)
             ON CONFLICT(platform) DO UPDATE SET next_index = excluded.next_index, updated_at = excluded.updated_at",
        )
        .bind(platform)
        .bind(next_index)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_cursor_save",
            message: "Could not save route pool cursor".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(())
    }

    pub async fn insert_usage_event(
        pool: &SqlitePool,
        account_id: &str,
        source_label: &str,
        metric_type: &str,
        amount: i64,
        unit: &str,
        metadata_json: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO usage_events
             (id, route_credential_id, source_label, metric_type, amount, unit, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(account_id)
        .bind(source_label)
        .bind(metric_type)
        .bind(amount)
        .bind(unit)
        .bind(metadata_json)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.usage_event_insert",
            message: "Could not record route usage event".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(())
    }

    pub async fn stats(pool: &SqlitePool, platform: &str) -> Result<RoutePoolStats, AppError> {
        let row = sqlx::query(
            "SELECT
               COUNT(DISTINCT rpm.route_credential_id) AS member_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN CASE WHEN ue.amount > 0 THEN ue.amount ELSE 1 END ELSE 0 END), 0) AS request_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'token' OR ue.unit = 'token' THEN ue.amount ELSE 0 END), 0) AS token_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'cost' AND ue.unit = 'usd_micros' THEN ue.amount ELSE 0 END), 0) AS cost_micros
             FROM route_pool_members rpm
             LEFT JOIN usage_events ue ON ue.route_credential_id = rpm.route_credential_id
             WHERE rpm.platform = ? AND rpm.enabled = 1",
        )
        .bind(platform)
        .fetch_one(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_stats",
            message: "Could not load route pool statistics".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        let log_rows = sqlx::query(
            "SELECT ue.id, ue.route_credential_id, a.display_name AS account_name,
                    ue.metric_type, ue.amount, ue.unit, ue.metadata_json, ue.created_at
             FROM usage_events ue
             INNER JOIN route_pool_members rpm
               ON rpm.route_credential_id = ue.route_credential_id
              AND rpm.platform = ?
              AND rpm.enabled = 1
             LEFT JOIN route_credentials a ON a.id = ue.route_credential_id
             ORDER BY ue.created_at DESC
             LIMIT 10",
        )
        .bind(platform)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_logs",
            message: "Could not load route pool logs".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(RoutePoolStats {
            member_count: row.get("member_count"),
            request_count: row.get("request_count"),
            token_count: row.get("token_count"),
            cost_micros: row.get("cost_micros"),
            recent_logs: log_rows
                .into_iter()
                .map(|row| RoutePoolUsageLog {
                    id: row.get("id"),
                    account_id: row.get("route_credential_id"),
                    account_name: row.get("account_name"),
                    metric_type: row.get("metric_type"),
                    amount: row.get("amount"),
                    unit: row.get("unit"),
                    metadata_json: row.get("metadata_json"),
                    created_at: row.get("created_at"),
                })
                .collect(),
        })
    }
}
