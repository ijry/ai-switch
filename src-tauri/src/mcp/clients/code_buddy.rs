//! CodeBuddy MCP configuration.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;

pub struct CodeBuddyAdapter {
    pub path: PathBuf,
    pub settings_path: PathBuf,
}

impl Default for CodeBuddyAdapter {
    fn default() -> Self {
        let home = common::home_dir();
        Self {
            path: home.join(".codebuddy.json"),
            settings_path: home.join(".codebuddy/settings.json"),
        }
    }
}

impl McpClientAdapter for CodeBuddyAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::CodeBuddy
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        common::read_json_servers(&self.path, "mcpServers", "CodeBuddy")
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        common::upsert_json_server(&self.path, "mcpServers", id, spec, "CodeBuddy write")?;
        common::set_local_plugin(&self.settings_path, id, true)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        let removed = common::remove_json_server(&self.path, "mcpServers", id)?;
        let _ = common::set_local_plugin(&self.settings_path, id, false);
        Ok(removed)
    }
}
