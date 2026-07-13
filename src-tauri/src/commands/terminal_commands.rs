use crate::app_state::AppState;
use crate::terminal_manager::{CreateTerminalSessionInput, TerminalSession};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn create_terminal_session(
    app: AppHandle,
    state: State<'_, AppState>,
    input: CreateTerminalSessionInput,
) -> Result<TerminalSession, String> {
    state.terminals.create_session(app, input)
}

#[tauri::command]
pub async fn write_terminal_input(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    state.terminals.write_input(&session_id, &data)
}

#[tauri::command]
pub async fn resize_terminal(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.terminals.resize(&session_id, cols, rows)
}

#[tauri::command]
pub async fn kill_terminal_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state.terminals.kill(&session_id)
}

#[tauri::command]
pub async fn list_terminal_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<TerminalSession>, String> {
    Ok(state.terminals.list_sessions())
}
