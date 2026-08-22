use crate::app_state::AppState;
use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
use crate::error::ApiError;
use crate::models::config_snapshot::{ConfigSnapshotSummary, ConfigWriteOutcome};
use crate::models::target_app::{TargetApp, TargetConfigStatus};
use crate::paths::AppPaths;
use crate::services::config_write_service::{ConfigWriteCoordinator, ConfigWriteRuntimeState};
use crate::services::target_service::TargetService;
use sqlx::SqlitePool;
#[cfg(test)]
use std::path::Path;
use tauri::State;

#[tauri::command]
pub async fn list_target_apps(state: State<'_, AppState>) -> Result<Vec<TargetApp>, ApiError> {
    TargetService::list_targets(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_target_config_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<TargetConfigStatus>, ApiError> {
    TargetService::list_config_statuses(&state.pool, &state.config_writes)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_config_snapshots(
    state: State<'_, AppState>,
    target_app_id: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<ConfigSnapshotSummary>, ApiError> {
    list_config_snapshots_inner(
        &state.pool,
        &state.config_writes,
        target_app_id.as_deref(),
        limit,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
pub async fn rollback_config_snapshot(
    state: State<'_, AppState>,
    id: String,
) -> Result<ConfigWriteOutcome, ApiError> {
    rollback_config_snapshot_inner(&state.paths, &state.pool, &state.config_writes, &id)
        .await
        .map_err(ApiError::from)
}

async fn list_config_snapshots_inner(
    pool: &SqlitePool,
    runtime: &ConfigWriteRuntimeState,
    target_app_id: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<ConfigSnapshotSummary>, crate::error::AppError> {
    ConfigWriteCoordinator::reconcile_prepared(pool, runtime).await?;
    ConfigSnapshotRepository::list(pool, target_app_id, limit.unwrap_or(50).clamp(1, 200)).await
}

async fn rollback_config_snapshot_inner(
    paths: &AppPaths,
    pool: &SqlitePool,
    runtime: &ConfigWriteRuntimeState,
    id: &str,
) -> Result<ConfigWriteOutcome, crate::error::AppError> {
    ConfigWriteCoordinator::rollback(paths, pool, runtime, id).await
}

#[cfg(test)]
async fn rollback_config_snapshot_for_home_inner(
    paths: &AppPaths,
    pool: &SqlitePool,
    runtime: &ConfigWriteRuntimeState,
    home: &Path,
    id: &str,
) -> Result<ConfigWriteOutcome, crate::error::AppError> {
    ConfigWriteCoordinator::rollback_for_home(paths, pool, runtime, home, id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::route_config::{RouteConfigInput, TargetAdapterRegistry};
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::platform::PlatformId;
    use crate::paths::AppPaths;
    use crate::services::config_write_service::{
        ConfigWriteCoordinator, ConfigWriteRequest, ConfigWriteRuntimeState,
    };

    #[tokio::test]
    async fn snapshot_commands_omit_backup_paths_and_rollback_creates_new_ids() {
        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path().join("home");
        let paths = AppPaths::from_data_dir(fixture.path().join("app-data"));
        paths.ensure().await.unwrap();
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let runtime = ConfigWriteRuntimeState::default();
        let config_path = home.join(".codex").join("config.toml");
        tokio::fs::create_dir_all(config_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&config_path, b"approval_policy = \"never\"\n")
            .await
            .unwrap();
        let written = ConfigWriteCoordinator::write_one(
            &paths,
            &pool,
            &runtime,
            ConfigWriteRequest {
                adapter: TargetAdapterRegistry::new()
                    .for_platform(PlatformId::Codex)
                    .unwrap(),
                home: home.clone(),
                input: RouteConfigInput {
                    base_url: "http://127.0.0.1:43111".to_string(),
                    route_proxy_key: "sk-ai-switch-test".to_string(),
                    subagent_model: None,
                },
            },
        )
        .await
        .unwrap();

        let snapshots = list_config_snapshots_inner(&pool, &runtime, None, Some(50))
            .await
            .unwrap();
        let serialized = serde_json::to_value(&snapshots).unwrap();
        assert!(serialized[0].get("backup_path").is_none());
        assert!(serialized[0].get("metadata_json").is_none());

        let rolled_back = rollback_config_snapshot_for_home_inner(
            &paths,
            &pool,
            &runtime,
            &home,
            written.snapshot_id.as_deref().unwrap(),
        )
        .await
        .unwrap();
        assert_ne!(rolled_back.operation_id, written.operation_id);
        assert_ne!(rolled_back.snapshot_id, written.snapshot_id);
        assert_eq!(
            tokio::fs::read(&config_path).await.unwrap(),
            b"approval_policy = \"never\"\n"
        );
    }
}
