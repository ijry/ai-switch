use crate::adapters::route_config::TargetAdapterRegistry;
use crate::config_writer::ConfigWriter;
use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
use crate::database::repositories::target_repository::TargetRepository;
use crate::database::repositories::target_state_repository::TargetStateRepository;
use crate::error::AppError;
use crate::models::platform::{PlatformId, SupportLevel};
use crate::models::target_app::{ConfigWriteClientStatus, TargetApp, TargetConfigStatus};
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

    pub async fn list_config_write_clients(
        pool: &SqlitePool,
        platform: PlatformId,
    ) -> Result<Vec<ConfigWriteClientStatus>, AppError> {
        let home = BaseDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .ok_or_else(|| AppError::Filesystem {
                code: "filesystem.home_not_found",
                message: "Could not resolve the current user home directory".to_string(),
                details: None,
                recoverable: false,
            })?;
        Self::list_config_write_clients_for_home(pool, platform, &home).await
    }

    /// Clients this platform can write config for, with each one's current file
    /// state. Deliberately narrower than `list_config_statuses`: no reconcile, no
    /// snapshot counts, and it carries the client identity the dialog needs.
    pub(crate) async fn list_config_write_clients_for_home(
        pool: &SqlitePool,
        platform: PlatformId,
        home: &Path,
    ) -> Result<Vec<ConfigWriteClientStatus>, AppError> {
        TargetRepository::ensure_defaults(pool).await?;
        let registry = TargetAdapterRegistry::new();
        let mut statuses = Vec::new();

        for client in registry.clients_for_platform(platform) {
            let Some(adapter) = registry.by_client_and_platform(&client.client_key, platform)
            else {
                continue;
            };
            let path = adapter.resolve_path(home);
            let inspection = match ConfigWriter::inspect(&path).await {
                Ok(file) => adapter.inspect(&path, file.bytes.as_deref()),
                Err(_) => crate::adapters::route_config::TargetInspection {
                    file_status: "error".to_string(),
                    managed: false,
                    error_code: None,
                },
            };
            statuses.push(ConfigWriteClientStatus {
                client_key: client.client_key,
                display_name: client.display_name,
                native: client.native,
                restart_required: client.restart_required,
                target_key: client.target_key,
                platform: platform.as_str().to_string(),
                config_path: Some(path.display().to_string()),
                file_status: inspection.file_status,
                error_code: inspection.error_code,
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
    use std::path::PathBuf;

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
experimental_bearer_token = "sentinel"
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

    struct TargetFixture {
        _temp: tempfile::TempDir,
        pool: SqlitePool,
        home: PathBuf,
    }

    impl TargetFixture {
        async fn new() -> Self {
            let temp = tempfile::tempdir().expect("temp dir");
            let pool = create_memory_pool().await.expect("pool");
            run_migrations(&pool).await.expect("migrations");
            TargetRepository::ensure_defaults(&pool)
                .await
                .expect("targets");
            let home = temp.path().join("home");
            tokio::fs::create_dir_all(&home).await.expect("home");

            Self {
                _temp: temp,
                pool,
                home,
            }
        }
    }

    #[tokio::test]
    async fn config_write_clients_report_per_client_file_status() {
        let fixture = TargetFixture::new().await;
        let zcode_path = fixture.home.join(".zcode/v2/config.json");
        tokio::fs::create_dir_all(zcode_path.parent().unwrap())
            .await
            .expect("dir");
        tokio::fs::write(
            &zcode_path,
            br#"{"provider":{"builtin:bigmodel":{"kind":"anthropic"}}}"#,
        )
        .await
        .expect("write");

        let clients = TargetService::list_config_write_clients_for_home(
            &fixture.pool,
            PlatformId::Codex,
            &fixture.home,
        )
        .await
        .expect("clients");

        // Ordering is asserted in the adapter registry's own tests; this one is
        // about per-client file status, so look clients up by key and let new
        // adapters land without touching it.
        let client = |key: &str| {
            clients
                .iter()
                .find(|client| client.client_key == key)
                .unwrap_or_else(|| panic!("no {key} client"))
        };

        let codex = client("codex");
        assert!(codex.native);
        assert!(!codex.restart_required);
        assert_eq!(codex.file_status, "missing");
        assert!(codex
            .config_path
            .as_deref()
            .expect("path")
            .ends_with("config.toml"));

        let zcode = client("zcode");
        assert!(!zcode.native);
        // ZCode has no file watcher, so the UI has to tell the user to restart.
        assert!(zcode.restart_required);
        // The file exists but carries no ai-switch entry for this platform.
        assert_eq!(zcode.file_status, "unmanaged");
        assert_eq!(zcode.target_key, "zcode_codex");

        // Third-party clients whose file was never created report `missing`
        // rather than erroring the whole listing.
        for key in ["deepseek_harness", "workbuddy", "qoder_cli"] {
            assert_eq!(client(key).file_status, "missing", "{key}");
            assert!(client(key).config_path.is_some(), "{key}");
        }
    }

    #[tokio::test]
    async fn config_write_clients_surface_a_corrupt_file_without_erroring() {
        let fixture = TargetFixture::new().await;
        let zcode_path = fixture.home.join(".zcode/v2/config.json");
        tokio::fs::create_dir_all(zcode_path.parent().unwrap())
            .await
            .expect("dir");
        tokio::fs::write(&zcode_path, b"{not json")
            .await
            .expect("write");

        let clients = TargetService::list_config_write_clients_for_home(
            &fixture.pool,
            PlatformId::Codex,
            &fixture.home,
        )
        .await
        .expect("listing must not fail on a bad file");

        let zcode = clients
            .iter()
            .find(|client| client.client_key == "zcode")
            .expect("zcode");
        assert_eq!(zcode.file_status, "invalid");
        assert_eq!(
            zcode.error_code.as_deref(),
            Some("validation.route_config_existing_invalid")
        );
    }
}
