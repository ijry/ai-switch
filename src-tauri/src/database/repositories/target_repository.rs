use crate::error::AppError;
use crate::models::target_app::TargetApp;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct TargetRepository;

impl TargetRepository {
    pub async fn ensure_defaults(pool: &SqlitePool) -> Result<Vec<TargetApp>, AppError> {
        let defaults = [
            ("claude_code", "claude", "Claude Code"),
            ("claude_desktop", "claude", "Claude Desktop"),
            ("codex", "codex", "Codex"),
            ("gemini_cli", "gemini", "Gemini CLI"),
            ("grok", "grok", "Grok"),
            ("zcode_codex", "codex", "ZCode (Codex)"),
            ("zcode_claude", "claude", "ZCode (Claude)"),
            ("opencode", "opencode", "OpenCode"),
            ("openclaw", "openclaw", "OpenClaw"),
            ("hermes", "hermes", "Hermes"),
        ];

        for (index, (key, platform, display_name)) in defaults.iter().enumerate() {
            let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM target_apps WHERE key = ?")
                .bind(key)
                .fetch_one(pool)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.target_count",
                    message: "Could not count target apps".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;

            if exists.0 == 0 {
                let now = Utc::now().to_rfc3339();
                sqlx::query(
                    "INSERT INTO target_apps (id, key, platform, display_name, enabled, sort_order, created_at, updated_at) VALUES (?, ?, ?, ?, 1, ?, ?, ?)",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(key)
                .bind(platform)
                .bind(display_name)
                .bind(index as i64)
                .bind(&now)
                .bind(&now)
                .execute(pool)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.target_insert",
                    message: "Could not insert target app".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;
            } else {
                let now = Utc::now().to_rfc3339();
                sqlx::query(
                    "UPDATE target_apps SET platform = ?, updated_at = ?
                     WHERE key = ? AND (platform IS NULL OR platform <> ?)",
                )
                .bind(platform)
                .bind(&now)
                .bind(key)
                .bind(platform)
                .execute(pool)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.target_platform_update",
                    message: "Could not update target platform mapping".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;
            }
        }

        sqlx::query_as::<_, TargetApp>("SELECT * FROM target_apps ORDER BY sort_order ASC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.target_list",
                message: "Could not list target apps".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn get_by_key(pool: &SqlitePool, key: &str) -> Result<TargetApp, AppError> {
        sqlx::query_as::<_, TargetApp>("SELECT * FROM target_apps WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.target_get",
                message: "Could not load target app".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?
            .ok_or_else(|| AppError::Validation {
                code: "validation.target_not_found",
                message: "Target app does not exist".to_string(),
                details: Some(key.to_string()),
                recoverable: true,
            })
    }

    pub async fn get_by_id(pool: &SqlitePool, id: &str) -> Result<TargetApp, AppError> {
        sqlx::query_as::<_, TargetApp>("SELECT * FROM target_apps WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.target_get",
                message: "Could not load target app".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?
            .ok_or_else(|| AppError::Validation {
                code: "validation.target_not_found",
                message: "Target app does not exist".to_string(),
                details: Some(id.to_string()),
                recoverable: true,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::TargetRepository;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn ensure_defaults_inserts_grok_target() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("target defaults");

        assert!(targets.iter().any(|target| target.key == "grok"));
        assert_eq!(
            TargetRepository::get_by_key(&pool, "grok")
                .await
                .expect("Grok target")
                .platform
                .as_deref(),
            Some("grok")
        );
    }
}
