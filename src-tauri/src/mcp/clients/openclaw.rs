//! OpenClaw MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct OpenClawAdapter {
    pub path: PathBuf,
}

impl Default for OpenClawAdapter {
    fn default() -> Self {
        Self {
            path: common::home_dir().join(".openclaw/openclaw.json"),
        }
    }
}

impl McpClientAdapter for OpenClawAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::OpenClaw
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        common::read_json_servers(&self.path, "mcpServers", "OpenClaw")
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        common::upsert_json_server(&self.path, "mcpServers", id, spec, "OpenClaw write")
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        common::remove_json_server(&self.path, "mcpServers", id)
    }
}
