use crate::adapters::route_config::TargetAdapterRegistry;
use crate::config_writer::ConfigWriter;
use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
use crate::database::repositories::target_repository::TargetRepository;
use crate::database::repositories::target_state_repository::TargetStateRepository;
use crate::error::AppError;
use crate::models::platform::{PlatformId, SupportLevel};
use crate::models::target_app::{TargetApp, TargetConfigStatus};
use crate::services::config_write_service::{ConfigWriteCoordinator, ConfigWriteRuntimeState};
use crate::services::platform_capability_service::PlatformCapabilityService;
use directories::BaseDirs;
use sqlx::SqlitePool;
use std::path::Path;

pub struct TargetService;

impl TargetService {
    pub async fn list_targets(pool: &SqlitePool) -> Result<Vec<TargetApp>, AppError> {
        TargetRepository::ensure_defaults(pool).await
    }

    pub async fn list_config_statuses(
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
    ) -> Result<Vec<TargetConfigStatus>, AppError> {
        let home = BaseDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .ok_or_else(|| AppError::Filesystem {
                code: "filesystem.home_not_found",
                message: "Could not resolve the current user home directory".to_string(),
                details: None,
                recoverable: false,
            })?;
        Self::list_config_statuses_for_home(pool, runtime, &home).await
    }

    pub(crate) async fn list_config_statuses_for_home(
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        home: &Path,
    ) -> Result<Vec<TargetConfigStatus>, AppError> {
        let targets = TargetRepository::ensure_defaults(pool).await?;
        ConfigWriteCoordinator::reconcile_prepared_for_home(pool, runtime, home).await?;
        let registry = TargetAdapterRegistry::new();
        let mut statuses = Vec::with_capacity(targets.len());

        for target in targets {
            let state = TargetStateRepository::get(pool, &target.id).await?;
            let snapshot_count =
                ConfigSnapshotRepository::count_for_target(pool, &target.id).await?;
            let latest_snapshot =
                ConfigSnapshotRepository::latest_for_target(pool, &target.id).await?;
            let base = TargetConfigStatus {
                target: target.clone(),
                support_level: None,
                adapter_available: false,
                config_path: None,
                file_status: "unrecognized".to_string(),
                last_write_status: state
                    .as_ref()
                    .and_then(|state| state.last_write_status.clone()),
                last_error_code: state
                    .as_ref()
                    .and_then(|state| state.last_error_code.clone()),
                last_written_at: state
                    .as_ref()
                    .and_then(|state| state.last_written_at.clone()),
                snapshot_count,
                latest_snapshot,
            };
            let Some(raw_platform) = target.platform.as_deref() else {
                statuses.push(base);
                continue;
            };
            let Ok(platform) = PlatformId::parse(raw_platform) else {
                statuses.push(base);
                continue;
            };
            let support_level = match PlatformCapabilityService::get(platform).support_level {
                SupportLevel::Supported => "supported",
                SupportLevel::Partial => "partial",
            }
            .to_string();
            let Some(adapter) = registry.by_target_key(&target.key) else {
                statuses.push(TargetConfigStatus {
                    support_level: Some(support_level),
                    file_status: "adapter_unavailable".to_string(),
                    ..base
                });
                continue;
            };
            if adapter.platform() != platform {
                statuses.push(TargetConfigStatus {
                    support_level: Some(support_level),
                    file_status: "adapter_unavailable".to_string(),
                    ..base
                });
                continue;
            }

            let path = adapter.resolve_path(home);
            let inspection = match ConfigWriter::inspect(&path).await {
                Ok(file) => adapter.inspect(&path, file.bytes.as_deref()),
                Err(_) => crate::adapters::route_config::TargetInspection {
                    file_status: "error".to_string(),
                    managed: false,
                    error_code: None,
                },
            };
            statuses.push(TargetConfigStatus {
                support_level: Some(support_level),
                adapter_available: true,
                config_path: Some(path.display().to_string()),
                file_status: inspection.file_status,
                last_error_code: inspection.error_code.or(base.last_error_code),
                ..base
            });
        }

        Ok(statuses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::services::config_write_service::ConfigWriteRuntimeState;

    #[tokio::test]
    async fn list_config_statuses_reports_managed_invalid_missing_and_adapter_unavailable() {
        let fixture = tempfile::tempdir().expect("fixture");
        let home = fixture.path().join("home");
        let codex_path = home.join(".codex").join("config.toml");
        let claude_path = home.join(".claude").join("settings.json");
        tokio::fs::create_dir_all(codex_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::create_dir_all(claude_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &codex_path,
            r#"model_provider = "ai-switch"

[model_providers.ai-switch]
name = "AI Switch Route Proxy"
base_url = "http://127.0.0.1:43111/v1"
wire_api = "responses"
api_key = "sentinel"
"#,
        )
        .await
        .unwrap();
        tokio::fs::write(&claude_path, b"{not valid JSON")
            .await
            .unwrap();

        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let statuses = TargetService::list_config_statuses_for_home(
            &pool,
            &ConfigWriteRuntimeState::default(),
            &home,
        )
        .await
        .unwrap();

        let status = |key: &str| {
            statuses
                .iter()
                .find(|status| status.target.key == key)
                .unwrap()
        };
        assert_eq!(status("codex").file_status, "managed");
        assert_eq!(status("claude_code").file_status, "invalid");
        assert_eq!(status("gemini_cli").file_status, "missing");
        assert_eq!(status("hermes").file_status, "adapter_unavailable");
        assert!(status("hermes").config_path.is_none());
        assert!(!status("hermes").adapter_available);
        assert_eq!(status("hermes").support_level.as_deref(), Some("partial"));
    }

    #[tokio::test]
    async fn list_config_statuses_keeps_unrecognized_legacy_targets_read_only() {
        let fixture = tempfile::tempdir().expect("fixture");
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO target_apps (
                id, key, platform, display_name, enabled, sort_order, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("legacy-target")
        .bind("legacy_target")
        .bind("legacy-unknown")
        .bind("Legacy Target")
        .bind(1_i64)
        .bind(999_i64)
        .bind("2026-08-04T00:00:00Z")
        .bind("2026-08-04T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();

        let statuses = TargetService::list_config_statuses_for_home(
            &pool,
            &ConfigWriteRuntimeState::default(),
            fixture.path(),
        )
        .await
        .unwrap();
        let legacy = statuses
            .iter()
            .find(|status| status.target.key == "legacy_target")
            .unwrap();

        assert_eq!(legacy.support_level, None);
        assert!(!legacy.adapter_available);
        assert_eq!(legacy.config_path, None);
        assert_eq!(legacy.file_status, "unrecognized");
    }
}
