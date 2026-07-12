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
    pub status: String,
}

pub struct ConfigWriter;

impl ConfigWriter {
    pub async fn write_atomic(path: &Path, content: &str) -> Result<WriteOutcome, AppError> {
        let parent = path.parent().ok_or_else(|| AppError::Filesystem {
            code: "filesystem.path_parent_missing",
            message: "Target path has no parent directory".to_string(),
            details: Some(path.display().to_string()),
            recoverable: false,
        })?;
        tokio::fs::create_dir_all(parent).await?;

        let before_hash = if path.exists() {
            let before = tokio::fs::read(path).await?;
            Some(hash_bytes(&before))
        } else {
            None
        };

        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("config");
        let temp_path = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
        let mut file = tokio::fs::File::create(&temp_path).await?;
        file.write_all(content.as_bytes()).await?;
        file.flush().await?;
        file.sync_all().await?;
        drop(file);

        replace_temp_file(&temp_path, path).await?;
        let after = tokio::fs::read(path).await?;
        let after_hash = Some(hash_bytes(&after));

        Ok(WriteOutcome {
            path: path.display().to_string(),
            before_hash,
            after_hash,
            status: "written".to_string(),
        })
    }
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
        assert_eq!(outcome.status, "written");
    }
}
