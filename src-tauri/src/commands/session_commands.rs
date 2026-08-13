use crate::core::sessions::{get_session_messages_core, list_sessions_core};
use crate::error::{ApiError, AppError};
use crate::session_manager;
use serde::Deserialize;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionTerminalInput {
    pub cwd: String,
    pub command: String,
}

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

#[tauri::command]
pub fn open_session_terminal(input: OpenSessionTerminalInput) -> Result<(), ApiError> {
    let (cwd, command) = validate_session_terminal_input(&input)?;
    spawn_system_terminal(&cwd, &command).map_err(|error| {
        ApiError::from(AppError::Filesystem {
            code: "filesystem.session_terminal_open",
            message: "Could not open the system terminal".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })
    })
}

fn validate_session_terminal_input(
    input: &OpenSessionTerminalInput,
) -> Result<(PathBuf, String), ApiError> {
    let cwd = input.cwd.trim();
    let command = input.command.trim();
    if cwd.is_empty() {
        return Err(ApiError::from(AppError::Validation {
            code: "validation.session_terminal_cwd_missing",
            message: "A project directory is required to open the system terminal".to_string(),
            details: None,
            recoverable: true,
        }));
    }
    if command.is_empty() {
        return Err(ApiError::from(AppError::Validation {
            code: "validation.session_terminal_command_missing",
            message: "A resume command is required to open the system terminal".to_string(),
            details: None,
            recoverable: true,
        }));
    }
    if input.cwd.contains('\0') || input.command.contains('\0') {
        return Err(ApiError::from(AppError::Validation {
            code: "validation.session_terminal_input_invalid",
            message: "The project directory and resume command cannot contain NUL characters"
                .to_string(),
            details: None,
            recoverable: true,
        }));
    }

    let cwd_path = PathBuf::from(cwd);
    if !cwd_path.is_dir() {
        return Err(ApiError::from(AppError::Validation {
            code: "validation.session_terminal_cwd_invalid",
            message: "The session project directory does not exist".to_string(),
            details: Some(cwd_path.display().to_string()),
            recoverable: true,
        }));
    }

    Ok((cwd_path, command.to_string()))
}

#[cfg(target_os = "windows")]
fn spawn_system_terminal(cwd: &Path, command: &str) -> io::Result<()> {
    Command::new("cmd.exe")
        .current_dir(cwd)
        .args(["/D", "/K", command])
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn spawn_system_terminal(cwd: &Path, command: &str) -> io::Result<()> {
    let shell_command = format!("cd -- {} && {command}", shell_quote(&cwd.to_string_lossy()));
    let script = format!(
        "tell application \"Terminal\"\n activate\n do script \"{}\"\nend tell",
        apple_script_quote(&shell_command)
    );
    Command::new("osascript")
        .args(["-e", script.as_str()])
        .spawn()
        .map(|_| ())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn spawn_system_terminal(cwd: &Path, command: &str) -> io::Result<()> {
    let candidates = [
        ("x-terminal-emulator", vec!["-e", "sh", "-lc", command]),
        ("gnome-terminal", vec!["--", "sh", "-lc", command]),
        ("konsole", vec!["-e", "sh", "-lc", command]),
        ("xfce4-terminal", vec!["--command", command]),
    ];
    let mut last_error = None;
    for (program, args) in candidates {
        match Command::new(program).current_dir(cwd).args(args).spawn() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "No supported terminal emulator was found",
        )
    }))
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn apple_script_quote(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn validates_session_terminal_directory_and_command() {
        let error = validate_session_terminal_input(&OpenSessionTerminalInput {
            cwd: String::new(),
            command: "codex resume session".to_string(),
        })
        .expect_err("empty cwd should be rejected");
        assert_eq!(error.code, "validation.session_terminal_cwd_missing");

        let directory = tempdir().expect("temporary directory");
        let (cwd, command) = validate_session_terminal_input(&OpenSessionTerminalInput {
            cwd: directory.path().to_string_lossy().to_string(),
            command: " codex resume session ".to_string(),
        })
        .expect("valid input");
        assert_eq!(cwd, directory.path());
        assert_eq!(command, "codex resume session");
    }

    #[test]
    fn rejects_missing_session_terminal_directory() {
        let error = validate_session_terminal_input(&OpenSessionTerminalInput {
            cwd: "this-directory-does-not-exist".to_string(),
            command: "codex resume session".to_string(),
        })
        .expect_err("missing cwd should be rejected");
        assert_eq!(error.code, "validation.session_terminal_cwd_invalid");
    }
}
