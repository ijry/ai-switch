use crate::error::AppError;
use crate::models::target_app::TargetApp;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct TargetRepository;

impl TargetRepository {
    pub async fn ensure_defaults(pool: &SqlitePool) -> Result<Vec<TargetApp>, AppError> {
        let defaults = [
            ("claude_code", "Claude Code"),
            ("claude_desktop", "Claude Desktop"),
            ("codex", "Codex"),
            ("gemini_cli", "Gemini CLI"),
            ("opencode", "OpenCode"),
            ("openclaw", "OpenClaw"),
            ("hermes", "Hermes"),
        ];

        for (index, (key, display_name)) in defaults.iter().enumerate() {
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
                    "INSERT INTO target_apps (id, key, display_name, enabled, sort_order, created_at, updated_at) VALUES (?, ?, ?, 1, ?, ?, ?)",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(key)
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
}
