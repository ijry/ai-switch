use crate::database::repositories::route_credential_model_repository::RouteCredentialModelRepository;
use crate::error::AppError;
use crate::models::route_credential::{
    RecoveryCandidate, ReorderRouteCredentialInput, RouteCredential, RouteCredentialFailurePolicy,
    RouteCredentialFilterOption, RouteCredentialPage, RouteCredentialPageRequest,
    RouteCredentialPoolScope, UpdateRouteCredentialInput,
    DEFAULT_ROUTE_CREDENTIAL_ERROR_STATUS_ENABLED, DEFAULT_ROUTE_CREDENTIAL_MAX_CONCURRENCY,
    DEFAULT_ROUTE_CREDENTIAL_PRIORITY,
};
use crate::models::route_credential_model::FailureScope;
use crate::models::route_credential_transfer::RouteCredentialSelectionContext;
use chrono::Utc;
use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool, Transaction};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryState {
    pub failure_count: i64,
    pub next_retry_at: Option<String>,
    pub cooldown_until: Option<String>,
}

/// Where an account came from when it was imported out of another desktop client.
///
/// `client` names the tool (`cc-switch`), `source_id` is that tool's own record
/// id. The pair is unique, which is what makes a repeated import overwrite the
/// same row instead of adding a near-copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalSourceRef<'a> {
    pub client: &'a str,
    pub source_id: &'a str,
}

/// Minimal projection of an already-imported account, enough to tell the user
/// which local row a re-import would overwrite and to check it is a row this
/// platform's API tab is allowed to touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSourceMatch {
    pub id: String,
    pub platform: String,
    pub kind: String,
    pub display_name: String,
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
    rc.route_priority, rc.max_concurrency,
    rc.batch_id, b.name AS batch_name, rc.secret_payload_json, rc.config_json, rc.preview_json,
    rc.subscription_type, rc.primary_remain, rc.weekly_remain, rc.reset_primary, rc.reset_weekly,
    rc.transient_failure_count, rc.next_retry_at, rc.cooldown_until, rc.last_failure_kind,
    rc.last_failure_message,
    rc.last_failure_response_json,
    COUNT(ue.id) AS request_count,
    COALESCE(SUM(CASE WHEN json_extract(ue.metadata_json, '$.success') = 1 THEN 1 ELSE 0 END), 0) AS success_count,
    COUNT(ue.id) - COALESCE(SUM(CASE WHEN json_extract(ue.metadata_json, '$.success') = 1 THEN 1 ELSE 0 END), 0) AS failure_count,
    CASE WHEN COUNT(ue.id) = 0 THEN NULL
         ELSE CAST(COALESCE(SUM(CASE WHEN json_extract(ue.metadata_json, '$.success') = 1 THEN 1 ELSE 0 END), 0) AS REAL) * 100.0 / COUNT(ue.id)
    END AS success_rate,
    rc.quota_remaining, rc.quota_limit, rc.quota_used, rc.quota_updated_at, rc.archived_at,
    rc.created_at, rc.updated_at
 FROM route_credentials rc
 LEFT JOIN batches b ON b.id = rc.batch_id
 LEFT JOIN usage_events ue
   ON ue.route_credential_id = rc.id
  AND ue.source_label IN ('route_proxy', 'route_pool_model_test')
  AND ue.metric_type = 'request'";

const SINGLE_SELECT: &str = "SELECT
    rc.id, rc.platform, rc.kind, rc.display_name, rc.email, rc.status, rc.sort_order,
    rc.route_priority, rc.max_concurrency,
    rc.batch_id, b.name AS batch_name, rc.secret_payload_json, rc.config_json, rc.preview_json,
    rc.subscription_type, rc.primary_remain, rc.weekly_remain, rc.reset_primary, rc.reset_weekly,
    rc.transient_failure_count, rc.next_retry_at, rc.cooldown_until, rc.last_failure_kind,
    rc.last_failure_message, rc.last_failure_response_json,
    rc.quota_remaining, rc.quota_limit, rc.quota_used, rc.quota_updated_at, rc.archived_at,
    rc.created_at, rc.updated_at
 FROM route_credentials rc
 LEFT JOIN batches b ON b.id = rc.batch_id
 WHERE rc.id = ?";

fn push_filter_predicate(builder: &mut QueryBuilder<Sqlite>, filters: &[String]) {
    let filters: Vec<&String> = filters
        .iter()
        .filter(|filter| !filter.trim().is_empty())
        .collect();
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

fn push_pool_scope_predicate(builder: &mut QueryBuilder<Sqlite>, scope: RouteCredentialPoolScope) {
    builder.push(" AND ");
    match scope {
        RouteCredentialPoolScope::Archived => {
            builder.push("rc.archived_at IS NOT NULL");
        }
        RouteCredentialPoolScope::InPool | RouteCredentialPoolScope::OutOfPool => {
            builder.push("rc.archived_at IS NULL AND ");
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
    }
}

pub(crate) fn database_error(code: &'static str, message: &str, error: impl ToString) -> AppError {
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
    let mut query =
        QueryBuilder::<Sqlite>::new("SELECT rc.id FROM route_credentials rc WHERE rc.platform = ");
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
        .map_err(|err| {
            database_error(
                "database.route_credential_boundary",
                "Could not load page boundary",
                err,
            )
        })
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
    .map_err(|err| {
        database_error(
            "database.route_credential_filter_options",
            "Could not load account filters",
            err,
        )
    })?;
    let has_single = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM route_credentials WHERE platform = ? AND batch_id IS NULL)",
    )
    .bind(platform)
    .fetch_one(pool)
    .await
    .map_err(|err| {
        database_error(
            "database.route_credential_filter_single",
            "Could not load unbatched filter",
            err,
        )
    })?;
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

async fn create_with_connection(
    connection: &mut SqliteConnection,
    platform: &str,
    kind: &str,
    display_name: &str,
    email: Option<String>,
    status: &str,
    batch_id: Option<String>,
    secret_payload_json: &str,
    config_json: &str,
    preview_json: &str,
    route_priority: i64,
    max_concurrency: i64,
    external_source: Option<ExternalSourceRef<'_>>,
) -> Result<RouteCredential, AppError> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let quota = quota_columns_from_config_json(config_json);
    let sort_order = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(sort_order) FROM route_credentials WHERE platform = ?",
    )
    .bind(platform)
    .fetch_one(&mut *connection)
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
            id, platform, kind, display_name, email, status, sort_order, route_priority,
            max_concurrency, batch_id,
            secret_payload_json, config_json, preview_json,
            subscription_type, primary_remain, weekly_remain, reset_primary, reset_weekly,
            quota_remaining, quota_limit, quota_used, quota_updated_at,
            external_source_client, external_source_id,
            created_at, updated_at
         )
         VALUES (
             ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
             ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
         )",
    )
    .bind(&id)
    .bind(platform)
    .bind(kind)
    .bind(display_name)
    .bind(email)
    .bind(status)
    .bind(sort_order)
    .bind(route_priority)
    .bind(max_concurrency)
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
    .bind(external_source.map(|source| source.client))
    .bind(external_source.map(|source| source.source_id))
    .bind(&now)
    .bind(&now)
    .execute(&mut *connection)
    .await
    .map_err(|err| AppError::Database {
        code: "database.route_credential_create",
        message: "Could not create route credential".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    sqlx::query_as::<_, RouteCredential>(
        "SELECT
            rc.id,
            rc.platform,
            rc.kind,
            rc.display_name,
            rc.email,
            rc.status,
            rc.sort_order,
            rc.route_priority,
            rc.max_concurrency,
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
            rc.last_failure_response_json,
            rc.quota_remaining,
            rc.quota_limit,
            rc.quota_used,
            rc.quota_updated_at,
            rc.archived_at,
            rc.created_at,
            rc.updated_at
         FROM route_credentials rc
         LEFT JOIN batches b ON b.id = rc.batch_id
         WHERE rc.id = ?",
    )
    .bind(id)
    .fetch_one(&mut *connection)
    .await
    .map_err(|err| AppError::Database {
        code: "database.route_credential_get",
        message: "Could not load route credential".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })
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
        Self::create_with_routing_settings(
            pool,
            platform,
            kind,
            display_name,
            email,
            status,
            batch_id,
            secret_payload_json,
            config_json,
            preview_json,
            DEFAULT_ROUTE_CREDENTIAL_PRIORITY,
            DEFAULT_ROUTE_CREDENTIAL_MAX_CONCURRENCY,
            None,
        )
        .await
    }

    pub(crate) async fn create_with_routing_settings(
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
        route_priority: i64,
        max_concurrency: i64,
        external_source: Option<ExternalSourceRef<'_>>,
    ) -> Result<RouteCredential, AppError> {
        let mut tx = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_credential_create_tx",
            message: "Could not start route credential create transaction".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        let created = create_with_connection(
            &mut *tx,
            platform,
            kind,
            display_name,
            email,
            status,
            batch_id,
            secret_payload_json,
            config_json,
            preview_json,
            route_priority,
            max_concurrency,
            external_source,
        )
        .await?;

        tx.commit().await.map_err(|err| AppError::Database {
            code: "database.route_credential_create_commit",
            message: "Could not save route credential".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(created)
    }

    pub async fn create_tx(
        tx: &mut Transaction<'_, Sqlite>,
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
        Self::create_tx_with_external_source(
            tx,
            platform,
            kind,
            display_name,
            email,
            status,
            batch_id,
            secret_payload_json,
            config_json,
            preview_json,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_tx_with_external_source(
        tx: &mut Transaction<'_, Sqlite>,
        platform: &str,
        kind: &str,
        display_name: &str,
        email: Option<String>,
        status: &str,
        batch_id: Option<String>,
        secret_payload_json: &str,
        config_json: &str,
        preview_json: &str,
        external_source: Option<ExternalSourceRef<'_>>,
    ) -> Result<RouteCredential, AppError> {
        create_with_connection(
            &mut **tx,
            platform,
            kind,
            display_name,
            email,
            status,
            batch_id,
            secret_payload_json,
            config_json,
            preview_json,
            DEFAULT_ROUTE_CREDENTIAL_PRIORITY,
            DEFAULT_ROUTE_CREDENTIAL_MAX_CONCURRENCY,
            external_source,
        )
        .await
    }

    /// Accounts already imported from `client`, keyed by the source's own id.
    ///
    /// The import preview needs every match up front — one query beats one
    /// round-trip per previewed item.
    pub async fn external_source_matches(
        pool: &SqlitePool,
        client: &str,
    ) -> Result<Vec<(String, ExternalSourceMatch)>, AppError> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT external_source_id, id, platform, kind, display_name
             FROM route_credentials
             WHERE external_source_client = ? AND external_source_id IS NOT NULL",
        )
        .bind(client)
        .fetch_all(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_external_source_list",
                "Could not load imported external accounts",
                err,
            )
        })?;

        Ok(rows
            .into_iter()
            .map(|(source_id, id, platform, kind, display_name)| {
                (
                    source_id,
                    ExternalSourceMatch {
                        id,
                        platform,
                        kind,
                        display_name,
                    },
                )
            })
            .collect())
    }

    /// Overwrites an account previously imported from the same external record.
    ///
    /// Deliberately narrower than [`Self::update`]: it replaces the payload the
    /// external client owns (name, secret, config, preview) and leaves local
    /// edits — priority, concurrency, pool membership, batch — alone. It also
    /// clears the failure bookkeeping, since a refreshed key deserves a clean
    /// slate. `status` is left as-is unless it is `revoked`; a revoked account
    /// stays revoked so a re-import cannot silently resurrect it.
    pub async fn overwrite_from_external_source(
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        display_name: &str,
        secret_payload_json: &str,
        config_json: &str,
        preview_json: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let quota = quota_columns_from_config_json(config_json);
        let result = sqlx::query(
            "UPDATE route_credentials
             SET display_name = ?, secret_payload_json = ?, config_json = ?, preview_json = ?,
                 status = CASE WHEN status = 'revoked' THEN status ELSE 'ok' END,
                 transient_failure_count = 0, next_retry_at = NULL, cooldown_until = NULL,
                 semantic_failure_streak_count = 0, semantic_failure_streak_fingerprint = NULL,
                 last_failure_kind = NULL, last_failure_message = NULL,
                 last_failure_response_json = NULL,
                 subscription_type = ?, primary_remain = ?, weekly_remain = ?,
                 reset_primary = ?, reset_weekly = ?,
                 quota_remaining = ?, quota_limit = ?, quota_used = ?, quota_updated_at = ?,
                 updated_at = ?
             WHERE id = ?",
        )
        .bind(display_name)
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
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_external_source_overwrite",
                "Could not overwrite the imported account",
                err,
            )
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

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<RouteCredential, AppError> {
        sqlx::query_as::<_, RouteCredential>(SINGLE_SELECT)
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

    /// Same as [`Self::get`] but inside a caller's transaction, so an import can
    /// return the row it just wrote without committing first.
    pub async fn get_tx(
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
    ) -> Result<RouteCredential, AppError> {
        sqlx::query_as::<_, RouteCredential>(SINGLE_SELECT)
            .bind(id)
            .fetch_one(&mut **tx)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_credential_get",
                message: "Could not load route credential".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_by_ids(
        pool: &SqlitePool,
        ids: &[String],
        selection: &RouteCredentialSelectionContext,
    ) -> Result<Vec<RouteCredential>, AppError> {
        let mut seen = HashSet::with_capacity(ids.len());
        let unique_ids = ids
            .iter()
            .filter(|id| seen.insert(id.as_str()))
            .collect::<Vec<_>>();
        if unique_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT
                rc.id,
                rc.platform,
                rc.kind,
                rc.display_name,
                rc.email,
                rc.status,
                rc.sort_order,
                rc.route_priority,
                rc.max_concurrency,
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
                rc.last_failure_response_json,
                rc.quota_remaining,
                rc.quota_limit,
                rc.quota_used,
                rc.quota_updated_at,
                rc.archived_at,
                rc.created_at,
                rc.updated_at
             FROM route_credentials rc
             LEFT JOIN batches b ON b.id = rc.batch_id
             WHERE rc.platform = ",
        );
        query.push_bind(&selection.platform).push(" AND rc.id IN (");
        let mut separated = query.separated(", ");
        for id in unique_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        push_pool_scope_predicate(&mut query, selection.pool_scope);
        query.push(" ORDER BY rc.sort_order ASC, rc.created_at DESC, rc.id ASC");

        query
            .build_query_as::<RouteCredential>()
            .fetch_all(pool)
            .await
            .map_err(|err| {
                database_error(
                    "database.route_credential_list_by_ids",
                    "Could not load selected route credentials",
                    err,
                )
            })
    }

    pub async fn list_transfer_fingerprint_candidates(
        pool: &SqlitePool,
        platforms: &[String],
    ) -> Result<Vec<RouteCredential>, AppError> {
        let mut seen = HashSet::with_capacity(platforms.len());
        let unique_platforms = platforms
            .iter()
            .filter(|platform| seen.insert(platform.as_str()))
            .collect::<Vec<_>>();
        if unique_platforms.is_empty() {
            return Ok(Vec::new());
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT
                rc.id,
                rc.platform,
                rc.kind,
                rc.display_name,
                rc.email,
                rc.status,
                rc.sort_order,
                rc.route_priority,
                rc.max_concurrency,
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
                rc.last_failure_response_json,
                rc.quota_remaining,
                rc.quota_limit,
                rc.quota_used,
                rc.quota_updated_at,
                rc.archived_at,
                rc.created_at,
                rc.updated_at
             FROM route_credentials rc
             LEFT JOIN batches b ON b.id = rc.batch_id
             WHERE rc.platform IN (",
        );
        let mut separated = query.separated(", ");
        for platform in unique_platforms {
            separated.push_bind(platform);
        }
        separated.push_unseparated(")");
        query.push(" ORDER BY rc.platform ASC, rc.kind ASC, rc.id ASC");

        query
            .build_query_as::<RouteCredential>()
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_credential_transfer_candidates",
                message: "Could not load route credential transfer candidates".to_string(),
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
                rc.route_priority,
                rc.max_concurrency,
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
                rc.last_failure_response_json,
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
                rc.archived_at,
                rc.created_at,
                rc.updated_at
             FROM route_credentials rc
             LEFT JOIN batches b ON b.id = rc.batch_id
             LEFT JOIN usage_events ue
               ON ue.route_credential_id = rc.id
              AND ue.source_label IN ('route_proxy', 'route_pool_model_test')
              AND ue.metric_type = 'request'
             WHERE rc.platform = ? AND rc.archived_at IS NULL
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
        let page_size = request
            .normalized_page_size()
            .map_err(|message| AppError::Validation {
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
            .map_err(|err| {
                database_error(
                    "database.route_credential_page_count",
                    "Could not count route credentials",
                    err,
                )
            })?;
        let page_count = if total == 0 {
            1
        } else {
            (total + page_size - 1) / page_size
        };
        let page = requested_page.min(page_count);
        let offset = (page - 1) * page_size;

        let mut item_query = QueryBuilder::<Sqlite>::new(PAGE_SELECT);
        item_query
            .push(" WHERE rc.platform = ")
            .push_bind(&request.platform);
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
            .map_err(|err| {
                database_error(
                    "database.route_credential_page_items",
                    "Could not load route credential page",
                    err,
                )
            })?;

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
            "SELECT COUNT(*) FROM route_credentials WHERE platform = ? AND kind = 'official' AND archived_at IS NULL",
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
            _ => {
                return Err(AppError::Validation {
                    code: "validation.route_credential_page_size",
                    message: "page_size must be 20, 50, or 100".to_string(),
                    details: None,
                    recoverable: true,
                })
            }
        };
        let mut tx = pool.begin().await.map_err(|err| {
            database_error(
                "database.route_credential_reorder_tx",
                "Could not start account reorder",
                err,
            )
        })?;
        let rows = sqlx::query_as::<_, (String, Option<String>, i64, i64)>(
            "SELECT rc.id, rc.batch_id,
                    EXISTS (
                        SELECT 1 FROM route_pool_members rpm
                        WHERE rpm.platform = rc.platform
                          AND rpm.route_credential_id = rc.id
                          AND rpm.enabled = 1
                    ) AS in_pool,
                    rc.archived_at IS NOT NULL AS archived
             FROM route_credentials rc
             WHERE rc.platform = ?
             ORDER BY rc.sort_order ASC, rc.created_at DESC, rc.id ASC",
        )
        .bind(&input.platform)
        .fetch_all(&mut *tx)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_reorder_read",
                "Could not load account order",
                err,
            )
        })?;
        let all_ids: Vec<String> = rows.iter().map(|(id, _, _, _)| id.clone()).collect();
        let pool_matches = |in_pool: i64, archived: i64| match input.pool_scope {
            RouteCredentialPoolScope::InPool => archived == 0 && in_pool != 0,
            RouteCredentialPoolScope::OutOfPool => archived == 0 && in_pool == 0,
            RouteCredentialPoolScope::Archived => archived != 0,
        };
        let matches = |batch_id: &Option<String>, in_pool: i64, archived: i64| {
            (input.filters.is_empty()
                || input.filters.iter().any(|filter| {
                    (filter == "__single__" && batch_id.is_none())
                        || batch_id.as_deref() == Some(filter.as_str())
                }))
                && pool_matches(in_pool, archived)
        };
        let filtered_ids: Vec<String> = rows
            .iter()
            .filter(|(_, batch_id, in_pool, archived)| matches(batch_id, *in_pool, *archived))
            .map(|(id, _, _, _)| id.clone())
            .collect();
        let Some(moved_index) = filtered_ids
            .iter()
            .position(|id| id == &input.moved_account_id)
        else {
            return Err(reorder_validation_error(
                "Moved route credential is not in the active filter",
            ));
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
                return Err(reorder_validation_error(
                    "Previous route credential is invalid",
                ));
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
                return Err(reorder_validation_error(
                    "Route credential neighbors are not adjacent",
                ));
            }
            index
        } else if let Some(previous) = previous_account_id {
            let index = remaining.iter().position(|id| id == previous).unwrap();
            if remaining.get(index + 1).is_some() {
                return Err(reorder_validation_error(
                    "Route credential neighbors are not adjacent",
                ));
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
            let (_, batch_id, in_pool, archived) =
                rows.iter().find(|(row_id, _, _, _)| row_id == id).unwrap();
            if matches(batch_id, *in_pool, *archived) {
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
                .map_err(|err| {
                    database_error(
                        "database.route_credential_reorder_update",
                        "Could not save account order",
                        err,
                    )
                })?;
        }
        tx.commit().await.map_err(|err| {
            database_error(
                "database.route_credential_reorder_commit",
                "Could not commit account order",
                err,
            )
        })?;
        let moved_position = remaining
            .iter()
            .position(|id| id == &input.moved_account_id)
            .unwrap_or(0);
        Self::page(
            pool,
            RouteCredentialPageRequest {
                platform: input.platform,
                page: moved_position as i64 / page_size + 1,
                page_size,
                filters: input.filters,
                pool_scope: input.pool_scope,
            },
        )
        .await
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
             SET display_name = ?, email = ?, status = ?, route_priority = ?,
                 semantic_failure_streak_count = CASE
                     WHEN ? = 'ok' THEN 0
                     ELSE semantic_failure_streak_count
                 END,
                 semantic_failure_streak_fingerprint = CASE
                     WHEN ? = 'ok' THEN NULL
                     ELSE semantic_failure_streak_fingerprint
                 END,
                 max_concurrency = ?, secret_payload_json = ?,
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
        .bind(input.route_priority)
        .bind(&input.status)
        .bind(&input.status)
        .bind(input.max_concurrency)
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

    pub async fn set_statuses(
        pool: &SqlitePool,
        ids: &[String],
        status: &str,
    ) -> Result<(), AppError> {
        if !matches!(status, "ok" | "warning" | "error" | "revoked" | "paused") {
            return Err(AppError::Validation {
                code: "validation.route_credential_status",
                message: "Route credential status is not supported".to_string(),
                details: Some(status.to_string()),
                recoverable: true,
            });
        }

        let mut seen = HashSet::with_capacity(ids.len());
        let unique_ids = ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .filter(|id| seen.insert(*id))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if unique_ids.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_credential_selection_empty",
                message: "At least one route credential must be selected".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let mut tx = pool.begin().await.map_err(|err| {
            database_error(
                "database.route_credential_statuses_tx",
                "Could not start route credential status update",
                err,
            )
        })?;

        let mut count_query =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM route_credentials WHERE id IN (");
        let mut count_separated = count_query.separated(", ");
        for id in &unique_ids {
            count_separated.push_bind(id);
        }
        count_separated.push_unseparated(")");
        let count: i64 = count_query
            .build_query_scalar()
            .fetch_one(&mut *tx)
            .await
            .map_err(|err| {
                database_error(
                    "database.route_credential_statuses_count",
                    "Could not validate selected route credentials",
                    err,
                )
            })?;
        if count != unique_ids.len() as i64 {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "One or more route credentials do not exist".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let now = Utc::now().to_rfc3339();
        let mut update_query =
            QueryBuilder::<Sqlite>::new("UPDATE route_credentials SET status = ");
        update_query
            .push_bind(status)
            .push(" , semantic_failure_streak_count = CASE WHEN ")
            .push_bind(status)
            .push(" = 'ok' THEN 0 ELSE semantic_failure_streak_count END")
            .push(" , semantic_failure_streak_fingerprint = CASE WHEN ")
            .push_bind(status)
            .push(" = 'ok' THEN NULL ELSE semantic_failure_streak_fingerprint END")
            .push(", updated_at = ")
            .push_bind(&now)
            .push(" WHERE id IN (");
        let mut update_separated = update_query.separated(", ");
        for id in &unique_ids {
            update_separated.push_bind(id);
        }
        update_separated.push_unseparated(")");
        update_query
            .build()
            .execute(&mut *tx)
            .await
            .map_err(|err| {
                database_error(
                    "database.route_credential_statuses_update",
                    "Could not update route credential statuses",
                    err,
                )
            })?;

        tx.commit().await.map_err(|err| {
            database_error(
                "database.route_credential_statuses_commit",
                "Could not commit route credential status update",
                err,
            )
        })
    }

    pub async fn update_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE route_credentials
             SET status = ?,
                 semantic_failure_streak_count = CASE
                     WHEN ? = 'ok' THEN 0
                     ELSE semantic_failure_streak_count
                 END,
                 semantic_failure_streak_fingerprint = CASE
                     WHEN ? = 'ok' THEN NULL
                     ELSE semantic_failure_streak_fingerprint
                 END,
                 updated_at = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(status)
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

    /// Overwrite only the free-form config JSON (used by the recovery-rule editor,
    /// which stores its rule under the `recovery` key). Quota-derived columns are
    /// left untouched because the recovery key never affects them.
    pub async fn update_config_json(
        pool: &SqlitePool,
        id: &str,
        config_json: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE route_credentials
             SET config_json = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(config_json)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_config_update",
            message: "Could not update route credential config".to_string(),
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

    /// Load the minimal fields the auto-recovery scheduler needs for every
    /// non-archived account across all platforms.
    pub async fn list_recovery_candidates(
        pool: &SqlitePool,
    ) -> Result<Vec<RecoveryCandidate>, AppError> {
        sqlx::query_as::<_, RecoveryCandidate>(
            "SELECT id, platform, status, config_json, next_retry_at, cooldown_until,
                    EXISTS (
                      SELECT 1 FROM route_credential_models m
                      WHERE m.route_credential_id = route_credentials.id AND m.status != 'paused'
                    ) AS has_model_failures
             FROM route_credentials
             WHERE archived_at IS NULL",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_recovery_candidates",
            message: "Could not load route credential recovery candidates".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn record_transient_failure(
        pool: &SqlitePool,
        id: &str,
        kind: &str,
        message: &str,
        response_body: Option<&[u8]>,
        scope: FailureScope<'_>,
    ) -> Result<RetryState, AppError> {
        let mut tx = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_credential_retry_tx",
            message: "Could not start route credential retry update".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        let current = sqlx::query_as::<_, (i64, String)>(
            "SELECT transient_failure_count, config_json FROM route_credentials WHERE id = ?",
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
        let Some((current, config_json)) = current else {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "Route credential does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            });
        };
        let policy = RouteCredentialFailurePolicy::from_config_json(&config_json);
        let cooldown_seconds = policy.cooldown_enabled.then_some(policy.cooldown_seconds);

        // A model-scoped failure charges the model row first, then asks whether
        // anything is left to serve. Both happen inside this transaction:
        // concurrent requests must not each see "not all parked yet" and skip
        // the escalation.
        let escalate = match scope {
            FailureScope::Account => true,
            FailureScope::Model { key, siblings } => {
                RouteCredentialModelRepository::record_transient_failure(
                    &mut tx,
                    id,
                    key,
                    kind,
                    message,
                    response_body,
                    cooldown_seconds,
                    None,
                    i64::from(policy.semantic_error_threshold),
                    policy.error_status_enabled,
                )
                .await?;
                let now = Utc::now().to_rfc3339();
                let unavailable =
                    RouteCredentialModelRepository::unavailable_keys(&mut tx, id, &now).await?;
                let paused = RouteCredentialModelRepository::paused_keys(&mut tx, id).await?;
                let serviceable = siblings
                    .iter()
                    .filter(|sibling| !paused.contains(sibling))
                    .count();
                let parked = siblings
                    .iter()
                    .filter(|sibling| !paused.contains(sibling) && unavailable.contains(sibling))
                    .count();
                // Only escalate when the account has run out of usable models.
                // Paused models are excluded from the denominator: pausing three
                // of four must not let the fourth's single failure fake an
                // account-wide outage.
                serviceable > 0 && parked >= serviceable
            }
        };

        let failure_count = current.saturating_add(1);
        // Every trigger uses the same account-configured window, so a flaky
        // account recovers predictably instead of sliding into a long backoff.
        let (retry_at, cooldown_until) = match (escalate, cooldown_seconds) {
            (true, Some(seconds)) => {
                let cooldown_until =
                    (Utc::now() + chrono::Duration::seconds(i64::from(seconds))).to_rfc3339();
                (Some(cooldown_until.clone()), Some(cooldown_until))
            }
            // A model-scoped failure leaves the account selectable for its other
            // models, so its own deadline must not be written.
            _ => (None, None),
        };
        let message = truncate_failure_message(message);
        let response = truncate_failure_response(response_body);
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE route_credentials
             SET transient_failure_count = ?, next_retry_at = ?, cooldown_until = ?,
                 semantic_failure_streak_count = 0, semantic_failure_streak_fingerprint = NULL,
                 last_failure_kind = ?, last_failure_message = ?, last_failure_response_json = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(failure_count)
        .bind(retry_at.as_deref())
        .bind(cooldown_until.as_deref())
        .bind(kind)
        .bind(&message)
        .bind(&response)
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
            next_retry_at: retry_at,
            cooldown_until,
        })
    }

    /// A success clears this account's backoff and, when the request named a
    /// model, that model's row. Sibling models keep their own state: proving
    /// `glm-5.3` works says nothing about `gpt-5.6-sol`.
    pub async fn clear_transient_failure(
        pool: &SqlitePool,
        id: &str,
        model_key: Option<&str>,
    ) -> Result<(), AppError> {
        if let Some(model_key) = model_key {
            RouteCredentialModelRepository::clear(pool, id, model_key).await?;
        }
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE route_credentials
             SET transient_failure_count = 0, next_retry_at = NULL, cooldown_until = NULL,
                 semantic_failure_streak_count = 0, semantic_failure_streak_fingerprint = NULL,
                 last_failure_kind = NULL, last_failure_message = NULL,
                 last_failure_response_json = NULL, updated_at = ?
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

    pub async fn record_semantic_failure_with_status(
        pool: &SqlitePool,
        id: &str,
        response_status: Option<u16>,
        error_threshold: i64,
        message: &str,
        response_body: Option<&[u8]>,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let error_threshold = error_threshold.max(1);
        let fingerprint = semantic_failure_fingerprint(response_status, message);
        let response = truncate_failure_response(response_body);
        let error_status_enabled = Self::error_status_enabled(pool, id).await?;
        // The streak keeps counting even when the toggle is off, so turning it
        // back on judges the account on its real history rather than from zero.
        let result = sqlx::query(
            "UPDATE route_credentials
             SET status = CASE
                     WHEN status IN ('revoked', 'paused') THEN status
                     WHEN NOT ? THEN status
                     WHEN CASE
                         WHEN semantic_failure_streak_fingerprint = ?
                             THEN MIN(semantic_failure_streak_count + 1, ?)
                         ELSE 1
                     END >= ? THEN 'error'
                     ELSE status
                 END,
                 transient_failure_count = 0,
                 next_retry_at = NULL, cooldown_until = NULL,
                 semantic_failure_streak_count = CASE
                     WHEN semantic_failure_streak_fingerprint = ?
                         THEN MIN(semantic_failure_streak_count + 1, ?)
                     ELSE 1
                 END,
                 semantic_failure_streak_fingerprint = ?,
                 last_failure_kind = 'semantic_response_failed',
                 last_failure_message = ?, last_failure_response_json = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(error_status_enabled)
        .bind(&fingerprint)
        .bind(error_threshold)
        .bind(error_threshold)
        .bind(&fingerprint)
        .bind(error_threshold)
        .bind(&fingerprint)
        .bind(truncate_failure_message(message))
        .bind(&response)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_semantic_failure",
                "Could not record semantic response failure",
                err,
            )
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

    /// Whether this account may be flipped to `error` by a failure streak.
    /// Missing rows return the default so callers keep their existing
    /// not-found handling instead of failing here.
    async fn error_status_enabled(pool: &SqlitePool, id: &str) -> Result<bool, AppError> {
        let config_json = sqlx::query_scalar::<_, String>(
            "SELECT config_json FROM route_credentials WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_failure_policy_read",
                "Could not read route credential failure policy",
                err,
            )
        })?;
        Ok(config_json
            .map(|config| {
                RouteCredentialFailurePolicy::from_config_json(&config).error_status_enabled
            })
            .unwrap_or(DEFAULT_ROUTE_CREDENTIAL_ERROR_STATUS_ENABLED))
    }

    pub async fn recover_after_explicit_test(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
        // Account columns only. An explicit test asks about one model, and
        // `clear_transient_failure` already cleared that model's row; dropping
        // its siblings here would claim the upstream answered for models it was
        // never asked about.
        Self::reactivate(pool, id, false).await
    }

    /// Set the account back to "ok" and clear any cooldown/retry backoff, unless
    /// it is `revoked` (a hard auth failure that must not be silently re-enabled).
    /// Scheduled recovery means "revive unconditionally", so it also drops every
    /// automatic model row.
    pub async fn reactivate_credential(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
        Self::reactivate(pool, id, true).await
    }

    async fn reactivate(
        pool: &SqlitePool,
        id: &str,
        clear_model_state: bool,
    ) -> Result<(), AppError> {
        let mut tx = pool.begin().await.map_err(|err| {
            database_error(
                "database.route_credential_recover",
                "Could not start route credential recovery",
                err,
            )
        })?;
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE route_credentials
             SET status = 'ok', transient_failure_count = 0,
                 next_retry_at = NULL, cooldown_until = NULL,
                 semantic_failure_streak_count = 0, semantic_failure_streak_fingerprint = NULL,
                 last_failure_kind = NULL, last_failure_message = NULL,
                 last_failure_response_json = NULL,
                 updated_at = ?
             WHERE id = ? AND status != 'revoked'",
        )
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|err| {
            database_error(
                "database.route_credential_recover",
                "Could not recover route credential",
                err,
            )
        })?;

        if clear_model_state {
            // Automation may undo automation, never a human decision.
            RouteCredentialModelRepository::clear_all_unpaused(&mut tx, id).await?;
        }

        if result.rows_affected() == 0 {
            let exists =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM route_credentials WHERE id = ?")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|err| {
                        database_error(
                            "database.route_credential_recover",
                            "Could not verify route credential",
                            err,
                        )
                    })?;
            if exists == 0 {
                return Err(AppError::Validation {
                    code: "validation.route_credential_not_found",
                    message: "Route credential does not exist".to_string(),
                    details: Some(id.to_string()),
                    recoverable: true,
                });
            }
        }
        tx.commit().await.map_err(|err| {
            database_error(
                "database.route_credential_recover",
                "Could not save route credential recovery",
                err,
            )
        })?;
        Ok(())
    }

    pub async fn set_archived(
        pool: &SqlitePool,
        ids: &[String],
        archived: bool,
    ) -> Result<(), AppError> {
        let mut seen = HashSet::with_capacity(ids.len());
        let unique_ids = ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .filter(|id| seen.insert((*id).to_string()))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if unique_ids.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_credential_selection_empty",
                message: "At least one route credential is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let mut tx = pool.begin().await.map_err(|err| {
            database_error(
                "database.route_credential_archive_tx",
                "Could not start route credential archive transaction",
                err,
            )
        })?;

        let mut count_query =
            QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM route_credentials WHERE id IN (");
        let mut count_ids = count_query.separated(", ");
        for id in &unique_ids {
            count_ids.push_bind(id);
        }
        count_ids.push_unseparated(")");
        let existing_count: i64 = count_query
            .build_query_scalar()
            .fetch_one(&mut *tx)
            .await
            .map_err(|err| {
                database_error(
                    "database.route_credential_archive_verify",
                    "Could not verify route credentials",
                    err,
                )
            })?;
        if existing_count != unique_ids.len() as i64 {
            return Err(AppError::Validation {
                code: "validation.route_credential_not_found",
                message: "One or more route credentials do not exist".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let now = Utc::now().to_rfc3339();
        let archived_at = archived.then_some(now.clone());
        let mut update_query =
            QueryBuilder::<Sqlite>::new("UPDATE route_credentials SET archived_at = ");
        update_query
            .push_bind(archived_at)
            .push(", updated_at = ")
            .push_bind(&now)
            .push(" WHERE id IN (");
        let mut update_ids = update_query.separated(", ");
        for id in &unique_ids {
            update_ids.push_bind(id);
        }
        update_ids.push_unseparated(")");
        update_query
            .build()
            .execute(&mut *tx)
            .await
            .map_err(|err| {
                database_error(
                    "database.route_credential_archive_update",
                    "Could not update route credential archive state",
                    err,
                )
            })?;

        tx.commit().await.map_err(|err| {
            database_error(
                "database.route_credential_archive_commit",
                "Could not commit route credential archive state",
                err,
            )
        })?;

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

pub(crate) fn truncate_failure_message(message: &str) -> String {
    let end = message
        .char_indices()
        .take_while(|(index, character)| *index + character.len_utf8() <= 512)
        .map(|(index, character)| index + character.len_utf8())
        .last()
        .unwrap_or(0);
    message[..end].to_string()
}

pub(crate) fn semantic_failure_fingerprint(response_status: Option<u16>, message: &str) -> String {
    use sha2::{Digest, Sha256};

    let normalized_message = message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let input = format!(
        "semantic_response_failed|{}|{normalized_message}",
        response_status
            .map(|status| status.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    let digest = Sha256::digest(input.as_bytes());
    format!("sha256:{digest:x}")
}

const MAX_FAILURE_RESPONSE_CHARS: usize = 8192;

pub(crate) fn truncate_failure_response(response_body: Option<&[u8]>) -> Option<String> {
    let body = std::str::from_utf8(response_body?).ok()?.trim();
    if body.is_empty() {
        return None;
    }
    let mut chars = body.chars();
    let mut response = chars
        .by_ref()
        .take(MAX_FAILURE_RESPONSE_CHARS.saturating_sub(1))
        .collect::<String>();
    if chars.next().is_some() {
        response.push('…');
    }
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::route_credential_transfer::RouteCredentialSelectionContext;
    use chrono::DateTime;

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
    async fn external_source_pair_is_unique_and_looked_up_by_source_id() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        let imported = RouteCredentialRepository::create_tx_with_external_source(
            &mut tx,
            "claude",
            "api",
            "goRouter",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-one"}"#,
            r#"{"base_url":"https://one.example","interface_format":"anthropic"}"#,
            "{}",
            Some(ExternalSourceRef {
                client: "cc-switch",
                source_id: "provider-1",
            }),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let matches = RouteCredentialRepository::external_source_matches(&pool, "cc-switch")
            .await
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, "provider-1");
        assert_eq!(matches[0].1.id, imported.id);
        assert_eq!(matches[0].1.display_name, "goRouter");

        // A different client with the same source id is a different account.
        let mut tx = pool.begin().await.unwrap();
        RouteCredentialRepository::create_tx_with_external_source(
            &mut tx,
            "claude",
            "api",
            "Other tool",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-two"}"#,
            "{}",
            "{}",
            Some(ExternalSourceRef {
                client: "other-tool",
                source_id: "provider-1",
            }),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // Re-inserting the same pair must fail: the import path overwrites
        // instead, and the index is what makes that the only option.
        let mut tx = pool.begin().await.unwrap();
        let duplicate = RouteCredentialRepository::create_tx_with_external_source(
            &mut tx,
            "claude",
            "api",
            "goRouter again",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-one"}"#,
            "{}",
            "{}",
            Some(ExternalSourceRef {
                client: "cc-switch",
                source_id: "provider-1",
            }),
        )
        .await
        .expect_err("duplicate external source pair must be rejected");
        tx.rollback().await.unwrap();
        assert!(matches!(
            duplicate,
            AppError::Database {
                code: "database.route_credential_create",
                ..
            }
        ));

        // Hand-made accounts leave the pair NULL, so any number of them coexist.
        for name in ["Manual one", "Manual two"] {
            create_api_credential(&pool, "claude", name).await;
        }
        assert_eq!(
            RouteCredentialRepository::external_source_matches(&pool, "cc-switch")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn overwrite_from_external_source_replaces_payload_and_keeps_local_edits() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let imported = RouteCredentialRepository::create_tx_with_external_source(
            &mut tx,
            "claude",
            "api",
            "goRouter",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-old"}"#,
            r#"{"base_url":"https://old.example","interface_format":"anthropic"}"#,
            "{}",
            Some(ExternalSourceRef {
                client: "cc-switch",
                source_id: "provider-1",
            }),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // Local routing edits plus a failure streak, both of which a re-import
        // must not undo (routing) and must clear (failure state).
        RouteCredentialRepository::update(
            &pool,
            &imported.id,
            &UpdateRouteCredentialInput {
                display_name: "goRouter".to_string(),
                email: None,
                status: "error".to_string(),
                route_priority: 5,
                max_concurrency: 9,
                secret_payload_json: r#"{"api_key":"sk-old"}"#.to_string(),
                config_json: r#"{"base_url":"https://old.example","interface_format":"anthropic"}"#
                    .to_string(),
                preview_json: "{}".to_string(),
            },
        )
        .await
        .unwrap();
        RouteCredentialRepository::record_transient_failure(
            &pool,
            &imported.id,
            "http_5xx",
            "upstream down",
            None,
            FailureScope::Account,
        )
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        RouteCredentialRepository::overwrite_from_external_source(
            &mut tx,
            &imported.id,
            "goRouter renamed",
            r#"{"api_key":"sk-new"}"#,
            r#"{"base_url":"https://new.example","interface_format":"anthropic"}"#,
            r#"{"settings_json":"{}"}"#,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let updated = RouteCredentialRepository::get(&pool, &imported.id)
            .await
            .unwrap();
        assert_eq!(updated.display_name, "goRouter renamed");
        assert!(updated.secret_payload_json.contains("sk-new"));
        assert!(updated.config_json.contains("https://new.example"));
        assert_eq!(updated.preview_json, r#"{"settings_json":"{}"}"#);
        assert_eq!(updated.route_priority, 5);
        assert_eq!(updated.max_concurrency, 9);
        assert_eq!(updated.status, "ok");
        assert_eq!(updated.transient_failure_count, 0);
        assert!(updated.last_failure_kind.is_none());
        assert!(updated.cooldown_until.is_none());
        // The row is still bound to the same source, so the next import finds it.
        let matches = RouteCredentialRepository::external_source_matches(&pool, "cc-switch")
            .await
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].1.id, imported.id);
    }

    #[tokio::test]
    async fn overwrite_from_external_source_keeps_revoked_accounts_revoked() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let imported = RouteCredentialRepository::create_tx_with_external_source(
            &mut tx,
            "claude",
            "api",
            "Revoked",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-old"}"#,
            "{}",
            "{}",
            Some(ExternalSourceRef {
                client: "cc-switch",
                source_id: "provider-1",
            }),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        RouteCredentialRepository::update_status(&pool, &imported.id, "revoked")
            .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        RouteCredentialRepository::overwrite_from_external_source(
            &mut tx,
            &imported.id,
            "Revoked",
            r#"{"api_key":"sk-new"}"#,
            "{}",
            "{}",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let updated = RouteCredentialRepository::get(&pool, &imported.id)
            .await
            .unwrap();
        assert_eq!(updated.status, "revoked");
        assert!(updated.secret_payload_json.contains("sk-new"));
    }

    #[tokio::test]
    async fn create_tx_preserves_quota_allocates_order_and_obeys_rollback() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let config = r#"{"subscription_type":"team","primary_remain":11,"weekly_remain":22,"reset_primary":"2026-08-05T00:00:00Z","reset_weekly":"2026-08-12T00:00:00Z","quota_limit":100,"quota_used":67}"#;
        let direct = RouteCredentialRepository::create(
            &pool,
            "codex",
            "official",
            "Direct",
            Some("direct@example.com".to_string()),
            "ok",
            None,
            r#"{"access_token":"direct"}"#,
            config,
            "{}",
        )
        .await
        .unwrap();
        assert_eq!(direct.sort_order, 0);
        assert_eq!(direct.subscription_type.as_deref(), Some("team"));
        assert_eq!(direct.primary_remain, Some(11));
        assert_eq!(direct.weekly_remain, Some(22));
        assert_eq!(direct.quota_remaining, Some(11));

        let mut tx = pool.begin().await.unwrap();
        let first = RouteCredentialRepository::create_tx(
            &mut tx,
            "codex",
            "official",
            "Transactional one",
            Some("one@example.com".to_string()),
            "ok",
            None,
            r#"{"access_token":"one"}"#,
            config,
            "{}",
        )
        .await
        .unwrap();
        let second = RouteCredentialRepository::create_tx(
            &mut tx,
            "codex",
            "api",
            "Transactional two",
            None,
            "ok",
            None,
            r#"{"api_key":"two"}"#,
            r#"{"base_url":"https://example.com"}"#,
            "{}",
        )
        .await
        .unwrap();

        assert_eq!(first.sort_order, 1);
        assert_eq!(second.sort_order, 2);
        assert_eq!(first.subscription_type, direct.subscription_type);
        assert_eq!(first.primary_remain, direct.primary_remain);
        assert_eq!(first.weekly_remain, direct.weekly_remain);
        assert_eq!(first.reset_primary, direct.reset_primary);
        assert_eq!(first.reset_weekly, direct.reset_weekly);
        assert_eq!(first.quota_remaining, direct.quota_remaining);
        assert_eq!(first.quota_limit, direct.quota_limit);
        assert_eq!(first.quota_used, direct.quota_used);

        tx.rollback().await.unwrap();

        let remaining = RouteCredentialRepository::list_by_platform(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, direct.id);
    }

    #[tokio::test]
    async fn transfer_fingerprint_candidates_filter_and_sort_deterministically() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let codex_official = RouteCredentialRepository::create(
            &pool,
            "codex",
            "official",
            "Codex official",
            None,
            "ok",
            None,
            "{}",
            "{}",
            "{}",
        )
        .await
        .unwrap();
        let codex_api = create_api_credential(&pool, "codex", "Codex API").await;
        let claude_api = create_api_credential(&pool, "claude", "Claude API").await;
        let _grok_api = create_api_credential(&pool, "grok", "Grok API").await;

        let candidates = RouteCredentialRepository::list_transfer_fingerprint_candidates(
            &pool,
            &[
                "codex".to_string(),
                "claude".to_string(),
                "codex".to_string(),
            ],
        )
        .await
        .unwrap();
        let actual = candidates
            .iter()
            .map(|item| (item.platform.as_str(), item.kind.as_str(), item.id.as_str()))
            .collect::<Vec<_>>();
        let mut expected = vec![
            ("codex", "official", codex_official.id.as_str()),
            ("codex", "api", codex_api.id.as_str()),
            ("claude", "api", claude_api.id.as_str()),
        ];
        expected.sort_unstable();
        assert_eq!(actual, expected);

        let empty = RouteCredentialRepository::list_transfer_fingerprint_candidates(&pool, &[])
            .await
            .unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn list_by_ids_deduplicates_and_filters_by_selection_context() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let pool_primary = create_api_credential(&pool, "codex", "Pool primary").await;
        let pool_created_old = create_api_credential(&pool, "codex", "Pool created old").await;
        let pool_tie_a = create_api_credential(&pool, "codex", "Pool tie A").await;
        let pool_tie_b = create_api_credential(&pool, "codex", "Pool tie B").await;
        let outside = create_api_credential(&pool, "codex", "Outside").await;
        let disabled_member = create_api_credential(&pool, "codex", "Disabled member").await;
        let other_platform = create_api_credential(&pool, "claude", "Claude pool").await;
        crate::database::repositories::route_pool_repository::RoutePoolRepository::replace_members(
            &pool,
            "codex",
            &[
                pool_primary.id.clone(),
                pool_created_old.id.clone(),
                pool_tie_a.id.clone(),
                pool_tie_b.id.clone(),
                disabled_member.id.clone(),
            ],
        )
        .await
        .unwrap();
        crate::database::repositories::route_pool_repository::RoutePoolRepository::replace_members(
            &pool,
            "claude",
            &[other_platform.id.clone()],
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE route_pool_members SET enabled = 0 WHERE platform = ? AND route_credential_id = ?",
        )
        .bind("codex")
        .bind(&disabled_member.id)
        .execute(&pool)
        .await
        .unwrap();

        for (id, sort_order, created_at) in [
            (&pool_primary.id, 0, "2026-08-04T00:00:00Z"),
            (&outside.id, 1, "2026-08-04T03:00:00Z"),
            (&disabled_member.id, 2, "2026-08-04T04:00:00Z"),
            (&pool_created_old.id, 10, "2026-08-04T01:00:00Z"),
            (&pool_tie_a.id, 10, "2026-08-04T02:00:00Z"),
            (&pool_tie_b.id, 10, "2026-08-04T02:00:00Z"),
        ] {
            sqlx::query(
                "UPDATE route_credentials SET sort_order = ?, created_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(sort_order)
            .bind(created_at)
            .bind(created_at)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }

        let mut all_ids = vec![
            outside.id.clone(),
            disabled_member.id.clone(),
            pool_primary.id.clone(),
            pool_created_old.id.clone(),
            pool_tie_b.id.clone(),
            other_platform.id.clone(),
            pool_tie_a.id.clone(),
            pool_tie_b.id.clone(),
            "missing".to_string(),
        ];
        all_ids.extend(std::iter::repeat(pool_tie_b.id.clone()).take(40_000));
        let in_pool = RouteCredentialRepository::list_by_ids(
            &pool,
            &all_ids,
            &RouteCredentialSelectionContext {
                platform: "codex".to_string(),
                pool_scope: RouteCredentialPoolScope::InPool,
            },
        )
        .await
        .unwrap();
        let mut tied_ids = [pool_tie_a.id.as_str(), pool_tie_b.id.as_str()];
        tied_ids.sort_unstable();
        assert_eq!(
            in_pool
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                pool_primary.id.as_str(),
                tied_ids[0],
                tied_ids[1],
                pool_created_old.id.as_str(),
            ]
        );

        let out_of_pool = RouteCredentialRepository::list_by_ids(
            &pool,
            &all_ids,
            &RouteCredentialSelectionContext {
                platform: "codex".to_string(),
                pool_scope: RouteCredentialPoolScope::OutOfPool,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            out_of_pool
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec![outside.id.as_str(), disabled_member.id.as_str()]
        );
    }

    #[tokio::test]
    async fn list_by_ids_returns_empty_for_empty_input() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();

        let rows = RouteCredentialRepository::list_by_ids(
            &pool,
            &[],
            &RouteCredentialSelectionContext {
                platform: "codex".to_string(),
                pool_scope: RouteCredentialPoolScope::InPool,
            },
        )
        .await
        .unwrap();

        assert!(rows.is_empty());
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
    async fn new_credentials_start_at_the_default_max_concurrency() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "Default concurrency").await;

        assert_eq!(
            created.max_concurrency,
            DEFAULT_ROUTE_CREDENTIAL_MAX_CONCURRENCY
        );

        let mut tx = pool.begin().await.unwrap();
        let transactional = RouteCredentialRepository::create_tx(
            &mut tx,
            "codex",
            "api",
            "Transactional concurrency",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            r#"{"base_url":"https://example.com"}"#,
            "{}",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            transactional.max_concurrency,
            DEFAULT_ROUTE_CREDENTIAL_MAX_CONCURRENCY
        );
    }

    #[tokio::test]
    async fn set_statuses_updates_all_selected_accounts_atomically() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let first = create_api_credential(&pool, "codex", "First").await;
        let second = create_api_credential(&pool, "codex", "Second").await;
        let third = create_api_credential(&pool, "codex", "Third").await;

        RouteCredentialRepository::set_statuses(
            &pool,
            &[first.id.clone(), second.id.clone(), first.id.clone()],
            "paused",
        )
        .await
        .unwrap();

        let listed = RouteCredentialRepository::list_by_platform(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|credential| (credential.id.as_str(), credential.status.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (first.id.as_str(), "paused"),
                (second.id.as_str(), "paused"),
                (third.id.as_str(), "ok"),
            ]
        );

        let empty_error = RouteCredentialRepository::set_statuses(&pool, &[], "ok")
            .await
            .expect_err("empty selection");
        assert!(matches!(
            empty_error,
            AppError::Validation {
                code: "validation.route_credential_selection_empty",
                ..
            }
        ));

        let invalid_status_error =
            RouteCredentialRepository::set_statuses(&pool, &[first.id.clone()], "invalid")
                .await
                .expect_err("invalid status");
        assert!(matches!(
            invalid_status_error,
            AppError::Validation {
                code: "validation.route_credential_status",
                ..
            }
        ));

        let missing_error = RouteCredentialRepository::set_statuses(
            &pool,
            &[first.id.clone(), "missing".to_string()],
            "error",
        )
        .await
        .expect_err("missing account");
        assert!(matches!(
            missing_error,
            AppError::Validation {
                code: "validation.route_credential_not_found",
                ..
            }
        ));

        let unchanged = RouteCredentialRepository::get(&pool, &first.id)
            .await
            .unwrap();
        assert_eq!(unchanged.status, "paused");
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
            in_pool
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
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
    async fn archive_scope_hides_active_views_preserves_pool_membership_and_restores() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let in_pool = create_api_credential(&pool, "codex", "In pool").await;
        let out_of_pool = create_api_credential(&pool, "codex", "Out of pool").await;
        crate::database::repositories::route_pool_repository::RoutePoolRepository::replace_members(
            &pool,
            "codex",
            std::slice::from_ref(&in_pool.id),
        )
        .await
        .unwrap();

        RouteCredentialRepository::set_archived(
            &pool,
            &[
                in_pool.id.clone(),
                out_of_pool.id.clone(),
                in_pool.id.clone(),
            ],
            true,
        )
        .await
        .unwrap();

        assert_eq!(
            RouteCredentialRepository::page(
                &pool,
                page_request("codex", 1, RouteCredentialPoolScope::InPool),
            )
            .await
            .unwrap()
            .total,
            0
        );
        assert_eq!(
            RouteCredentialRepository::page(
                &pool,
                page_request("codex", 1, RouteCredentialPoolScope::OutOfPool),
            )
            .await
            .unwrap()
            .total,
            0
        );
        let archived = RouteCredentialRepository::page(
            &pool,
            page_request("codex", 1, RouteCredentialPoolScope::Archived),
        )
        .await
        .unwrap();
        assert_eq!(archived.total, 2);
        assert!(archived.items.iter().all(|item| item.archived_at.is_some()));

        let member_ids = crate::database::repositories::route_pool_repository::RoutePoolRepository::list_member_ids(
            &pool,
            "codex",
        )
        .await
        .unwrap();
        assert_eq!(member_ids, vec![in_pool.id.clone()]);
        assert!(RouteCredentialRepository::list_by_platform(&pool, "codex")
            .await
            .unwrap()
            .is_empty());

        let empty_error = RouteCredentialRepository::set_archived(&pool, &[], true)
            .await
            .unwrap_err();
        assert!(matches!(
            empty_error,
            AppError::Validation {
                code: "validation.route_credential_selection_empty",
                ..
            }
        ));

        let missing_error = RouteCredentialRepository::set_archived(
            &pool,
            &[out_of_pool.id.clone(), "missing".to_string()],
            true,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            missing_error,
            AppError::Validation {
                code: "validation.route_credential_not_found",
                ..
            }
        ));
        assert!(RouteCredentialRepository::get(&pool, &out_of_pool.id)
            .await
            .unwrap()
            .archived_at
            .is_some());

        RouteCredentialRepository::set_archived(
            &pool,
            &[in_pool.id.clone(), out_of_pool.id.clone()],
            false,
        )
        .await
        .unwrap();
        assert_eq!(
            RouteCredentialRepository::page(
                &pool,
                page_request("codex", 1, RouteCredentialPoolScope::InPool),
            )
            .await
            .unwrap()
            .total,
            1
        );
        assert_eq!(
            RouteCredentialRepository::page(
                &pool,
                page_request("codex", 1, RouteCredentialPoolScope::OutOfPool),
            )
            .await
            .unwrap()
            .total,
            1
        );
        assert_eq!(
            RouteCredentialRepository::page(
                &pool,
                page_request("codex", 1, RouteCredentialPoolScope::Archived),
            )
            .await
            .unwrap()
            .total,
            0
        );
    }

    #[tokio::test]
    async fn reorder_in_pool_across_page_boundary_preserves_out_of_pool_slots() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let mut credentials = Vec::new();
        for index in 0..23 {
            credentials
                .push(create_api_credential(&pool, "codex", &format!("Account {index:02}")).await);
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
        assert_eq!(
            first_page.next_page_account_id.as_deref(),
            Some(member_ids[20].as_str())
        );
        assert_eq!(
            second_page.previous_page_account_id.as_deref(),
            Some(member_ids[19].as_str())
        );

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
            second_page_after
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
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
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[],"failure_policy":{"cooldown_enabled":true,"cooldown_seconds":30}}"#,
            "{}",
        )
        .await
        .unwrap();

        let before_first = Utc::now();
        let first = RouteCredentialRepository::record_transient_failure(
            &pool,
            &created.id,
            "transport",
            &"x".repeat(600),
            None,
            FailureScope::Account,
        )
        .await
        .unwrap();
        assert_eq!(first.failure_count, 1);
        // Every trigger uses the same configured cooldown, so the very first
        // failure already parks the account instead of only counting it.
        assert_eq!(first.next_retry_at, first.cooldown_until);
        assert_cooldown_within(&first, before_first, 30);

        let before_second = Utc::now();
        let second = RouteCredentialRepository::record_transient_failure(
            &pool,
            &created.id,
            "transport",
            "temporary",
            None,
            FailureScope::Account,
        )
        .await
        .unwrap();
        assert_eq!(second.failure_count, 2);
        assert_cooldown_within(&second, before_second, 30);

        let before_third = Utc::now();
        let third = RouteCredentialRepository::record_transient_failure(
            &pool,
            &created.id,
            "upstream",
            "temporary",
            Some(br#"{"error":{"message":"bad key"}}"#),
            FailureScope::Account,
        )
        .await
        .unwrap();
        assert_eq!(third.failure_count, 3);
        assert_eq!(third.next_retry_at, third.cooldown_until);
        // The old schedule jumped to 10 minutes here; a repeated failure must
        // not stretch beyond the configured window any more.
        assert_cooldown_within(&third, before_third, 30);

        let stored = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .unwrap();
        assert_eq!(stored.transient_failure_count, 3);
        assert_eq!(stored.last_failure_kind.as_deref(), Some("upstream"));
        assert_eq!(stored.last_failure_message.as_deref(), Some("temporary"));
        assert_eq!(
            stored.last_failure_response_json.as_deref(),
            Some(r#"{"error":{"message":"bad key"}}"#)
        );

        RouteCredentialRepository::clear_transient_failure(&pool, &created.id, None)
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
        assert!(cleared.last_failure_response_json.is_none());
    }

    /// A cooldown deadline is "correct" when it lands inside the configured
    /// window measured from just before the call, allowing for clock movement
    /// during the write.
    fn assert_cooldown_within(state: &RetryState, started: DateTime<Utc>, seconds: i64) {
        let cooldown_until = state
            .cooldown_until
            .as_deref()
            .expect("cooldown deadline is set");
        let deadline = DateTime::parse_from_rfc3339(cooldown_until)
            .expect("cooldown deadline parses")
            .with_timezone(&Utc);
        let elapsed = (deadline - started).num_milliseconds();
        assert!(
            elapsed > (seconds - 1) * 1_000 && elapsed <= (seconds + 5) * 1_000,
            "cooldown {cooldown_until} should be about {seconds}s after {started}, got {elapsed}ms"
        );
    }

    #[tokio::test]
    async fn transient_failure_cooldown_defaults_to_ten_seconds() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Default Cooldown",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            r#"{"base_url":"https://example.com","failure_policy":{"cooldown_enabled":true}}"#,
            "{}",
        )
        .await
        .unwrap();

        let started = Utc::now();
        let state = RouteCredentialRepository::record_transient_failure(
            &pool,
            &created.id,
            "transport",
            "temporary",
            None,
            FailureScope::Account,
        )
        .await
        .unwrap();

        assert_eq!(state.failure_count, 1);
        assert_cooldown_within(&state, started, 10);
    }

    #[tokio::test]
    async fn transient_failure_skips_backoff_when_cooldown_is_disabled_by_default() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "No Cooldown").await;

        for expected in 1..=3 {
            let state = RouteCredentialRepository::record_transient_failure(
                &pool,
                &created.id,
                "transport",
                "temporary",
                None,
                FailureScope::Account,
            )
            .await
            .unwrap();
            assert_eq!(state.failure_count, expected);
            assert!(state.next_retry_at.is_none());
            assert!(state.cooldown_until.is_none());
        }

        let stored = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .unwrap();
        assert_eq!(stored.transient_failure_count, 3);
        assert!(stored.next_retry_at.is_none());
        assert!(stored.cooldown_until.is_none());
        assert_eq!(stored.last_failure_message.as_deref(), Some("temporary"));
    }

    #[tokio::test]
    async fn semantic_failure_keeps_status_ok_but_counts_streak_when_error_status_disabled() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "No Error Flag",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-test"}"#,
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[],"failure_policy":{"retry_count":2,"retry_interval_ms":200,"semantic_error_threshold":2,"error_status_enabled":false}}"#,
            "{}",
        )
        .await
        .unwrap();

        for _ in 0..4 {
            RouteCredentialRepository::record_semantic_failure_with_status(
                &pool,
                &created.id,
                Some(400),
                2,
                "permanent",
                None,
            )
            .await
            .unwrap();
        }

        let stored = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .unwrap();
        assert_eq!(stored.status, "ok");
        // The streak still accrues, so re-enabling the toggle judges real history.
        let streak_count: i64 = sqlx::query_scalar(
            "SELECT semantic_failure_streak_count FROM route_credentials WHERE id = ?",
        )
        .bind(&created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(streak_count, 2);
    }

    #[tokio::test]
    async fn semantic_failure_requires_ten_matching_errors_before_marking_account_error() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "Semantic retry API").await;

        for expected_count in 1..10 {
            RouteCredentialRepository::record_semantic_failure_with_status(
                &pool,
                &created.id,
                None,
                10,
                "Upstream rejected this request",
                Some(br#"{"error":{"message":"Upstream rejected this request"}}"#),
            )
            .await
            .unwrap();

            let stored = RouteCredentialRepository::get(&pool, &created.id)
                .await
                .unwrap();
            assert_eq!(stored.status, "ok");
            let streak_count: i64 = sqlx::query_scalar(
                "SELECT semantic_failure_streak_count FROM route_credentials WHERE id = ?",
            )
            .bind(&created.id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(streak_count, expected_count);
        }

        RouteCredentialRepository::record_semantic_failure_with_status(
            &pool,
            &created.id,
            None,
            10,
            "Upstream rejected this request",
            Some(br#"{"error":{"message":"Upstream rejected this request"}}"#),
        )
        .await
        .unwrap();

        let stored = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .unwrap();
        assert_eq!(stored.status, "error");
        let streak_count: i64 = sqlx::query_scalar(
            "SELECT semantic_failure_streak_count FROM route_credentials WHERE id = ?",
        )
        .bind(&created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(streak_count, 10);
    }

    #[tokio::test]
    async fn semantic_failure_counter_resets_when_the_error_changes() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "Semantic reset API").await;

        for _ in 0..9 {
            RouteCredentialRepository::record_semantic_failure_with_status(
                &pool,
                &created.id,
                None,
                10,
                "First upstream error",
                None,
            )
            .await
            .unwrap();
        }

        RouteCredentialRepository::record_semantic_failure_with_status(
            &pool,
            &created.id,
            None,
            10,
            "Second upstream error",
            None,
        )
        .await
        .unwrap();

        let stored = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .unwrap();
        assert_eq!(stored.status, "ok");
        let streak_count: i64 = sqlx::query_scalar(
            "SELECT semantic_failure_streak_count FROM route_credentials WHERE id = ?",
        )
        .bind(&created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(streak_count, 1);
        assert_eq!(
            stored.last_failure_message.as_deref(),
            Some("Second upstream error")
        );
    }

    #[tokio::test]
    async fn restoring_an_account_to_ok_clears_its_semantic_failure_streak() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "Semantic restore API").await;

        for _ in 0..10 {
            RouteCredentialRepository::record_semantic_failure_with_status(
                &pool,
                &created.id,
                None,
                10,
                "Upstream rejected this request",
                None,
            )
            .await
            .unwrap();
        }
        RouteCredentialRepository::set_statuses(&pool, &[created.id.clone()], "ok")
            .await
            .unwrap();

        RouteCredentialRepository::record_semantic_failure_with_status(
            &pool,
            &created.id,
            None,
            10,
            "Upstream rejected this request",
            None,
        )
        .await
        .unwrap();

        let stored = RouteCredentialRepository::get(&pool, &created.id)
            .await
            .unwrap();
        assert_eq!(stored.status, "ok");
        let streak_count: i64 = sqlx::query_scalar(
            "SELECT semantic_failure_streak_count FROM route_credentials WHERE id = ?",
        )
        .bind(&created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(streak_count, 1);
    }

    #[tokio::test]
    async fn updating_an_account_to_ok_clears_semantic_streak_without_reordering_it() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "Semantic update API").await;

        RouteCredentialRepository::record_semantic_failure_with_status(
            &pool,
            &created.id,
            None,
            10,
            "Upstream rejected this request",
            None,
        )
        .await
        .unwrap();

        let updated = RouteCredentialRepository::update(
            &pool,
            &created.id,
            &UpdateRouteCredentialInput {
                display_name: "Updated semantic API".to_string(),
                email: Some("updated@example.com".to_string()),
                status: "ok".to_string(),
                route_priority: 5,
                max_concurrency: 3,
                secret_payload_json: created.secret_payload_json.clone(),
                config_json: created.config_json.clone(),
                preview_json: created.preview_json.clone(),
            },
        )
        .await
        .unwrap();

        assert_eq!(updated.display_name, "Updated semantic API");
        assert_eq!(updated.email.as_deref(), Some("updated@example.com"));
        assert_eq!(updated.status, "ok");
        assert_eq!(updated.route_priority, 5);
        assert_eq!(updated.max_concurrency, 3);
        let streak_count: i64 = sqlx::query_scalar(
            "SELECT semantic_failure_streak_count FROM route_credentials WHERE id = ?",
        )
        .bind(&created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(streak_count, 0);
    }

    #[tokio::test]
    async fn semantic_failure_uses_the_account_specific_error_threshold() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "Custom threshold API").await;

        for _ in 0..2 {
            RouteCredentialRepository::record_semantic_failure_with_status(
                &pool,
                &created.id,
                Some(400),
                3,
                "Unsupported model",
                None,
            )
            .await
            .unwrap();
        }
        assert_eq!(
            RouteCredentialRepository::get(&pool, &created.id)
                .await
                .unwrap()
                .status,
            "ok"
        );

        RouteCredentialRepository::record_semantic_failure_with_status(
            &pool,
            &created.id,
            Some(400),
            3,
            "Unsupported model",
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            RouteCredentialRepository::get(&pool, &created.id)
                .await
                .unwrap()
                .status,
            "error"
        );
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

    #[tokio::test]
    async fn reactivating_an_account_keeps_paused_models_paused() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "Reactivate").await;

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &created.id,
            "auto-parked",
            "upstream_status",
            "boom",
            None,
            Some(600),
            Some(429),
            10,
            true,
        )
        .await
        .expect("park");
        drop(conn);
        RouteCredentialModelRepository::set_status(&pool, &created.id, "held", "paused")
            .await
            .expect("pause");

        RouteCredentialRepository::reactivate_credential(&pool, &created.id)
            .await
            .expect("reactivate");

        let states = RouteCredentialModelRepository::list_for_credentials(&pool, &[created.id])
            .await
            .expect("states");
        // Scheduled recovery clears what automation parked, never what the user did.
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].model_key, "held");
    }

    #[tokio::test]
    async fn recovery_candidates_flag_accounts_whose_only_problem_is_a_model() {
        let pool = crate::database::create_memory_pool().await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let created = create_api_credential(&pool, "codex", "Model only").await;

        let candidates = RouteCredentialRepository::list_recovery_candidates(&pool)
            .await
            .expect("candidates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].has_model_failures, 0);

        // A paused model is the user's decision, not something to recover from.
        RouteCredentialModelRepository::set_status(&pool, &created.id, "held", "paused")
            .await
            .expect("pause");
        let candidates = RouteCredentialRepository::list_recovery_candidates(&pool)
            .await
            .expect("candidates");
        assert_eq!(candidates[0].has_model_failures, 0);

        let mut conn = pool.acquire().await.expect("conn");
        RouteCredentialModelRepository::record_transient_failure(
            &mut conn,
            &created.id,
            "auto-parked",
            "upstream_status",
            "boom",
            None,
            Some(600),
            Some(429),
            10,
            true,
        )
        .await
        .expect("park");
        drop(conn);

        let candidates = RouteCredentialRepository::list_recovery_candidates(&pool)
            .await
            .expect("candidates");
        assert_eq!(candidates[0].has_model_failures, 1);
    }
}
