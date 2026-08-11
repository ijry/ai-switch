//! Kimi Code MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct KimiCodeAdapter {
    pub path: PathBuf,
}

impl Default for KimiCodeAdapter {
    fn default() -> Self {
        Self {
            path: common::env_path("KIMI_CODE_HOME", common::home_dir().join(".kimi-code"))
                .join("mcp.json"),
        }
    }
}

impl KimiCodeAdapter {
    pub fn read_servers_at(path: &std::path::Path) -> Result<BTreeMap<String, Value>, AppError> {
        let root = common::read_json_file(path)?;
        let Some(servers) = root.get("mcpServers").and_then(Value::as_object) else {
            return Ok(BTreeMap::new());
        };
        let mut result = BTreeMap::new();
        for (id, value) in servers {
            let mut value = value.clone();
            if let Some(object) = value.as_object_mut() {
                let transport = object
                    .get("transport")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                object.remove("transport");
                object.remove("type");
                if let Some(transport) = transport {
                    object.insert("type".to_string(), Value::String(transport));
                }
            }
            result.insert(
                id.clone(),
                crate::mcp::normalize::canonicalize_spec(&value, "Kimi Code")?,
            );
        }
        Ok(result)
    }

    fn kimi_spec(spec: &Value) -> Result<Value, AppError> {
        let canonical = crate::mcp::normalize::canonicalize_spec(spec, "Kimi Code write")?;
        let Some(object) = canonical.as_object() else {
            return Ok(canonical);
        };
        let mut result = Map::new();
        for key in [
            "type", "command", "args", "env", "cwd", "url", "headers", "enabled",
        ] {
            if let Some(value) = object.get(key) {
                result.insert(key.to_string(), value.clone());
            }
        }
        if matches!(
            object.get("type").and_then(Value::as_str),
            Some("http" | "sse")
        ) {
            result.insert(
                "transport".to_string(),
                object.get("type").cloned().unwrap(),
            );
        }
        Ok(Value::Object(result))
    }
}

impl McpClientAdapter for KimiCodeAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::KimiCode
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
            .insert(id.to_string(), Self::kimi_spec(spec)?);
        common::write_json_file(&self.path, &root)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        common::remove_json_server(&self.path, "mcpServers", id)
    }
}
