use crate::error::AppError;
use crate::models::route_pool::{
    RoutePoolMemberAccount, RoutePoolStats, RoutePoolUsageLog, RouteUsageBreakdown,
};
use chrono::Utc;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashSet;
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

    pub async fn pool_membership_map(
        pool: &SqlitePool,
        platform: &str,
        ids: &[String],
    ) -> Result<HashSet<String>, AppError> {
        let mut seen = HashSet::with_capacity(ids.len());
        let unique_ids = ids
            .iter()
            .filter(|id| seen.insert(id.as_str()))
            .collect::<Vec<_>>();
        if unique_ids.is_empty() {
            return Ok(HashSet::new());
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT route_credential_id
             FROM route_pool_members
             WHERE platform = ",
        );
        query
            .push_bind(platform)
            .push(" AND enabled = 1 AND route_credential_id IN (");
        let mut separated = query.separated(", ");
        for id in unique_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        query
            .build_query_scalar::<String>()
            .fetch_all(pool)
            .await
            .map(|rows| rows.into_iter().collect())
            .map_err(|err| AppError::Database {
                code: "database.route_pool_membership_map",
                message: "Could not load route pool membership".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
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

    pub async fn append_members_tx(
        tx: &mut Transaction<'_, Sqlite>,
        platform: &str,
        credential_ids: &[String],
    ) -> Result<usize, AppError> {
        if credential_ids.is_empty() {
            return Ok(0);
        }

        let mut current_max = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(sort_order) FROM route_pool_members WHERE platform = ?",
        )
        .bind(platform)
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_pool_append_order",
            message: "Could not allocate route pool member order".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?
        .unwrap_or(-1);
        let now = Utc::now().to_rfc3339();
        let mut inserted = 0usize;

        for credential_id in credential_ids {
            let next_sort_order = current_max.saturating_add(1);
            let result = sqlx::query(
                "INSERT INTO route_pool_members
                 (id, platform, route_credential_id, enabled, sort_order, created_at, updated_at)
                 VALUES (?, ?, ?, 1, ?, ?, ?)
                 ON CONFLICT(platform, route_credential_id) DO NOTHING",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(platform)
            .bind(credential_id)
            .bind(next_sort_order)
            .bind(&now)
            .bind(&now)
            .execute(&mut **tx)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_append",
                message: "Could not append route pool member".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

            if result.rows_affected() == 1 {
                current_max = next_sort_order;
                inserted += 1;
            }
        }

        Ok(inserted)
    }

    pub async fn member_accounts(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Vec<RoutePoolMemberAccount>, AppError> {
        let rows = sqlx::query(
            "SELECT a.id, a.display_name, a.status, a.route_priority, a.max_concurrency
             FROM route_pool_members rpm
             INNER JOIN route_credentials a ON a.id = rpm.route_credential_id
             WHERE rpm.platform = ? AND rpm.enabled = 1 AND a.archived_at IS NULL
             ORDER BY a.route_priority ASC, rpm.sort_order ASC, rpm.created_at ASC",
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
                status: row.get("status"),
                route_priority: row.get("route_priority"),
                max_concurrency: row.get("max_concurrency"),
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

    pub async fn insert_request_event(
        pool: &SqlitePool,
        account_id: &str,
        source_label: &str,
        metadata_json: &str,
        usage: &RouteUsageBreakdown,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO usage_events
             (id, route_credential_id, source_label, metric_type, amount, unit,
              metadata_json, input_tokens, output_tokens, cache_tokens,
              price_usd_micros, price_cny_micros, price_currency, created_at)
             VALUES (?, ?, ?, 'request', 1, 'count', ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(account_id)
        .bind(source_label)
        .bind(metadata_json)
        .bind(usage.input_tokens)
        .bind(usage.output_tokens)
        .bind(usage.cache_tokens)
        .bind(usage.price_usd_micros)
        .bind(usage.price_cny_micros)
        .bind(&usage.price_currency)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.request_usage_insert",
            message: "Could not record route request usage".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(())
    }

    pub async fn stats(
        pool: &SqlitePool,
        platform: &str,
        since: Option<&str>,
        request_page: i64,
        request_page_size: i64,
    ) -> Result<RoutePoolStats, AppError> {
        let usage_since_clause = if since.is_some() {
            " AND ue.created_at >= ?"
        } else {
            ""
        };
        let summary_sql = format!(
            "SELECT
               (SELECT COUNT(DISTINCT rpm.route_credential_id)
                FROM route_pool_members rpm
                INNER JOIN route_credentials a ON a.id = rpm.route_credential_id
                WHERE rpm.platform = ? AND rpm.enabled = 1 AND a.archived_at IS NULL) AS member_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN CASE WHEN ue.amount > 0 THEN ue.amount ELSE 1 END ELSE 0 END), 0) AS request_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.input_tokens, 0) ELSE 0 END), 0) AS input_token_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.output_tokens, 0) ELSE 0 END), 0) AS output_token_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.cache_tokens, 0) ELSE 0 END), 0) AS cache_token_count,
               COALESCE(SUM(CASE WHEN ue.metric_type = 'request' THEN COALESCE(ue.input_tokens, 0) + COALESCE(ue.output_tokens, 0) ELSE 0 END), 0)
                 + COALESCE(SUM(CASE WHEN ue.metric_type = 'token' OR ue.unit = 'token' THEN ue.amount ELSE 0 END), 0) AS token_count,
               COALESCE(SUM(CASE
                   WHEN ue.metric_type = 'request' AND ue.price_currency = 'usd' THEN COALESCE(ue.price_usd_micros, 0)
                   WHEN ue.metric_type = 'request' AND ue.price_currency = 'cny' THEN CAST(ROUND(COALESCE(ue.price_cny_micros, 0) / 7.1) AS INTEGER)
                   WHEN ue.metric_type = 'cost' AND ue.unit = 'usd_micros' THEN ue.amount
                   ELSE 0
               END), 0) AS cost_micros
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND a.archived_at IS NULL{usage_since_clause}"
        );
        let mut summary_query = sqlx::query(&summary_sql).bind(platform).bind(platform);
        if let Some(since) = since {
            summary_query = summary_query.bind(since);
        }
        let row = summary_query
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_stats",
                message: "Could not load route pool statistics".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let log_sql = format!(
            "SELECT ue.id, ue.route_credential_id, a.display_name AS account_name,
                    ue.source_label, ue.metric_type, ue.amount, ue.unit, ue.metadata_json, ue.created_at,
                    ue.input_tokens, ue.output_tokens, ue.cache_tokens,
                    ue.price_usd_micros, ue.price_cny_micros, ue.price_currency
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND a.archived_at IS NULL{usage_since_clause}
             ORDER BY ue.created_at DESC, ue.id DESC
             LIMIT 10"
        );
        let mut log_query = sqlx::query(&log_sql).bind(platform);
        if let Some(since) = since {
            log_query = log_query.bind(since);
        }
        let log_rows = log_query
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_logs",
                message: "Could not load route pool logs".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let request_count_sql = format!(
            "SELECT COUNT(*) AS request_row_count
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND a.archived_at IS NULL AND ue.metric_type = 'request'{usage_since_clause}"
        );
        let mut request_count_query = sqlx::query(&request_count_sql).bind(platform);
        if let Some(since) = since {
            request_count_query = request_count_query.bind(since);
        }
        let request_count_row =
            request_count_query
                .fetch_one(pool)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.route_pool_request_count",
                    message: "Could not count route pool requests".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;
        let request_row_count: i64 = request_count_row.get("request_row_count");

        let request_sql = format!(
            "SELECT ue.id, ue.route_credential_id, a.display_name AS account_name,
                    ue.source_label, ue.metric_type, ue.amount, ue.unit, ue.metadata_json, ue.created_at,
                    ue.input_tokens, ue.output_tokens, ue.cache_tokens,
                    ue.price_usd_micros, ue.price_cny_micros, ue.price_currency
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE a.platform = ? AND a.archived_at IS NULL AND ue.metric_type = 'request'{usage_since_clause}
             ORDER BY ue.created_at DESC, ue.id DESC
             LIMIT ? OFFSET ?"
        );
        let offset = (request_page - 1) * request_page_size;
        let mut request_query = sqlx::query(&request_sql).bind(platform);
        if let Some(since) = since {
            request_query = request_query.bind(since);
        }
        let request_rows = request_query
            .bind(request_page_size)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_pool_requests",
                message: "Could not load route pool requests".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let map_usage_log = |row: sqlx::sqlite::SqliteRow| RoutePoolUsageLog {
            id: row.get("id"),
            account_id: row.get("route_credential_id"),
            account_name: row.get("account_name"),
            source_label: row.get("source_label"),
            metric_type: row.get("metric_type"),
            amount: row.get("amount"),
            unit: row.get("unit"),
            metadata_json: row.get("metadata_json"),
            created_at: row.get("created_at"),
            input_tokens: row.get("input_tokens"),
            output_tokens: row.get("output_tokens"),
            cache_tokens: row.get("cache_tokens"),
            price_usd_micros: row.get("price_usd_micros"),
            price_cny_micros: row.get("price_cny_micros"),
            price_currency: row.get("price_currency"),
        };

        Ok(RoutePoolStats {
            member_count: row.get("member_count"),
            request_count: row.get("request_count"),
            token_count: row.get("token_count"),
            input_token_count: row.get("input_token_count"),
            output_token_count: row.get("output_token_count"),
            cache_token_count: row.get("cache_token_count"),
            cost_micros: row.get("cost_micros"),
            recent_logs: log_rows.into_iter().map(map_usage_log).collect(),
            requests: request_rows.into_iter().map(map_usage_log).collect(),
            request_row_count,
            request_page,
            request_page_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::route_credential_repository::RouteCredentialRepository;

    async fn create_credential(pool: &SqlitePool, platform: &str, display_name: &str) -> String {
        RouteCredentialRepository::create(
            pool,
            platform,
            "api",
            display_name,
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}"#,
            "{}",
        )
        .await
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn member_accounts_excludes_archived_but_membership_ids_are_preserved() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let archived = create_credential(&pool, "codex", "Archived").await;
        let active = create_credential(&pool, "codex", "Active").await;
        RoutePoolRepository::replace_members(&pool, "codex", &[archived.clone(), active.clone()])
            .await
            .unwrap();
        RouteCredentialRepository::set_archived(&pool, std::slice::from_ref(&archived), true)
            .await
            .unwrap();

        let runtime_members = RoutePoolRepository::member_accounts(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(runtime_members.len(), 1);
        assert_eq!(runtime_members[0].id, active);
        assert_eq!(
            RoutePoolRepository::list_member_ids(&pool, "codex")
                .await
                .unwrap(),
            vec![archived, runtime_members[0].id.clone()]
        );
    }

    #[tokio::test]
    async fn pool_membership_map_uses_enabled_members_from_full_selection() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let mut ids = Vec::new();
        for index in 0..21 {
            ids.push(create_credential(&pool, "codex", &format!("Credential {index}")).await);
        }
        let other_platform_id = create_credential(&pool, "claude", "Claude credential").await;
        RoutePoolRepository::replace_members(&pool, "codex", &[ids[0].clone(), ids[20].clone()])
            .await
            .unwrap();
        RoutePoolRepository::replace_members(&pool, "claude", &[other_platform_id.clone()])
            .await
            .unwrap();
        sqlx::query(
            "UPDATE route_pool_members SET enabled = 0 WHERE platform = ? AND route_credential_id = ?",
        )
        .bind("codex")
        .bind(&ids[0])
        .execute(&pool)
        .await
        .unwrap();

        let mut selected_ids = ids.clone();
        selected_ids.push(ids[20].clone());
        selected_ids.push(other_platform_id.clone());
        selected_ids.push("missing".to_string());
        let memberships = RoutePoolRepository::pool_membership_map(&pool, "codex", &selected_ids)
            .await
            .unwrap();

        assert_eq!(
            memberships,
            std::collections::HashSet::from([ids[20].clone()])
        );
    }

    #[tokio::test]
    async fn pool_membership_map_returns_empty_for_empty_input() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();

        let memberships = RoutePoolRepository::pool_membership_map(&pool, "codex", &[])
            .await
            .unwrap();

        assert!(memberships.is_empty());
    }

    #[tokio::test]
    async fn request_usage_event_persists_breakdown_and_converts_cny_cost() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let account_id = create_credential(&pool, "codex", "Usage account").await;
        let usage = RouteUsageBreakdown {
            input_tokens: Some(120),
            output_tokens: Some(30),
            cache_tokens: Some(80),
            price_usd_micros: None,
            price_cny_micros: Some(7_100_000),
            price_currency: Some("cny".to_string()),
        };

        RoutePoolRepository::insert_request_event(
            &pool,
            &account_id,
            "route_proxy",
            r#"{"path":"/chat/completions","status":200}"#,
            &usage,
        )
        .await
        .unwrap();

        let stats = RoutePoolRepository::stats(&pool, "codex", None, 1, 20)
            .await
            .unwrap();
        assert_eq!(stats.input_token_count, 120);
        assert_eq!(stats.output_token_count, 30);
        assert_eq!(stats.cache_token_count, 80);
        assert_eq!(stats.token_count, 150);
        assert_eq!(stats.cost_micros, 1_000_000);
        assert_eq!(stats.requests.len(), 1);
        assert_eq!(stats.requests[0].input_tokens, Some(120));
        assert_eq!(stats.requests[0].output_tokens, Some(30));
        assert_eq!(stats.requests[0].cache_tokens, Some(80));
        assert_eq!(stats.requests[0].price_cny_micros, Some(7_100_000));
        assert_eq!(stats.requests[0].price_currency.as_deref(), Some("cny"));
    }

    #[tokio::test]
    async fn append_members_tx_appends_after_max_without_consuming_duplicate_positions() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let existing = create_credential(&pool, "codex", "Existing").await;
        let high = create_credential(&pool, "codex", "High").await;
        let first = create_credential(&pool, "codex", "First append").await;
        let second = create_credential(&pool, "codex", "Second append").await;
        RoutePoolRepository::replace_members(&pool, "codex", &[existing.clone(), high.clone()])
            .await
            .unwrap();
        sqlx::query(
            "UPDATE route_pool_members SET sort_order = 7 WHERE platform = ? AND route_credential_id = ?",
        )
        .bind("codex")
        .bind(&high)
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        let inserted = RoutePoolRepository::append_members_tx(
            &mut tx,
            "codex",
            &[
                existing.clone(),
                first.clone(),
                first.clone(),
                second.clone(),
            ],
        )
        .await
        .unwrap();
        assert_eq!(inserted, 2);
        tx.commit().await.unwrap();

        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT route_credential_id, sort_order FROM route_pool_members WHERE platform = ? ORDER BY sort_order, route_credential_id",
        )
        .bind("codex")
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![(existing, 0), (high, 7), (first, 8), (second, 9)]
        );
    }

    #[tokio::test]
    async fn append_members_tx_obeys_caller_rollback() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let existing = create_credential(&pool, "claude", "Existing").await;
        let appended = create_credential(&pool, "claude", "Appended").await;
        RoutePoolRepository::replace_members(&pool, "claude", &[existing.clone()])
            .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        assert_eq!(
            RoutePoolRepository::append_members_tx(
                &mut tx,
                "claude",
                std::slice::from_ref(&appended),
            )
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM route_pool_members WHERE platform = ? AND route_credential_id = ?",
            )
            .bind("claude")
            .bind(&appended)
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
            1
        );
        tx.rollback().await.unwrap();

        let members = RoutePoolRepository::list_member_ids(&pool, "claude")
            .await
            .unwrap();
        assert_eq!(members, vec![existing]);
    }
}
