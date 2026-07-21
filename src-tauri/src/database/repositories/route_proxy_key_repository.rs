use crate::error::AppError;
use chrono::Utc;
use sqlx::{Row, SqlitePool};

pub struct RouteProxyKeyRepository;

impl RouteProxyKeyRepository {
    pub async fn get_by_platform(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Option<String>, AppError> {
        let row = sqlx::query("SELECT proxy_key FROM route_proxy_keys WHERE platform = ?")
            .bind(platform)
            .fetch_optional(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_proxy_key_get",
                message: "Could not load route proxy key".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(row.map(|row| row.get::<String, _>("proxy_key")))
    }

    pub async fn get_platform_by_key(
        pool: &SqlitePool,
        proxy_key: &str,
    ) -> Result<Option<String>, AppError> {
        let key = proxy_key.trim();
        if key.is_empty() {
            return Ok(None);
        }

        let row = sqlx::query("SELECT platform FROM route_proxy_keys WHERE proxy_key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_proxy_key_lookup",
                message: "Could not resolve route proxy key".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(row.map(|row| row.get::<String, _>("platform")))
    }

    pub async fn list_all(pool: &SqlitePool) -> Result<Vec<(String, String)>, AppError> {
        let rows = sqlx::query("SELECT platform, proxy_key FROM route_proxy_keys")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_proxy_key_list",
                message: "Could not load route proxy keys".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("proxy_key"),
                    row.get::<String, _>("platform"),
                )
            })
            .collect())
    }

    /// Return the existing key for the platform, or insert `proxy_key` if none exists.
    pub async fn ensure_platform_key(
        pool: &SqlitePool,
        platform: &str,
        proxy_key: &str,
    ) -> Result<String, AppError> {
        if let Some(existing) = Self::get_by_platform(pool, platform).await? {
            return Ok(existing);
        }

        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "INSERT INTO route_proxy_keys (platform, proxy_key, created_at, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(platform) DO NOTHING",
        )
        .bind(platform)
        .bind(proxy_key)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_proxy_key_insert",
            message: "Could not save route proxy key".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        if result.rows_affected() == 0 {
            if let Some(existing) = Self::get_by_platform(pool, platform).await? {
                return Ok(existing);
            }
        }

        Ok(proxy_key.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn ensure_reuses_existing_platform_key() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let first = RouteProxyKeyRepository::ensure_platform_key(&pool, "grok", "sk-ai-switch-a")
            .await
            .expect("first key");
        let second = RouteProxyKeyRepository::ensure_platform_key(&pool, "grok", "sk-ai-switch-b")
            .await
            .expect("second key");

        assert_eq!(first, "sk-ai-switch-a");
        assert_eq!(second, first);
        assert_eq!(
            RouteProxyKeyRepository::get_platform_by_key(&pool, &first)
                .await
                .expect("lookup")
                .as_deref(),
            Some("grok")
        );
    }
}
