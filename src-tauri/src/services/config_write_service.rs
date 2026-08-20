use crate::adapters::route_config::{
    ClaudeEnvPlan, RouteConfigInput, TargetAdapter, TargetAdapterRegistry,
};
use crate::config_writer::{hash_bytes, ConfigWriter, FileState};
use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
use crate::database::repositories::target_repository::TargetRepository;
use crate::database::repositories::target_state_repository::TargetStateRepository;
use crate::error::AppError;
use crate::models::config_snapshot::{ConfigSnapshotRecord, ConfigWriteOutcome, NewConfigSnapshot};
use crate::models::platform::{PlatformId, PlatformOperation};
use crate::models::target_app::{TargetApp, TargetAppStateUpdate};
use crate::paths::AppPaths;
use crate::services::platform_capability_service::PlatformCapabilityService;
use chrono::{Duration, Utc};
use directories::BaseDirs;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct ConfigWriteRuntimeState {
    locks: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
}

impl ConfigWriteRuntimeState {
    pub(crate) async fn lock_for_path(&self, path: &Path) -> Result<Arc<Mutex<()>>, AppError> {
        let path = normalized_absolute_path(path)?;
        let mut locks = self.locks.lock().await;
        Ok(locks
            .entry(path)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone())
    }
}

#[derive(Clone)]
pub struct ConfigWriteRequest {
    pub adapter: Arc<dyn TargetAdapter>,
    pub home: PathBuf,
    pub input: RouteConfigInput,
}

#[derive(Clone)]
pub(crate) struct PreparedConfigWrite {
    pub(crate) operation_id: String,
    pub(crate) snapshot_id: String,
    pub(crate) target_app_id: String,
    pub(crate) target_key: String,
    pub(crate) platform: PlatformId,
    pub(crate) path: PathBuf,
    pub(crate) original: FileState,
    pub(crate) replacement: Vec<u8>,
    pub(crate) expected_after_hash: String,
}

struct GroupTargetStatus {
    target_key: String,
    status: String,
    error_code: Option<String>,
}

pub struct ConfigWriteCoordinator;

impl ConfigWriteCoordinator {
    pub async fn write_one(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        request: ConfigWriteRequest,
    ) -> Result<ConfigWriteOutcome, AppError> {
        let path = request.adapter.resolve_path(&request.home);
        let lock = runtime.lock_for_path(&path).await?;
        let _guard = lock.lock().await;
        let prepared = Self::prepare_one(paths, pool, request, Uuid::new_v4().to_string()).await?;
        Self::commit_prepared(pool, prepared).await
    }

    pub async fn write_group(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        requests: Vec<ConfigWriteRequest>,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError> {
        Self::write_group_inner(paths, pool, runtime, requests, |_| {}).await
    }

    #[cfg(test)]
    pub(crate) async fn write_group_with_after_commit<F>(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        requests: Vec<ConfigWriteRequest>,
        after_commit: F,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError>
    where
        F: FnMut(&PreparedConfigWrite),
    {
        Self::write_group_inner(paths, pool, runtime, requests, after_commit).await
    }

    async fn write_group_inner<F>(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        requests: Vec<ConfigWriteRequest>,
        mut after_commit: F,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError>
    where
        F: FnMut(&PreparedConfigWrite),
    {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        let operation_id = Uuid::new_v4().to_string();
        let mut lock_paths = requests
            .iter()
            .map(|request| normalized_absolute_path(&request.adapter.resolve_path(&request.home)))
            .collect::<Result<Vec<_>, _>>()?;
        lock_paths.sort();
        lock_paths.dedup();

        let mut locks = Vec::with_capacity(lock_paths.len());
        for path in lock_paths {
            locks.push(runtime.lock_for_path(&path).await?);
        }
        let mut _guards = Vec::with_capacity(locks.len());
        for lock in locks {
            _guards.push(lock.lock_owned().await);
        }

        let mut prepared = Vec::with_capacity(requests.len());
        for request in requests {
            let pending_target_key = request.adapter.target_key().to_string();
            match Self::prepare_one(paths, pool, request, operation_id.clone()).await {
                Ok(item) => prepared.push(item),
                Err(error) => {
                    let cause_code = app_error_code(&error);
                    let mut statuses = Vec::with_capacity(prepared.len() + 1);
                    for item in &prepared {
                        let status = finalize_group_status(
                            pool,
                            item,
                            "failed",
                            Some("config.group_aborted"),
                        )
                        .await;
                        statuses.push(status);
                    }
                    statuses.push(GroupTargetStatus {
                        target_key: pending_target_key,
                        status: status_for_error(cause_code).to_string(),
                        error_code: Some(cause_code.to_string()),
                    });
                    return Err(group_write_error(&operation_id, cause_code, &statuses));
                }
            }
        }

        let mut outcomes = Vec::with_capacity(prepared.len());
        let mut committed = Vec::with_capacity(prepared.len());
        for (index, item) in prepared.iter().enumerate() {
            match Self::commit_prepared(pool, item.clone()).await {
                Ok(outcome) => {
                    committed.push(index);
                    outcomes.push(outcome);
                    after_commit(item);
                }
                Err(error) => {
                    let cause_code = app_error_code(&error);
                    let mut statuses = (0..prepared.len())
                        .map(|_| None)
                        .collect::<Vec<Option<GroupTargetStatus>>>();
                    statuses[index] = Some(GroupTargetStatus {
                        target_key: item.target_key.clone(),
                        status: status_for_error(cause_code).to_string(),
                        error_code: Some(cause_code.to_string()),
                    });

                    if current_matches_written(item).await {
                        committed.push(index);
                    }
                    for committed_index in committed.into_iter().rev() {
                        statuses[committed_index] =
                            Some(rollback_group_write(pool, &prepared[committed_index]).await);
                    }
                    for remaining in (index + 1)..prepared.len() {
                        statuses[remaining] = Some(
                            finalize_group_status(
                                pool,
                                &prepared[remaining],
                                "failed",
                                Some("config.group_aborted"),
                            )
                            .await,
                        );
                    }
                    let statuses = statuses.into_iter().flatten().collect::<Vec<_>>();
                    return Err(group_write_error(&operation_id, cause_code, &statuses));
                }
            }
        }

        Ok(outcomes)
    }

    pub(crate) async fn prepare_one(
        paths: &AppPaths,
        pool: &SqlitePool,
        request: ConfigWriteRequest,
        operation_id: String,
    ) -> Result<PreparedConfigWrite, AppError> {
        let platform = request.adapter.platform();
        PlatformCapabilityService::require(platform, PlatformOperation::ConfigWrite)?;
        TargetRepository::ensure_defaults(pool).await?;
        let target = TargetRepository::get_by_key(pool, request.adapter.target_key()).await?;
        validate_adapter_target(&target, request.adapter.as_ref())?;

        let path = request.adapter.resolve_path(&request.home);
        let original = ConfigWriter::inspect(&path).await?;
        let replacement =
            request
                .adapter
                .render(&path, original.bytes.as_deref(), &request.input)?;
        let expected_after_hash = hash_bytes(&replacement);
        let snapshot_id = Uuid::new_v4().to_string();
        let backup_path = backup_path(paths, &snapshot_id);
        let persisted_backup = if original.existed {
            let original_bytes = original
                .bytes
                .as_deref()
                .ok_or_else(|| invalid_file_state(&path, "existing file has no captured bytes"))?;
            ConfigWriter::write_private_backup(&backup_path, original_bytes).await?;
            Some(backup_path.clone())
        } else {
            None
        };

        let snapshot = ConfigSnapshotRepository::prepare_with_id(
            pool,
            &snapshot_id,
            NewConfigSnapshot {
                target_app_id: Some(target.id.clone()),
                platform: Some(platform.as_str().to_string()),
                operation: "write".to_string(),
                operation_group_id: Some(operation_id.clone()),
                source_snapshot_id: None,
                path: path.display().to_string(),
                before_hash: original.hash.clone(),
                after_hash: Some(expected_after_hash.clone()),
                backup_path: persisted_backup
                    .as_ref()
                    .map(|path| path.display().to_string()),
                original_file_existed: original.existed,
                metadata_json: safe_metadata_json(request.adapter.target_key(), "write"),
            },
        )
        .await;
        if let Err(error) = snapshot {
            cleanup_backup(persisted_backup.as_deref(), original.hash.as_deref()).await?;
            return Err(error);
        }

        Ok(PreparedConfigWrite {
            operation_id,
            snapshot_id,
            target_app_id: target.id,
            target_key: target.key,
            platform,
            path,
            original,
            replacement,
            expected_after_hash,
        })
    }

    pub(crate) async fn commit_prepared(
        pool: &SqlitePool,
        prepared: PreparedConfigWrite,
    ) -> Result<ConfigWriteOutcome, AppError> {
        match ConfigWriter::write_atomic_if_unchanged(
            &prepared.path,
            &prepared.replacement,
            &prepared.original,
        )
        .await
        {
            Ok(write) => {
                ConfigSnapshotRepository::mark_status(
                    pool,
                    &prepared.snapshot_id,
                    "succeeded",
                    write.after_hash.as_deref(),
                    None,
                )
                .await?;
                record_target_state(pool, &prepared.target_app_id, "succeeded", None).await?;
                Ok(ConfigWriteOutcome {
                    operation_id: prepared.operation_id,
                    snapshot_id: Some(prepared.snapshot_id),
                    target_app_id: Some(prepared.target_app_id),
                    target_key: prepared.target_key,
                    platform: prepared.platform.as_str().to_string(),
                    path: prepared.path.display().to_string(),
                    status: "succeeded".to_string(),
                    before_hash: write.before_hash,
                    after_hash: write.after_hash,
                    error_code: None,
                })
            }
            Err(error) => {
                let code = app_error_code(&error);
                let status = if code == "config.concurrent_modification" {
                    "conflict"
                } else {
                    "failed"
                };
                ConfigSnapshotRepository::mark_status(
                    pool,
                    &prepared.snapshot_id,
                    status,
                    None,
                    Some(code),
                )
                .await?;
                record_target_state(pool, &prepared.target_app_id, status, Some(code)).await?;
                Err(error)
            }
        }
    }

    pub async fn rollback(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        source_snapshot_id: &str,
    ) -> Result<ConfigWriteOutcome, AppError> {
        let home = resolve_home_dir()?;
        Self::rollback_for_home(paths, pool, runtime, &home, source_snapshot_id).await
    }

    pub(crate) async fn rollback_for_home(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        home: &Path,
        source_snapshot_id: &str,
    ) -> Result<ConfigWriteOutcome, AppError> {
        let source = ConfigSnapshotRepository::get(pool, source_snapshot_id).await?;
        let (target, platform, adapter) = validate_rollback_source(pool, &source).await?;
        validate_snapshot_path(&source, adapter.as_ref(), home)?;
        let path = PathBuf::from(&source.path);
        let lock = runtime.lock_for_path(&path).await?;
        let _guard = lock.lock().await;

        let source_after_hash = source.after_hash.as_deref().ok_or_else(|| {
            rollback_unavailable(source_snapshot_id, "source snapshot has no after hash")
        })?;
        let current = ConfigWriter::inspect(&path).await?;
        if !current.existed || current.hash.as_deref() != Some(source_after_hash) {
            return Err(rollback_conflict(&path));
        }

        let restore_bytes = if source.original_file_existed != 0 {
            Some(load_source_backup(paths, &source).await?)
        } else {
            None
        };
        let operation_id = Uuid::new_v4().to_string();
        let rollback_snapshot_id = Uuid::new_v4().to_string();
        let rollback_backup = backup_path(paths, &rollback_snapshot_id);
        let current_bytes = current
            .bytes
            .as_deref()
            .ok_or_else(|| invalid_file_state(&path, "rollback source has no captured bytes"))?;
        ConfigWriter::write_private_backup(&rollback_backup, current_bytes).await?;

        let intended_after_hash = if source.original_file_existed != 0 {
            source.before_hash.clone()
        } else {
            None
        };
        let prepared_snapshot = ConfigSnapshotRepository::prepare_with_id(
            pool,
            &rollback_snapshot_id,
            NewConfigSnapshot {
                target_app_id: Some(target.id.clone()),
                platform: Some(platform.as_str().to_string()),
                operation: "rollback".to_string(),
                operation_group_id: Some(operation_id.clone()),
                source_snapshot_id: Some(source.id.clone()),
                path: source.path.clone(),
                before_hash: current.hash.clone(),
                after_hash: intended_after_hash.clone(),
                backup_path: Some(rollback_backup.display().to_string()),
                original_file_existed: true,
                metadata_json: safe_metadata_json(&target.key, "rollback"),
            },
        )
        .await;
        if let Err(error) = prepared_snapshot {
            cleanup_backup(Some(&rollback_backup), current.hash.as_deref()).await?;
            return Err(error);
        }

        let mutation: Result<Option<String>, AppError> = match restore_bytes {
            Some(bytes) => ConfigWriter::write_atomic_if_unchanged(&path, &bytes, &current)
                .await
                .map(|write| write.after_hash),
            None => {
                async {
                    ConfigWriter::remove_if_hash_matches(&path, source_after_hash).await?;
                    let final_state = ConfigWriter::inspect(&path).await?;
                    if final_state.existed {
                        Err(rollback_conflict(&path))
                    } else {
                        Ok(None)
                    }
                }
                .await
            }
        };

        match mutation {
            Ok(after_hash) => {
                ConfigSnapshotRepository::mark_status(
                    pool,
                    &rollback_snapshot_id,
                    "succeeded",
                    after_hash.as_deref(),
                    None,
                )
                .await?;
                record_target_state(pool, &target.id, "succeeded", None).await?;
                Ok(ConfigWriteOutcome {
                    operation_id,
                    snapshot_id: Some(rollback_snapshot_id),
                    target_app_id: Some(target.id),
                    target_key: target.key,
                    platform: platform.as_str().to_string(),
                    path: path.display().to_string(),
                    status: "succeeded".to_string(),
                    before_hash: current.hash,
                    after_hash,
                    error_code: None,
                })
            }
            Err(error) => {
                let original_code = app_error_code(&error);
                let (status, public_error) = if matches!(
                    original_code,
                    "config.concurrent_modification" | "config.rollback_conflict"
                ) {
                    ("conflict", rollback_conflict(&path))
                } else {
                    ("failed", error)
                };
                let code = app_error_code(&public_error);
                ConfigSnapshotRepository::mark_status(
                    pool,
                    &rollback_snapshot_id,
                    status,
                    None,
                    Some(code),
                )
                .await?;
                record_target_state(pool, &target.id, status, Some(code)).await?;
                Err(public_error)
            }
        }
    }

    pub async fn reconcile_prepared(
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
    ) -> Result<(), AppError> {
        let home = resolve_home_dir()?;
        Self::reconcile_prepared_for_home(pool, runtime, &home).await
    }

    pub(crate) async fn reconcile_prepared_for_home(
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        home: &Path,
    ) -> Result<(), AppError> {
        let cutoff = (Utc::now() - Duration::minutes(5)).to_rfc3339();
        for snapshot in ConfigSnapshotRepository::list_prepared_before(pool, &cutoff).await? {
            let authorization = validate_reconciliation_target(pool, &snapshot).await;
            let (target, path) = match authorization {
                Ok((target, adapter)) => {
                    if let Err(error) = validate_snapshot_path(&snapshot, adapter.as_ref(), home) {
                        let code = app_error_code(&error);
                        ConfigSnapshotRepository::mark_status(
                            pool,
                            &snapshot.id,
                            "conflict",
                            None,
                            Some(code),
                        )
                        .await?;
                        continue;
                    }
                    (Some(target), PathBuf::from(&snapshot.path))
                }
                Err(error) => {
                    let code = app_error_code(&error);
                    ConfigSnapshotRepository::mark_status(
                        pool,
                        &snapshot.id,
                        "conflict",
                        None,
                        Some(code),
                    )
                    .await?;
                    continue;
                }
            };
            let lock = runtime.lock_for_path(&path).await?;
            let _guard = lock.lock().await;
            let inspection = ConfigWriter::inspect(&path).await;
            let (status, error_code, after_hash) = match inspection {
                Ok(current) if matches_after(&snapshot, &current) => {
                    ("succeeded", None, current.hash)
                }
                Ok(current) if matches_before(&snapshot, &current) => {
                    ("failed", Some("config.interrupted_before_commit"), None)
                }
                Ok(_) => ("conflict", Some("config.concurrent_modification"), None),
                Err(error) => ("conflict", Some(app_error_code(&error)), None),
            };
            ConfigSnapshotRepository::mark_status(
                pool,
                &snapshot.id,
                status,
                after_hash.as_deref(),
                error_code,
            )
            .await?;
            if let Some(target) = target {
                record_target_state(pool, &target.id, status, error_code).await?;
            }
        }
        Ok(())
    }
}

async fn current_matches_written(prepared: &PreparedConfigWrite) -> bool {
    matches!(
        ConfigWriter::inspect(&prepared.path).await,
        Ok(current)
            if current.existed
                && current.hash.as_deref() == Some(prepared.expected_after_hash.as_str())
    )
}

async fn rollback_group_write(
    pool: &SqlitePool,
    prepared: &PreparedConfigWrite,
) -> GroupTargetStatus {
    let current = match ConfigWriter::inspect(&prepared.path).await {
        Ok(current) => current,
        Err(error) => {
            let code = app_error_code(&error);
            return finalize_group_status(pool, prepared, status_for_error(code), Some(code)).await;
        }
    };
    if !current.existed || current.hash.as_deref() != Some(prepared.expected_after_hash.as_str()) {
        return finalize_group_status(pool, prepared, "conflict", Some("config.rollback_conflict"))
            .await;
    }

    let restored = if prepared.original.existed {
        match prepared.original.bytes.as_deref() {
            Some(bytes) => ConfigWriter::write_atomic_if_unchanged(&prepared.path, bytes, &current)
                .await
                .map(|_| ()),
            None => Err(invalid_file_state(
                &prepared.path,
                "existing group target has no captured bytes",
            )),
        }
    } else {
        async {
            ConfigWriter::remove_if_hash_matches(&prepared.path, &prepared.expected_after_hash)
                .await?;
            let final_state = ConfigWriter::inspect(&prepared.path).await?;
            if final_state.existed {
                Err(rollback_conflict(&prepared.path))
            } else {
                Ok(())
            }
        }
        .await
    };

    match restored {
        Ok(()) => {
            finalize_group_status(pool, prepared, "failed", Some("config.group_rolled_back")).await
        }
        Err(error) => {
            let code = app_error_code(&error);
            finalize_group_status(pool, prepared, status_for_error(code), Some(code)).await
        }
    }
}

async fn finalize_group_status(
    pool: &SqlitePool,
    prepared: &PreparedConfigWrite,
    status: &str,
    error_code: Option<&str>,
) -> GroupTargetStatus {
    let mut final_status = status.to_string();
    let mut final_error_code = error_code.map(str::to_string);
    if let Err(error) =
        ConfigSnapshotRepository::mark_status(pool, &prepared.snapshot_id, status, None, error_code)
            .await
    {
        final_status = "failed".to_string();
        final_error_code = Some(app_error_code(&error).to_string());
    }
    if let Err(error) = record_target_state(
        pool,
        &prepared.target_app_id,
        &final_status,
        final_error_code.as_deref(),
    )
    .await
    {
        final_status = "failed".to_string();
        final_error_code = Some(app_error_code(&error).to_string());
    }

    GroupTargetStatus {
        target_key: prepared.target_key.clone(),
        status: final_status,
        error_code: final_error_code,
    }
}

fn status_for_error(code: &str) -> &'static str {
    if matches!(
        code,
        "config.concurrent_modification" | "config.rollback_conflict"
    ) {
        "conflict"
    } else {
        "failed"
    }
}

fn group_write_error(
    operation_id: &str,
    cause_code: &str,
    statuses: &[GroupTargetStatus],
) -> AppError {
    let targets = statuses
        .iter()
        .map(|status| {
            serde_json::json!({
                "target_key": status.target_key,
                "status": status.status,
                "error_code": status.error_code,
            })
        })
        .collect::<Vec<_>>();
    AppError::Filesystem {
        code: "filesystem.route_config_write",
        message: "Could not complete grouped configuration writes".to_string(),
        details: Some(
            serde_json::json!({
                "operation_id": operation_id,
                "cause_code": cause_code,
                "targets": targets,
            })
            .to_string(),
        ),
        recoverable: true,
    }
}

fn validate_adapter_target(
    target: &TargetApp,
    adapter: &dyn TargetAdapter,
) -> Result<(), AppError> {
    let target_platform = target
        .platform
        .as_deref()
        .ok_or_else(|| adapter_target_mismatch(&target.key))
        .and_then(PlatformId::parse)?;
    if target_platform != adapter.platform() {
        return Err(adapter_target_mismatch(&target.key));
    }
    Ok(())
}

async fn validate_rollback_source(
    pool: &SqlitePool,
    source: &ConfigSnapshotRecord,
) -> Result<(TargetApp, PlatformId, Arc<dyn TargetAdapter>), AppError> {
    if source.status != "succeeded" || source.operation != "write" {
        return Err(rollback_unavailable(
            &source.id,
            "only succeeded write snapshots can be rolled back",
        ));
    }
    let target_id = source
        .target_app_id
        .as_deref()
        .ok_or_else(|| rollback_unavailable(&source.id, "source snapshot has no target app"))?;
    let target = TargetRepository::get_by_id(pool, target_id).await?;
    let platform = snapshot_platform(source, &target)?;
    PlatformCapabilityService::require(platform, PlatformOperation::ConfigWrite)?;
    let adapter = TargetAdapterRegistry::new()
        .by_target_key(&target.key)
        .ok_or_else(|| adapter_unavailable(&target.key))?;
    validate_adapter_target(&target, adapter.as_ref())?;
    if adapter.platform() != platform {
        return Err(adapter_target_mismatch(&target.key));
    }
    Ok((target, platform, adapter))
}

async fn validate_reconciliation_target(
    pool: &SqlitePool,
    snapshot: &ConfigSnapshotRecord,
) -> Result<(TargetApp, Arc<dyn TargetAdapter>), AppError> {
    let target_id = snapshot
        .target_app_id
        .as_deref()
        .ok_or_else(|| adapter_unavailable("snapshot target missing"))?;
    let target = TargetRepository::get_by_id(pool, target_id).await?;
    let platform = snapshot_platform(snapshot, &target)?;
    PlatformCapabilityService::require(platform, PlatformOperation::ConfigWrite)?;
    let adapter = TargetAdapterRegistry::new()
        .by_target_key(&target.key)
        .ok_or_else(|| adapter_unavailable(&target.key))?;
    validate_adapter_target(&target, adapter.as_ref())?;
    if adapter.platform() != platform {
        return Err(adapter_target_mismatch(&target.key));
    }
    Ok((target, adapter))
}

fn validate_snapshot_path(
    snapshot: &ConfigSnapshotRecord,
    adapter: &dyn TargetAdapter,
    home: &Path,
) -> Result<(), AppError> {
    let recorded = normalized_absolute_path(Path::new(&snapshot.path))?;
    let resolved = normalized_absolute_path(&adapter.resolve_path(home))?;
    if recorded != resolved {
        return Err(AppError::Validation {
            code: "config.path_unsafe",
            message: "Snapshot path does not match the registered target adapter".to_string(),
            details: Some(snapshot.id.clone()),
            recoverable: false,
        });
    }
    Ok(())
}

fn snapshot_platform(
    snapshot: &ConfigSnapshotRecord,
    target: &TargetApp,
) -> Result<PlatformId, AppError> {
    let value = snapshot
        .platform
        .as_deref()
        .or(target.platform.as_deref())
        .ok_or_else(|| adapter_target_mismatch(&target.key))?;
    PlatformId::parse(value)
}

async fn load_source_backup(
    paths: &AppPaths,
    source: &ConfigSnapshotRecord,
) -> Result<Vec<u8>, AppError> {
    let path = source
        .backup_path
        .as_deref()
        .ok_or_else(|| snapshot_invalid(&source.id, "source snapshot has no backup path"))?;
    let path = PathBuf::from(path);
    let recorded = normalized_absolute_path(&path)?;
    let expected = normalized_absolute_path(&backup_path(paths, &source.id))?;
    if recorded != expected {
        return Err(snapshot_invalid(
            &source.id,
            "source backup path is outside the private snapshot location",
        ));
    }
    let state = ConfigWriter::inspect(&path).await?;
    if !state.existed || state.hash != source.before_hash {
        return Err(snapshot_invalid(
            &source.id,
            "source backup does not match the recorded original hash",
        ));
    }
    state
        .bytes
        .ok_or_else(|| snapshot_invalid(&source.id, "source backup has no bytes"))
}

async fn cleanup_backup(path: Option<&Path>, hash: Option<&str>) -> Result<(), AppError> {
    if let (Some(path), Some(hash)) = (path, hash) {
        ConfigWriter::remove_if_hash_matches(path, hash).await?;
    }
    Ok(())
}

async fn record_target_state(
    pool: &SqlitePool,
    target_app_id: &str,
    status: &str,
    error_code: Option<&str>,
) -> Result<(), AppError> {
    TargetStateRepository::record(
        pool,
        TargetAppStateUpdate {
            target_app_id: target_app_id.to_string(),
            active_item_type: Some("route_proxy".to_string()),
            active_item_id: None,
            last_write_status: Some(status.to_string()),
            last_error_code: error_code.map(str::to_string),
            last_written_at: Some(Utc::now().to_rfc3339()),
        },
    )
    .await?;
    Ok(())
}

fn matches_after(snapshot: &ConfigSnapshotRecord, current: &FileState) -> bool {
    match snapshot.after_hash.as_deref() {
        Some(hash) => current.existed && current.hash.as_deref() == Some(hash),
        None => !current.existed,
    }
}

fn matches_before(snapshot: &ConfigSnapshotRecord, current: &FileState) -> bool {
    if snapshot.original_file_existed != 0 {
        current.existed && current.hash == snapshot.before_hash
    } else {
        !current.existed
    }
}

fn backup_path(paths: &AppPaths, snapshot_id: &str) -> PathBuf {
    paths
        .config_snapshots_dir
        .join(format!("{snapshot_id}.backup"))
}

fn safe_metadata_json(adapter_key: &str, operation: &str) -> String {
    serde_json::json!({
        "adapter_key": adapter_key,
        "operation": operation,
    })
    .to_string()
}

fn normalized_absolute_path(path: &Path) -> Result<PathBuf, AppError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| AppError::Filesystem {
                code: "filesystem.current_dir",
                message: "Could not resolve the current directory".to_string(),
                details: Some(error.to_string()),
                recoverable: false,
            })?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    #[cfg(windows)]
    {
        Ok(PathBuf::from(normalized.to_string_lossy().to_lowercase()))
    }

    #[cfg(not(windows))]
    {
        Ok(normalized)
    }
}

fn resolve_home_dir() -> Result<PathBuf, AppError> {
    BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or_else(|| AppError::Filesystem {
            code: "filesystem.home_not_found",
            message: "Could not resolve the current user home directory".to_string(),
            details: None,
            recoverable: false,
        })
}

fn app_error_code(error: &AppError) -> &'static str {
    match error {
        AppError::Validation { code, .. }
        | AppError::Filesystem { code, .. }
        | AppError::Database { code, .. }
        | AppError::Secret { code, .. } => code,
    }
}

fn invalid_file_state(path: &Path, reason: &str) -> AppError {
    AppError::Filesystem {
        code: "config.inspect_invalid_state",
        message: "Configuration inspection returned an invalid state".to_string(),
        details: Some(format!("{}: {reason}", path.display())),
        recoverable: false,
    }
}

fn adapter_target_mismatch(target_key: &str) -> AppError {
    AppError::Validation {
        code: "config.adapter_target_mismatch",
        message: "Configuration adapter does not match its target app".to_string(),
        details: Some(target_key.to_string()),
        recoverable: false,
    }
}

fn adapter_unavailable(target_key: &str) -> AppError {
    AppError::Validation {
        code: "config.adapter_unavailable",
        message: "No verified native configuration adapter is available".to_string(),
        details: Some(target_key.to_string()),
        recoverable: true,
    }
}

fn rollback_unavailable(snapshot_id: &str, reason: &str) -> AppError {
    AppError::Validation {
        code: "config.rollback_unavailable",
        message: "Configuration snapshot cannot be rolled back".to_string(),
        details: Some(format!("{snapshot_id}: {reason}")),
        recoverable: true,
    }
}

fn rollback_conflict(path: &Path) -> AppError {
    AppError::Validation {
        code: "config.rollback_conflict",
        message: "Configuration changed after the selected snapshot".to_string(),
        details: Some(path.display().to_string()),
        recoverable: true,
    }
}

fn snapshot_invalid(snapshot_id: &str, reason: &str) -> AppError {
    AppError::Filesystem {
        code: "config.snapshot_failed",
        message: "Configuration snapshot backup is invalid".to_string(),
        details: Some(format!("{snapshot_id}: {reason}")),
        recoverable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::route_config::{RouteConfigInput, TargetAdapterRegistry};
    use crate::config_writer::hash_bytes;
    use crate::database::repositories::config_snapshot_repository::ConfigSnapshotRepository;
    use crate::database::repositories::target_repository::TargetRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::config_snapshot::NewConfigSnapshot;
    use crate::models::platform::PlatformId;
    use crate::paths::AppPaths;
    use sqlx::SqlitePool;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tempfile::tempdir;

    const BASE_URL: &str = "http://127.0.0.1:43111";
    const ROUTE_PROXY_KEY: &str = "sk-ai-switch-test";

    struct Fixture {
        _temp: tempfile::TempDir,
        paths: AppPaths,
        pool: SqlitePool,
        runtime: ConfigWriteRuntimeState,
        home: PathBuf,
    }

    impl Fixture {
        async fn new() -> Self {
            let temp = tempdir().expect("temp dir");
            let paths = AppPaths::from_data_dir(temp.path().join("app-data"));
            paths.ensure().await.expect("paths");
            let pool = create_memory_pool().await.expect("pool");
            run_migrations(&pool).await.expect("migrations");
            TargetRepository::ensure_defaults(&pool)
                .await
                .expect("targets");
            let home = temp.path().join("home");
            tokio::fs::create_dir_all(&home).await.expect("home");

            Self {
                _temp: temp,
                paths,
                pool,
                runtime: ConfigWriteRuntimeState::default(),
                home,
            }
        }

        fn codex_request(&self) -> ConfigWriteRequest {
            ConfigWriteRequest {
                adapter: TargetAdapterRegistry::new()
                    .for_platform(PlatformId::Codex)
                    .expect("Codex adapter"),
                home: self.home.clone(),
                input: RouteConfigInput {
                    base_url: BASE_URL.to_string(),
                    route_proxy_key: ROUTE_PROXY_KEY.to_string(),
                    claude_env: ClaudeEnvPlan::default(),
                },
            }
        }

        fn codex_path(&self) -> PathBuf {
            self.home.join(".codex").join("config.toml")
        }

        fn conflicting_claude_request(&self) -> ConfigWriteRequest {
            let adapter = TargetAdapterRegistry::new()
                .for_platform(PlatformId::Claude)
                .expect("Claude adapter");
            ConfigWriteRequest {
                adapter: Arc::new(ConflictOnCommitAdapter { inner: adapter }),
                home: self.home.clone(),
                input: RouteConfigInput {
                    base_url: BASE_URL.to_string(),
                    route_proxy_key: ROUTE_PROXY_KEY.to_string(),
                    claude_env: ClaudeEnvPlan::default(),
                },
            }
        }

        fn claude_path(&self) -> PathBuf {
            self.home.join(".claude").join("settings.json")
        }
    }

    struct ConflictOnCommitAdapter {
        inner: Arc<dyn TargetAdapter>,
    }

    impl TargetAdapter for ConflictOnCommitAdapter {
        fn target_key(&self) -> &'static str {
            self.inner.target_key()
        }

        fn platform(&self) -> PlatformId {
            self.inner.platform()
        }

        fn resolve_path(&self, home: &Path) -> PathBuf {
            self.inner.resolve_path(home)
        }

        fn render(
            &self,
            path: &Path,
            existing: Option<&[u8]>,
            input: &RouteConfigInput,
        ) -> Result<Vec<u8>, AppError> {
            let replacement = self.inner.render(path, existing, input)?;
            std::fs::write(path, b"external-before-commit").expect("mutate before commit");
            Ok(replacement)
        }

        fn inspect(
            &self,
            path: &Path,
            existing: Option<&[u8]>,
        ) -> crate::adapters::route_config::TargetInspection {
            self.inner.inspect(path, existing)
        }
    }

    #[tokio::test]
    async fn existing_file_write_creates_exact_backup_and_succeeds() {
        let fixture = Fixture::new().await;
        let path = fixture.codex_path();
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let original = br#"approval_policy = "never"
"#;
        tokio::fs::write(&path, original).await.unwrap();

        let outcome = ConfigWriteCoordinator::write_one(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            fixture.codex_request(),
        )
        .await
        .expect("write");

        assert_eq!(outcome.status, "succeeded");
        assert_eq!(outcome.target_key, "codex");
        let snapshot = ConfigSnapshotRepository::get(
            &fixture.pool,
            outcome.snapshot_id.as_deref().expect("snapshot id"),
        )
        .await
        .expect("snapshot");
        assert_eq!(snapshot.status, "succeeded");
        assert_eq!(snapshot.original_file_existed, 1);
        let backup_path = PathBuf::from(snapshot.backup_path.expect("backup path"));
        assert_eq!(tokio::fs::read(backup_path).await.unwrap(), original);
        let written = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(written.contains("model_provider = \"ai-switch\""));
        assert!(written.contains("approval_policy = \"never\""));
        let serialized = serde_json::to_value(outcome).unwrap();
        assert!(serialized.get("route_proxy_key").is_none());
    }

    #[tokio::test]
    async fn new_file_write_records_no_backup() {
        let fixture = Fixture::new().await;

        let outcome = ConfigWriteCoordinator::write_one(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            fixture.codex_request(),
        )
        .await
        .expect("write");
        let snapshot = ConfigSnapshotRepository::get(
            &fixture.pool,
            outcome.snapshot_id.as_deref().expect("snapshot id"),
        )
        .await
        .expect("snapshot");

        assert_eq!(snapshot.original_file_existed, 0);
        assert_eq!(snapshot.backup_path, None);
        assert!(fixture.codex_path().is_file());
    }

    #[tokio::test]
    async fn snapshot_insert_failure_leaves_target_unchanged_and_cleans_backup() {
        let fixture = Fixture::new().await;
        let path = fixture.codex_path();
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let original = br#"approval_policy = "never"
"#;
        tokio::fs::write(&path, original).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_config_snapshot_insert
             BEFORE INSERT ON config_snapshots
             BEGIN SELECT RAISE(FAIL, 'snapshot insert blocked'); END",
        )
        .execute(&fixture.pool)
        .await
        .unwrap();

        ConfigWriteCoordinator::write_one(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            fixture.codex_request(),
        )
        .await
        .expect_err("snapshot insert must fail");

        assert_eq!(tokio::fs::read(&path).await.unwrap(), original);
        assert_directory_empty(&fixture.paths.config_snapshots_dir).await;
    }

    #[tokio::test]
    async fn external_change_before_commit_marks_snapshot_conflict() {
        let fixture = Fixture::new().await;
        let path = fixture.codex_path();
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, br#"approval_policy = "never""#)
            .await
            .unwrap();

        let prepared = ConfigWriteCoordinator::prepare_one(
            &fixture.paths,
            &fixture.pool,
            fixture.codex_request(),
            "operation-conflict".to_string(),
        )
        .await
        .expect("prepare");
        let snapshot_id = prepared.snapshot_id.clone();
        tokio::fs::write(&path, b"external").await.unwrap();

        let error = ConfigWriteCoordinator::commit_prepared(&fixture.pool, prepared)
            .await
            .expect_err("commit must conflict");

        assert!(matches!(
            error,
            crate::error::AppError::Validation {
                code: "config.concurrent_modification",
                ..
            }
        ));
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"external");
        assert_eq!(
            ConfigSnapshotRepository::get(&fixture.pool, &snapshot_id)
                .await
                .unwrap()
                .status,
            "conflict"
        );
    }

    #[tokio::test]
    async fn write_group_restores_committed_targets_when_a_later_target_fails() {
        let fixture = Fixture::new().await;
        let codex_path = fixture.codex_path();
        let claude_path = fixture.claude_path();
        tokio::fs::create_dir_all(codex_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::create_dir_all(claude_path.parent().unwrap())
            .await
            .unwrap();
        let codex_original = b"approval_policy = \"never\"\n";
        tokio::fs::write(&codex_path, codex_original).await.unwrap();
        tokio::fs::write(&claude_path, br#"{"env":{"EXISTING_FLAG":"1"}}"#)
            .await
            .unwrap();

        let error = ConfigWriteCoordinator::write_group(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            vec![
                fixture.codex_request(),
                fixture.conflicting_claude_request(),
            ],
        )
        .await
        .expect_err("second target must fail");

        assert!(matches!(
            error,
            AppError::Filesystem {
                code: "filesystem.route_config_write",
                ..
            }
        ));
        assert_eq!(tokio::fs::read(&codex_path).await.unwrap(), codex_original);
        assert_eq!(
            tokio::fs::read(&claude_path).await.unwrap(),
            b"external-before-commit"
        );

        let snapshots = ConfigSnapshotRepository::list(&fixture.pool, None, 20)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 2);
        let codex = snapshots
            .iter()
            .find(|snapshot| snapshot.platform.as_deref() == Some("codex"))
            .unwrap();
        let claude = snapshots
            .iter()
            .find(|snapshot| snapshot.platform.as_deref() == Some("claude"))
            .unwrap();
        assert_eq!(codex.status, "failed");
        assert_eq!(
            codex.error_code.as_deref(),
            Some("config.group_rolled_back")
        );
        assert_eq!(claude.status, "conflict");
        assert_eq!(
            codex.operation_group_id.as_deref(),
            claude.operation_group_id.as_deref()
        );
    }

    #[tokio::test]
    async fn write_group_reports_conflict_when_a_committed_target_changes_before_rollback() {
        let fixture = Fixture::new().await;
        let codex_path = fixture.codex_path();
        let claude_path = fixture.claude_path();
        tokio::fs::create_dir_all(codex_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::create_dir_all(claude_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&codex_path, b"approval_policy = \"never\"\n")
            .await
            .unwrap();
        tokio::fs::write(&claude_path, br#"{"env":{"EXISTING_FLAG":"1"}}"#)
            .await
            .unwrap();

        ConfigWriteCoordinator::write_group_with_after_commit(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            vec![
                fixture.codex_request(),
                fixture.conflicting_claude_request(),
            ],
            |prepared| {
                if prepared.target_key == "codex" {
                    std::fs::write(&prepared.path, b"external-after-commit")
                        .expect("external edit");
                }
            },
        )
        .await
        .expect_err("rollback must conflict");

        assert_eq!(
            tokio::fs::read(&codex_path).await.unwrap(),
            b"external-after-commit"
        );
        let snapshots = ConfigSnapshotRepository::list(&fixture.pool, None, 20)
            .await
            .unwrap();
        let codex = snapshots
            .iter()
            .find(|snapshot| snapshot.platform.as_deref() == Some("codex"))
            .unwrap();
        assert_eq!(codex.status, "conflict");
        assert_eq!(
            codex.error_code.as_deref(),
            Some("config.rollback_conflict")
        );
    }

    #[tokio::test]
    async fn rollback_existing_file_restores_original_and_records_snapshot() {
        let fixture = Fixture::new().await;
        let path = fixture.codex_path();
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let original = br#"approval_policy = "never"
"#;
        tokio::fs::write(&path, original).await.unwrap();
        let written = ConfigWriteCoordinator::write_one(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            fixture.codex_request(),
        )
        .await
        .expect("write");
        let source_snapshot_id = written.snapshot_id.clone().expect("source snapshot");

        let rolled_back = ConfigWriteCoordinator::rollback_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            &fixture.home,
            &source_snapshot_id,
        )
        .await
        .expect("rollback");

        assert_eq!(tokio::fs::read(&path).await.unwrap(), original);
        assert_eq!(rolled_back.status, "succeeded");
        let rollback_snapshot = ConfigSnapshotRepository::get(
            &fixture.pool,
            rolled_back
                .snapshot_id
                .as_deref()
                .expect("rollback snapshot"),
        )
        .await
        .unwrap();
        assert_eq!(rollback_snapshot.operation, "rollback");
        assert_eq!(
            rollback_snapshot.source_snapshot_id.as_deref(),
            Some(source_snapshot_id.as_str())
        );
        assert_eq!(rollback_snapshot.status, "succeeded");
        assert_eq!(
            ConfigSnapshotRepository::get(&fixture.pool, &source_snapshot_id)
                .await
                .unwrap()
                .status,
            "succeeded"
        );
    }

    #[tokio::test]
    async fn rollback_new_file_deletes_only_matching_written_file() {
        let fixture = Fixture::new().await;
        let written = ConfigWriteCoordinator::write_one(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            fixture.codex_request(),
        )
        .await
        .expect("write");

        ConfigWriteCoordinator::rollback_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            &fixture.home,
            written.snapshot_id.as_deref().expect("snapshot"),
        )
        .await
        .expect("rollback");

        assert!(!tokio::fs::try_exists(fixture.codex_path()).await.unwrap());
    }

    #[tokio::test]
    async fn rollback_changed_file_reports_conflict_without_touching_it() {
        let fixture = Fixture::new().await;
        let written = ConfigWriteCoordinator::write_one(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            fixture.codex_request(),
        )
        .await
        .expect("write");
        tokio::fs::write(fixture.codex_path(), b"external")
            .await
            .unwrap();

        let error = ConfigWriteCoordinator::rollback_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            &fixture.home,
            written.snapshot_id.as_deref().expect("snapshot"),
        )
        .await
        .expect_err("rollback must conflict");

        assert!(matches!(
            error,
            crate::error::AppError::Validation {
                code: "config.rollback_conflict",
                ..
            }
        ));
        assert_eq!(
            tokio::fs::read(fixture.codex_path()).await.unwrap(),
            b"external"
        );
        assert_eq!(
            ConfigSnapshotRepository::list(&fixture.pool, None, 20)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn rollback_rejects_a_tampered_path_before_touching_hermes() {
        let fixture = Fixture::new().await;
        let written = ConfigWriteCoordinator::write_one(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            fixture.codex_request(),
        )
        .await
        .expect("write");
        let source_snapshot_id = written.snapshot_id.expect("snapshot");
        let hermes_path = fixture.home.join(".hermes").join("config.yaml");
        tokio::fs::create_dir_all(hermes_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&hermes_path, b"model: sentinel\n")
            .await
            .unwrap();
        sqlx::query("UPDATE config_snapshots SET path = ? WHERE id = ?")
            .bind(hermes_path.display().to_string())
            .bind(&source_snapshot_id)
            .execute(&fixture.pool)
            .await
            .unwrap();

        let error = ConfigWriteCoordinator::rollback_for_home(
            &fixture.paths,
            &fixture.pool,
            &fixture.runtime,
            &fixture.home,
            &source_snapshot_id,
        )
        .await
        .expect_err("tampered path must fail");

        assert!(matches!(
            error,
            crate::error::AppError::Validation {
                code: "config.path_unsafe",
                ..
            }
        ));
        assert_eq!(
            tokio::fs::read(&hermes_path).await.unwrap(),
            b"model: sentinel\n"
        );
        assert!(fixture.codex_path().is_file());
    }

    #[tokio::test]
    async fn normalized_paths_share_one_runtime_lock() {
        let fixture = Fixture::new().await;
        let direct = fixture.codex_path();
        let equivalent = fixture
            .home
            .join(".codex")
            .join("..")
            .join(".codex")
            .join("config.toml");

        let first = fixture.runtime.lock_for_path(&direct).await.unwrap();
        let second = fixture.runtime.lock_for_path(&equivalent).await.unwrap();

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[tokio::test]
    async fn reconcile_prepared_rows_is_read_only_and_rejects_hermes_before_path_access() {
        let fixture = Fixture::new().await;
        let codex = TargetRepository::get_by_key(&fixture.pool, "codex")
            .await
            .unwrap();
        let hermes = TargetRepository::get_by_key(&fixture.pool, "hermes")
            .await
            .unwrap();
        let path = fixture.codex_path();
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let current = b"current";
        tokio::fs::write(&path, current).await.unwrap();
        let current_hash = hash_bytes(current);

        for (id, before_hash, after_hash) in [
            (
                "reconcile-succeeded",
                hash_bytes(b"before"),
                current_hash.clone(),
            ),
            (
                "reconcile-failed",
                current_hash.clone(),
                hash_bytes(b"after"),
            ),
            (
                "reconcile-conflict",
                hash_bytes(b"different-before"),
                hash_bytes(b"different-after"),
            ),
        ] {
            ConfigSnapshotRepository::prepare_with_id(
                &fixture.pool,
                id,
                NewConfigSnapshot {
                    target_app_id: Some(codex.id.clone()),
                    platform: Some("codex".to_string()),
                    operation: "write".to_string(),
                    operation_group_id: Some(id.to_string()),
                    source_snapshot_id: None,
                    path: path.display().to_string(),
                    before_hash: Some(before_hash),
                    after_hash: Some(after_hash),
                    backup_path: None,
                    original_file_existed: true,
                    metadata_json: r#"{"adapter_key":"codex","operation":"write"}"#.to_string(),
                },
            )
            .await
            .unwrap();
        }

        let hermes_path = fixture.home.join(".hermes").join("config.yaml");
        tokio::fs::create_dir_all(hermes_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&hermes_path, b"model: sentinel\n")
            .await
            .unwrap();
        ConfigSnapshotRepository::prepare_with_id(
            &fixture.pool,
            "reconcile-hermes",
            NewConfigSnapshot {
                target_app_id: Some(hermes.id),
                platform: Some("hermes".to_string()),
                operation: "write".to_string(),
                operation_group_id: Some("reconcile-hermes".to_string()),
                source_snapshot_id: None,
                path: hermes_path.display().to_string(),
                before_hash: Some(hash_bytes(b"before")),
                after_hash: Some(hash_bytes(b"after")),
                backup_path: None,
                original_file_existed: true,
                metadata_json: r#"{"adapter_key":"hermes","operation":"write"}"#.to_string(),
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE config_snapshots SET created_at = '2000-01-01T00:00:00Z'")
            .execute(&fixture.pool)
            .await
            .unwrap();

        ConfigWriteCoordinator::reconcile_prepared_for_home(
            &fixture.pool,
            &fixture.runtime,
            &fixture.home,
        )
        .await
        .unwrap();

        assert_eq!(
            ConfigSnapshotRepository::get(&fixture.pool, "reconcile-succeeded")
                .await
                .unwrap()
                .status,
            "succeeded"
        );
        assert_eq!(
            ConfigSnapshotRepository::get(&fixture.pool, "reconcile-failed")
                .await
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            ConfigSnapshotRepository::get(&fixture.pool, "reconcile-conflict")
                .await
                .unwrap()
                .status,
            "conflict"
        );
        let hermes_snapshot = ConfigSnapshotRepository::get(&fixture.pool, "reconcile-hermes")
            .await
            .unwrap();
        assert_eq!(hermes_snapshot.status, "conflict");
        assert_eq!(
            hermes_snapshot.error_code.as_deref(),
            Some("capability.unavailable")
        );
        assert_eq!(tokio::fs::read(&path).await.unwrap(), current);
        assert_eq!(
            tokio::fs::read(&hermes_path).await.unwrap(),
            b"model: sentinel\n"
        );
    }

    async fn assert_directory_empty(path: &Path) {
        let mut entries = tokio::fs::read_dir(path).await.unwrap();
        assert!(entries.next_entry().await.unwrap().is_none());
    }
}
