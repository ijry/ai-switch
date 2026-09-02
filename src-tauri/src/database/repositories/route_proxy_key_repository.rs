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

        if let Some(row) = row {
            return Ok(Some(row.get::<String, _>("platform")));
        }

        let row = sqlx::query("SELECT platform FROM route_proxy_key_aliases WHERE proxy_key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_proxy_key_alias_lookup",
                message: "Could not resolve route proxy key alias".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(row.map(|row| row.get::<String, _>("platform")))
    }

    pub async fn list_all(pool: &SqlitePool) -> Result<Vec<(String, String)>, AppError> {
        let rows = sqlx::query(
            "SELECT platform, proxy_key FROM route_proxy_keys
             UNION ALL
             SELECT platform, proxy_key FROM route_proxy_key_aliases",
        )
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

    /// Lists only platforms that already have a managed local proxy key.
    /// This read path must not create rows because HTTPS scheme changes should
    /// not generate new client configuration files.
    pub async fn list_platforms(pool: &SqlitePool) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query("SELECT platform FROM route_proxy_keys ORDER BY platform ASC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_proxy_key_list_platforms",
                message: "Could not load route proxy platforms".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("platform"))
            .collect())
    }

    /// Keys this platform used before rotation. Adoption uses them to recognize
    /// a client entry the user wired up with an older key.
    pub async fn list_aliases_for_platform(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query(
            "SELECT proxy_key FROM route_proxy_key_aliases WHERE platform = ? ORDER BY created_at DESC",
        )
        .bind(platform)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_proxy_key_alias_list",
            message: "Could not load route proxy key aliases".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("proxy_key"))
            .collect())
    }

    pub async fn get_existing_platform_key(
        pool: &SqlitePool,
        platform: &str,
    ) -> Result<Option<String>, AppError> {
        Self::get_by_platform(pool, platform).await
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

    pub async fn replace_platform_key(
        pool: &SqlitePool,
        platform: &str,
        proxy_key: &str,
    ) -> Result<Option<String>, AppError> {
        let Some(previous_key) = Self::get_by_platform(pool, platform).await? else {
            return Ok(None);
        };
        if previous_key == proxy_key {
            return Ok(Some(previous_key));
        }

        let now = Utc::now().to_rfc3339();
        let mut transaction = pool.begin().await.map_err(|err| AppError::Database {
            code: "database.route_proxy_key_rotate_begin",
            message: "Could not rotate route proxy key".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        sqlx::query(
            "INSERT OR IGNORE INTO route_proxy_key_aliases (proxy_key, platform, created_at)
             VALUES (?, ?, ?)",
        )
        .bind(&previous_key)
        .bind(platform)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_proxy_key_alias_insert",
            message: "Could not preserve the previous route proxy key".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        sqlx::query(
            "UPDATE route_proxy_keys
             SET proxy_key = ?, updated_at = ?
             WHERE platform = ? AND proxy_key = ?",
        )
        .bind(proxy_key)
        .bind(&now)
        .bind(platform)
        .bind(&previous_key)
        .execute(&mut *transaction)
        .await
        .map_err(|err| AppError::Database {
            code: "database.route_proxy_key_rotate",
            message: "Could not save the new route proxy key".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        transaction
            .commit()
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_proxy_key_rotate_commit",
                message: "Could not finish route proxy key rotation".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(Some(previous_key))
    }

    /// Removes only a key created by the current write attempt.
    pub async fn delete_if_matches(
        pool: &SqlitePool,
        platform: &str,
        proxy_key: &str,
    ) -> Result<(), AppError> {
        sqlx::query("DELETE FROM route_proxy_keys WHERE platform = ? AND proxy_key = ?")
            .bind(platform)
            .bind(proxy_key)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.route_proxy_key_delete",
                message: "Could not remove unused route proxy key".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;
        Ok(())
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

    #[tokio::test]
    async fn list_platforms_returns_existing_keys_without_creating_new_rows() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "grok", "sk-grok")
            .await
            .expect("grok");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-codex")
            .await
            .expect("codex");

        assert_eq!(
            RouteProxyKeyRepository::list_platforms(&pool)
                .await
                .expect("platforms"),
            vec!["codex".to_string(), "grok".to_string()]
        );
    }

    #[tokio::test]
    async fn delete_if_matches_does_not_remove_a_different_platform_key() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "grok", "sk-grok")
            .await
            .expect("grok key");

        RouteProxyKeyRepository::delete_if_matches(&pool, "grok", "sk-other")
            .await
            .expect("delete mismatch");
        assert_eq!(
            RouteProxyKeyRepository::get_by_platform(&pool, "grok")
                .await
                .expect("get key")
                .as_deref(),
            Some("sk-grok")
        );

        RouteProxyKeyRepository::delete_if_matches(&pool, "grok", "sk-grok")
            .await
            .expect("delete match");
        assert!(RouteProxyKeyRepository::get_by_platform(&pool, "grok")
            .await
            .expect("get removed key")
            .is_none());
    }

    #[tokio::test]
    async fn replacing_a_legacy_key_preserves_it_as_an_alias() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-test-old")
            .await
            .expect("legacy key");

        let previous =
            RouteProxyKeyRepository::replace_platform_key(&pool, "codex", "sk-ai-switch-new")
                .await
                .expect("replace key");

        assert_eq!(previous.as_deref(), Some("sk-ai-switch-test-old"));
        assert_eq!(
            RouteProxyKeyRepository::get_by_platform(&pool, "codex")
                .await
                .expect("current key")
                .as_deref(),
            Some("sk-ai-switch-new")
        );
        assert_eq!(
            RouteProxyKeyRepository::get_platform_by_key(&pool, "sk-ai-switch-test-old")
                .await
                .expect("legacy alias")
                .as_deref(),
            Some("codex")
        );
    }
}
