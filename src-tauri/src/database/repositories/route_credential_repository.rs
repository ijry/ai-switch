use crate::error::AppError;
use crate::models::route_credential::{RouteCredential, UpdateRouteCredentialInput};
use chrono::Utc;
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct RouteCredentialRepository;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct QuotaColumns {
    subscription_type: Option<String>,
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
    let quota_remaining = json_i64(object.get("quota_remaining"));
    let quota_limit = json_i64(object.get("quota_limit"));
    let quota_used = json_i64(object.get("quota_used"));
    let quota_updated_at = object
        .get("quota_updated_at")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string);

    QuotaColumns {
        subscription_type,
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
                subscription_type, quota_remaining, quota_limit, quota_used, quota_updated_at,
                created_at, updated_at
             )
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        sqlx::query_as::<_, RouteCredential>("SELECT * FROM route_credentials WHERE id = ?")
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
            "SELECT * FROM route_credentials
             WHERE platform = ?
             ORDER BY sort_order ASC, created_at DESC",
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
                 subscription_type = ?, quota_remaining = ?, quota_limit = ?,
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
                 subscription_type = ?, quota_remaining = ?, quota_limit = ?,
                 quota_used = ?, quota_updated_at = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(secret_payload_json)
        .bind(config_json)
        .bind(quota.subscription_type)
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
            r#"{"subscription_type":"free","quota_remaining":0,"quota_limit":1000000,"quota_used":1177205,"quota_updated_at":"2026-07-22T00:00:00Z"}"#,
            r#"{"auth_json":"{}","config_toml":""}"#,
        )
        .await
        .unwrap();
        assert_eq!(created.subscription_type.as_deref(), Some("free"));
        assert_eq!(created.quota_remaining, Some(0));
        assert_eq!(created.quota_limit, Some(1_000_000));
        assert_eq!(created.quota_used, Some(1_177_205));
        assert_eq!(
            created.quota_updated_at.as_deref(),
            Some("2026-07-22T00:00:00Z")
        );
    }

    #[test]
    fn quota_columns_from_config_json_reads_values() {
        let quota = quota_columns_from_config_json(
            r#"{"subscription_type":"free","quota_remaining":0,"quota_limit":1000000,"quota_used":1177205,"quota_updated_at":"2026-07-22T00:00:00Z"}"#,
        );
        assert_eq!(quota.subscription_type.as_deref(), Some("free"));
        assert_eq!(quota.quota_remaining, Some(0));
        assert_eq!(quota.quota_limit, Some(1_000_000));
        assert_eq!(quota.quota_used, Some(1_177_205));
        assert_eq!(
            quota.quota_updated_at.as_deref(),
            Some("2026-07-22T00:00:00Z")
        );
    }
}
