//! Grok MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct GrokAdapter {
    pub path: PathBuf,
}

impl Default for GrokAdapter {
    fn default() -> Self {
        Self {
            path: common::env_path("GROK_HOME", common::home_dir().join(".grok"))
                .join("config.toml"),
        }
    }
}

fn grok_entry(value: &toml::Value) -> Result<Value, AppError> {
    let object = super::super::normalize::toml_to_json(value)
        .as_object()
        .cloned()
        .unwrap_or_default();
    let typ = object
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| object.get("url").map(|_| "http".to_string()))
        .unwrap_or_else(|| "stdio".to_string());
    let mut output = object;
    output.insert("type".to_string(), Value::String(typ));
    super::super::normalize::canonicalize_spec(&Value::Object(output), "Grok")
}

impl GrokAdapter {
    fn canonical_to_entry(spec: &Value) -> Result<toml::Value, AppError> {
        let canonical = super::super::normalize::canonicalize_spec(spec, "Grok write")?;
        let object = canonical.as_object().unwrap();
        let typ = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("stdio");
        let mut output = Map::new();
        if typ == "stdio" {
            for key in ["command", "args", "env", "cwd", "enabled", "required"] {
                if let Some(value) = object.get(key) {
                    output.insert(key.to_string(), value.clone());
                }
            }
        } else {
            if let Some(url) = object.get("url") {
                output.insert("url".to_string(), url.clone());
            }
            if typ == "sse" {
                output.insert("type".to_string(), Value::String("sse".to_string()));
            }
            if let Some(headers) = object.get("headers") {
                output.insert("headers".to_string(), headers.clone());
            }
        }
        super::super::normalize::json_to_toml(&Value::Object(output)).ok_or_else(|| {
            AppError::Validation {
                code: "mcp.config_invalid",
                message: "Could not convert MCP entry to Grok TOML".to_string(),
                details: None,
                recoverable: true,
            }
        })
    }
}

impl McpClientAdapter for GrokAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::Grok
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        let root = common::read_toml_file(&self.path)?;
        let Some(servers) = root.get("mcp_servers").and_then(toml::Value::as_table) else {
            return Ok(BTreeMap::new());
        };
        servers
            .iter()
            .map(|(id, value)| grok_entry(value).map(|value| (id.clone(), value)))
            .collect()
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        let mut root = common::read_toml_file(&self.path)?;
        let table = root.as_table_mut().unwrap();
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
        common::write_toml_file(&self.path, &root)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        if !self.path.exists() {
            return Ok(false);
        }
        let mut root = common::read_toml_file(&self.path)?;
        let Some(servers) = root
            .get_mut("mcp_servers")
            .and_then(toml::Value::as_table_mut)
        else {
            return Ok(false);
        };
        let removed = servers.remove(id).is_some();
        if removed {
            common::write_toml_file(&self.path, &root)?;
        }
        Ok(removed)
    }
}
