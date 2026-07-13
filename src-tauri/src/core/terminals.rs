use crate::terminal_manager::{CreateTerminalSessionInput, TerminalManager, TerminalSession};
use crate::web::event_bridge::EventEmitter;

pub fn create_terminal_session_core(
    manager: &TerminalManager,
    emitter: EventEmitter,
    input: CreateTerminalSessionInput,
) -> Result<TerminalSession, String> {
    manager.create_session(emitter, input)
}

pub fn write_terminal_input_core(
    manager: &TerminalManager,
    session_id: &str,
    data: &str,
) -> Result<(), String> {
    manager.write_input(session_id, data)
}

pub fn resize_terminal_core(
    manager: &TerminalManager,
    session_id: &str,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    manager.resize(session_id, cols, rows)
}

pub fn kill_terminal_session_core(
    manager: &TerminalManager,
    session_id: &str,
) -> Result<(), String> {
    manager.kill(session_id)
}

pub fn list_terminal_sessions_core(manager: &TerminalManager) -> Vec<TerminalSession> {
    manager.list_sessions()
}
