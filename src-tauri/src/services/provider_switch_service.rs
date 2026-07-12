use crate::adapters::provider_renderers::render_provider_sandbox_config;
use crate::config_writer::ConfigWriter;
use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
use crate::database::repositories::provider_repository::ProviderRepository;
use crate::database::repositories::target_repository::TargetRepository;
use crate::database::repositories::target_state_repository::TargetStateRepository;
use crate::error::AppError;
use crate::models::config_snapshot::NewConfigSnapshot;
use crate::models::provider::Provider;
use crate::models::provider_switch::{ProviderSwitchOutcome, ProviderSwitchRequest};
use crate::models::target_app::TargetApp;
use crate::models::target_state::TargetSwitchStatus;
use crate::paths::AppPaths;
use chrono::Utc;
use sqlx::SqlitePool;
use std::path::PathBuf;

pub struct ProviderSwitchService;

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

        if request.mode != "sandbox" {
            return Err(AppError::Validation {
                code: "validation.switch_mode",
                message: "Provider switching only supports sandbox mode in B1".to_string(),
                details: Some(request.mode),
                recoverable: true,
            });
        }

        let path = sandbox_provider_path(paths, &target)?;
        let written_at = Utc::now().to_rfc3339();
        let rendered = match render_provider_sandbox_config(&target, &provider) {
            Ok(rendered) => rendered,
            Err(error) => {
                record_failed_attempt(pool, &target, &path, error.code(), &written_at).await;
                return Err(error);
            }
        };
        let write_outcome = match ConfigWriter::write_atomic(&path, &rendered).await {
            Ok(outcome) => outcome,
            Err(error) => {
                record_failed_attempt(pool, &target, &path, error.code(), &written_at).await;
                return Err(error);
            }
        };

        let snapshot = ConfigSnapshotRepository::insert(
            pool,
            NewConfigSnapshot {
                target_app_id: Some(target.id.clone()),
                operation: "switch_provider:sandbox".to_string(),
                path: write_outcome.path.clone(),
                before_hash: write_outcome.before_hash.clone(),
                after_hash: write_outcome.after_hash.clone(),
                backup_path: None,
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
            mode: "sandbox".to_string(),
            path: write_outcome.path,
            status: "written".to_string(),
            before_hash: write_outcome.before_hash,
            after_hash: write_outcome.after_hash,
            snapshot_id: snapshot.id,
            state_id: state.id,
            written_at,
        })
    }
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
    error_code: &str,
    written_at: &str,
) {
    let _ = ConfigSnapshotRepository::insert(
        pool,
        NewConfigSnapshot {
            target_app_id: Some(target.id.clone()),
            operation: "switch_provider:sandbox".to_string(),
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
                mode: "real".to_string(),
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
}
