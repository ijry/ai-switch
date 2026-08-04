#![allow(dead_code)]

mod platform;

use crate::error::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::Permissions,
    io::ErrorKind,
    path::{Path, PathBuf},
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteOutcome {
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct FileState {
    pub existed: bool,
    pub bytes: Option<Vec<u8>>,
    pub hash: Option<String>,
    pub permissions: Option<Permissions>,
}

pub struct ConfigWriter;

impl ConfigWriter {
    pub async fn inspect(path: &Path) -> Result<FileState, AppError> {
        inspect_parent(path).await?;
        let metadata = match tokio::fs::symlink_metadata(path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(FileState {
                    existed: false,
                    bytes: None,
                    hash: None,
                    permissions: None,
                });
            }
            Err(error) => return Err(inspect_failed(path, error)),
        };
        validate_target_metadata(path, &metadata)?;

        let bytes = tokio::fs::read(path)
            .await
            .map_err(|error| inspect_failed(path, error))?;
        let confirmed = tokio::fs::symlink_metadata(path)
            .await
            .map_err(|error| inspect_failed(path, error))?;
        validate_target_metadata(path, &confirmed)?;
        inspect_parent(path).await?;

        Ok(FileState {
            existed: true,
            hash: Some(hash_bytes(&bytes)),
            bytes: Some(bytes),
            permissions: Some(confirmed.permissions()),
        })
    }

    pub async fn write_atomic(path: &Path, content: &str) -> Result<WriteOutcome, AppError> {
        let expected = Self::inspect(path).await?;
        Self::write_atomic_if_unchanged(path, content.as_bytes(), &expected).await
    }

    pub async fn write_atomic_if_unchanged(
        path: &Path,
        bytes: &[u8],
        expected: &FileState,
    ) -> Result<WriteOutcome, AppError> {
        let parent = prepare_parent(path).await?;
        let temp_path = temporary_path(&parent, path);
        let result = async {
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
                .await
                .map_err(|error| atomic_replace_failed(path, error))?;
            file.write_all(bytes)
                .await
                .map_err(|error| atomic_replace_failed(path, error))?;
            file.flush()
                .await
                .map_err(|error| atomic_replace_failed(path, error))?;
            file.sync_all()
                .await
                .map_err(|error| atomic_replace_failed(path, error))?;

            platform::apply_config_permissions(&temp_path, expected.permissions.as_ref())
                .await
                .map_err(|error| atomic_replace_failed(path, error))?;
            file.sync_all()
                .await
                .map_err(|error| atomic_replace_failed(path, error))?;
            drop(file);

            let current = Self::inspect(path).await?;
            if !same_expected_state(expected, &current) {
                return Err(concurrent_modification(path));
            }

            platform::replace_temp_file(&temp_path, path, expected.existed)
                .await
                .map_err(|error| atomic_replace_failed(path, error))?;

            let actual = Self::inspect(path).await?;
            let expected_hash = hash_bytes(bytes);
            if !actual.existed || actual.hash.as_deref() != Some(expected_hash.as_str()) {
                return Err(verify_failed(path));
            }

            Ok(WriteOutcome {
                path: path.display().to_string(),
                before_hash: expected.hash.clone(),
                after_hash: actual.hash,
                status: "written".to_string(),
            })
        }
        .await;

        if result.is_err() {
            let _ = tokio::fs::remove_file(&temp_path).await;
        }
        result
    }

    pub async fn remove_if_hash_matches(path: &Path, expected_hash: &str) -> Result<(), AppError> {
        let current = Self::inspect(path).await?;
        if !current.existed || current.hash.as_deref() != Some(expected_hash) {
            return Err(rollback_conflict(path));
        }

        tokio::fs::remove_file(path)
            .await
            .map_err(|error| rollback_failed(path, error))?;
        let parent = target_parent(path)?;
        platform::sync_parent(&parent)
            .await
            .map_err(|error| rollback_failed(path, error))?;
        Ok(())
    }

    pub async fn write_private_backup(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
        let parent = prepare_backup_parent(path).await?;
        let state = Self::inspect(path).await?;
        if state.existed {
            return Err(snapshot_failed(
                path,
                "backup path already exists".to_string(),
            ));
        }

        let mut created = false;
        let result = async {
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .await
                .map_err(|error| snapshot_failed(path, error.to_string()))?;
            created = true;
            platform::apply_private_permissions(path)
                .await
                .map_err(|error| snapshot_failed(path, error.to_string()))?;
            file.write_all(bytes)
                .await
                .map_err(|error| snapshot_failed(path, error.to_string()))?;
            file.flush()
                .await
                .map_err(|error| snapshot_failed(path, error.to_string()))?;
            file.sync_all()
                .await
                .map_err(|error| snapshot_failed(path, error.to_string()))?;
            drop(file);
            platform::sync_parent(&parent)
                .await
                .map_err(|error| snapshot_failed(path, error.to_string()))?;
            Ok(())
        }
        .await;

        if result.is_err() && created {
            let _ = tokio::fs::remove_file(path).await;
        }
        result
    }
}

async fn inspect_parent(path: &Path) -> Result<bool, AppError> {
    let parent = target_parent(path)?;
    match tokio::fs::symlink_metadata(&parent).await {
        Ok(metadata) => {
            if platform::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
                return Err(path_unsafe(
                    &parent,
                    "target parent is linked, reparsed, or not a directory",
                ));
            }
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(inspect_failed(&parent, error)),
    }
}

async fn prepare_parent(path: &Path) -> Result<PathBuf, AppError> {
    let parent = target_parent(path)?;
    if !inspect_parent(path).await? {
        tokio::fs::create_dir_all(&parent)
            .await
            .map_err(|error| atomic_replace_failed(path, error))?;
    }
    inspect_parent(path).await?;
    Ok(parent)
}

async fn prepare_backup_parent(path: &Path) -> Result<PathBuf, AppError> {
    let parent = target_parent(path)?;
    if !inspect_parent(path).await? {
        tokio::fs::create_dir_all(&parent)
            .await
            .map_err(|error| snapshot_failed(path, error.to_string()))?;
    }
    inspect_parent(path).await?;
    Ok(parent)
}

fn validate_target_metadata(path: &Path, metadata: &std::fs::Metadata) -> Result<(), AppError> {
    if platform::metadata_is_link_or_reparse_point(metadata) || !metadata.is_file() {
        return Err(path_unsafe(
            path,
            "target is linked, reparsed, or not a regular file",
        ));
    }
    Ok(())
}

fn target_parent(path: &Path) -> Result<PathBuf, AppError> {
    let parent = path.parent().ok_or_else(|| AppError::Filesystem {
        code: "filesystem.path_parent_missing",
        message: "Target path has no parent directory".to_string(),
        details: Some(path.display().to_string()),
        recoverable: false,
    })?;
    if parent.as_os_str().is_empty() {
        Ok(PathBuf::from("."))
    } else {
        Ok(parent.to_path_buf())
    }
}

fn temporary_path(parent: &Path, target: &Path) -> PathBuf {
    let file_name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("config");
    parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()))
}

fn same_expected_state(expected: &FileState, current: &FileState) -> bool {
    expected.existed == current.existed && expected.hash == current.hash
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn inspect_failed(path: &Path, error: std::io::Error) -> AppError {
    AppError::Filesystem {
        code: "filesystem.config_inspect",
        message: "Could not inspect configuration file".to_string(),
        details: Some(format!("{}: {error}", path.display())),
        recoverable: true,
    }
}

fn path_unsafe(path: &Path, reason: &str) -> AppError {
    AppError::Validation {
        code: "config.path_unsafe",
        message: "Configuration path is unsafe".to_string(),
        details: Some(format!("{}: {reason}", path.display())),
        recoverable: false,
    }
}

fn concurrent_modification(path: &Path) -> AppError {
    AppError::Validation {
        code: "config.concurrent_modification",
        message: "Configuration changed before it could be written".to_string(),
        details: Some(path.display().to_string()),
        recoverable: true,
    }
}

fn atomic_replace_failed(path: &Path, error: std::io::Error) -> AppError {
    AppError::Filesystem {
        code: "config.atomic_replace_failed",
        message: "Could not atomically replace configuration".to_string(),
        details: Some(format!("{}: {error}", path.display())),
        recoverable: true,
    }
}

fn verify_failed(path: &Path) -> AppError {
    AppError::Filesystem {
        code: "config.verify_failed",
        message: "Configuration write could not be verified".to_string(),
        details: Some(path.display().to_string()),
        recoverable: true,
    }
}

fn rollback_conflict(path: &Path) -> AppError {
    AppError::Validation {
        code: "config.rollback_conflict",
        message: "Configuration changed after the recorded write".to_string(),
        details: Some(path.display().to_string()),
        recoverable: true,
    }
}

fn rollback_failed(path: &Path, error: std::io::Error) -> AppError {
    AppError::Filesystem {
        code: "config.rollback_failed",
        message: "Could not remove the recorded configuration".to_string(),
        details: Some(format!("{}: {error}", path.display())),
        recoverable: true,
    }
}

fn snapshot_failed(path: &Path, reason: String) -> AppError {
    AppError::Filesystem {
        code: "config.snapshot_failed",
        message: "Could not create a private configuration backup".to_string(),
        details: Some(format!("{}: {reason}", path.display())),
        recoverable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn write_atomic_replaces_content_and_reports_hashes() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("config.json");
        tokio::fs::write(&target, "{\"old\":true}")
            .await
            .expect("seed");

        let outcome = ConfigWriter::write_atomic(&target, "{\"new\":true}")
            .await
            .expect("write");
        let written = tokio::fs::read_to_string(&target).await.expect("read");

        assert_eq!(written, "{\"new\":true}");
        assert!(outcome.before_hash.is_some());
        assert!(outcome.after_hash.is_some());
        assert_eq!(outcome.status, "written");
    }

    #[tokio::test]
    async fn write_refuses_a_changed_target() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        tokio::fs::write(&path, b"old").await.unwrap();
        let expected = ConfigWriter::inspect(&path).await.unwrap();
        tokio::fs::write(&path, b"external").await.unwrap();

        let error = ConfigWriter::write_atomic_if_unchanged(&path, b"new", &expected)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            AppError::Validation {
                code: "config.concurrent_modification",
                ..
            }
        ));
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"external");
    }

    #[tokio::test]
    async fn write_refuses_a_target_created_after_missing_inspection() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let expected = ConfigWriter::inspect(&path).await.unwrap();
        tokio::fs::write(&path, b"external").await.unwrap();

        let error = ConfigWriter::write_atomic_if_unchanged(&path, b"new", &expected)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            AppError::Validation {
                code: "config.concurrent_modification",
                ..
            }
        ));
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"external");
    }

    #[tokio::test]
    async fn failed_write_removes_its_temporary_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        tokio::fs::write(&path, b"old").await.unwrap();
        let expected = ConfigWriter::inspect(&path).await.unwrap();
        tokio::fs::write(&path, b"external").await.unwrap();

        ConfigWriter::write_atomic_if_unchanged(&path, b"new", &expected)
            .await
            .unwrap_err();

        let mut entries = tokio::fs::read_dir(dir.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.ends_with(".tmp"), "temporary file leaked: {name}");
        }
    }

    #[tokio::test]
    async fn remove_requires_the_recorded_after_hash() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("created.json");
        tokio::fs::write(&path, b"created").await.unwrap();
        let created_hash = ConfigWriter::inspect(&path).await.unwrap().hash.unwrap();
        ConfigWriter::remove_if_hash_matches(&path, &created_hash)
            .await
            .unwrap();
        assert!(!tokio::fs::try_exists(&path).await.unwrap());

        tokio::fs::write(&path, b"changed").await.unwrap();
        let error = ConfigWriter::remove_if_hash_matches(&path, &created_hash)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            AppError::Validation {
                code: "config.rollback_conflict",
                ..
            }
        ));
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"changed");
    }

    #[tokio::test]
    async fn private_backup_preserves_exact_bytes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("backup.bin");
        let bytes = b"\x00secret\r\nbytes\xff";

        ConfigWriter::write_private_backup(&path, bytes)
            .await
            .unwrap();

        assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes);
    }

    #[tokio::test]
    async fn private_backup_never_overwrites_an_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("backup.bin");
        tokio::fs::write(&path, b"existing").await.unwrap();

        let error = ConfigWriter::write_private_backup(&path, b"replacement")
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            AppError::Filesystem {
                code: "config.snapshot_failed",
                ..
            }
        ));
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"existing");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn write_preserves_existing_unix_mode_and_secures_new_files() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let existing_path = dir.path().join("existing.json");
        tokio::fs::write(&existing_path, b"old").await.unwrap();
        tokio::fs::set_permissions(&existing_path, std::fs::Permissions::from_mode(0o640))
            .await
            .unwrap();
        let expected = ConfigWriter::inspect(&existing_path).await.unwrap();
        ConfigWriter::write_atomic_if_unchanged(&existing_path, b"new", &expected)
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::metadata(&existing_path)
                .await
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );

        let new_path = dir.path().join("new.json");
        let missing = ConfigWriter::inspect(&new_path).await.unwrap();
        ConfigWriter::write_atomic_if_unchanged(&new_path, b"new", &missing)
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::metadata(&new_path)
                .await
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        let backup_path = dir.path().join("backup.bin");
        ConfigWriter::write_private_backup(&backup_path, b"backup")
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::metadata(&backup_path)
                .await
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn inspect_refuses_target_and_parent_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let real_file = dir.path().join("real.json");
        tokio::fs::write(&real_file, b"real").await.unwrap();
        let linked_file = dir.path().join("linked.json");
        symlink(&real_file, &linked_file).unwrap();
        assert_path_unsafe(ConfigWriter::inspect(&linked_file).await.unwrap_err());

        let real_parent = dir.path().join("real-parent");
        tokio::fs::create_dir(&real_parent).await.unwrap();
        let linked_parent = dir.path().join("linked-parent");
        symlink(&real_parent, &linked_parent).unwrap();
        assert_path_unsafe(
            ConfigWriter::inspect(&linked_parent.join("config.json"))
                .await
                .unwrap_err(),
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn inspect_refuses_target_and_parent_reparse_points_when_available() {
        use std::os::windows::fs::{symlink_dir, symlink_file};

        let dir = tempdir().unwrap();
        let real_file = dir.path().join("real.json");
        tokio::fs::write(&real_file, b"real").await.unwrap();
        let linked_file = dir.path().join("linked.json");
        if symlink_file(&real_file, &linked_file).is_ok() {
            assert_path_unsafe(ConfigWriter::inspect(&linked_file).await.unwrap_err());
        }

        let real_parent = dir.path().join("real-parent");
        tokio::fs::create_dir(&real_parent).await.unwrap();
        let linked_parent = dir.path().join("linked-parent");
        if symlink_dir(&real_parent, &linked_parent).is_ok() {
            assert_path_unsafe(
                ConfigWriter::inspect(&linked_parent.join("config.json"))
                    .await
                    .unwrap_err(),
            );
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_atomic_write_handles_existing_and_new_targets() {
        let dir = tempdir().unwrap();
        let existing_path = dir.path().join("existing.json");
        tokio::fs::write(&existing_path, b"old").await.unwrap();
        let existing = ConfigWriter::inspect(&existing_path).await.unwrap();
        ConfigWriter::write_atomic_if_unchanged(&existing_path, b"new", &existing)
            .await
            .unwrap();
        assert_eq!(tokio::fs::read(&existing_path).await.unwrap(), b"new");

        let new_path = dir.path().join("new.json");
        let missing = ConfigWriter::inspect(&new_path).await.unwrap();
        ConfigWriter::write_atomic_if_unchanged(&new_path, b"created", &missing)
            .await
            .unwrap();
        assert_eq!(tokio::fs::read(&new_path).await.unwrap(), b"created");
    }

    fn assert_path_unsafe(error: AppError) {
        assert!(matches!(
            error,
            AppError::Validation {
                code: "config.path_unsafe",
                ..
            }
        ));
    }
}
