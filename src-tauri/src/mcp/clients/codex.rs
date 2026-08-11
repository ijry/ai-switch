//! Codex MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct CodexAdapter {
    pub path: PathBuf,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self {
            path: common::env_path("CODEX_HOME", common::home_dir().join(".codex"))
                .join("config.toml"),
        }
    }
}

fn toml_entry_to_canonical(id: &str, value: &toml::Value) -> Result<Value, AppError> {
    let Some(table) = value.as_table() else {
        return Err(AppError::Validation {
            code: "mcp.config_invalid",
            message: format!("Codex MCP entry {id} must be a table"),
            details: None,
            recoverable: true,
        });
    };
    let mut object = super::super::normalize::toml_to_json(value)
        .as_object()
        .cloned()
        .unwrap_or_default();
    if object.get("type").is_none() {
        let typ = if object.get("command").is_some() {
            "stdio"
        } else {
            "http"
        };
        object.insert("type".to_string(), Value::String(typ.to_string()));
    }
    if let Some(headers) = table.get("http_headers") {
        object.insert(
            "headers".to_string(),
            super::super::normalize::toml_to_json(headers),
        );
    }
    object.remove("http_headers");
    super::super::normalize::canonicalize_spec(&Value::Object(object), &format!("Codex {id}"))
}

impl CodexAdapter {
    pub fn read_servers_at(path: &std::path::Path) -> Result<BTreeMap<String, Value>, AppError> {
        let root = common::read_toml_file(path)?;
        let Some(table) = root.as_table() else {
            return Ok(BTreeMap::new());
        };
        let mut result = BTreeMap::new();
        if let Some(servers) = table.get("mcp_servers").and_then(toml::Value::as_table) {
            for (id, value) in servers {
                result.insert(id.clone(), toml_entry_to_canonical(id, value)?);
            }
        }
        if let Some(servers) = table
            .get("mcp")
            .and_then(toml::Value::as_table)
            .and_then(|mcp| mcp.get("servers"))
            .and_then(toml::Value::as_table)
        {
            for (id, value) in servers {
                if !result.contains_key(id) {
                    result.insert(id.clone(), toml_entry_to_canonical(id, value)?);
                }
            }
        }
        Ok(result)
    }

    fn canonical_to_entry(spec: &Value) -> Result<toml::Value, AppError> {
        let canonical = super::super::normalize::canonicalize_spec(spec, "Codex write")?;
        let object = canonical.as_object().unwrap();
        let typ = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("stdio");
        if typ == "sse" {
            return Err(AppError::Validation {
                code: "mcp.unsupported_transport",
                message: "Codex does not support SSE MCP servers".to_string(),
                details: None,
                recoverable: true,
            });
        }
        let mut output = Map::new();
        if typ == "stdio" {
            for key in ["command", "args", "env", "cwd"] {
                if let Some(value) = object.get(key) {
                    output.insert(key.to_string(), value.clone());
                }
            }
        } else {
            if let Some(url) = object.get("url") {
                output.insert("url".to_string(), url.clone());
            }
            if let Some(headers) = object.get("headers") {
                output.insert("http_headers".to_string(), headers.clone());
            }
        }
        super::super::normalize::json_to_toml(&Value::Object(output)).ok_or_else(|| {
            AppError::Validation {
                code: "mcp.config_invalid",
                message: "Could not convert MCP entry to Codex TOML".to_string(),
                details: None,
                recoverable: true,
            }
        })
    }
}

impl McpClientAdapter for CodexAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::Codex
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        Self::read_servers_at(&self.path)
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        let mut root = common::read_toml_file(&self.path)?;
        let table = root.as_table_mut().ok_or_else(|| AppError::Validation {
            code: "mcp.config_invalid",
            message: "Codex config root must be a table".to_string(),
            details: None,
            recoverable: true,
        })?;
        if !table
            .get("mcp_servers")
            .map(toml::Value::is_table)
            .unwrap_or(false)
        {
            table.insert(
                "mcp_servers".to_string(),
                toml::Value::Table(toml::map::Map::new()),
            );
        }
        table
            .get_mut("mcp_servers")
            .and_then(toml::Value::as_table_mut)
            .unwrap()
            .insert(id.to_string(), Self::canonical_to_entry(spec)?);
        if let Some(mcp) = table.get_mut("mcp").and_then(toml::Value::as_table_mut) {
            if let Some(servers) = mcp.get_mut("servers").and_then(toml::Value::as_table_mut) {
                servers.remove(id);
            }
            if mcp
                .get("servers")
                .and_then(toml::Value::as_table)
                .map(|servers| servers.is_empty())
                .unwrap_or(false)
            {
                mcp.remove("servers");
            }
            if mcp.is_empty() {
                table.remove("mcp");
            }
        }
        common::write_toml_file(&self.path, &root)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        if !self.path.exists() {
            return Ok(false);
        }
        let mut root = common::read_toml_file(&self.path)?;
        let table = root.as_table_mut().unwrap();
        let mut removed = false;
        if let Some(servers) = table
            .get_mut("mcp_servers")
            .and_then(toml::Value::as_table_mut)
        {
            removed |= servers.remove(id).is_some();
        }
        if let Some(servers) = table
            .get_mut("mcp")
            .and_then(toml::Value::as_table_mut)
            .and_then(|mcp| mcp.get_mut("servers"))
            .and_then(toml::Value::as_table_mut)
        {
            removed |= servers.remove(id).is_some();
        }
        if removed {
            common::write_toml_file(&self.path, &root)?;
        }
        Ok(removed)
    }
}
