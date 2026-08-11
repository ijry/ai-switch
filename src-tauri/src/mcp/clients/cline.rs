//! Cline MCP configuration.

use super::super::model::McpAppType;
use super::{common, McpClientAdapter};
use crate::error::AppError;
use serde_json::{Map, Value};
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
        let mut root = common::read_json_file(&self.path)?;
        if !root.is_object() {
            root = Value::Object(Map::new());
        }
        let object = root.as_object_mut().expect("object initialized above");
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
            .ok_or_else(|| AppError::Validation {
                code: "mcp.config_invalid",
                message: "Cline mcpServers must be an object".to_string(),
                details: None,
                recoverable: true,
            })?
            .insert(id.to_string(), cline_spec(spec)?);
        common::write_json_file(&self.path, &root)
    }
    fn remove_server(&self, id: &str) -> Result<bool, AppError> {
        common::remove_json_server(&self.path, "mcpServers", id)
    }
}

fn cline_spec(spec: &Value) -> Result<Value, AppError> {
    let mut canonical = crate::mcp::normalize::canonicalize_spec(spec, "Cline write")?;
    if canonical.get("type").and_then(Value::as_str) == Some("http") {
        canonical["type"] = Value::String("streamableHttp".to_string());
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_streamable_http_literal_expected_by_cline() {
        let value = cline_spec(&serde_json::json!({
            "type": "http",
            "url": "https://example.test/mcp"
        }))
        .unwrap();
        assert_eq!(value["type"], "streamableHttp");
        assert_eq!(
            crate::mcp::normalize::canonicalize_spec(&value, "test").unwrap()["type"],
            "http"
        );
    }
}
