//! Cursor MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct CursorAdapter {
    pub path: PathBuf,
}

impl Default for CursorAdapter {
    fn default() -> Self {
        Self {
            path: common::home_dir().join(".cursor/mcp.json"),
        }
    }
}

impl CursorAdapter {
    pub fn read_servers_at(path: &std::path::Path) -> Result<BTreeMap<String, Value>, AppError> {
        let mut result = BTreeMap::new();
        for (id, mut spec) in common::read_json_servers(path, "mcpServers", "Cursor")? {
            if let Some(object) = spec.as_object_mut() {
                object.remove("type");
            }
            result.insert(
                id,
                crate::mcp::normalize::canonicalize_spec(&spec, "Cursor")?,
            );
        }
        Ok(result)
    }

    fn cursor_spec(spec: &Value) -> Result<Value, AppError> {
        let canonical = crate::mcp::normalize::canonicalize_spec(spec, "Cursor write")?;
        let Some(object) = canonical.as_object() else {
            return Ok(canonical);
        };
        let mut filtered = Map::new();
        for key in ["command", "args", "env", "cwd", "url", "headers"] {
            if let Some(value) = object.get(key) {
                filtered.insert(key.to_string(), value.clone());
            }
        }
        Ok(Value::Object(filtered))
    }
}

impl McpClientAdapter for CursorAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::Cursor
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        Self::read_servers_at(&self.path)
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        let mut root = common::read_json_file(&self.path)?;
        if !root.is_object() {
            root = Value::Object(Map::new());
        }
        let object = root.as_object_mut().unwrap();
        if !object
            .get("mcpServers")
            .map(Value::is_object)
            .unwrap_or(false)
        {
            object.insert("mcpServers".to_string(), Value::Object(Map::new()));
        }
        object
            .get_mut("mcpServers")
            .and_then(Value::as_object_mut)
            .unwrap()
            .insert(id.to_string(), Self::cursor_spec(spec)?);
        common::write_json_file(&self.path, &root)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        common::remove_json_server(&self.path, "mcpServers", id)
    }
}
