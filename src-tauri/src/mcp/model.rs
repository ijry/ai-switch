//! MCP data contracts.
//!
//! Portions are adapted from xintaofei/codeg (Apache-2.0).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAppType {
    ClaudeCode,
    Codex,
    Gemini,
    OpenClaw,
    OpenCode,
    Hermes,
    Cline,
    Cursor,
    KimiCode,
    CodeBuddy,
    Grok,
}

impl McpAppType {
    pub const ALL: [Self; 11] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::Gemini,
        Self::OpenClaw,
        Self::OpenCode,
        Self::Hermes,
        Self::Cline,
        Self::Cursor,
        Self::KimiCode,
        Self::CodeBuddy,
        Self::Grok,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::OpenClaw => "open_claw",
            Self::OpenCode => "open_code",
            Self::Hermes => "hermes",
            Self::Cline => "cline",
            Self::Cursor => "cursor",
            Self::KimiCode => "kimi_code",
            Self::CodeBuddy => "code_buddy",
            Self::Grok => "grok",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex CLI",
            Self::Gemini => "Gemini CLI",
            Self::OpenClaw => "OpenClaw",
            Self::OpenCode => "OpenCode",
            Self::Hermes => "Hermes Agent",
            Self::Cline => "Cline",
            Self::Cursor => "Cursor",
            Self::KimiCode => "Kimi Code",
            Self::CodeBuddy => "CodeBuddy",
            Self::Grok => "Grok",
        }
    }
}

impl std::fmt::Display for McpAppType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalMcpServer {
    pub id: String,
    pub spec: Value,
    pub apps: Vec<McpAppType>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpMarketplaceProvider {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpMarketplaceItem {
    pub provider_id: String,
    pub server_id: String,
    pub name: String,
    pub description: String,
    pub homepage: Option<String>,
    pub remote: bool,
    pub verified: bool,
    pub icon_url: Option<String>,
    pub latest_version: Option<String>,
    pub protocols: Vec<String>,
    pub owner: Option<String>,
    pub namespace: Option<String>,
    pub downloads: Option<u64>,
    pub score: Option<f64>,
    pub is_deployed: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpMarketplaceInstallParameter {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub required: bool,
    pub secret: bool,
    pub kind: String,
    pub default_value: Option<Value>,
    pub placeholder: Option<String>,
    pub enum_values: Vec<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpMarketplaceInstallOption {
    pub id: String,
    pub protocol: String,
    pub label: String,
    pub description: Option<String>,
    pub spec: Value,
    pub parameters: Vec<McpMarketplaceInstallParameter>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpMarketplaceServerDetail {
    pub provider_id: String,
    pub server_id: String,
    pub name: String,
    pub description: String,
    pub homepage: Option<String>,
    pub remote: bool,
    pub verified: bool,
    pub icon_url: Option<String>,
    pub latest_version: Option<String>,
    pub protocols: Vec<String>,
    pub owner: Option<String>,
    pub namespace: Option<String>,
    pub downloads: Option<u64>,
    pub score: Option<f64>,
    pub is_deployed: Option<bool>,
    pub default_option_id: Option<String>,
    pub install_options: Vec<McpMarketplaceInstallOption>,
    pub spec: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_type_serializes_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&McpAppType::ClaudeCode).unwrap(),
            "\"claude_code\""
        );
        assert_eq!(
            serde_json::to_string(&McpAppType::CodeBuddy).unwrap(),
            "\"code_buddy\""
        );
    }

    #[test]
    fn all_apps_are_unique_and_stable() {
        let mut values = McpAppType::ALL
            .iter()
            .map(|app| app.as_str())
            .collect::<Vec<_>>();
        values.sort_unstable();
        values.dedup();
        assert_eq!(values.len(), McpAppType::ALL.len());
    }
}
