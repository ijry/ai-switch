//! Cline MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct ClineAdapter {
    pub path: PathBuf,
}

impl Default for ClineAdapter {
    fn default() -> Self {
        Self {
            path: common::home_dir().join(".cline/data/settings/cline_mcp_settings.json"),
        }
    }
}

impl McpClientAdapter for ClineAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::Cline
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        common::read_json_servers(&self.path, "mcpServers", "Cline")
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        common::upsert_json_server(&self.path, "mcpServers", id, spec, "Cline write")
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        common::remove_json_server(&self.path, "mcpServers", id)
    }
}
