use crate::error::AppError;
use directories::BaseDirs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub database_file: PathBuf,
    pub settings_file: PathBuf,
    pub web_service_file: PathBuf,
    pub backups_dir: PathBuf,
    pub imports_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self, AppError> {
        let base = BaseDirs::new().ok_or_else(|| AppError::Filesystem {
            code: "filesystem.home_not_found",
            message: "Could not resolve the current user home directory".to_string(),
            details: None,
            recoverable: false,
        })?;

        Ok(Self::from_data_dir(base.home_dir().join(".ai-switch")))
    }

    pub fn from_data_dir(data_dir: PathBuf) -> Self {
        Self {
            database_file: data_dir.join("ai-switch.db"),
            settings_file: data_dir.join("settings.json"),
            web_service_file: data_dir.join("web-service.json"),
            backups_dir: data_dir.join("backups"),
            imports_dir: data_dir.join("imports"),
            logs_dir: data_dir.join("logs"),
            data_dir,
        }
    }

    pub async fn ensure(&self) -> Result<(), AppError> {
        tokio::fs::create_dir_all(&self.data_dir).await?;
        tokio::fs::create_dir_all(&self.backups_dir).await?;
        tokio::fs::create_dir_all(&self.imports_dir).await?;
        tokio::fs::create_dir_all(&self.logs_dir).await?;
        Ok(())
    }
}
