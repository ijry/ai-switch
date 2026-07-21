use chrono::Utc;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::web::event_bridge::EventEmitter;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TerminalLaunchKind {
    Shell,
    Agent,
    Resume,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTerminalSessionInput {
    pub kind: TerminalLaunchKind,
    pub platform: Option<String>,
    pub command: Option<String>,
    pub title: Option<String>,
    pub cwd: String,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    pub id: String,
    pub title: String,
    pub platform: Option<String>,
    pub cwd: String,
    pub command: String,
    pub status: TerminalStatus,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TerminalStatus {
    Running,
    Exited,
    Error,
}

#[derive(Debug, Clone)]
pub struct ResolvedCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Default)]
pub struct TerminalManager {
    sessions: Arc<Mutex<HashMap<String, TerminalProcess>>>,
}

struct TerminalProcess {
    meta: TerminalSession,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalOutputEvent {
    session_id: String,
    data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalExitEvent {
    session_id: String,
    exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalErrorEvent {
    session_id: String,
    message: String,
}

pub fn validate_launch_input(input: &CreateTerminalSessionInput) -> Result<(), String> {
    let cwd = input.cwd.trim();
    if cwd.is_empty() {
        return Err("Working directory is required.".to_string());
    }
    if !Path::new(cwd).is_dir() {
        return Err(format!("Working directory does not exist: {cwd}"));
    }
    if input.kind == TerminalLaunchKind::Resume
        && input.command.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err("Resume command is required.".to_string());
    }
    Ok(())
}

pub fn resolve_launch_command(
    input: &CreateTerminalSessionInput,
) -> Result<ResolvedCommand, String> {
    match input.kind {
        TerminalLaunchKind::Shell => Ok(default_shell_command()),
        TerminalLaunchKind::Agent => {
            let platform = input.platform.as_deref().unwrap_or("").trim();
            let program = match platform {
                "codex" => "codex",
                "claude" => "claude",
                "grok" => "grok",
                "gemini" => "gemini",
                "opencode" => "opencode",
                "openclaw" => "openclaw",
                "hermes" => "hermes",
                _ => return Err(format!("Unsupported terminal platform: {platform}")),
            };
            Ok(ResolvedCommand {
                program: program.to_string(),
                args: Vec::new(),
            })
        }
        TerminalLaunchKind::Resume => {
            let command = input.command.as_deref().unwrap_or("").trim();
            if command.is_empty() {
                return Err("Resume command is required.".to_string());
            }
            Ok(shell_command(command))
        }
    }
}

impl TerminalManager {
    pub fn create_session(
        &self,
        emitter: EventEmitter,
        input: CreateTerminalSessionInput,
    ) -> Result<TerminalSession, String> {
        validate_launch_input(&input)?;
        let resolved = resolve_launch_command(&input)?;
        let pty_system = native_pty_system();
        let size = PtySize {
            rows: input.rows.unwrap_or(30),
            cols: input.cols.unwrap_or(100),
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = pty_system
            .openpty(size)
            .map_err(|error| format!("Failed to open PTY: {error}"))?;

        let mut command = CommandBuilder::new(&resolved.program);
        for arg in &resolved.args {
            command.arg(arg);
        }
        command.cwd(input.cwd.trim());

        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Failed to start terminal command: {error}"))?;
        let killer = child.clone_killer();
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Failed to read PTY output: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Failed to write PTY input: {error}"))?;

        let id = Uuid::new_v4().to_string();
        let command_label = if input.kind == TerminalLaunchKind::Resume {
            input
                .command
                .clone()
                .unwrap_or_else(|| resolved.program.clone())
        } else {
            std::iter::once(resolved.program.clone())
                .chain(resolved.args.clone())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let title = input
            .title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| command_label.clone());
        let meta = TerminalSession {
            id: id.clone(),
            title,
            platform: input.platform.clone(),
            cwd: input.cwd.trim().to_string(),
            command: command_label,
            status: TerminalStatus::Running,
            created_at: Utc::now().timestamp(),
        };

        self.sessions.lock().unwrap().insert(
            id.clone(),
            TerminalProcess {
                meta: meta.clone(),
                master: pair.master,
                writer,
                killer,
            },
        );

        let output_emitter = emitter.clone();
        let output_id = id.clone();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        let data = String::from_utf8_lossy(&buffer[..count]).to_string();
                        output_emitter.emit(
                            "terminal://output",
                            &TerminalOutputEvent {
                                session_id: output_id.clone(),
                                data,
                            },
                        );
                    }
                    Err(error) => {
                        output_emitter.emit(
                            "terminal://error",
                            &TerminalErrorEvent {
                                session_id: output_id.clone(),
                                message: format!("Failed to read terminal output: {error}"),
                            },
                        );
                        break;
                    }
                }
            }
        });

        let exit_emitter = emitter;
        let exit_id = id.clone();
        let sessions = Arc::clone(&self.sessions);
        std::thread::spawn(move || match child.wait() {
            Ok(status) => {
                if let Some(process) = sessions.lock().unwrap().get_mut(&exit_id) {
                    process.meta.status = TerminalStatus::Exited;
                }
                exit_emitter.emit(
                    "terminal://exit",
                    &TerminalExitEvent {
                        session_id: exit_id,
                        exit_code: Some(status.exit_code() as i32),
                    },
                );
            }
            Err(error) => {
                if let Some(process) = sessions.lock().unwrap().get_mut(&exit_id) {
                    process.meta.status = TerminalStatus::Error;
                }
                exit_emitter.emit(
                    "terminal://error",
                    &TerminalErrorEvent {
                        session_id: exit_id,
                        message: format!("Failed to wait for terminal exit: {error}"),
                    },
                );
            }
        });

        Ok(meta)
    }

    pub fn write_input(&self, session_id: &str, data: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        let process = sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("Unknown terminal session: {session_id}"))?;
        process
            .writer
            .write_all(data.as_bytes())
            .map_err(|error| format!("Failed to write terminal input: {error}"))?;
        process
            .writer
            .flush()
            .map_err(|error| format!("Failed to flush terminal input: {error}"))
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        if cols == 0 || rows == 0 {
            return Err("Terminal dimensions must be greater than zero.".to_string());
        }
        let sessions = self.sessions.lock().unwrap();
        let process = sessions
            .get(session_id)
            .ok_or_else(|| format!("Unknown terminal session: {session_id}"))?;
        process
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Failed to resize terminal: {error}"))
    }

    pub fn kill(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        let mut process = sessions
            .remove(session_id)
            .ok_or_else(|| format!("Unknown terminal session: {session_id}"))?;
        match process.killer.kill() {
            Ok(()) => Ok(()),
            Err(error) if is_missing_process_error(&error) => Ok(()),
            Err(error) => Err(format!("Failed to kill terminal: {error}")),
        }
    }

    pub fn list_sessions(&self) -> Vec<TerminalSession> {
        self.sessions
            .lock()
            .unwrap()
            .values()
            .map(|process| process.meta.clone())
            .collect()
    }
}

fn is_missing_process_error(error: &std::io::Error) -> bool {
    error.kind() == ErrorKind::NotFound || error.raw_os_error().is_some_and(|code| code == 3)
}

fn default_shell_command() -> ResolvedCommand {
    #[cfg(windows)]
    {
        ResolvedCommand {
            program: std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
            args: Vec::new(),
        }
    }
    #[cfg(not(windows))]
    {
        ResolvedCommand {
            program: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            args: Vec::new(),
        }
    }
}

fn shell_command(command: &str) -> ResolvedCommand {
    #[cfg(windows)]
    {
        ResolvedCommand {
            program: std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
            args: vec!["/C".to_string(), command.to_string()],
        }
    }
    #[cfg(not(windows))]
    {
        ResolvedCommand {
            program: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            args: vec!["-lc".to_string(), command.to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> CreateTerminalSessionInput {
        CreateTerminalSessionInput {
            kind: TerminalLaunchKind::Agent,
            platform: Some("codex".to_string()),
            command: None,
            title: None,
            cwd: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            cols: Some(100),
            rows: Some(30),
        }
    }

    #[test]
    fn rejects_empty_cwd() {
        let mut input = base_input();
        input.cwd = " ".to_string();
        assert!(validate_launch_input(&input).is_err());
    }

    #[test]
    fn rejects_missing_resume_command() {
        let mut input = base_input();
        input.kind = TerminalLaunchKind::Resume;
        input.command = None;
        assert!(validate_launch_input(&input).is_err());
    }

    #[test]
    fn resolves_agent_command() {
        let input = base_input();
        let command = resolve_launch_command(&input).unwrap();
        assert_eq!(command.program, "codex");
        assert!(command.args.is_empty());
    }

    #[test]
    fn resolves_resume_command_through_shell() {
        let mut input = base_input();
        input.kind = TerminalLaunchKind::Resume;
        input.command = Some("codex resume abc123".to_string());
        let command = resolve_launch_command(&input).unwrap();
        assert!(!command.program.trim().is_empty());
        assert!(command.args.join(" ").contains("codex resume abc123"));
    }

    #[test]
    fn rejects_unsupported_platform() {
        let mut input = base_input();
        input.platform = Some("unknown".to_string());
        assert!(resolve_launch_command(&input).is_err());
    }

    #[test]
    fn list_sessions_starts_empty() {
        let manager = TerminalManager::default();
        assert!(manager.list_sessions().is_empty());
    }

    #[test]
    fn treats_missing_process_as_already_closed() {
        assert!(is_missing_process_error(&std::io::Error::from(
            ErrorKind::NotFound,
        )));
        assert!(is_missing_process_error(
            &std::io::Error::from_raw_os_error(3,)
        ));
    }
}
