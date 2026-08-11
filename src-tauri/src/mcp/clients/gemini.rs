//! Gemini CLI MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct GeminiAdapter {
    pub path: PathBuf,
}

impl Default for GeminiAdapter {
    fn default() -> Self {
        Self {
            path: common::home_dir().join(".gemini/settings.json"),
        }
    }
}

impl McpClientAdapter for GeminiAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::Gemini
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        common::read_json_servers(&self.path, "mcpServers", "Gemini")
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        common::upsert_json_server(&self.path, "mcpServers", id, spec, "Gemini write")
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        common::remove_json_server(&self.path, "mcpServers", id)
    }
}
