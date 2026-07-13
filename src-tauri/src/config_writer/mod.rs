#![allow(dead_code)]

use crate::error::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteOutcome {
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub backup_path: Option<String>,
    pub status: String,
}

pub struct ConfigWriter;

impl ConfigWriter {
    pub async fn write_atomic(path: &Path, content: &str) -> Result<WriteOutcome, AppError> {
        Self::write_atomic_bytes_inner(path, content.as_bytes(), None).await
    }

    pub async fn write_atomic_bytes(path: &Path, content: &[u8]) -> Result<WriteOutcome, AppError> {
        Self::write_atomic_bytes_inner(path, content, None).await
    }

    pub async fn write_atomic_with_backup(
        path: &Path,
        content: &str,
        backup_dir: &Path,
    ) -> Result<WriteOutcome, AppError> {
        Self::write_atomic_bytes_inner(path, content.as_bytes(), Some(backup_dir)).await
    }

    pub async fn hash_existing_file(path: &Path) -> Result<Option<String>, AppError> {
        match tokio::fs::read(path).await {
            Ok(bytes) => Ok(Some(hash_bytes(&bytes))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn hash_bytes(bytes: &[u8]) -> String {
        hash_bytes(bytes)
    }

    async fn write_atomic_bytes_inner(
        path: &Path,
        content: &[u8],
        backup_dir: Option<&Path>,
    ) -> Result<WriteOutcome, AppError> {
        let parent = path.parent().ok_or_else(|| AppError::Filesystem {
            code: "filesystem.path_parent_missing",
            message: "Target path has no parent directory".to_string(),
            details: Some(path.display().to_string()),
            recoverable: false,
        })?;
        tokio::fs::create_dir_all(parent).await?;

        let before = match tokio::fs::read(path).await {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let before_hash = before.as_ref().map(|bytes| hash_bytes(bytes));

        let backup_path = if let Some(backup_dir) = backup_dir {
            tokio::fs::create_dir_all(backup_dir).await?;
            let extension = if before.is_some() { "bak" } else { "missing" };
            let backup_path = backup_dir.join(format!("{}.{}", Uuid::new_v4(), extension));
            let backup_contents = before.as_deref().unwrap_or(
                b"ai-switch rollback marker: target file did not exist before this write\n",
            );
            write_file_atomically(&backup_path, backup_contents).await?;
            Some(backup_path.display().to_string())
        } else {
            None
        };

        write_file_atomically(path, content).await?;
        let after = tokio::fs::read(path).await?;
        let after_hash = Some(hash_bytes(&after));

        Ok(WriteOutcome {
            path: path.display().to_string(),
            before_hash,
            after_hash,
            backup_path,
            status: "written".to_string(),
        })
    }
}

async fn write_file_atomically(path: &Path, content: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| AppError::Filesystem {
        code: "filesystem.path_parent_missing",
        message: "Target path has no parent directory".to_string(),
        details: Some(path.display().to_string()),
        recoverable: false,
    })?;
    tokio::fs::create_dir_all(parent).await?;

    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("config");
    let temp_path = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let mut file = tokio::fs::File::create(&temp_path).await?;
    file.write_all(content).await?;
    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    replace_temp_file(&temp_path, path).await
}

async fn replace_temp_file(temp_path: &Path, target_path: &Path) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        replace_temp_file_windows(temp_path, target_path)
    }

    #[cfg(not(windows))]
    {
        tokio::fs::rename(temp_path, target_path).await?;
        Ok(())
    }
}

#[cfg(windows)]
fn replace_temp_file_windows(temp_path: &Path, target_path: &Path) -> Result<(), AppError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from: Vec<u16> = temp_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let to: Vec<u16> = target_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let moved = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if moved == 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(())
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
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
        assert!(outcome.backup_path.is_none());
        assert_eq!(outcome.status, "written");
    }

    #[tokio::test]
    async fn write_atomic_with_backup_copies_previous_content() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("config.json");
        let backups = dir.path().join("backups");
        tokio::fs::write(&target, "{\"old\":true}")
            .await
            .expect("seed");

        let outcome = ConfigWriter::write_atomic_with_backup(&target, "{\"new\":true}", &backups)
            .await
            .expect("write");
        let backup_path = outcome.backup_path.expect("backup path");
        let backup = tokio::fs::read_to_string(backup_path)
            .await
            .expect("backup");

        assert_eq!(backup, "{\"old\":true}");
        assert!(outcome.before_hash.is_some());
        assert!(outcome.after_hash.is_some());
    }

    #[tokio::test]
    async fn write_atomic_with_backup_marks_missing_previous_file() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("config.json");
        let backups = dir.path().join("backups");

        let outcome = ConfigWriter::write_atomic_with_backup(&target, "{\"new\":true}", &backups)
            .await
            .expect("write");
        let backup_path = outcome.backup_path.expect("backup path");
        let backup = tokio::fs::read_to_string(backup_path)
            .await
            .expect("backup");

        assert!(backup.contains("target file did not exist"));
        assert!(outcome.before_hash.is_none());
        assert!(outcome.after_hash.is_some());
    }
}
