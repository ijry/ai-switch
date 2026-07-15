use chrono::{DateTime, Utc};
use directories::BaseDirs;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub provider_id: String,
    pub session_id: String,
    pub title: Option<String>,
    pub project_dir: Option<String>,
    pub created_at: Option<i64>,
    pub last_active_at: Option<i64>,
    pub source_path: String,
    pub resume_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub ts: Option<i64>,
}

#[derive(Debug, Clone)]
struct ProviderSpec {
    id: &'static str,
    roots: Vec<PathBuf>,
    extensions: &'static [&'static str],
}

pub fn scan_sessions(platform: Option<&str>) -> Vec<SessionMeta> {
    let platform = platform.map(str::to_lowercase);
    let mut seen = HashSet::new();
    let mut sessions = Vec::new();

    for spec in provider_specs() {
        if platform.as_deref().is_some_and(|value| value != spec.id) {
            continue;
        }

        for root in spec.roots.iter().filter(|root| root.exists()) {
            let mut files = Vec::new();
            collect_session_files(root, spec.extensions, 6, &mut files);

            for path in files {
                let key = path.to_string_lossy().to_string();
                if !seen.insert(key.clone()) {
                    continue;
                }

                if let Some(session) = session_from_file(spec.id, &path) {
                    sessions.push(session);
                }
            }
        }
    }

    sessions.sort_by(|a, b| {
        let a_ts = a.last_active_at.or(a.created_at).unwrap_or(0);
        let b_ts = b.last_active_at.or(b.created_at).unwrap_or(0);
        b_ts.cmp(&a_ts)
    });
    sessions.truncate(500);
    sessions
}

pub fn load_messages(_provider_id: &str, source_path: &str) -> Result<Vec<SessionMessage>, String> {
    let path = Path::new(source_path);
    if !path.exists() {
        return Err(format!("Session source not found: {}", path.display()));
    }

    let file = fs::File::open(path)
        .map_err(|error| format!("Could not open session source {}: {error}", path.display()))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines().take(2_000) {
        let line = line.map_err(|error| format!("Could not read session line: {error}"))?;
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            if let Some(message) = message_from_value(&value) {
                messages.push(message);
            }
        }
    }

    Ok(messages)
}

fn provider_specs() -> Vec<ProviderSpec> {
    let Some(base_dirs) = BaseDirs::new() else {
        return Vec::new();
    };
    let home = base_dirs.home_dir();

    vec![
        ProviderSpec {
            id: "codex",
            roots: vec![home.join(".codex").join("sessions"), home.join(".codex")],
            extensions: &["jsonl"],
        },
        ProviderSpec {
            id: "claude",
            roots: vec![
                home.join(".claude").join("projects"),
                home.join(".cache").join("claude").join("projects"),
            ],
            extensions: &["jsonl"],
        },
        ProviderSpec {
            id: "gemini",
            roots: vec![
                home.join(".gemini").join("tmp"),
                home.join(".cache").join("gemini").join("tmp"),
            ],
            extensions: &["json", "jsonl"],
        },
        ProviderSpec {
            id: "opencode",
            roots: vec![
                home.join(".local").join("share").join("opencode"),
                home.join("AppData").join("Local").join("opencode"),
            ],
            extensions: &["json", "jsonl"],
        },
        ProviderSpec {
            id: "openclaw",
            roots: vec![home.join(".openclaw").join("agents")],
            extensions: &["jsonl"],
        },
        ProviderSpec {
            id: "hermes",
            roots: vec![home.join(".hermes").join("sessions")],
            extensions: &["json", "jsonl"],
        },
    ]
}

fn collect_session_files(dir: &Path, extensions: &[&str], depth: usize, files: &mut Vec<PathBuf>) {
    if depth == 0 || files.len() >= 1_000 {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_session_files(&path, extensions, depth - 1, files);
            continue;
        }

        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_lowercase);
        if extension
            .as_deref()
            .is_some_and(|value| extensions.contains(&value))
        {
            files.push(path);
        }
    }
}

fn session_from_file(provider_id: &str, path: &Path) -> Option<SessionMeta> {
    let metadata = fs::metadata(path).ok();
    let modified_at = metadata
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64);

    let messages = preview_messages(path, 80);
    let title = title_from_messages(&messages);
    let created_at = messages
        .first()
        .and_then(|message| message.ts)
        .or(modified_at);
    let last_active_at = messages
        .last()
        .and_then(|message| message.ts)
        .or(modified_at);
    let session_id = extract_session_id(path).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("session")
            .to_string()
    });
    let project_dir = extract_project_dir(path).or_else(|| {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|value| value.to_str())
            .map(str::to_string)
    });

    Some(SessionMeta {
        provider_id: provider_id.to_string(),
        session_id: session_id.clone(),
        title,
        project_dir,
        created_at,
        last_active_at,
        source_path: path.to_string_lossy().to_string(),
        resume_command: resume_command(provider_id, &session_id),
    })
}

fn preview_messages(path: &Path, limit: usize) -> Vec<SessionMessage> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);

    reader
        .lines()
        .take(limit)
        .flatten()
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter_map(|value| message_from_value(&value))
        .collect()
}

fn message_from_value(value: &Value) -> Option<SessionMessage> {
    let candidates = [
        value,
        value.get("payload").unwrap_or(&Value::Null),
        value.get("message").unwrap_or(&Value::Null),
        value.pointer("/payload/message").unwrap_or(&Value::Null),
    ];

    for candidate in candidates {
        let content = text_field(candidate, &["content", "text", "message"])
            .or_else(|| content_array(candidate.get("content")));
        if let Some(content) = content {
            let role = text_field(candidate, &["role", "author", "type"])
                .unwrap_or_else(|| "message".to_string());
            return Some(SessionMessage {
                role: normalize_role(&role),
                content,
                ts: timestamp_from_value(value).or_else(|| timestamp_from_value(candidate)),
            });
        }
    }

    None
}

fn text_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key))
        .find_map(|candidate| candidate.as_str().map(str::to_string))
        .filter(|text| !text.trim().is_empty())
}

fn content_array(value: Option<&Value>) -> Option<String> {
    let Value::Array(items) = value? else {
        return None;
    };

    let text = items
        .iter()
        .filter_map(|item| text_field(item, &["text", "content"]))
        .collect::<Vec<_>>()
        .join("\n");

    (!text.trim().is_empty()).then_some(text)
}

fn timestamp_from_value(value: &Value) -> Option<i64> {
    for key in ["timestamp", "created_at", "createdAt", "ts"] {
        let candidate = value.get(key)?;
        if let Some(number) = candidate.as_i64() {
            return Some(number);
        }
        if let Some(text) = candidate.as_str() {
            if let Ok(number) = text.parse::<i64>() {
                return Some(number);
            }
            if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
                return Some(parsed.with_timezone(&Utc).timestamp());
            }
        }
    }
    None
}

fn extract_session_id(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(20).flatten() {
        let value: Value = serde_json::from_str(&line).ok()?;
        for candidate in [
            value.get("session_id"),
            value.get("sessionId"),
            value.get("id"),
            value.pointer("/payload/id"),
        ] {
            if let Some(id) = candidate.and_then(Value::as_str) {
                return Some(id.to_string());
            }
        }
    }

    None
}

fn extract_project_dir(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(20).flatten() {
        let value: Value = serde_json::from_str(&line).ok()?;
        for candidate in [
            value.get("cwd"),
            value.get("project_dir"),
            value.get("projectDir"),
            value.pointer("/payload/cwd"),
            value.pointer("/payload/project_dir"),
        ] {
            if let Some(project_dir) = candidate.and_then(Value::as_str) {
                return Some(project_dir.to_string());
            }
        }
    }

    None
}

fn normalize_role(role: &str) -> String {
    match role.to_lowercase().as_str() {
        "human" | "user_message" => "user".to_string(),
        "assistant_message" | "ai" => "assistant".to_string(),
        "system" => "system".to_string(),
        "tool" | "tool_result" | "function_call" => "tool".to_string(),
        value => value.to_string(),
    }
}

fn title_from_content(content: &str) -> String {
    let single_line = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() > 72 {
        format!("{}...", single_line.chars().take(72).collect::<String>())
    } else {
        single_line
    }
}

fn title_from_messages(messages: &[SessionMessage]) -> Option<String> {
    messages
        .iter()
        .find(|message| is_title_candidate(message))
        .map(|message| title_from_content(&message.content))
}

fn is_title_candidate(message: &SessionMessage) -> bool {
    if matches!(
        message.role.as_str(),
        "assistant" | "developer" | "system" | "tool"
    ) {
        return false;
    }

    let trimmed = message.content.trim();
    if trimmed.is_empty() {
        return false;
    }

    !is_context_blob(trimmed)
}

fn is_context_blob(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.starts_with("<permissions instructions>")
        || lower.starts_with("<skills_instructions>")
        || lower.starts_with("<environment_context>")
        || lower.starts_with("# agents.md instructions")
        || lower.starts_with("<instructions>")
}

fn resume_command(provider_id: &str, session_id: &str) -> Option<String> {
    let command = match provider_id {
        "codex" => format!("codex resume {session_id}"),
        "claude" => format!("claude --resume {session_id}"),
        "gemini" => format!("gemini --resume {session_id}"),
        "opencode" => format!("opencode session {session_id}"),
        "openclaw" => format!("openclaw resume {session_id}"),
        "hermes" => format!("hermes resume {session_id}"),
        _ => return None,
    };
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, content: &str) -> SessionMessage {
        SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
            ts: None,
        }
    }

    #[test]
    fn title_skips_codex_context_messages() {
        let messages = vec![
            message(
                "developer",
                "<permissions instructions>Filesystem sandboxing",
            ),
            message(
                "user",
                "<environment_context><cwd>D:/repo/app</cwd></environment_context>",
            ),
            message("user", "Fix Vibe page dark mode tabs"),
        ];

        assert_eq!(
            title_from_messages(&messages),
            Some("Fix Vibe page dark mode tabs".to_string())
        );
    }

    #[test]
    fn title_skips_agents_md_context() {
        let messages = vec![
            message(
                "user",
                "# AGENTS.md instructions for D:/repo/app\n\n<INSTRUCTIONS>Work on main.</INSTRUCTIONS>",
            ),
            message("user", "vibe page issues"),
        ];

        assert_eq!(
            title_from_messages(&messages),
            Some("vibe page issues".to_string())
        );
    }
}
