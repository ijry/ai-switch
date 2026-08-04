use crate::models::platform::PlatformCapability;
use crate::services::platform_capability_service::PlatformCapabilityService;

#[tauri::command]
pub async fn list_platform_capabilities() -> Vec<PlatformCapability> {
    PlatformCapabilityService::list()
}
