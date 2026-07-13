use crate::core::sessions::{get_session_messages_core, list_sessions_core};
use crate::session_manager;

#[tauri::command]
pub async fn list_sessions(
    platform: Option<String>,
) -> Result<Vec<session_manager::SessionMeta>, String> {
    list_sessions_core(platform).await
}

#[tauri::command]
pub async fn get_session_messages(
    provider_id: String,
    source_path: String,
) -> Result<Vec<session_manager::SessionMessage>, String> {
    get_session_messages_core(provider_id, source_path).await
}
