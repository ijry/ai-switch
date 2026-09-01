use std::sync::Arc;

use crate::terminal_manager::{
    CreateTerminalSessionInput, TerminalLaunchKind, TerminalManager, TerminalSession,
};
use crate::web::event_bridge::EventEmitter;
use crate::web::terminal_hub::TerminalHub;

pub fn create_terminal_session_core(
    manager: &TerminalManager,
    emitter: EventEmitter,
    hub: Arc<TerminalHub>,
    input: CreateTerminalSessionInput,
) -> Result<TerminalSession, String> {
    manager.create_session(emitter, hub, input)
}

/// Resume a historical Vibe session using server-derived metadata. Callers
/// can select only the history id and viewport dimensions; command, platform,
/// and working directory are read from the local session scanner.
pub async fn resume_session_terminal_core(
    manager: &TerminalManager,
    emitter: EventEmitter,
    hub: Arc<TerminalHub>,
    session_id: &str,
    cols: u16,
    rows: u16,
) -> Result<TerminalSession, String> {
    let sessions = crate::core::sessions::list_sessions_core(None).await?;
    let meta = sessions
        .into_iter()
        .find(|item| item.session_id == session_id)
        .ok_or_else(|| format!("Unknown vibe session: {session_id}"))?;
    let command = meta
        .resume_command
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Session {session_id} has no resume command."))?;
    let cwd = meta
        .project_dir
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Session {session_id} has no project directory."))?;
    let input = CreateTerminalSessionInput {
        kind: TerminalLaunchKind::Resume,
        platform: Some(meta.provider_id),
        command: Some(command),
        title: meta.title,
        cwd,
        cols: Some(cols),
        rows: Some(rows),
        model: None,
        reasoning_effort: None,
    };
    manager.create_session(emitter, hub, input)
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
