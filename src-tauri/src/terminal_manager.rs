use chrono::Utc;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
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
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
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
            let program = agent_program_name(platform)
                .ok_or_else(|| format!("Unsupported terminal platform: {platform}"))?;
            Ok(ResolvedCommand {
                program: program.to_string(),
                args: agent_launch_args(platform, input),
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

/// Maps an agent platform id to the CLI entry point AI Switch spawns for it.
pub fn agent_program_name(platform: &str) -> Option<&'static str> {
    match platform.trim() {
        "codex" => Some("codex"),
        "claude" => Some("claude"),
        "grok" => Some("grok"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        "openclaw" => Some("openclaw"),
        "hermes" => Some("hermes"),
        _ => None,
    }
}

/// Only the CLIs whose `--model` flag has been verified take an explicit model
/// argument; the rest keep whatever their own config selects.
pub fn agent_supports_model_flag(platform: &str) -> bool {
    matches!(platform.trim(), "codex" | "claude" | "grok" | "gemini")
}

/// Codex is the only agent that exposes a reasoning-effort knob AI Switch can
/// set at launch time (`-c model_reasoning_effort=<level>`).
pub fn agent_supports_reasoning(platform: &str) -> bool {
    platform.trim() == "codex"
}

fn agent_launch_args(platform: &str, input: &CreateTerminalSessionInput) -> Vec<String> {
    let mut args = Vec::new();
    let model = input
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(model) = model {
        if agent_supports_model_flag(platform) {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
    }

    let reasoning = input
        .reasoning_effort
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(reasoning) = reasoning {
        if agent_supports_reasoning(platform) {
            args.push("-c".to_string());
            args.push(format!("model_reasoning_effort=\"{reasoning}\""));
        }
    }

    args
}

/// Resolves `program` against `PATH`, honoring `PATHEXT` on Windows where the
/// agent CLIs are shims (`codex.cmd`, `codex.ps1`) rather than bare executables.
pub fn find_program_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let dirs = std::env::split_paths(&path).collect::<Vec<_>>();
    let pathext = std::env::var("PATHEXT").unwrap_or_default();
    find_program_in_dirs(&dirs, program, &pathext)
}

pub fn find_program_in_dirs(dirs: &[PathBuf], program: &str, pathext: &str) -> Option<PathBuf> {
    let program = program.trim();
    if program.is_empty() {
        return None;
    }

    let extensions = executable_extensions(pathext);
    for dir in dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let base = dir.join(program);
        if base.is_file() {
            return Some(base);
        }
        for extension in &extensions {
            let candidate = dir.join(format!("{program}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn executable_extensions(pathext: &str) -> Vec<String> {
    pathext
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.starts_with('.') {
                value.to_string()
            } else {
                format!(".{value}")
            }
        })
        .collect()
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

    /// Kills every live terminal child. Intended for app shutdown, where no
    /// caller is left to report errors to.
    ///
    /// Only the direct PTY child is killed. A shell that launched an agent CLI
    /// has grandchildren this cannot reach; ending those too would need a
    /// Windows Job Object. They live outside the install directory, so unlike
    /// the Tailscale sidecar they do not block the updater.
    pub fn kill_all(&self) {
        let mut sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            // Shutdown cleanup must still happen if some other thread panicked
            // while holding this lock.
            Err(poisoned) => poisoned.into_inner(),
        };
        for (_, mut process) in sessions.drain() {
            let _ = process.killer.kill();
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
            model: None,
            reasoning_effort: None,
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
    fn forwards_model_and_reasoning_to_codex() {
        let mut input = base_input();
        input.model = Some("gpt-5.6-sol".to_string());
        input.reasoning_effort = Some("high".to_string());
        let command = resolve_launch_command(&input).unwrap();
        assert_eq!(
            command.args,
            vec![
                "--model".to_string(),
                "gpt-5.6-sol".to_string(),
                "-c".to_string(),
                "model_reasoning_effort=\"high\"".to_string(),
            ]
        );
    }

    #[test]
    fn skips_reasoning_for_agents_without_the_knob() {
        let mut input = base_input();
        input.platform = Some("claude".to_string());
        input.model = Some("claude-sonnet-4-6".to_string());
        input.reasoning_effort = Some("high".to_string());
        let command = resolve_launch_command(&input).unwrap();
        assert_eq!(
            command.args,
            vec!["--model".to_string(), "claude-sonnet-4-6".to_string()]
        );
    }

    #[test]
    fn ignores_blank_model_and_reasoning_values() {
        let mut input = base_input();
        input.model = Some("  ".to_string());
        input.reasoning_effort = Some("".to_string());
        let command = resolve_launch_command(&input).unwrap();
        assert!(command.args.is_empty());
    }

    #[test]
    fn finds_windows_style_shims_through_pathext() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("codex.cmd"), "@echo off").unwrap();
        let dirs = vec![PathBuf::new(), dir.path().to_path_buf()];

        let found = find_program_in_dirs(&dirs, "codex", ".COM;.EXE;.CMD").unwrap();
        // Windows matches paths case-insensitively, so the probe returns the
        // candidate spelled the way PATHEXT spells it.
        assert_eq!(found.parent(), Some(dir.path()));
        assert_eq!(
            found
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_ascii_lowercase),
            Some("codex.cmd".to_string())
        );
        assert!(find_program_in_dirs(&dirs, "codex", ".COM;.EXE").is_none());
        assert!(find_program_in_dirs(&dirs, "  ", ".CMD").is_none());
    }

    #[test]
    fn finds_extensionless_programs_without_pathext() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("claude"), "#!/bin/sh\n").unwrap();
        let dirs = vec![dir.path().to_path_buf()];

        assert_eq!(
            find_program_in_dirs(&dirs, "claude", "").unwrap(),
            dir.path().join("claude")
        );
        assert!(find_program_in_dirs(&dirs, "codex", "").is_none());
    }

    #[test]
    fn normalizes_pathext_entries_without_a_leading_dot() {
        assert_eq!(
            executable_extensions("COM; EXE ;;.CMD"),
            vec![
                ".COM".to_string(),
                ".EXE".to_string(),
                ".CMD".to_string()
            ]
        );
    }

    #[test]
    fn list_sessions_starts_empty() {
        let manager = TerminalManager::default();
        assert!(manager.list_sessions().is_empty());
    }

    #[test]
    fn kill_all_on_an_idle_manager_is_a_no_op() {
        let manager = TerminalManager::default();
        manager.kill_all();
        assert!(manager.list_sessions().is_empty());
    }

    #[test]
    fn kill_all_still_runs_after_a_thread_poisoned_the_session_lock() {
        let manager = TerminalManager::default();
        let poisoner = manager.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.sessions.lock().unwrap();
            panic!("poison the session lock");
        })
        .join();

        // Shutdown cleanup runs after arbitrary threads may have died, so a
        // poisoned lock must not stop it. `lock().unwrap()` here would panic and
        // leave every terminal child alive.
        manager.kill_all();
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
