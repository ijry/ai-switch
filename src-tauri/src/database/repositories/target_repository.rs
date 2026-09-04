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
            ("codex", "codex", "Codex"),
            ("gemini_cli", "gemini", "Gemini CLI"),
            ("grok", "grok", "Grok"),
            ("zcode_codex", "codex", "ZCode (Codex)"),
            ("zcode_claude", "claude", "ZCode (Claude)"),
            (
                "deepseek_harness_codex",
                "codex",
                "DeepSeek Harness (Codex)",
            ),
            (
                "deepseek_harness_claude",
                "claude",
                "DeepSeek Harness (Claude)",
            ),
            ("workbuddy_codex", "codex", "WorkBuddy (Codex)"),
            ("workbuddy_claude", "claude", "WorkBuddy (Claude)"),
            ("codebuddy_cli_codex", "codex", "CodeBuddy CLI (Codex)"),
            ("codebuddy_cli_claude", "claude", "CodeBuddy CLI (Claude)"),
            ("qoder_cli_codex", "codex", "Qoder CLI (Codex)"),
            ("qoder_cli_claude", "claude", "Qoder CLI (Claude)"),
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
                // `sort_order` is realigned to this array too, not just the
                // platform. It is derived from the array index, so inserting a new
                // target shifts every one after it — and an upgraded install kept
                // the numbers it was first given, leaving the pre-existing
                // opencode/openclaw/hermes rows colliding with the newly inserted
                // ones. `list_targets` orders by this column, so ties made the
                // client list order arbitrary and unstable between machines.
                // Nothing else writes it (no user-facing reordering), so this
                // array is the single source of truth.
                sqlx::query(
                    "UPDATE target_apps SET platform = ?, sort_order = ?, updated_at = ?
                     WHERE key = ? AND (platform IS NULL OR platform <> ? OR sort_order <> ?)",
                )
                .bind(platform)
                .bind(index as i64)
                .bind(&now)
                .bind(key)
                .bind(platform)
                .bind(index as i64)
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

    /// An upgraded install used to keep whatever `sort_order` it was first given,
    /// so rows added later collided with the pre-existing opencode/openclaw/hermes
    /// ones. `list_targets` orders by that column, so ties left the client list in
    /// an arbitrary order that differed between machines.
    #[tokio::test]
    async fn ensure_defaults_realigns_sort_order_for_an_upgraded_install() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        TargetRepository::ensure_defaults(&pool)
            .await
            .expect("seed");

        // Simulate the older numbering: everything piled onto one value.
        sqlx::query("UPDATE target_apps SET sort_order = 7")
            .execute(&pool)
            .await
            .expect("collide");

        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("reseed");

        let orders: Vec<i64> = targets.iter().map(|target| target.sort_order).collect();
        let mut unique = orders.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            orders.len(),
            unique.len(),
            "sort_order must be unique after a reseed: {orders:?}"
        );
        // And it is the seed array's order, ascending from zero.
        assert_eq!(orders, (0..orders.len() as i64).collect::<Vec<_>>());
    }

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

    /// `ConfigWriteService::prepare` resolves `adapter.target_key()` through
    /// `get_by_key`, which fails with `validation.target_not_found` when the seed
    /// row is missing. A registered adapter without a row is therefore an
    /// adapter whose every write fails at runtime, so the registry and this seed
    /// table have to stay in lockstep.
    #[tokio::test]
    async fn ensure_defaults_seeds_a_row_for_every_registered_adapter() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        TargetRepository::ensure_defaults(&pool)
            .await
            .expect("target defaults");

        for adapter in crate::adapters::route_config::TargetAdapterRegistry::new().adapters() {
            let target = TargetRepository::get_by_key(&pool, adapter.target_key())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "no target_apps seed row for adapter target_key {}",
                        adapter.target_key()
                    )
                });
            assert_eq!(
                target.platform.as_deref(),
                Some(adapter.platform().as_str()),
                "seed row platform disagrees with the adapter: {}",
                adapter.target_key()
            );
        }
    }
}
