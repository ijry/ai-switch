//! OpenCode MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct OpenCodeAdapter {
    pub path: PathBuf,
}

impl Default for OpenCodeAdapter {
    fn default() -> Self {
        Self {
            path: common::home_dir().join(".config/opencode/opencode.json"),
        }
    }
}

fn old_to_canonical(value: &Value) -> Result<Value, AppError> {
    let Some(object) = value.as_object() else {
        return Err(AppError::Validation {
            code: "mcp.config_invalid",
            message: "OpenCode MCP entry must be an object".to_string(),
            details: None,
            recoverable: true,
        });
    };
    let typ = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("local");
    if typ == "local" {
        let command = object
            .get("command")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = object
            .get("command")
            .and_then(Value::as_array)
            .map(|items| items.iter().skip(1).cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut result = Map::new();
        result.insert("type".to_string(), Value::String("stdio".to_string()));
        result.insert("command".to_string(), Value::String(command.to_string()));
        result.insert("args".to_string(), Value::Array(args));
        if let Some(environment) = object.get("environment") {
            result.insert("env".to_string(), environment.clone());
        }
        crate::mcp::normalize::canonicalize_spec(&Value::Object(result), "OpenCode")
    } else {
        let mut result = object.clone();
        result.insert(
            "type".to_string(),
            Value::String(if typ == "sse" { "sse" } else { "http" }.to_string()),
        );
        crate::mcp::normalize::canonicalize_spec(&Value::Object(result), "OpenCode")
    }
}

impl McpClientAdapter for OpenCodeAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::OpenCode
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        let root = common::read_json_file(&self.path)?;
        let mut result = common::read_json_servers(&self.path, "mcpServers", "OpenCode")?;
        if let Some(servers) = root.get("mcp").and_then(Value::as_object) {
            for (id, spec) in servers {
                if !result.contains_key(id) {
                    result.insert(id.clone(), old_to_canonical(spec)?);
                }
            }
        }
        Ok(result)
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        let mut root = common::read_json_file(&self.path)?;
        if !root.is_object() {
            root = Value::Object(Map::new());
        }
        let object = root.as_object_mut().unwrap();
        if object
            .get("mcpServers")
            .map(Value::is_object)
            .unwrap_or(false)
        {
            object
                .get_mut("mcpServers")
                .and_then(Value::as_object_mut)
                .unwrap()
                .insert(
                    id.to_string(),
                    crate::mcp::normalize::canonicalize_spec(spec, "OpenCode write")?,
                );
        } else {
            if !object.get("mcp").map(Value::is_object).unwrap_or(false) {
                object.insert("mcp".to_string(), Value::Object(Map::new()));
            }
            let canonical = crate::mcp::normalize::canonicalize_spec(spec, "OpenCode write")?;
            let typ = canonical
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("stdio");
            let mut entry = Map::new();
            if typ == "stdio" {
                entry.insert("type".to_string(), Value::String("local".to_string()));
                let mut command = vec![canonical
                    .get("command")
                    .cloned()
                    .unwrap_or(Value::String(String::new()))];
                command.extend(
                    canonical
                        .get("args")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                );
                entry.insert("command".to_string(), Value::Array(command));
                if let Some(env) = canonical.get("env") {
                    entry.insert("environment".to_string(), env.clone());
                }
            } else {
                entry.insert("type".to_string(), Value::String(typ.to_string()));
                if let Some(url) = canonical.get("url") {
                    entry.insert("url".to_string(), url.clone());
                }
                if let Some(headers) = canonical.get("headers") {
                    entry.insert("headers".to_string(), headers.clone());
                }
            }
            object
                .get_mut("mcp")
                .and_then(Value::as_object_mut)
                .unwrap()
                .insert(id.to_string(), Value::Object(entry));
        }
        common::write_json_file(&self.path, &root)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        let mut removed = common::remove_json_server(&self.path, "mcpServers", id)?;
        if self.path.exists() {
            let mut root = common::read_json_file(&self.path)?;
            if let Some(servers) = root.get_mut("mcp").and_then(Value::as_object_mut) {
                removed |= servers.remove(id).is_some();
            }
            if removed {
                common::write_json_file(&self.path, &root)?;
            }
        }
        Ok(removed)
    }
}
