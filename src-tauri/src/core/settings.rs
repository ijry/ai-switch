use crate::app_state::CloseToTrayRuntime;
use crate::error::AppError;
use crate::models::settings::{AppSettings, AppSettingsView};
use crate::paths::AppPaths;
use crate::services::deeplink_protocol_service::DeepLinkProtocolRuntime;
use crate::services::settings_service::SettingsService;

pub async fn get_settings_core(
    paths: &AppPaths,
    runtime: &DeepLinkProtocolRuntime,
) -> Result<AppSettingsView, AppError> {
    Ok(runtime.view(SettingsService::load(paths).await?))
}

pub async fn save_settings_core(
    paths: &AppPaths,
    runtime: &DeepLinkProtocolRuntime,
    close_to_tray: &CloseToTrayRuntime,
    settings: AppSettings,
) -> Result<AppSettingsView, AppError> {
    let previous = SettingsService::load(paths).await?;
    let changed =
        previous.ccswitch_deeplink_compat_enabled != settings.ccswitch_deeplink_compat_enabled;
    if changed {
        runtime.set_ccswitch_enabled(settings.ccswitch_deeplink_compat_enabled)?;
    }
    if let Err(error) = SettingsService::save(paths, &settings).await {
        if changed {
            let _ = runtime.set_ccswitch_enabled(previous.ccswitch_deeplink_compat_enabled);
        }
        return Err(error);
    }
    // Only after the file lands, so a failed write leaves the close button
    // behaving the way the persisted settings still describe.
    close_to_tray.set_enabled(settings.close_to_tray);
    Ok(runtime.view(settings))
}
