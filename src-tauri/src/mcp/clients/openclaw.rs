//! OpenClaw MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::Map;
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
        let root = common::read_json_file(&self.path)?;
        let Some(servers) = root
            .get("mcp")
            .and_then(Value::as_object)
            .and_then(|mcp| mcp.get("servers"))
            .and_then(Value::as_object)
        else {
            return Ok(BTreeMap::new());
        };
        let mut result = BTreeMap::new();
        for (id, spec) in servers {
            match crate::mcp::normalize::canonicalize_spec(
                spec,
                &format!("OpenClaw MCP entry {id}"),
            ) {
                Ok(value) => {
                    result.insert(id.clone(), value);
                }
                Err(error) => eprintln!("[MCP] skipping invalid OpenClaw entry {id}: {error}"),
            }
        }
        Ok(result)
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        let mut root = common::read_json_file(&self.path)?;
        if !root.is_object() {
            root = Value::Object(Map::new());
        }
        let object = root.as_object_mut().expect("object initialized above");
        if !object.get("mcp").map(Value::is_object).unwrap_or(false) {
            object.insert("mcp".to_string(), Value::Object(Map::new()));
        }
        let mcp = object
            .get_mut("mcp")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| invalid_object("mcp"))?;
        if !mcp.get("servers").map(Value::is_object).unwrap_or(false) {
            mcp.insert("servers".to_string(), Value::Object(Map::new()));
        }
        mcp.get_mut("servers")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| invalid_object("mcp.servers"))?
            .insert(
                id.to_string(),
                crate::mcp::normalize::canonicalize_spec(spec, "OpenClaw write")?,
            );
        common::write_json_file(&self.path, &root)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        if !self.path.exists() {
            return Ok(false);
        }
        let mut root = common::read_json_file(&self.path)?;
        let Some(object) = root.as_object_mut() else {
            return Ok(false);
        };
        let Some(mcp) = object.get_mut("mcp").and_then(Value::as_object_mut) else {
            return Ok(false);
        };
        let Some(servers) = mcp.get_mut("servers").and_then(Value::as_object_mut) else {
            return Ok(false);
        };
        let removed = servers.remove(id).is_some();
        if removed {
            if servers.is_empty() {
                mcp.remove("servers");
            }
            if mcp.is_empty() {
                object.remove("mcp");
            }
            common::write_json_file(&self.path, &root)?;
        }
        Ok(removed)
    }
}

fn invalid_object(path: &str) -> AppError {
    AppError::Validation {
        code: "mcp.config_invalid",
        message: format!("OpenClaw {path} must be an object"),
        details: None,
        recoverable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_servers_under_openclaw_mcp_namespace() {
        let directory = tempfile::tempdir().unwrap();
        let adapter = OpenClawAdapter {
            path: directory.path().join("openclaw.json"),
        };
        adapter
            .upsert_server(
                "demo",
                &serde_json::json!({"type":"http","url":"https://example.test/mcp"}),
            )
            .unwrap();

        let root = common::read_json_file(&adapter.path).unwrap();
        assert_eq!(root["mcp"]["servers"]["demo"]["type"], "http");
        assert!(root.get("mcpServers").is_none());
        assert_eq!(adapter.read_servers().unwrap().len(), 1);
        assert!(adapter.remove_server("demo").unwrap());
        assert!(common::read_json_file(&adapter.path)
            .unwrap()
            .get("mcp")
            .is_none());
    }
}
