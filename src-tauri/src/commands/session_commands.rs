use crate::session_manager;

#[tauri::command]
pub async fn list_sessions(platform: Option<String>) -> Result<Vec<session_manager::SessionMeta>, String> {
    tauri::async_runtime::spawn_blocking(move || session_manager::scan_sessions(platform.as_deref()))
        .await
        .map_err(|error| format!("Failed to scan sessions: {error}"))
}

#[tauri::command]
pub async fn get_session_messages(
    provider_id: String,
    source_path: String,
) -> Result<Vec<session_manager::SessionMessage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::load_messages(&provider_id, &source_path)
    })
    .await
    .map_err(|error| format!("Failed to load session messages: {error}"))?
}
