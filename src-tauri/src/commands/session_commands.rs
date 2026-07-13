use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::session::{
    ListSessionEventsRequest, NewSessionEvent, NewSessionRecord, SessionEvent, SessionRecord,
    SetSessionStatusRequest,
};
use crate::services::session_service::SessionService;
use tauri::State;

#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<SessionRecord>, ApiError> {
    SessionService::list_sessions(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    request: NewSessionRecord,
) -> Result<SessionRecord, ApiError> {
    SessionService::create_session(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_session_status(
    state: State<'_, AppState>,
    request: SetSessionStatusRequest,
) -> Result<SessionRecord, ApiError> {
    SessionService::set_session_status(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_session_events(
    state: State<'_, AppState>,
    request: ListSessionEventsRequest,
) -> Result<Vec<SessionEvent>, ApiError> {
    SessionService::list_session_events(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_session_event(
    state: State<'_, AppState>,
    request: NewSessionEvent,
) -> Result<SessionEvent, ApiError> {
    SessionService::create_session_event(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
