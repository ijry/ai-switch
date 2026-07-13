use crate::session_manager::{self, SessionMessage, SessionMeta};

pub async fn list_sessions_core(platform: Option<String>) -> Result<Vec<SessionMeta>, String> {
    tokio::task::spawn_blocking(move || session_manager::scan_sessions(platform.as_deref()))
        .await
        .map_err(|error| format!("Failed to scan sessions: {error}"))
}

pub async fn get_session_messages_core(
    provider_id: String,
    source_path: String,
) -> Result<Vec<SessionMessage>, String> {
    tokio::task::spawn_blocking(move || {
        session_manager::load_messages(&provider_id, &source_path)
    })
    .await
    .map_err(|error| format!("Failed to load session messages: {error}"))?
}
