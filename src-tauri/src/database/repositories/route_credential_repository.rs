use crate::error::AppError;
use crate::models::route_credential::{
    ReorderRouteCredentialInput, RouteCredential, RouteCredentialFilterOption,
    RouteCredentialPage, RouteCredentialPageRequest, RouteCredentialPoolScope,
    UpdateRouteCredentialInput,
};
use chrono::Utc;
use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryState {
    pub failure_count: i64,
    pub next_retry_at: Option<String>,
    pub cooldown_until: Option<String>,
}

pub struct RouteCredentialRepository;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct QuotaColumns {
    subscription_type: Option<String>,
    primary_remain: Option<i64>,
    weekly_remain: Option<i64>,
    reset_primary: Option<String>,
    reset_weekly: Option<String>,
    // Legacy single-window columns kept in sync for older readers.
    quota_remaining: Option<i64>,
    quota_limit: Option<i64>,
    quota_used: Option<i64>,
    quota_updated_at: Option<String>,
}

const PAGE_SELECT: &str = "SELECT
    rc.id, rc.platform, rc.kind, rc.display_name, rc.email, rc.status, rc.sort_order,
    rc.batch_id, b.name AS batch_name, rc.secret_payload_json, rc.config_json, rc.preview_json,
    rc.subscription_type, rc.primary_remain, rc.weekly_remain, rc.reset_primary, rc.reset_weekly,
    rc.transient_failure_count, rc.next_retry_at, rc.cooldown_until, rc.last_failure_kind,
    rc.last_failure_message,
    COUNT(ue.id) AS request_count,
    COALESCE(SUM(CASE WHEN json_extract(ue.metadata_json, '$.success') = 1 THEN 1 ELSE 0 END), 0) AS success_count,
    COUNT(ue.id) - COALESCE(SUM(CASE WHEN json_extract(ue.metadata_json, '$.success') = 1 THEN 1 ELSE 0 END), 0) AS failure_count,
    CASE WHEN COUNT(ue.id) = 0 THEN NULL
         ELSE CAST(COALESCE(SUM(CASE WHEN json_extract(ue.metadata_json, '$.success') = 1 THEN 1 ELSE 0 END), 0) AS REAL) * 100.0 / COUNT(ue.id)
    END AS success_rate,
    rc.quota_remaining, rc.quota_limit, rc.quota_used, rc.quota_updated_at,
    rc.created_at, rc.updated_at
 FROM route_credentials rc
 LEFT JOIN batches b ON b.id = rc.batch_id
 LEFT JOIN usage_events ue
   ON ue.route_credential_id = rc.id
  AND ue.source_label IN ('route_proxy', 'route_pool_model_test')
  AND ue.metric_type = 'request'";

fn push_filter_predicate(builder: &mut QueryBuilder<Sqlite>, filters: &[String]) {
    let filters: Vec<&String> = filters.iter().filter(|filter| !filter.trim().is_empty()).collect();
    if filters.is_empty() {
        return;
    }
    builder.push(" AND (");
    for (index, filter) in filters.iter().enumerate() {
        if index > 0 {
            builder.push(" OR ");
        }
        if filter.as_str() == "__single__" {
            builder.push("rc.batch_id IS NULL");
        } else {
            builder.push("rc.batch_id = ").push_bind(filter.to_string());
        }
    }
    builder.push(")");
}

fn push_pool_scope_predicate(
    builder: &mut QueryBuilder<Sqlite>,
    scope: RouteCredentialPoolScope,
) {
    builder.push(" AND ");
    if matches!(scope, RouteCredentialPoolScope::OutOfPool) {
        builder.push("NOT ");
    }
    builder.push(
        "EXISTS (
            SELECT 1 FROM route_pool_members rpm
            WHERE rpm.platform = rc.platform
              AND rpm.route_credential_id = rc.id
              AND rpm.enabled = 1
        )",
    );
}

fn database_error(code: &'static str, message: &str, error: impl ToString) -> AppError {
    AppError::Database {
        code,
        message: message.to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    }
}

fn reorder_validation_error(message: &str) -> AppError {
    AppError::Validation {
        code: "validation.route_credential_reorder",
        message: message.to_string(),
        details: None,
        recoverable: true,
    }
}

async fn boundary_id(
    pool: &SqlitePool,
    request: &RouteCredentialPageRequest,
    offset: i64,
) -> Result<Option<String>, AppError> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT rc.id FROM route_credentials rc WHERE rc.platform = ",
    );
    query.push_bind(&request.platform);
    push_pool_scope_predicate(&mut query, request.pool_scope);
    push_filter_predicate(&mut query, &request.filters);
    query
        .push(" ORDER BY rc.sort_order ASC, rc.created_at DESC LIMIT 1 OFFSET ")
        .push_bind(offset);
    query
        .build_query_scalar::<String>()
        .fetch_optional(pool)
        .await
        .map_err(|err| database_error("database.route_credential_boundary", "Could not load page boundary", err))
}

async fn load_filter_options(
    pool: &SqlitePool,
    platform: &str,
) -> Result<Vec<RouteCredentialFilterOption>, AppError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT b.id, b.name FROM batches b WHERE EXISTS (
             SELECT 1 FROM route_credentials rc WHERE rc.platform = ? AND rc.batch_id = b.id
         ) ORDER BY b.sort_order ASC, b.created_at ASC, b.id ASC",
    )
    .bind(platform)
    .fetch_all(pool)
    .await
    .map_err(|err| database_error("database.route_credential_filter_options", "Could not load account filters", err))?;
    let has_single = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM route_credentials WHERE platform = ? AND batch_id IS NULL)",
    )
    .bind(platform)
    .fetch_one(pool)
    .await
    .map_err(|err| database_error("database.route_credential_filter_single", "Could not load unbatched filter", err))?;
    let mut options = rows
        .into_iter()
        .map(|(key, label)| RouteCredentialFilterOption { key, label })
        .collect::<Vec<_>>();
    if has_single != 0 {
        options.push(RouteCredentialFilterOption {
            key: "__single__".to_string(),
            label: "未分组".to_string(),
        });
    }
    Ok(options)
}

fn quota_columns_from_config_json(config_json: &str) -> QuotaColumns {
    let Ok(value) = serde_json::from_str::<Value>(config_json) else {
        return QuotaColumns::default();
    };
    let Some(object) = value.as_object() else {
        return QuotaColumns::default();
    };

    let subscription_type = object
        .get("subscription_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);
    let primary_remain =
        json_i64(object.get("primary_remain")).or_else(|| json_i64(object.get("quota_remaining")));
    let weekly_remain = json_i64(object.get("weekly_remain"));
    let reset_primary = json_string(object.get("reset_primary"))
        .or_else(|| json_string(object.get("quota_updated_at")));
    let reset_weekly = json_string(object.get("reset_weekly"));
    // Dual-write legacy remaining from the primary window when present.
    let quota_remaining = json_i64(object.get("quota_remaining")).or(primary_remain);
    let quota_limit = json_i64(object.get("quota_limit"));
    let quota_used = json_i64(object.get("quota_used"));
    let quota_updated_at = json_string(object.get("quota_updated_at")).or_else(|| {
        // Prefer the latest known reset time for legacy "updated at" display.
        match (&reset_primary, &reset_weekly) {
            (Some(primary), Some(weekly)) => {
                if primary.as_str() >= weekly.as_str() {
                    Some(primary.clone())
                } else {
                    Some(weekly.clone())
                }
            }
            (Some(primary), None) => Some(primary.clone()),
            (None, Some(weekly)) => Some(weekly.clone()),
            (None, None) => None,
        }
    });

    QuotaColumns {
        subscription_type,
        primary_remain,
        weekly_remain,
        reset_primary,
        reset_weekly,
        quota_remaining,
        quota_limit,
        quota_used,
        quota_updated_at,
    }
}

fn json_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

impl RouteCredentialRepository {
    pub async fn create(
        pool: &SqlitePool,
        platform: &str,
        kind: &str,
        display_name: &str,
        email: Option<String>,
        status: &str,
        batch_id: Option<String>,
        secret_payload_json: &str,
        config_json: &str,
        preview_json: &str,
    ) -> Result<RouteCredential, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        let quota = quota_columns_from_config_json(config_json);
        let mut tx = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_credential_create_tx",
            message: "Could not start route credential create transaction".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        let sort_order = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(sort_order) FROM route_credentials WHERE platform = ?",
        )
        .bind(platform)
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_sort_order",
            message: "Could not allocate route credential order".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?
        .unwrap_or(-1)
        .saturating_add(1);
        sqlx::query(
            "INSERT INTO route_credentials (
                id, platform, kind, display_name, email, status, sort_order, batch_id,
                secret_payload_json, config_json, preview_json,
                subscription_type, primary_remain, weekly_remain, reset_primary, reset_weekly,
                quota_remaining, quota_limit, quota_used, quota_updated_at,
                created_at, updated_at
             )
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(platform)
        .bind(kind)
        .bind(display_name)
        .bind(email)
        .bind(status)
        .bind(sort_order)
        .bind(batch_id)
        .bind(secret_payload_json)
        .bind(config_json)
        .bind(preview_json)
        .bind(quota.subscription_type)
        .bind(quota.primary_remain)
        .bind(quota.weekly_remain)
        .bind(quota.reset_primary)
        .bind(quota.reset_weekly)
        .bind(quota.quota_remaining)
        .bind(quota.quota_limit)
        .bind(quota.quota_used)
        .bind(quota.quota_updated_at)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_create",
            message: "Could not create route credential".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        tx.commit().await.map_err(|err| AppError::Database {
            code: "database.route_credential_create_commit",
            message: "Could not save route credential".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<RouteCredential, AppError> {
        sqlx::query_as::<_, RouteCredential>(
            "SELECT
                rc.id,
                rc.platform,
                rc.kind,
                rc.display_name,
                rc.email,
                rc.status,
                rc.sort_order,
                rc.batch_id,
                b.name AS batch_name,
                rc.secret_payload_json,
                rc.config_json,
                rc.preview_json,
                rc.subscription_type,
                rc.primary_remain,
                rc.weekly_remain,
                rc.reset_primary,
                rc.reset_weekly,
                rc.transient_failure_count,
                rc.next_retry_at,
                rc.cooldown_until,
                rc.last_failure_kind,
                rc.last_failure_message,
                rc.quota_remaining,
                rc.quota_limit,
                rc.quota_used,
                rc.quota_updated_at,
                rc.created_at,
                rc.updated_at
             FROM route_credentials rc
             LEFT JOIN batches b ON b.id = rc.batch_id
             WHERE rc.id = ?",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_get",
            message: "Could not load route credential".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn list_by_platform(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Vec<RouteCredential>, AppError> {
        sqlx::query_as::<_, RouteCredential>(
            "SELECT
                rc.id,
                rc.platform,
                rc.kind,
                rc.display_name,
                rc.email,
                rc.status,
                rc.sort_order,
                rc.batch_id,
                b.name AS batch_name,
                rc.secret_payload_json,
                rc.config_json,
                rc.preview_json,
                rc.subscription_type,
                rc.primary_remain,
                rc.weekly_remain,
                rc.reset_primary,
                rc.reset_weekly,
                rc.transient_failure_count,
                rc.next_retry_at,
                rc.cooldown_until,
                rc.last_failure_kind,
                rc.last_failure_message,
                COUNT(ue.id) AS request_count,
                COALESCE(SUM(CASE WHEN json_extract(ue.metadata_json, '$.success') = 1 THEN 1 ELSE 0 END), 0) AS success_count,
                COUNT(ue.id) - COALESCE(SUM(CASE WHEN json_extract(ue.metadata_json, '$.success') = 1 THEN 1 ELSE 0 END), 0) AS failure_count,
                CASE WHEN COUNT(ue.id) = 0 THEN NULL
                     ELSE CAST(COALESCE(SUM(CASE WHEN json_extract(ue.metadata_json, '$.success') = 1 THEN 1 ELSE 0 END), 0) AS REAL) * 100.0 / COUNT(ue.id)
                END AS success_rate,
                rc.quota_remaining,
                rc.quota_limit,
                rc.quota_used,
                rc.quota_updated_at,
                rc.created_at,
                rc.updated_at
             FROM route_credentials rc
             LEFT JOIN batches b ON b.id = rc.batch_id
             LEFT JOIN usage_events ue
               ON ue.route_credential_id = rc.id
              AND ue.source_label IN ('route_proxy', 'route_pool_model_test')
              AND ue.metric_type = 'request'
             WHERE rc.platform = ?
             GROUP BY rc.id
             ORDER BY rc.sort_order ASC, rc.created_at DESC",
        )
        .bind(platform)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_list",
            message: "Could not list route credentials".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn page(
        pool: &SqlitePool,
        request: RouteCredentialPageRequest,
    ) -> Result<RouteCredentialPage, AppError> {
        let page_size = request.normalized_page_size().map_err(|message| AppError::Validation {
            code: "validation.route_credential_page_size",
            message,
            details: None,
            recoverable: true,
        })?;
        let requested_page = request.normalized_page();

        let mut count_query = QueryBuilder::<Sqlite>::new(
            "SELECT COUNT(*) FROM route_credentials rc WHERE rc.platform = ",
        );
        count_query.push_bind(&request.platform);
        push_pool_scope_predicate(&mut count_query, request.pool_scope);
        push_filter_predicate(&mut count_query, &request.filters);
        let total: i64 = count_query
            .build_query_scalar()
            .fetch_one(pool)
            .await
            .map_err(|err| database_error("database.route_credential_page_count", "Could not count route credentials", err))?;
        let page_count = if total == 0 { 1 } else { (total + page_size - 1) / page_size };
        let page = requested_page.min(page_count);
        let offset = (page - 1) * page_size;

        let mut item_query = QueryBuilder::<Sqlite>::new(PAGE_SELECT);
        item_query.push(" WHERE rc.platform = ").push_bind(&request.platform);
        push_pool_scope_predicate(&mut item_query, request.pool_scope);
        push_filter_predicate(&mut item_query, &request.filters);
        item_query
            .push(" GROUP BY rc.id ORDER BY rc.sort_order ASC, rc.created_at DESC LIMIT ")
            .push_bind(page_size)
            .push(" OFFSET ")
            .push_bind(offset);
        let items = item_query
            .build_query_as::<RouteCredential>()
            .fetch_all(pool)
            .await
            .map_err(|err| database_error("database.route_credential_page_items", "Could not load route credential page", err))?;

        let previous_page_account_id = if page > 1 {
            boundary_id(pool, &request, offset - 1).await?
        } else {
            None
        };
        let next_page_account_id = if page < page_count {
            boundary_id(pool, &request, offset + page_size).await?
        } else {
            None
        };

        let filter_options = load_filter_options(pool, &request.platform).await?;
        let official_account_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM route_credentials WHERE platform = ? AND kind = 'official'",
        )
        .bind(&request.platform)
        .fetch_one(pool)
        .await
        .map_err(|err| database_error("database.route_credential_official_count", "Could not count official accounts", err))?;

        Ok(RouteCredentialPage {
            items,
            total,
            page,
            page_count,
            page_size,
            previous_page_account_id,
            next_page_account_id,
            filter_options,
            official_account_count,
        })
    }

    pub async fn reorder(
        pool: &SqlitePool,
        input: ReorderRouteCredentialInput,
    ) -> Result<RouteCredentialPage, AppError> {
        let page_size = match input.page_size {
            20 | 50 | 100 => input.page_size,
            _ => return Err(AppError::Validation {
                code: "validation.route_credential_page_size",
                message: "page_size must be 20, 50, or 100".to_string(),
                details: None,
                recoverable: true,
            }),
        };
        let mut tx = pool.begin().await.map_err(|err| database_error("database.route_credential_reorder_tx", "Could not start account reorder", err))?;
        let rows = sqlx::query_as::<_, (String, Option<String>, i64)>(
            "SELECT rc.id, rc.batch_id,
                    EXISTS (
                        SELECT 1 FROM route_pool_members rpm
                        WHERE rpm.platform = rc.platform
                          AND rpm.route_credential_id = rc.id
                          AND rpm.enabled = 1
                    ) AS in_pool
             FROM route_credentials rc
             WHERE rc.platform = ?
             ORDER BY rc.sort_order ASC, rc.created_at DESC, rc.id ASC",
        )
        .bind(&input.platform)
        .fetch_all(&mut *tx)
        .await
        .map_err(|err| database_error("database.route_credential_reorder_read", "Could not load account order", err))?;
        let all_ids: Vec<String> = rows.iter().map(|(id, _, _)| id.clone()).collect();
        let pool_matches = |in_pool: i64| match input.pool_scope {
            RouteCredentialPoolScope::InPool => in_pool != 0,
            RouteCredentialPoolScope::OutOfPool => in_pool == 0,
        };
        let matches = |batch_id: &Option<String>, in_pool: i64| {
            (input.filters.is_empty()
                || input.filters.iter().any(|filter| {
                    (filter == "__single__" && batch_id.is_none())
                        || batch_id.as_deref() == Some(filter.as_str())
                }))
                && pool_matches(in_pool)
        };
        let filtered_ids: Vec<String> = rows
            .iter()
            .filter(|(_, batch_id, in_pool)| matches(batch_id, *in_pool))
            .map(|(id, _, _)| id.clone())
            .collect();
        let Some(moved_index) = filtered_ids.iter().position(|id| id == &input.moved_account_id) else {
            return Err(reorder_validation_error("Moved route credential is not in the active filter"));
        };
        let previous_account_id = input
            .previous_account_id
            .as_deref()
            .filter(|id| *id != input.moved_account_id);
        let next_account_id = input
            .next_account_id
            .as_deref()
            .filter(|id| *id != input.moved_account_id);
        let mut remaining = filtered_ids.clone();
        remaining.remove(moved_index);
        if let Some(previous) = previous_account_id {
            if !remaining.iter().any(|id| id == previous) {
                return Err(reorder_validation_error("Previous route credential is invalid"));
            }
        }
        if let Some(next) = next_account_id {
            if !remaining.iter().any(|id| id == next) {
                return Err(reorder_validation_error("Next route credential is invalid"));
            }
        }
        let insert_at = if let Some(next) = next_account_id {
            let index = remaining.iter().position(|id| id == next).unwrap();
            if previous_account_id != remaining.get(index.saturating_sub(1)).map(String::as_str) {
                return Err(reorder_validation_error("Route credential neighbors are not adjacent"));
            }
            index
        } else if let Some(previous) = previous_account_id {
            let index = remaining.iter().position(|id| id == previous).unwrap();
            if remaining.get(index + 1).is_some() {
                return Err(reorder_validation_error("Route credential neighbors are not adjacent"));
            }
            index + 1
        } else if remaining.is_empty() {
            0
        } else {
            return Err(reorder_validation_error("A reorder neighbor is required"));
        };
        remaining.insert(insert_at, input.moved_account_id.clone());
        let mut reordered = all_ids.clone();
        let mut filtered_cursor = 0;
        for id in &mut reordered {
            let (_, batch_id, in_pool) = rows.iter().find(|(row_id, _, _)| row_id == id).unwrap();
            if matches(batch_id, *in_pool) {
                *id = remaining[filtered_cursor].clone();
                filtered_cursor += 1;
            }
        }
        let now = Utc::now().to_rfc3339();
        for (sort_order, id) in reordered.iter().enumerate() {
            sqlx::query("UPDATE route_credentials SET sort_order = ?, updated_at = ? WHERE id = ?")
                .bind(sort_order as i64)
                .bind(&now)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|err| database_error("database.route_credential_reorder_update", "Could not save account order", err))?;
        }
        tx.commit().await.map_err(|err| database_error("database.route_credential_reorder_commit", "Could not commit account order", err))?;
        let moved_position = remaining.iter().position(|id| id == &input.moved_account_id).unwrap_or(0);
        Self::page(pool, RouteCredentialPageRequest {
            platform: input.platform,
            page: moved_position as i64 / page_size + 1,
            page_size,
            filters: input.filters,
            pool_scope: input.pool_scope,
        }).await
    }

    pub async fn update(
        pool: &SqlitePool,
        id: &str,
        input: &UpdateRouteCredentialInput,
    ) -> Result<RouteCredential, AppError> {
        let now = Utc::now().to_rfc3339();

        let quota = quota_columns_from_config_json(&input.config_json);
        let result = sqlx::query(
            "UPDATE route_credentials
             SET display_name = ?, email = ?, status = ?, secret_payload_json = ?,
                 config_json = ?, preview_json = ?,
                 subscription_type = ?, primary_remain = ?, weekly_remain = ?,
                 reset_primary = ?, reset_weekly = ?,
                 quota_remaining = ?, quota_limit = ?,
                 quota_used = ?, quota_updated_at = ?, updated_at = ?
            WHERE id = ? AND status != 'revoked'",
        )
        .bind(&input.display_name)
        .bind(&input.email)
        .bind(&input.status)
        .bind(&input.secret_payload_json)
        .bind(&input.config_json)
        .bind(&input.preview_json)
        .bind(quota.subscription_type)
        .bind(quota.primary_remain)
        .bind(quota.weekly_remain)
        .bind(quota.reset_primary)
        .bind(quota.reset_weekly)
        .bind(quota.quota_remaining)
        .bind(quota.quota_limit)
        .bind(quota.quota_used)
        .bind(quota.quota_updated_at)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_update",
            message: "Could not update route credential".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "Route credential does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        }

        Self::get(pool, id).await
    }

    pub async fn update_secret_and_config(
        pool: &SqlitePool,
        id: &str,
        secret_payload_json: &str,
        config_json: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let quota = quota_columns_from_config_json(config_json);
        let result = sqlx::query(
            "UPDATE route_credentials
             SET secret_payload_json = ?, config_json = ?,
                 subscription_type = ?, primary_remain = ?, weekly_remain = ?,
                 reset_primary = ?, reset_weekly = ?,
                 quota_remaining = ?, quota_limit = ?,
                 quota_used = ?, quota_updated_at = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(secret_payload_json)
        .bind(config_json)
        .bind(quota.subscription_type)
        .bind(quota.primary_remain)
        .bind(quota.weekly_remain)
        .bind(quota.reset_primary)
        .bind(quota.reset_weekly)
        .bind(quota.quota_remaining)
        .bind(quota.quota_limit)
        .bind(quota.quota_used)
        .bind(quota.quota_updated_at)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_secret_update",
            message: "Could not update route credential tokens".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "Route credential does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        }

        Ok(())
    }

    pub async fn update_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE route_credentials
             SET status = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_status_update",
            message: "Could not update route credential status".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "Route credential does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        }

        Ok(())
    }

    pub async fn record_transient_failure(
        pool: &SqlitePool,
        id: &str,
        kind: &str,
        message: &str,
    ) -> Result<RetryState, AppError> {
        let mut tx = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_credential_retry_tx",
            message: "Could not start route credential retry update".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        let current = sqlx::query_scalar::<_, i64>(
            "SELECT transient_failure_count FROM route_credentials WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_retry_read",
            message: "Could not read route credential retry state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        let Some(current) = current else {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "Route credential does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        };

        let failure_count = current.saturating_add(1);
        let base_seconds = match failure_count {
            1 => 30,
            2 => 120,
            _ => 600,
        };
        let jitter_seconds = jitter_seconds(id, failure_count, base_seconds);
        let retry_at = Utc::now() + chrono::Duration::seconds(jitter_seconds);
        let retry_at = retry_at.to_rfc3339();
        let cooldown_until = if failure_count >= 3 {
            Some(retry_at.clone())
        } else {
            None
        };
        let message = truncate_failure_message(message);
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE route_credentials
             SET transient_failure_count = ?, next_retry_at = ?, cooldown_until = ?,
                 last_failure_kind = ?, last_failure_message = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(failure_count)
        .bind(&retry_at)
        .bind(&cooldown_until)
        .bind(kind)
        .bind(&message)
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_retry_update",
            message: "Could not update route credential retry state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        tx.commit().await.map_err(|err| AppError::Database {
            code: "database.route_credential_retry_commit",
            message: "Could not save route credential retry state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(RetryState {
            failure_count,
            next_retry_at: Some(retry_at),
            cooldown_until,
        })
    }

    pub async fn clear_transient_failure(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE route_credentials
             SET transient_failure_count = 0, next_retry_at = NULL, cooldown_until = NULL,
                 last_failure_kind = NULL, last_failure_message = NULL, updated_at = ?
             WHERE id = ?",
        )
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_retry_clear",
            message: "Could not clear route credential retry state".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        if result.rows_affected() == 0 {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "Route credential does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        }
        Ok(())
    }

    pub async fn record_semantic_failure(
        pool: &SqlitePool,
        id: &str,
        message: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE route_credentials
             SET status = 'error', transient_failure_count = 0,
                 next_retry_at = NULL, cooldown_until = NULL,
                 last_failure_kind = 'semantic_response_failed',
                 last_failure_message = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(truncate_failure_message(message))
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| database_error("database.route_credential_semantic_failure", "Could not record semantic response failure", err))?;
        if result.rows_affected() == 0 {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "Route credential does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        }
        Ok(())
    }

    pub async fn recover_after_explicit_test(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE route_credentials
             SET status = 'ok', transient_failure_count = 0,
                 next_retry_at = NULL, cooldown_until = NULL,
                 last_failure_kind = NULL, last_failure_message = NULL,
                 updated_at = ?
             WHERE id = ? AND status != 'revoked'",
        )
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| database_error("database.route_credential_recover", "Could not recover route credential", err))?;
        if result.rows_affected() == 0 {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM route_credentials WHERE id = ?",
            )
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| database_error("database.route_credential_recover", "Could not verify route credential", err))?;
            if exists == 0 {
                return Err(AppError::Validation {
                    code: "validation.route_credential_not_found",
                    message: "Route credential does not exist".to_string(),
                    details: Some(id.to_string()),
                    recoverable: true,
                });
            }
        }
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM route_credentials WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_credential_delete",
                message: "Could not delete route credential".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "Route credential does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        }

        Ok(())
    }

    pub async fn platform_of(pool: &SqlitePool, id: &str) -> Result<String, AppError> {
        let row =
            sqlx::query_scalar::<_, String>("SELECT platform FROM route_credentials WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.route_credential_platform",
                    message: "Could not load route credential platform".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;

        row.ok_or_else(|| AppError::Validation {
            code: "validation.route_credential_not_found",
            message: "Route credential does not exist".to_string(),
            details: Some(id.to_string()),
            recoverable: true,
        })
    }
}

fn truncate_failure_message(message: &str) -> String {
    let end = message
        .char_indices()
        .take_while(|(index, character)| *index + character.len_utf8() <= 512)
        .map(|(index, character)| index + character.len_utf8())
        .last()
        .unwrap_or(0);
    message[..end].to_string()
}

fn jitter_seconds(id: &str, failure_count: i64, base_seconds: i64) -> i64 {
    let seed = id.bytes().fold(failure_count as u64, |value, byte| {
        value.wrapping_mul(31).wrapping_add(byte as u64)
    });
    let jitter_percent = 80 + (seed % 41) as i64;
    (base_seconds * jitter_percent / 100).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_api_credential(
        pool: &SqlitePool,
        platform: &str,
        display_name: &str,
    ) -> RouteCredential {
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
    }

    fn page_request(
        platform: &str,
        page: i64,
        pool_scope: RouteCredentialPoolScope,
    ) -> RouteCredentialPageRequest {
        RouteCredentialPageRequest {
            platform: platform.to_string(),
            page,
            page_size: 20,
            filters: Vec::new(),
            pool_scope,
        }
    }

    #[tokio::test]
    async fn create_and_list_api_credential() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Demo API",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}"#,
            r#"{"auth_json":"{}","config_toml":""}"#,
        )
        .await
        .unwrap();
        let listed = RouteCredentialRepository::list_by_platform(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);
        assert_eq!(listed[0].kind, "api");
        assert_eq!(listed[0].request_count, 0);
        assert_eq!(listed[0].success_count, 0);
        assert_eq!(listed[0].failure_count, 0);
        assert_eq!(listed[0].success_rate, None);
    }

    #[tokio::test]
    async fn list_includes_request_success_statistics() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Demo API",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}"#,
            r#"{}"#,
        )
        .await
        .unwrap();
        for success in [true, true, false] {
            crate::database::repositories::route_pool_repository::RoutePoolRepository::insert_usage_event(
                &pool,
                &created.id,
                "route_proxy",
                "request",
                1,
                "count",
                &serde_json::json!({"success": success}).to_string(),
            )
            .await
            .unwrap();
        }
        let listed = RouteCredentialRepository::list_by_platform(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(listed[0].request_count, 3);
        assert_eq!(listed[0].success_count, 2);
        assert_eq!(listed[0].failure_count, 1);
        assert!((listed[0].success_rate.unwrap() - (200.0 / 3.0)).abs() < 0.01);
    }

    #[tokio::test]
    async fn page_filters_pool_scope_and_clamps_empty_or_oversized_pages() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let first = create_api_credential(&pool, "codex", "First").await;
        let second = create_api_credential(&pool, "codex", "Second").await;
        let outside = create_api_credential(&pool, "codex", "Outside").await;
        crate::database::repositories::route_pool_repository::RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[first.id.clone(), second.id.clone()],
        )
        .await
        .unwrap();

        let in_pool = RouteCredentialRepository::page(
            &pool,
            page_request("codex", 1, RouteCredentialPoolScope::InPool),
        )
        .await
        .unwrap();
        assert_eq!(in_pool.total, 2);
        assert_eq!(in_pool.page, 1);
        assert_eq!(in_pool.page_count, 1);
        assert_eq!(
            in_pool.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            vec![first.id.as_str(), second.id.as_str()]
        );

        let out_of_pool = RouteCredentialRepository::page(
            &pool,
            page_request("codex", 99, RouteCredentialPoolScope::OutOfPool),
        )
        .await
        .unwrap();
        assert_eq!(out_of_pool.total, 1);
        assert_eq!(out_of_pool.page, 1);
        assert_eq!(out_of_pool.items[0].id, outside.id);

        let empty = RouteCredentialRepository::page(
            &pool,
            page_request("claude", 5, RouteCredentialPoolScope::InPool),
        )
        .await
        .unwrap();
        assert_eq!(empty.total, 0);
        assert_eq!(empty.page, 1);
        assert_eq!(empty.page_count, 1);
        assert!(empty.items.is_empty());
        assert!(empty.previous_page_account_id.is_none());
        assert!(empty.next_page_account_id.is_none());
    }

    #[tokio::test]
    async fn reorder_in_pool_across_page_boundary_preserves_out_of_pool_slots() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let mut credentials = Vec::new();
        for index in 0..23 {
            credentials.push(
                create_api_credential(&pool, "codex", &format!("Account {index:02}"))
                    .await,
            );
        }
        let outside_id = credentials[1].id.clone();
        let member_ids = credentials
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 1)
            .map(|(_, credential)| credential.id.clone())
            .collect::<Vec<_>>();
        crate::database::repositories::route_pool_repository::RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &member_ids,
        )
        .await
        .unwrap();

        let first_page = RouteCredentialRepository::page(
            &pool,
            page_request("codex", 1, RouteCredentialPoolScope::InPool),
        )
        .await
        .unwrap();
        let second_page = RouteCredentialRepository::page(
            &pool,
            page_request("codex", 2, RouteCredentialPoolScope::InPool),
        )
        .await
        .unwrap();
        assert_eq!(first_page.total, 22);
        assert_eq!(first_page.next_page_account_id.as_deref(), Some(member_ids[20].as_str()));
        assert_eq!(second_page.previous_page_account_id.as_deref(), Some(member_ids[19].as_str()));

        let reordered_page = RouteCredentialRepository::reorder(
            &pool,
            ReorderRouteCredentialInput {
                platform: "codex".to_string(),
                moved_account_id: member_ids[20].clone(),
                previous_account_id: Some(member_ids[18].clone()),
                next_account_id: Some(member_ids[19].clone()),
                filters: Vec::new(),
                pool_scope: RouteCredentialPoolScope::InPool,
                page_size: 20,
            },
        )
        .await
        .unwrap();
        assert_eq!(reordered_page.page, 1);
        assert_eq!(reordered_page.items[19].id, member_ids[20]);

        let all = RouteCredentialRepository::list_by_platform(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(all[1].id, outside_id);
        let second_page_after = RouteCredentialRepository::page(
            &pool,
            page_request("codex", 2, RouteCredentialPoolScope::InPool),
        )
        .await
        .unwrap();
        assert_eq!(
            second_page_after.previous_page_account_id.as_deref(),
            Some(member_ids[20].as_str())
        );
        assert_eq!(
            second_page_after.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            vec![member_ids[19].as_str(), member_ids[21].as_str()]
        );
    }

    #[tokio::test]
    async fn create_persists_quota_columns_from_config_json() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = RouteCredentialRepository::create(
            &pool,
            "grok",
            "official",
            "Grok Free",
            Some("free@example.com".to_string()),
            "ok",
            None,
            r#"{"access_token":"at"}"#,
            r#"{"subscription_type":"free","primary_remain":0,"weekly_remain":12,"reset_primary":"2026-07-22T00:00:00Z","reset_weekly":"2026-07-28T00:00:00Z","quota_limit":1000000,"quota_used":1177205}"#,
            r#"{"auth_json":"{}","config_toml":""}"#,
        )
        .await
        .unwrap();
        assert_eq!(created.subscription_type.as_deref(), Some("free"));
        assert_eq!(created.primary_remain, Some(0));
        assert_eq!(created.weekly_remain, Some(12));
        assert_eq!(
            created.reset_primary.as_deref(),
            Some("2026-07-22T00:00:00Z")
        );
        assert_eq!(
            created.reset_weekly.as_deref(),
            Some("2026-07-28T00:00:00Z")
        );
        assert_eq!(created.quota_remaining, Some(0));
        assert_eq!(created.quota_limit, Some(1_000_000));
        assert_eq!(created.quota_used, Some(1_177_205));
        assert_eq!(
            created.quota_updated_at.as_deref(),
            Some("2026-07-28T00:00:00Z")
        );
    }

    #[tokio::test]
    async fn transient_failure_state_uses_backoff_and_clears() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Retry API",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}"#,
            "{}",
        )
        .await
        .unwrap();

        let first = RouteCredentialRepository::record_transient_failure(
            &pool,
            &created.id,
            "transport",
            &"x".repeat(600),
        )
        .await
        .unwrap();
        assert_eq!(first.failure_count, 1);
        assert!(first.cooldown_until.is_none());
        let second = RouteCredentialRepository::record_transient_failure(
            &pool,
            &created.id,
            "transport",
            "temporary",
        )
        .await
        .unwrap();
        assert_eq!(second.failure_count, 2);
        let third = RouteCredentialRepository::record_transient_failure(
            &pool,
            &created.id,
            "upstream",
            "temporary",
        )
        .await
        .unwrap();
        assert_eq!(third.failure_count, 3);
        assert_eq!(third.next_retry_at, third.cooldown_until);

        let stored = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .unwrap();
        assert_eq!(stored.transient_failure_count, 3);
        assert_eq!(stored.last_failure_kind.as_deref(), Some("upstream"));
        assert_eq!(stored.last_failure_message.as_deref(), Some("temporary"));

        RouteCredentialRepository::clear_transient_failure(&pool, &created.id)
            .await
            .unwrap();
        let cleared = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .unwrap();
        assert_eq!(cleared.transient_failure_count, 0);
        assert!(cleared.next_retry_at.is_none());
        assert!(cleared.cooldown_until.is_none());
        assert!(cleared.last_failure_kind.is_none());
        assert!(cleared.last_failure_message.is_none());
    }

    #[test]
    fn quota_columns_from_config_json_reads_values() {
        let quota = quota_columns_from_config_json(
            r#"{"subscription_type":"free","primary_remain":0,"weekly_remain":12,"reset_primary":"2026-07-22T00:00:00Z","reset_weekly":"2026-07-28T00:00:00Z","quota_limit":1000000,"quota_used":1177205}"#,
        );
        assert_eq!(quota.subscription_type.as_deref(), Some("free"));
        assert_eq!(quota.primary_remain, Some(0));
        assert_eq!(quota.weekly_remain, Some(12));
        assert_eq!(quota.reset_primary.as_deref(), Some("2026-07-22T00:00:00Z"));
        assert_eq!(quota.reset_weekly.as_deref(), Some("2026-07-28T00:00:00Z"));
        assert_eq!(quota.quota_remaining, Some(0));
        assert_eq!(quota.quota_limit, Some(1_000_000));
        assert_eq!(quota.quota_used, Some(1_177_205));
        assert_eq!(
            quota.quota_updated_at.as_deref(),
            Some("2026-07-28T00:00:00Z")
        );
    }

    #[test]
    fn quota_columns_from_config_json_falls_back_to_legacy_remaining() {
        let quota = quota_columns_from_config_json(
            r#"{"subscription_type":"free","quota_remaining":3,"quota_updated_at":"2026-07-22T00:00:00Z"}"#,
        );
        assert_eq!(quota.primary_remain, Some(3));
        assert_eq!(quota.quota_remaining, Some(3));
        assert_eq!(quota.reset_primary.as_deref(), Some("2026-07-22T00:00:00Z"));
    }
}
