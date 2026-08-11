//! Skills data contracts.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SkillAgentType {
    ClaudeCode,
    Codex,
    Gemini,
    Grok,
    OpenClaw,
    OpenCode,
    Hermes,
    Cline,
    Cursor,
    KimiCode,
    CodeBuddy,
}

impl SkillAgentType {
    pub const ALL: [Self; 11] = [
        Self::Codex,
        Self::ClaudeCode,
        Self::Gemini,
        Self::Grok,
        Self::OpenCode,
        Self::OpenClaw,
        Self::Hermes,
        Self::Cline,
        Self::Cursor,
        Self::KimiCode,
        Self::CodeBuddy,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
            Self::OpenClaw => "open_claw",
            Self::OpenCode => "open_code",
            Self::Hermes => "hermes",
            Self::Cline => "cline",
            Self::Cursor => "cursor",
            Self::KimiCode => "kimi_code",
            Self::CodeBuddy => "code_buddy",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex CLI",
            Self::Gemini => "Gemini CLI",
            Self::Grok => "Grok",
            Self::OpenClaw => "OpenClaw",
            Self::OpenCode => "OpenCode",
            Self::Hermes => "Hermes Agent",
            Self::Cline => "Cline",
            Self::Cursor => "Cursor",
            Self::KimiCode => "Kimi Code",
            Self::CodeBuddy => "CodeBuddy",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillLayout {
    MarkdownFile,
    SkillDirectory,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillLocation {
    pub scope: SkillScope,
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillItem {
    pub id: String,
    pub name: String,
    pub scope: SkillScope,
    pub layout: SkillLayout,
    pub path: String,
    pub description: Option<String>,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillsListResult {
    pub supported: bool,
    pub message: Option<String>,
    pub locations: Vec<SkillLocation>,
    pub skills: Vec<SkillItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillContent {
    pub skill: SkillItem,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillAgentInfo {
    pub agent_type: SkillAgentType,
    pub display_name: String,
    pub skills_capable: bool,
}
