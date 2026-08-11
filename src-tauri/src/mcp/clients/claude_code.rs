//! Claude Code MCP configuration.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;

pub struct ClaudeCodeAdapter {
    pub path: PathBuf,
    pub settings_path: PathBuf,
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        let home = common::home_dir();
        Self {
            path: home.join(".claude.json"),
            settings_path: home.join(".claude/settings.json"),
        }
    }
}

impl ClaudeCodeAdapter {
    pub fn read_servers_at(path: &std::path::Path) -> Result<BTreeMap<String, Value>, AppError> {
        common::read_json_servers(path, "mcpServers", "Claude Code")
    }
    pub fn upsert_server_at(
        path: &std::path::Path,
        id: &str,
        spec: &Value,
    ) -> Result<(), AppError> {
        common::upsert_json_server(path, "mcpServers", id, spec, "Claude Code write")
    }
}

impl McpClientAdapter for ClaudeCodeAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::ClaudeCode
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        Self::read_servers_at(&self.path)
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        Self::upsert_server_at(&self.path, id, spec)?;
        common::set_local_plugin(&self.settings_path, id, true)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        let removed = common::remove_json_server(&self.path, "mcpServers", id)?;
        let _ = common::set_local_plugin(&self.settings_path, id, false);
        Ok(removed)
    }
}
