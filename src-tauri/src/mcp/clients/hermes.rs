//! Hermes Agent MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct HermesAdapter {
    pub path: PathBuf,
}

impl Default for HermesAdapter {
    fn default() -> Self {
        Self {
            path: common::env_path("HERMES_HOME", common::home_dir().join(".hermes"))
                .join("config.yaml"),
        }
    }
}

impl HermesAdapter {
    fn yaml_to_canonical(value: &serde_yaml::Value) -> Result<Value, AppError> {
        let mut object = serde_json::to_value(value)
            .map_err(|error| AppError::Validation {
                code: "mcp.config_invalid",
                message: "Invalid Hermes MCP entry".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            })?
            .as_object()
            .cloned()
            .unwrap_or_default();
        let typ = if object.get("command").is_some() {
            "stdio"
        } else if object
            .get("transport")
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("sse"))
        {
            "sse"
        } else {
            "http"
        };
        object.insert("type".to_string(), Value::String(typ.to_string()));
        object.remove("transport");
        super::super::normalize::canonicalize_spec(&Value::Object(object), "Hermes")
    }

    fn canonical_to_yaml(value: &Value) -> Result<serde_yaml::Value, AppError> {
        let canonical = super::super::normalize::canonicalize_spec(value, "Hermes write")?;
        let object = canonical.as_object().unwrap();
        let typ = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("stdio");
        let mut output = Map::new();
        if typ == "stdio" {
            for key in ["command", "args", "env", "enabled", "required"] {
                if let Some(value) = object.get(key) {
                    output.insert(key.to_string(), value.clone());
                }
            }
        } else {
            if let Some(url) = object.get("url") {
                output.insert("url".to_string(), url.clone());
            }
            if typ == "sse" {
                output.insert("transport".to_string(), Value::String("sse".to_string()));
            }
            if let Some(headers) = object.get("headers") {
                output.insert("headers".to_string(), headers.clone());
            }
        }
        for (key, value) in object {
            if matches!(
                key.as_str(),
                "type" | "command" | "args" | "env" | "cwd" | "url" | "headers" | "transport"
            ) {
                continue;
            }
            if !value.is_null() {
                output.insert(key.to_string(), value.clone());
            }
        }
        serde_yaml::to_value(Value::Object(output)).map_err(|error| AppError::Validation {
            code: "mcp.config_invalid",
            message: "Could not serialize Hermes MCP entry".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })
    }
}

impl McpClientAdapter for HermesAdapter {
    fn app(&self) -> McpAppType {
        McpAppType::Hermes
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        let root = common::read_yaml_file(&self.path)?;
        let Some(servers) = root
            .get("mcp_servers")
            .and_then(serde_yaml::Value::as_mapping)
        else {
            return Ok(BTreeMap::new());
        };
        let mut result = BTreeMap::new();
        for (id, value) in servers {
            if let Some(id) = id.as_str() {
                result.insert(id.to_string(), Self::yaml_to_canonical(value)?);
            }
        }
        Ok(result)
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        let mut root = common::read_yaml_file(&self.path)?;
        if !root.is_mapping() {
            root = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
        }
        let mapping = root.as_mapping_mut().unwrap();
        let key = serde_yaml::Value::String("mcp_servers".to_string());
        if !mapping
            .get(&key)
            .map(serde_yaml::Value::is_mapping)
            .unwrap_or(false)
        {
            mapping.insert(
                key.clone(),
                serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
            );
        }
        mapping
            .get_mut(&key)
            .and_then(serde_yaml::Value::as_mapping_mut)
            .unwrap()
            .insert(
                serde_yaml::Value::String(id.to_string()),
                Self::canonical_to_yaml(spec)?,
            );
        common::write_yaml_file(&self.path, &root)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        if !self.path.exists() {
            return Ok(false);
        }
        let mut root = common::read_yaml_file(&self.path)?;
        let key = serde_yaml::Value::String("mcp_servers".to_string());
        let Some(servers) = root
            .get_mut(&key)
            .and_then(serde_yaml::Value::as_mapping_mut)
        else {
            return Ok(false);
        };
        let removed = servers
            .remove(serde_yaml::Value::String(id.to_string()))
            .is_some();
        if removed {
            common::write_yaml_file(&self.path, &root)?;
        }
        Ok(removed)
    }
}
