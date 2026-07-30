use crate::error::AppError;
use crate::models::route_credential::{RouteCredential, UpdateRouteCredentialInput};
use chrono::Utc;
use serde_json::Value;
use sqlx::SqlitePool;
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
        sqlx::query(
            "INSERT INTO route_credentials (
                id, platform, kind, display_name, email, status, sort_order, batch_id,
                secret_payload_json, config_json, preview_json,
                subscription_type, primary_remain, weekly_remain, reset_primary, reset_weekly,
                quota_remaining, quota_limit, quota_used, quota_updated_at,
                created_at, updated_at
             )
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(platform)
        .bind(kind)
        .bind(display_name)
        .bind(email)
        .bind(status)
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
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_credential_create",
            message: "Could not create route credential".to_string(),
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
                rc.quota_remaining,
                rc.quota_limit,
                rc.quota_used,
                rc.quota_updated_at,
                rc.created_at,
                rc.updated_at
             FROM route_credentials rc
             LEFT JOIN batches b ON b.id = rc.batch_id
             WHERE rc.platform = ?
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
             WHERE id = ?",
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
