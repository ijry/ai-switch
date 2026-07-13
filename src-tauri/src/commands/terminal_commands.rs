use crate::app_state::AppState;
use crate::core::terminals::{
    create_terminal_session_core, kill_terminal_session_core, list_terminal_sessions_core,
    resize_terminal_core, write_terminal_input_core,
};
use crate::terminal_manager::{CreateTerminalSessionInput, TerminalSession};
use crate::web::event_bridge::EventEmitter;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn create_terminal_session(
    app: AppHandle,
    state: State<'_, AppState>,
    input: CreateTerminalSessionInput,
) -> Result<TerminalSession, String> {
    create_terminal_session_core(&state.terminals, EventEmitter::Tauri(app), input)
}

#[tauri::command]
pub async fn write_terminal_input(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    write_terminal_input_core(&state.terminals, &session_id, &data)
}

#[tauri::command]
pub async fn resize_terminal(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    resize_terminal_core(&state.terminals, &session_id, cols, rows)
}

#[tauri::command]
pub async fn kill_terminal_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    kill_terminal_session_core(&state.terminals, &session_id)
}

#[tauri::command]
pub async fn list_terminal_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<TerminalSession>, String> {
    Ok(list_terminal_sessions_core(&state.terminals))
}
