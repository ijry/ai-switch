use crate::error::AppError;
use directories::BaseDirs;
use std::path::PathBuf;

/// Database file name. Debug builds (`tauri dev` / `cargo run`) use a separate
/// `ai-switch-dev.db` so development never clobbers or quarantines the installed
/// release's `ai-switch.db` (they share the same `~/.ai-switch` data dir but
/// keep independent databases). Release builds use the production database.
pub const DATABASE_FILE_NAME: &str = if cfg!(debug_assertions) {
    "ai-switch-dev.db"
} else {
    "ai-switch.db"
};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub database_file: PathBuf,
    pub settings_file: PathBuf,
    pub web_service_file: PathBuf,
    pub route_proxy_https_config_file: PathBuf,
    pub backups_dir: PathBuf,
    pub config_snapshots_dir: PathBuf,
    pub imports_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub tailscale_dir: PathBuf,
    pub route_proxy_https_dir: PathBuf,
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
            database_file: data_dir.join(DATABASE_FILE_NAME),
            settings_file: data_dir.join("settings.json"),
            web_service_file: data_dir.join("web-service.json"),
            route_proxy_https_config_file: data_dir.join("route-proxy-https.json"),
            backups_dir: data_dir.join("backups"),
            config_snapshots_dir: data_dir.join("backups").join("config-snapshots"),
            imports_dir: data_dir.join("imports"),
            logs_dir: data_dir.join("logs"),
            tailscale_dir: data_dir.join("tailscale"),
            route_proxy_https_dir: data_dir.join("certs").join("route-proxy"),
            data_dir,
        }
    }

    pub async fn ensure(&self) -> Result<(), AppError> {
        tokio::fs::create_dir_all(&self.data_dir).await?;
        tokio::fs::create_dir_all(&self.backups_dir).await?;
        tokio::fs::create_dir_all(&self.config_snapshots_dir).await?;
        set_private_directory_permissions(&self.config_snapshots_dir).await?;
        tokio::fs::create_dir_all(&self.imports_dir).await?;
        tokio::fs::create_dir_all(&self.logs_dir).await?;
        tokio::fs::create_dir_all(&self.tailscale_dir).await?;
        Ok(())
    }
}

#[cfg(unix)]
async fn set_private_directory_permissions(path: &std::path::Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn set_private_directory_permissions(_path: &std::path::Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::AppPaths;
    use std::path::PathBuf;

    #[test]
    fn app_paths_include_tailscale_dir() {
        let paths = AppPaths::from_data_dir(PathBuf::from("C:/tmp/ai-switch-data"));
        assert_eq!(
            paths.tailscale_dir,
            PathBuf::from("C:/tmp/ai-switch-data/tailscale")
        );
    }

    #[test]
    fn debug_builds_use_a_separate_dev_database() {
        let paths = AppPaths::from_data_dir(PathBuf::from("C:/tmp/ai-switch-data"));
        assert_eq!(
            paths.database_file,
            PathBuf::from("C:/tmp/ai-switch-data").join(super::DATABASE_FILE_NAME)
        );
        // Development (debug) must never point at the release database file.
        #[cfg(debug_assertions)]
        assert_eq!(
            paths.database_file,
            PathBuf::from("C:/tmp/ai-switch-data/ai-switch-dev.db")
        );
        #[cfg(not(debug_assertions))]
        assert_eq!(
            paths.database_file,
            PathBuf::from("C:/tmp/ai-switch-data/ai-switch.db")
        );
    }

    #[test]
    fn app_paths_include_route_proxy_https_paths() {
        let paths = AppPaths::from_data_dir(PathBuf::from("C:/tmp/ai-switch-data"));

        assert_eq!(
            paths.route_proxy_https_config_file,
            PathBuf::from("C:/tmp/ai-switch-data/route-proxy-https.json")
        );
        assert_eq!(
            paths.route_proxy_https_dir,
            PathBuf::from("C:/tmp/ai-switch-data/certs/route-proxy")
        );
    }

    #[tokio::test]
    async fn ensure_creates_private_config_snapshot_directory() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().join("app-data"));

        paths.ensure().await.expect("ensure paths");

        assert!(paths.config_snapshots_dir.is_dir());
        assert_eq!(
            paths.config_snapshots_dir,
            temp.path()
                .join("app-data")
                .join("backups")
                .join("config-snapshots")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&paths.config_snapshots_dir)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }
    }
}
