//! Pre-accepts the Claude Code workspace trust prompt for folders the user
//! explicitly launches from AI Switch.
//!
//! Claude Code asks "Do you trust the files in this folder?" once per project
//! directory and records the answer in `~/.claude.json` under
//! `projects["<forward/slash/path>"].hasTrustDialogAccepted`. Launching a session
//! from the Vibe screen already is an explicit "run the agent here" action, so we
//! record the answer up front instead of making the user retype it on every
//! launch. Nothing else about the project entry is touched, and no other config
//! file is written.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// Location of the Claude Code config file. Claude keeps `.claude.json` in the
/// user's home directory even when its data directory moves elsewhere.
pub fn claude_config_path() -> PathBuf {
    home_dir().join(".claude.json")
}

fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Claude keys its `projects` map by the absolute path with forward slashes and
/// no trailing separator, e.g. `D:/Repos/app`.
pub fn project_key(cwd: &str) -> Option<String> {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = Path::new(trimmed);
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut text = absolute.to_string_lossy().replace('\\', "/");
    // `canonicalize` returns a `\\?\C:\...` extended path on Windows.
    if let Some(rest) = text.strip_prefix("//?/") {
        text = rest.to_string();
    }
    while text.len() > 1 && text.ends_with('/') && !text.ends_with(":/") {
        text.pop();
    }
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Marks `cwd` as trusted in the Claude config at `config_path`.
///
/// Returns `true` when the file was rewritten. Missing or unreadable configs are
/// left alone: Claude itself creates the file on first run, and guessing its
/// shape is riskier than letting the prompt appear once.
pub fn trust_project_at(config_path: &Path, cwd: &str) -> Result<bool, String> {
    let Some(key) = project_key(cwd) else {
        return Ok(false);
    };

    let raw = match std::fs::read_to_string(config_path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Could not read {}: {error}",
                config_path.display()
            ))
        }
    };

    let mut root: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("Invalid JSON in {}: {error}", config_path.display()))?;
    let Some(object) = root.as_object_mut() else {
        return Ok(false);
    };

    let projects = object
        .entry("projects".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(projects) = projects.as_object_mut() else {
        return Ok(false);
    };

    let entry = projects
        .entry(key)
        .or_insert_with(|| Value::Object(default_project_entry()));
    let Some(entry) = entry.as_object_mut() else {
        return Ok(false);
    };

    if entry.get("hasTrustDialogAccepted") == Some(&Value::Bool(true)) {
        return Ok(false);
    }
    entry.insert("hasTrustDialogAccepted".to_string(), Value::Bool(true));

    let serialized = serde_json::to_string_pretty(&root)
        .map_err(|error| format!("Could not serialize Claude config: {error}"))?;
    write_atomically(config_path, &format!("{serialized}\n"))?;
    Ok(true)
}

/// Best-effort variant used on the launch path: never fails a terminal launch
/// because the Claude config could not be updated.
pub fn trust_project_best_effort(cwd: &str) {
    if let Err(error) = trust_project_at(&claude_config_path(), cwd) {
        eprintln!("[claude-trust] {error}");
    }
}

fn default_project_entry() -> Map<String, Value> {
    let mut entry = Map::new();
    entry.insert("allowedTools".to_string(), Value::Array(Vec::new()));
    entry.insert("mcpServers".to_string(), Value::Object(Map::new()));
    entry.insert(
        "enabledMcpjsonServers".to_string(),
        Value::Array(Vec::new()),
    );
    entry.insert(
        "disabledMcpjsonServers".to_string(),
        Value::Array(Vec::new()),
    );
    entry.insert(
        "hasClaudeMdExternalIncludesApproved".to_string(),
        Value::Bool(false),
    );
    entry.insert(
        "hasClaudeMdExternalIncludesWarningShown".to_string(),
        Value::Bool(false),
    );
    entry
}

fn write_atomically(path: &Path, contents: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let temp = parent.join(format!(
        ".claude.json.ai-switch-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&temp, contents)
        .map_err(|error| format!("Could not write {}: {error}", temp.display()))?;
    match std::fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(format!("Could not update {}: {error}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn project_key_uses_forward_slashes_without_trailing_separator() {
        let dir = tempfile::tempdir().unwrap();
        let raw = format!("{}\\", dir.path().to_string_lossy());
        let key = project_key(&raw).unwrap();
        assert!(!key.contains('\\'), "unexpected backslash in {key}");
        assert!(!key.ends_with('/'), "unexpected trailing slash in {key}");
        assert!(!key.starts_with("//?/"), "extended path leaked into {key}");
    }

    #[test]
    fn project_key_rejects_blank_input() {
        assert_eq!(project_key("   "), None);
    }

    #[test]
    fn trusting_a_new_project_adds_an_accepted_entry() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join(".claude.json");
        std::fs::write(&config, "{\n  \"numStartups\": 3\n}\n").unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let cwd = workspace.to_string_lossy().to_string();

        assert!(trust_project_at(&config, &cwd).unwrap());

        let root = read_json(&config);
        assert_eq!(root["numStartups"], Value::from(3));
        let key = project_key(&cwd).unwrap();
        assert_eq!(root["projects"][&key]["hasTrustDialogAccepted"], Value::Bool(true));
    }

    #[test]
    fn trusting_keeps_existing_project_settings() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join(".claude.json");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let cwd = workspace.to_string_lossy().to_string();
        let key = project_key(&cwd).unwrap();
        let existing = serde_json::json!({
            "projects": {
                key.clone(): {
                    "hasTrustDialogAccepted": false,
                    "allowedTools": ["Bash"],
                    "lastSessionId": "abc",
                }
            }
        });
        std::fs::write(&config, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        assert!(trust_project_at(&config, &cwd).unwrap());

        let root = read_json(&config);
        assert_eq!(root["projects"][&key]["hasTrustDialogAccepted"], Value::Bool(true));
        assert_eq!(root["projects"][&key]["allowedTools"][0], Value::from("Bash"));
        assert_eq!(root["projects"][&key]["lastSessionId"], Value::from("abc"));
    }

    #[test]
    fn already_trusted_projects_are_not_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join(".claude.json");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let cwd = workspace.to_string_lossy().to_string();
        let key = project_key(&cwd).unwrap();
        let existing = serde_json::json!({
            "projects": { key: { "hasTrustDialogAccepted": true } }
        });
        let text = serde_json::to_string_pretty(&existing).unwrap();
        std::fs::write(&config, &text).unwrap();

        assert!(!trust_project_at(&config, &cwd).unwrap());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), text);
    }

    #[test]
    fn missing_config_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join(".claude.json");
        assert!(!trust_project_at(&config, &dir.path().to_string_lossy()).unwrap());
        assert!(!config.exists());
    }

    #[test]
    fn invalid_config_reports_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join(".claude.json");
        std::fs::write(&config, "{ not json").unwrap();
        assert!(trust_project_at(&config, &dir.path().to_string_lossy()).is_err());
    }
}
