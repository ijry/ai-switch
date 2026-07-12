use crate::error::AppError;
use crate::models::settings::AppSettings;
use crate::paths::AppPaths;

pub struct SettingsService;

impl SettingsService {
    pub async fn load(paths: &AppPaths) -> Result<AppSettings, AppError> {
        paths.ensure().await?;
        if !paths.settings_file.exists() {
            let settings = AppSettings::defaults_for_data_dir(paths.data_dir.display().to_string());
            Self::save(paths, &settings).await?;
            return Ok(settings);
        }

        let contents = tokio::fs::read_to_string(&paths.settings_file).await?;
        Ok(serde_json::from_str(&contents)?)
    }

    pub async fn save(paths: &AppPaths, settings: &AppSettings) -> Result<(), AppError> {
        paths.ensure().await?;
        let contents = serde_json::to_string_pretty(settings)?;
        tokio::fs::write(&paths.settings_file, contents).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use tempfile::tempdir;

    #[tokio::test]
    async fn load_creates_default_settings_when_file_is_missing() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());

        let settings = SettingsService::load(&paths).await.expect("settings");

        assert_eq!(settings.language, "zh-CN");
        assert_eq!(settings.theme, "system");
        assert!(paths.settings_file.exists());
    }

    #[tokio::test]
    async fn save_then_load_round_trips_settings() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        let settings = AppSettings {
            language: "en".to_string(),
            theme: "dark".to_string(),
            copy_import_sources: true,
            logging_enabled: true,
            secret_storage: "keyring".to_string(),
            data_dir: paths.data_dir.display().to_string(),
        };

        SettingsService::save(&paths, &settings).await.expect("save");
        let loaded = SettingsService::load(&paths).await.expect("load");

        assert_eq!(loaded.language, "en");
        assert_eq!(loaded.theme, "dark");
        assert!(loaded.copy_import_sources);
    }
}
