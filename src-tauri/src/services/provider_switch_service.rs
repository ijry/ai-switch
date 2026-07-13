use crate::adapters::codex_config::{render_codex_provider_config, resolve_codex_config_path};
use crate::adapters::opencode_config::{
    render_opencode_provider_config, resolve_opencode_config_path,
};
use crate::adapters::provider_renderers::render_provider_sandbox_config;
use crate::config_writer::ConfigWriter;
use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
use crate::database::repositories::provider_repository::ProviderRepository;
use crate::database::repositories::target_repository::TargetRepository;
use crate::database::repositories::target_state_repository::TargetStateRepository;
use crate::error::AppError;
use crate::models::config_snapshot::{ConfigSnapshot, NewConfigSnapshot};
use crate::models::provider::Provider;
use crate::models::provider_switch::{
    ConfigRollbackOutcome, ProviderSwitchOutcome, ProviderSwitchRequest,
};
use crate::models::target_app::TargetApp;
use crate::models::target_state::TargetSwitchStatus;
use crate::paths::AppPaths;
use chrono::Utc;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

pub struct ProviderSwitchService;

#[derive(Debug, Clone, Default)]
struct RealConfigPathOverrides {
    codex: Option<PathBuf>,
    opencode: Option<PathBuf>,
}

impl ProviderSwitchService {
    pub async fn list_providers(pool: &SqlitePool) -> Result<Vec<Provider>, AppError> {
        ProviderRepository::list(pool).await
    }

    pub async fn list_target_switch_statuses(
        pool: &SqlitePool,
    ) -> Result<Vec<TargetSwitchStatus>, AppError> {
        TargetRepository::ensure_defaults(pool).await?;
        TargetStateRepository::list_switch_statuses(pool).await
    }

    pub async fn switch_provider(
        pool: &SqlitePool,
        paths: &AppPaths,
        request: ProviderSwitchRequest,
    ) -> Result<ProviderSwitchOutcome, AppError> {
        Self::switch_provider_inner(pool, paths, request, RealConfigPathOverrides::default()).await
    }

    #[cfg(test)]
    pub async fn switch_provider_with_codex_config_path(
        pool: &SqlitePool,
        paths: &AppPaths,
        request: ProviderSwitchRequest,
        codex_config_path: PathBuf,
    ) -> Result<ProviderSwitchOutcome, AppError> {
        Self::switch_provider_inner(
            pool,
            paths,
            request,
            RealConfigPathOverrides {
                codex: Some(codex_config_path),
                opencode: None,
            },
        )
        .await
    }

    #[cfg(test)]
    pub async fn switch_provider_with_opencode_config_path(
        pool: &SqlitePool,
        paths: &AppPaths,
        request: ProviderSwitchRequest,
        opencode_config_path: PathBuf,
    ) -> Result<ProviderSwitchOutcome, AppError> {
        Self::switch_provider_inner(
            pool,
            paths,
            request,
            RealConfigPathOverrides {
                codex: None,
                opencode: Some(opencode_config_path),
            },
        )
        .await
    }

    async fn switch_provider_inner(
        pool: &SqlitePool,
        paths: &AppPaths,
        request: ProviderSwitchRequest,
        path_overrides: RealConfigPathOverrides,
    ) -> Result<ProviderSwitchOutcome, AppError> {
        let target = TargetRepository::get(pool, &request.target_app_id).await?;
        let provider = ProviderRepository::get(pool, &request.provider_id).await?;

        if target.enabled == 0 {
            return Err(AppError::Validation {
                code: "validation.target_disabled",
                message: "Target app is disabled".to_string(),
                details: Some(target.key),
                recoverable: true,
            });
        }

        match request.mode.as_str() {
            "sandbox" => switch_provider_sandbox(pool, paths, target, provider).await,
            "real" => switch_provider_real(pool, paths, target, provider, path_overrides).await,
            _ => Err(AppError::Validation {
                code: "validation.switch_mode",
                message: "Provider switching supports sandbox or real mode".to_string(),
                details: Some(request.mode),
                recoverable: true,
            }),
        }
    }

    pub async fn rollback_config_snapshot(
        pool: &SqlitePool,
        paths: &AppPaths,
        snapshot_id: &str,
    ) -> Result<ConfigRollbackOutcome, AppError> {
        rollback_config_snapshot(pool, paths, snapshot_id).await
    }
}

async fn switch_provider_sandbox(
    pool: &SqlitePool,
    paths: &AppPaths,
    target: TargetApp,
    provider: Provider,
) -> Result<ProviderSwitchOutcome, AppError> {
    let path = sandbox_provider_path(paths, &target)?;
    let written_at = Utc::now().to_rfc3339();
    let rendered = match render_provider_sandbox_config(&target, &provider) {
        Ok(rendered) => rendered,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:sandbox",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };
    let write_outcome = match ConfigWriter::write_atomic(&path, &rendered).await {
        Ok(outcome) => outcome,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:sandbox",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };

    record_successful_attempt(
        pool,
        target,
        provider,
        "sandbox",
        "switch_provider:sandbox",
        write_outcome,
        written_at,
    )
    .await
}

async fn switch_provider_real(
    pool: &SqlitePool,
    paths: &AppPaths,
    target: TargetApp,
    provider: Provider,
    path_overrides: RealConfigPathOverrides,
) -> Result<ProviderSwitchOutcome, AppError> {
    match target.key.as_str() {
        "codex" => {
            let path = match path_overrides.codex {
                Some(path) => path,
                None => resolve_codex_config_path()?,
            };
            switch_provider_real_codex(pool, paths, target, provider, path).await
        }
        "opencode" => {
            let path = match path_overrides.opencode {
                Some(path) => path,
                None => resolve_opencode_config_path()?,
            };
            switch_provider_real_opencode(pool, paths, target, provider, path).await
        }
        _ => Err(AppError::Validation {
            code: "validation.real_target_not_supported",
            message: "Real provider switching is available for Codex and OpenCode".to_string(),
            details: Some(target.key),
            recoverable: true,
        }),
    }
}

async fn switch_provider_real_codex(
    pool: &SqlitePool,
    paths: &AppPaths,
    target: TargetApp,
    provider: Provider,
    path: PathBuf,
) -> Result<ProviderSwitchOutcome, AppError> {
    let written_at = Utc::now().to_rfc3339();
    let rendered = match render_codex_provider_config(&path, &provider).await {
        Ok(rendered) => rendered,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:real",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };
    let backup_dir = real_config_backup_dir(paths, &target)?;
    let write_outcome = match ConfigWriter::write_atomic_with_backup(
        &rendered.path,
        &rendered.contents,
        &backup_dir,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:real",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };

    record_successful_attempt(
        pool,
        target,
        provider,
        "real",
        "switch_provider:real",
        write_outcome,
        written_at,
    )
    .await
}

async fn switch_provider_real_opencode(
    pool: &SqlitePool,
    paths: &AppPaths,
    target: TargetApp,
    provider: Provider,
    path: PathBuf,
) -> Result<ProviderSwitchOutcome, AppError> {
    let written_at = Utc::now().to_rfc3339();
    let rendered = match render_opencode_provider_config(&path, &provider).await {
        Ok(rendered) => rendered,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:real",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };
    let backup_dir = real_config_backup_dir(paths, &target)?;
    let write_outcome = match ConfigWriter::write_atomic_with_backup(
        &rendered.path,
        &rendered.contents,
        &backup_dir,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:real",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };

    record_successful_attempt(
        pool,
        target,
        provider,
        "real",
        "switch_provider:real",
        write_outcome,
        written_at,
    )
    .await
}

async fn record_successful_attempt(
    pool: &SqlitePool,
    target: TargetApp,
    provider: Provider,
    mode: &str,
    operation: &str,
    write_outcome: crate::config_writer::WriteOutcome,
    written_at: String,
) -> Result<ProviderSwitchOutcome, AppError> {
    let snapshot = ConfigSnapshotRepository::insert(
        pool,
        NewConfigSnapshot {
            target_app_id: Some(target.id.clone()),
            operation: operation.to_string(),
            path: write_outcome.path.clone(),
            before_hash: write_outcome.before_hash.clone(),
            after_hash: write_outcome.after_hash.clone(),
            backup_path: write_outcome.backup_path.clone(),
            status: "written".to_string(),
            error_code: None,
        },
    )
    .await?;
    let state = TargetStateRepository::upsert_provider_state(
        pool,
        &target.id,
        &provider.id,
        "written",
        None,
        &written_at,
    )
    .await?;

    Ok(ProviderSwitchOutcome {
        target_app_id: target.id,
        target_key: target.key,
        provider_id: provider.id,
        provider_name: provider.name,
        mode: mode.to_string(),
        path: write_outcome.path,
        status: "written".to_string(),
        before_hash: write_outcome.before_hash,
        after_hash: write_outcome.after_hash,
        snapshot_id: snapshot.id,
        state_id: state.id,
        written_at,
    })
}

async fn rollback_config_snapshot(
    pool: &SqlitePool,
    paths: &AppPaths,
    snapshot_id: &str,
) -> Result<ConfigRollbackOutcome, AppError> {
    let source_snapshot = ConfigSnapshotRepository::get(pool, snapshot_id).await?;

    if source_snapshot.operation != "switch_provider:real" || source_snapshot.status != "written" {
        return Err(AppError::Validation {
            code: "validation.rollback_snapshot_not_supported",
            message: "Only successful real provider switch snapshots can be rolled back"
                .to_string(),
            details: Some(source_snapshot.id),
            recoverable: true,
        });
    }

    let target_app_id =
        source_snapshot
            .target_app_id
            .clone()
            .ok_or_else(|| AppError::Validation {
                code: "validation.rollback_snapshot_missing_target",
                message: "Rollback snapshot is not linked to a target app".to_string(),
                details: Some(source_snapshot.id.clone()),
                recoverable: true,
            })?;
    let backup_path = source_snapshot
        .backup_path
        .clone()
        .ok_or_else(|| AppError::Validation {
            code: "validation.rollback_backup_missing",
            message: "Rollback backup is missing for this snapshot".to_string(),
            details: Some(source_snapshot.id.clone()),
            recoverable: true,
        })?;
    let target = TargetRepository::get(pool, &target_app_id).await?;
    let target_path = PathBuf::from(&source_snapshot.path);
    let backup_path = PathBuf::from(backup_path);
    let rolled_back_at = Utc::now().to_rfc3339();

    if let Err(error) = ensure_backup_path_is_under_app(paths, &backup_path).await {
        record_failed_attempt(
            pool,
            &target,
            &target_path,
            "rollback_config:real",
            error.code(),
            &rolled_back_at,
        )
        .await;
        return Err(error);
    }

    let restore_outcome =
        match restore_snapshot_file(&source_snapshot, &target_path, &backup_path).await {
            Ok(outcome) => outcome,
            Err(error) => {
                record_failed_attempt(
                    pool,
                    &target,
                    &target_path,
                    "rollback_config:real",
                    error.code(),
                    &rolled_back_at,
                )
                .await;
                return Err(error);
            }
        };

    let rollback_snapshot = ConfigSnapshotRepository::insert(
        pool,
        NewConfigSnapshot {
            target_app_id: Some(target.id.clone()),
            operation: "rollback_config:real".to_string(),
            path: restore_outcome.path.clone(),
            before_hash: restore_outcome.before_hash.clone(),
            after_hash: restore_outcome.after_hash.clone(),
            backup_path: Some(backup_path.display().to_string()),
            status: "rolled_back".to_string(),
            error_code: None,
        },
    )
    .await?;
    let state = TargetStateRepository::record_rollback(pool, &target.id, &rolled_back_at).await?;

    Ok(ConfigRollbackOutcome {
        target_app_id: target.id,
        target_key: target.key,
        source_snapshot_id: source_snapshot.id,
        rollback_snapshot_id: rollback_snapshot.id,
        state_id: state.id,
        path: restore_outcome.path,
        status: "rolled_back".to_string(),
        before_hash: restore_outcome.before_hash,
        after_hash: restore_outcome.after_hash,
        rolled_back_at,
    })
}

async fn restore_snapshot_file(
    source_snapshot: &ConfigSnapshot,
    target_path: &Path,
    backup_path: &Path,
) -> Result<crate::config_writer::WriteOutcome, AppError> {
    let current_hash = ConfigWriter::hash_existing_file(target_path).await?;

    if let Some(expected_hash) = &source_snapshot.before_hash {
        let backup = tokio::fs::read(backup_path).await?;
        let backup_hash = ConfigWriter::hash_bytes(&backup);
        if &backup_hash != expected_hash {
            return Err(AppError::Filesystem {
                code: "filesystem.rollback_backup_hash_mismatch",
                message: "Rollback backup does not match the snapshot before-hash".to_string(),
                details: Some(backup_path.display().to_string()),
                recoverable: true,
            });
        }

        let mut outcome = ConfigWriter::write_atomic_bytes(target_path, &backup).await?;
        outcome.status = "rolled_back".to_string();
        return Ok(outcome);
    }

    match tokio::fs::remove_file(target_path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    Ok(crate::config_writer::WriteOutcome {
        path: target_path.display().to_string(),
        before_hash: current_hash,
        after_hash: None,
        backup_path: Some(backup_path.display().to_string()),
        status: "rolled_back".to_string(),
    })
}

async fn ensure_backup_path_is_under_app(
    paths: &AppPaths,
    backup_path: &Path,
) -> Result<(), AppError> {
    let backup_root = tokio::fs::canonicalize(&paths.backups_dir).await?;
    let backup_path = match tokio::fs::canonicalize(backup_path).await {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::Validation {
                code: "validation.rollback_backup_missing",
                message: "Rollback backup file is missing".to_string(),
                details: Some(backup_path.display().to_string()),
                recoverable: true,
            });
        }
        Err(error) => return Err(error.into()),
    };

    if !backup_path.starts_with(&backup_root) {
        return Err(AppError::Filesystem {
            code: "filesystem.rollback_backup_outside_app",
            message: "Rollback backup path is outside the app backup directory".to_string(),
            details: Some(backup_path.display().to_string()),
            recoverable: false,
        });
    }

    Ok(())
}

fn real_config_backup_dir(paths: &AppPaths, target: &TargetApp) -> Result<PathBuf, AppError> {
    if target.key.is_empty()
        || target.key.contains("..")
        || target.key.contains('/')
        || target.key.contains('\\')
    {
        return Err(AppError::Filesystem {
            code: "filesystem.backup_path_invalid",
            message: "Target key cannot be used in a backup path".to_string(),
            details: Some(target.key.clone()),
            recoverable: false,
        });
    }

    let config_backups_dir = paths.backups_dir.join("config");
    let backup_dir = config_backups_dir.join(&target.key);

    if !backup_dir.starts_with(&config_backups_dir) {
        return Err(AppError::Filesystem {
            code: "filesystem.backup_path_invalid",
            message: "Backup path escaped the config backups directory".to_string(),
            details: Some(backup_dir.display().to_string()),
            recoverable: false,
        });
    }

    Ok(backup_dir)
}

fn sandbox_provider_path(paths: &AppPaths, target: &TargetApp) -> Result<PathBuf, AppError> {
    if target.key.is_empty()
        || target.key.contains("..")
        || target.key.contains('/')
        || target.key.contains('\\')
    {
        return Err(AppError::Filesystem {
            code: "filesystem.sandbox_path_invalid",
            message: "Target key cannot be used in a sandbox path".to_string(),
            details: Some(target.key.clone()),
            recoverable: false,
        });
    }

    let targets_dir = paths.data_dir.join("targets");
    let path = targets_dir.join(&target.key).join("provider.json");

    if !path.starts_with(&targets_dir) {
        return Err(AppError::Filesystem {
            code: "filesystem.sandbox_path_invalid",
            message: "Sandbox config path escaped the targets directory".to_string(),
            details: Some(path.display().to_string()),
            recoverable: false,
        });
    }

    Ok(path)
}

async fn record_failed_attempt(
    pool: &SqlitePool,
    target: &TargetApp,
    path: &PathBuf,
    operation: &str,
    error_code: &str,
    written_at: &str,
) {
    let _ = ConfigSnapshotRepository::insert(
        pool,
        NewConfigSnapshot {
            target_app_id: Some(target.id.clone()),
            operation: operation.to_string(),
            path: path.display().to_string(),
            before_hash: None,
            after_hash: None,
            backup_path: None,
            status: "failed".to_string(),
            error_code: Some(error_code.to_string()),
        },
    )
    .await;
    let _ = TargetStateRepository::record_failure(pool, &target.id, error_code, written_at).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
    use crate::database::repositories::provider_repository::ProviderRepository;
    use crate::database::repositories::target_repository::TargetRepository;
    use crate::database::repositories::target_state_repository::TargetStateRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::provider::{NewProvider, Provider};
    use crate::models::provider_switch::ProviderSwitchRequest;
    use crate::paths::AppPaths;
    use serde_json::Value;
    use sqlx::SqlitePool;
    use tempfile::tempdir;

    async fn seeded_provider(pool: &SqlitePool) -> Provider {
        ProviderRepository::create(
            pool,
            NewProvider {
                name: "Acme Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://api.example.com/v1".to_string()),
                model_config_json: "{\"default\":\"gpt-4.1\"}".to_string(),
                target_options_json: "{\"codex\":{\"model\":\"gpt-4.1-mini\"}}".to_string(),
                secret_ref: Some("secret://provider/acme".to_string()),
            },
        )
        .await
        .expect("provider")
    }

    #[tokio::test]
    async fn switch_provider_writes_sandbox_config_and_records_state() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let codex = targets
            .iter()
            .find(|target| target.key == "codex")
            .expect("codex")
            .clone();
        let provider = seeded_provider(&pool).await;
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        paths.ensure().await.expect("paths");

        let outcome = ProviderSwitchService::switch_provider(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: codex.id.clone(),
                provider_id: provider.id.clone(),
                mode: "sandbox".to_string(),
            },
        )
        .await
        .expect("switch");

        let written = tokio::fs::read_to_string(
            paths
                .data_dir
                .join("targets")
                .join("codex")
                .join("provider.json"),
        )
        .await
        .expect("written config");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &codex.id)
            .await
            .expect("state");

        assert_eq!(outcome.status, "written");
        assert_eq!(outcome.target_key, "codex");
        assert!(written.contains("ai-switch.provider-switch.sandbox.v1"));
        assert!(
            outcome.path.ends_with("targets\\codex\\provider.json")
                || outcome.path.ends_with("targets/codex/provider.json")
        );
        assert_eq!(snapshot.status, "written");
        assert_eq!(state.active_item_type.as_deref(), Some("provider"));
        assert_eq!(state.active_item_id.as_deref(), Some(provider.id.as_str()));
    }

    #[tokio::test]
    async fn switch_provider_rejects_non_sandbox_mode() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let provider = seeded_provider(&pool).await;
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());

        let error = ProviderSwitchService::switch_provider(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: targets[0].id.clone(),
                provider_id: provider.id,
                mode: "invalid".to_string(),
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.switch_mode");
    }

    #[tokio::test]
    async fn switch_provider_records_failure_state_when_rendering_fails() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let codex = targets
            .iter()
            .find(|target| target.key == "codex")
            .expect("codex")
            .clone();
        let provider = ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Broken Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: None,
                model_config_json: "{".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("provider");
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        paths.ensure().await.expect("paths");

        let error = ProviderSwitchService::switch_provider(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: codex.id.clone(),
                provider_id: provider.id,
                mode: "sandbox".to_string(),
            },
        )
        .await
        .expect_err("error");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &codex.id)
            .await
            .expect("state");

        assert_eq!(error.code(), "validation.provider_model_config_json");
        assert_eq!(snapshot.status, "failed");
        assert_eq!(
            snapshot.error_code.as_deref(),
            Some("validation.provider_model_config_json")
        );
        assert_eq!(state.last_write_status.as_deref(), Some("failed"));
        assert_eq!(
            state.last_error_code.as_deref(),
            Some("validation.provider_model_config_json")
        );
    }

    #[tokio::test]
    async fn switch_provider_real_mode_writes_codex_config_and_records_state() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let codex = targets
            .iter()
            .find(|target| target.key == "codex")
            .expect("codex")
            .clone();
        let provider = seeded_provider(&pool).await;
        let data_dir = tempdir().expect("data dir");
        let codex_home = tempdir().expect("codex home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let codex_config_path = codex_home.path().join("config.toml");

        let outcome = ProviderSwitchService::switch_provider_with_codex_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: codex.id.clone(),
                provider_id: provider.id.clone(),
                mode: "real".to_string(),
            },
            codex_config_path.clone(),
        )
        .await
        .expect("switch");

        let written = tokio::fs::read_to_string(&codex_config_path)
            .await
            .expect("codex config");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &codex.id)
            .await
            .expect("state");

        assert_eq!(outcome.mode, "real");
        assert_eq!(outcome.status, "written");
        assert_eq!(outcome.path, codex_config_path.display().to_string());
        assert!(written.contains("model_provider"));
        assert!(written.contains("[model_providers.ai_switch_"));
        assert_eq!(snapshot.operation, "switch_provider:real");
        assert_eq!(snapshot.status, "written");
        assert!(snapshot.backup_path.is_some());
        assert_eq!(state.active_item_type.as_deref(), Some("provider"));
        assert_eq!(state.active_item_id.as_deref(), Some(provider.id.as_str()));
    }

    #[tokio::test]
    async fn switch_provider_real_mode_writes_opencode_config_and_records_state() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let opencode = targets
            .iter()
            .find(|target| target.key == "opencode")
            .expect("opencode")
            .clone();
        let provider = seeded_provider(&pool).await;
        let data_dir = tempdir().expect("data dir");
        let opencode_home = tempdir().expect("opencode home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let opencode_config_path = opencode_home.path().join("opencode.json");

        let outcome = ProviderSwitchService::switch_provider_with_opencode_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: opencode.id.clone(),
                provider_id: provider.id.clone(),
                mode: "real".to_string(),
            },
            opencode_config_path.clone(),
        )
        .await
        .expect("switch");

        let written = tokio::fs::read_to_string(&opencode_config_path)
            .await
            .expect("opencode config");
        let parsed: Value = serde_json::from_str(&written).expect("json");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &opencode.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &opencode.id)
            .await
            .expect("state");

        assert_eq!(outcome.mode, "real");
        assert_eq!(outcome.status, "written");
        assert_eq!(outcome.path, opencode_config_path.display().to_string());
        let model = parsed["model"].as_str().expect("model");
        assert!(model.starts_with("ai-switch-"));
        assert!(model.ends_with("/gpt-4.1"));
        assert!(written.contains("\"provider\""));
        assert!(written.contains("\"baseURL\""));
        assert!(written.contains("\"apiKey\""));
        assert!(!written.contains("secret://provider/acme"));
        assert_eq!(snapshot.operation, "switch_provider:real");
        assert_eq!(snapshot.status, "written");
        assert!(snapshot.backup_path.is_some());
        assert_eq!(state.active_item_type.as_deref(), Some("provider"));
        assert_eq!(state.active_item_id.as_deref(), Some(provider.id.as_str()));
    }

    #[tokio::test]
    async fn rollback_config_snapshot_restores_previous_real_config_file() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let codex = targets
            .iter()
            .find(|target| target.key == "codex")
            .expect("codex")
            .clone();
        let provider = seeded_provider(&pool).await;
        let data_dir = tempdir().expect("data dir");
        let codex_home = tempdir().expect("codex home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let codex_config_path = codex_home.path().join("config.toml");
        let original = "model = \"original\"\n";
        tokio::fs::write(&codex_config_path, original)
            .await
            .expect("seed codex config");

        ProviderSwitchService::switch_provider_with_codex_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: codex.id.clone(),
                provider_id: provider.id,
                mode: "real".to_string(),
            },
            codex_config_path.clone(),
        )
        .await
        .expect("switch");
        let source_snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");

        let outcome =
            ProviderSwitchService::rollback_config_snapshot(&pool, &paths, &source_snapshot.id)
                .await
                .expect("rollback");
        let restored = tokio::fs::read_to_string(&codex_config_path)
            .await
            .expect("restored config");
        let rollback_snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("rollback snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &codex.id)
            .await
            .expect("state");

        assert_eq!(restored, original);
        assert_eq!(outcome.status, "rolled_back");
        assert_eq!(outcome.source_snapshot_id, source_snapshot.id);
        assert_eq!(rollback_snapshot.operation, "rollback_config:real");
        assert_eq!(rollback_snapshot.status, "rolled_back");
        assert_eq!(rollback_snapshot.after_hash, source_snapshot.before_hash);
        assert_eq!(state.active_item_type, None);
        assert_eq!(state.active_item_id, None);
        assert_eq!(state.last_write_status.as_deref(), Some("rolled_back"));
    }

    #[tokio::test]
    async fn rollback_config_snapshot_removes_file_created_by_real_switch() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let opencode = targets
            .iter()
            .find(|target| target.key == "opencode")
            .expect("opencode")
            .clone();
        let provider = seeded_provider(&pool).await;
        let data_dir = tempdir().expect("data dir");
        let opencode_home = tempdir().expect("opencode home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let opencode_config_path = opencode_home.path().join("opencode.json");

        ProviderSwitchService::switch_provider_with_opencode_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: opencode.id.clone(),
                provider_id: provider.id,
                mode: "real".to_string(),
            },
            opencode_config_path.clone(),
        )
        .await
        .expect("switch");
        assert!(opencode_config_path.exists());
        let source_snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &opencode.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");

        ProviderSwitchService::rollback_config_snapshot(&pool, &paths, &source_snapshot.id)
            .await
            .expect("rollback");
        let rollback_snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &opencode.id)
            .await
            .expect("snapshot query")
            .expect("rollback snapshot");

        assert!(source_snapshot.before_hash.is_none());
        assert!(source_snapshot.backup_path.is_some());
        assert!(!opencode_config_path.exists());
        assert_eq!(rollback_snapshot.operation, "rollback_config:real");
        assert_eq!(rollback_snapshot.after_hash, None);
    }

    #[tokio::test]
    async fn switch_provider_real_mode_rejects_unsupported_target() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let claude = targets
            .iter()
            .find(|target| target.key == "claude_code")
            .expect("claude")
            .clone();
        let provider = seeded_provider(&pool).await;
        let data_dir = tempdir().expect("data dir");
        let codex_home = tempdir().expect("codex home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());

        let error = ProviderSwitchService::switch_provider_with_codex_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: claude.id,
                provider_id: provider.id,
                mode: "real".to_string(),
            },
            codex_home.path().join("config.toml"),
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.real_target_not_supported");
    }

    #[tokio::test]
    async fn switch_provider_real_mode_records_failure_after_codex_path_resolution() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let codex = targets
            .iter()
            .find(|target| target.key == "codex")
            .expect("codex")
            .clone();
        let provider = ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Broken Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: None,
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("provider");
        let data_dir = tempdir().expect("data dir");
        let codex_home = tempdir().expect("codex home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");

        let error = ProviderSwitchService::switch_provider_with_codex_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: codex.id.clone(),
                provider_id: provider.id,
                mode: "real".to_string(),
            },
            codex_home.path().join("config.toml"),
        )
        .await
        .expect_err("error");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &codex.id)
            .await
            .expect("state");

        assert_eq!(error.code(), "validation.provider_base_url_required");
        assert_eq!(snapshot.operation, "switch_provider:real");
        assert_eq!(snapshot.status, "failed");
        assert_eq!(
            snapshot.error_code.as_deref(),
            Some("validation.provider_base_url_required")
        );
        assert_eq!(state.last_write_status.as_deref(), Some("failed"));
    }

    #[tokio::test]
    async fn switch_provider_real_mode_records_failure_after_opencode_path_resolution() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let opencode = targets
            .iter()
            .find(|target| target.key == "opencode")
            .expect("opencode")
            .clone();
        let provider = ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Broken Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: None,
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("provider");
        let data_dir = tempdir().expect("data dir");
        let opencode_home = tempdir().expect("opencode home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");

        let error = ProviderSwitchService::switch_provider_with_opencode_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: opencode.id.clone(),
                provider_id: provider.id,
                mode: "real".to_string(),
            },
            opencode_home.path().join("opencode.json"),
        )
        .await
        .expect_err("error");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &opencode.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &opencode.id)
            .await
            .expect("state");

        assert_eq!(error.code(), "validation.provider_base_url_required");
        assert_eq!(snapshot.operation, "switch_provider:real");
        assert_eq!(snapshot.status, "failed");
        assert_eq!(
            snapshot.error_code.as_deref(),
            Some("validation.provider_base_url_required")
        );
        assert_eq!(state.last_write_status.as_deref(), Some("failed"));
    }
}
