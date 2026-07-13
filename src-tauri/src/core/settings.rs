use crate::error::AppError;
use crate::models::settings::AppSettings;
use crate::paths::AppPaths;
use crate::services::settings_service::SettingsService;

pub async fn get_settings_core(paths: &AppPaths) -> Result<AppSettings, AppError> {
    SettingsService::load(paths).await
}

pub async fn save_settings_core(
    paths: &AppPaths,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    SettingsService::save(paths, &settings).await?;
    Ok(settings)
}
