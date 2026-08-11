//! Shared MCP config file helpers.
//!
//! Adapted from xintaofei/codeg (Apache-2.0).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::AppError;

use super::super::model::McpAppType;
use super::McpClientAdapter;

fn validation(code: &'static str, message: impl Into<String>, details: Option<String>) -> AppError {
    AppError::Validation {
        code,
        message: message.into(),
        details,
        recoverable: true,
    }
}

fn filesystem(code: &'static str, message: impl Into<String>, details: Option<String>) -> AppError {
    AppError::Filesystem {
        code,
        message: message.into(),
        details,
        recoverable: true,
    }
}

pub fn read_text_file(path: &Path) -> Result<Option<String>, AppError> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(filesystem(
            "mcp.config_io",
            format!("Could not read MCP configuration at {}", path.display()),
            Some(error.to_string()),
        )),
    }
}

pub fn read_json_file(path: &Path) -> Result<Value, AppError> {
    let Some(raw) = read_text_file(path)? else {
        return Ok(Value::Object(Map::new()));
    };
    serde_json::from_str(&raw).map_err(|error| {
        validation(
            "mcp.config_invalid",
            format!("Invalid JSON configuration at {}", path.display()),
            Some(error.to_string()),
        )
    })
}

pub fn read_toml_file(path: &Path) -> Result<toml::Value, AppError> {
    let Some(raw) = read_text_file(path)? else {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    };
    raw.parse::<toml::Value>().map_err(|error| {
        validation(
            "mcp.config_invalid",
            format!("Invalid TOML configuration at {}", path.display()),
            Some(error.to_string()),
        )
    })
}

pub fn read_yaml_file(path: &Path) -> Result<serde_yaml::Value, AppError> {
    let Some(raw) = read_text_file(path)? else {
        return Ok(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    };
    serde_yaml::from_str(&raw).map_err(|error| {
        validation(
            "mcp.config_invalid",
            format!("Invalid YAML configuration at {}", path.display()),
            Some(error.to_string()),
        )
    })
}

pub fn write_text_file(path: &Path, text: &str) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| {
        filesystem(
            "mcp.config_io",
            "MCP configuration has no parent directory",
            None,
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        filesystem(
            "mcp.config_io",
            format!(
                "Could not create MCP configuration directory {}",
                parent.display()
            ),
            Some(error.to_string()),
        )
    })?;

    let temp = parent.join(format!(
        ".{}.ai-switch-{}",
        path.file_name().and_then(|v| v.to_str()).unwrap_or("mcp"),
        uuid::Uuid::new_v4()
    ));
    fs::write(&temp, text).map_err(|error| {
        filesystem(
            "mcp.config_io",
            "Could not write temporary MCP configuration",
            Some(error.to_string()),
        )
    })?;

    if let Err(error) = fs::rename(&temp, path) {
        if path.exists() {
            fs::remove_file(path).map_err(|remove_error| {
                filesystem(
                    "mcp.config_io",
                    "Could not replace MCP configuration",
                    Some(remove_error.to_string()),
                )
            })?;
            fs::rename(&temp, path).map_err(|rename_error| {
                filesystem(
                    "mcp.config_io",
                    "Could not replace MCP configuration",
                    Some(rename_error.to_string()),
                )
            })?;
        } else {
            let _ = fs::remove_file(&temp);
            return Err(filesystem(
                "mcp.config_io",
                "Could not replace MCP configuration",
                Some(error.to_string()),
            ));
        }
    }
    Ok(())
}

pub fn write_json_file(path: &Path, value: &Value) -> Result<(), AppError> {
    let serialized = serde_json::to_string_pretty(value).map_err(|error| {
        validation(
            "mcp.config_invalid",
            "Could not serialize MCP JSON",
            Some(error.to_string()),
        )
    })?;
    write_text_file(path, &format!("{serialized}\n"))
}

pub fn write_toml_file(path: &Path, value: &toml::Value) -> Result<(), AppError> {
    let serialized = toml::to_string_pretty(value).map_err(|error| {
        validation(
            "mcp.config_invalid",
            "Could not serialize MCP TOML",
            Some(error.to_string()),
        )
    })?;
    write_text_file(path, &format!("{serialized}\n"))
}

pub fn write_yaml_file(path: &Path, value: &serde_yaml::Value) -> Result<(), AppError> {
    let serialized = serde_yaml::to_string(value).map_err(|error| {
        validation(
            "mcp.config_invalid",
            "Could not serialize MCP YAML",
            Some(error.to_string()),
        )
    })?;
    write_text_file(path, &serialized)
}

pub fn home_dir() -> std::path::PathBuf {
    directories::BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

pub fn env_path(name: &str, fallback: std::path::PathBuf) -> std::path::PathBuf {
    let Some(value) = std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return fallback;
    };
    if value == "~" {
        return home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    std::path::PathBuf::from(value)
}

pub fn read_json_servers(
    path: &Path,
    key: &str,
    source: &str,
) -> Result<BTreeMap<String, Value>, AppError> {
    let root = read_json_file(path)?;
    let Some(servers) = root.get(key).and_then(Value::as_object) else {
        return Ok(BTreeMap::new());
    };
    let mut result = BTreeMap::new();
    for (id, spec) in servers {
        match crate::mcp::normalize::canonicalize_spec(spec, &format!("{source} MCP entry {id}")) {
            Ok(value) => {
                result.insert(id.clone(), value);
            }
            Err(error) => eprintln!("[MCP] skipping invalid {source} entry {id}: {error}"),
        }
    }
    Ok(result)
}

pub fn upsert_json_server(
    path: &Path,
    key: &str,
    id: &str,
    spec: &Value,
    source: &str,
) -> Result<(), AppError> {
    let mut root = read_json_file(path)?;
    if !root.is_object() {
        root = Value::Object(Map::new());
    }
    let object = root
        .as_object_mut()
        .ok_or_else(|| invalid_json_root(path))?;
    if !object.get(key).map(Value::is_object).unwrap_or(false) {
        object.insert(key.to_string(), Value::Object(Map::new()));
    }
    let servers = object
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid_json_root(path))?;
    servers.insert(
        id.to_string(),
        crate::mcp::normalize::canonicalize_spec(spec, source)?,
    );
    write_json_file(path, &root)
}

pub fn remove_json_server(path: &Path, key: &str, id: &str) -> Result<bool, AppError> {
    if !path.exists() {
        return Ok(false);
    }
    let mut root = read_json_file(path)?;
    let Some(object) = root.as_object_mut() else {
        return Ok(false);
    };
    let Some(servers) = object.get_mut(key).and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let removed = servers.remove(id).is_some();
    if removed {
        write_json_file(path, &root)?;
    }
    Ok(removed)
}

fn invalid_json_root(path: &Path) -> AppError {
    AppError::Validation {
        code: "mcp.config_invalid",
        message: format!("Invalid JSON root at {}", path.display()),
        details: None,
        recoverable: true,
    }
}

pub struct JsonMcpAdapter {
    app: McpAppType,
    path: std::path::PathBuf,
    key: &'static str,
    source: &'static str,
}

impl JsonMcpAdapter {
    pub fn new(
        app: McpAppType,
        path: std::path::PathBuf,
        key: &'static str,
        source: &'static str,
    ) -> Self {
        Self {
            app,
            path,
            key,
            source,
        }
    }
}

impl McpClientAdapter for JsonMcpAdapter {
    fn app(&self) -> McpAppType {
        self.app
    }
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError> {
        read_json_servers(&self.path, self.key, self.source)
    }
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError> {
        upsert_json_server(&self.path, self.key, id, spec, self.source)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        remove_json_server(&self.path, self.key, id)
    }
}

pub fn set_local_plugin(path: &Path, id: &str, enabled: bool) -> Result<(), AppError> {
    let mut root = read_json_file(path)?;
    if !root.is_object() {
        root = Value::Object(Map::new());
    }
    let object = root
        .as_object_mut()
        .ok_or_else(|| invalid_json_root(path))?;
    if !object
        .get("enabledPlugins")
        .map(Value::is_object)
        .unwrap_or(false)
    {
        object.insert("enabledPlugins".to_string(), Value::Object(Map::new()));
    }
    let plugins = object
        .get_mut("enabledPlugins")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid_json_root(path))?;
    let key = format!("{id}@local");
    if enabled {
        plugins.insert(key, Value::Bool(true));
    } else {
        plugins.remove(&key);
    }
    write_json_file(path, &root)
}
