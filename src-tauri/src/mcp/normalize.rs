//! Canonical MCP spec normalization.
//!
//! Adapted from xintaofei/codeg (Apache-2.0).

use serde_json::{Map, Value};

use super::model::McpAppType;
use crate::error::AppError;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Validation {
        code: "mcp.invalid_spec",
        message: message.into(),
        details: None,
        recoverable: true,
    }
}

pub fn normalize_mcp_type(value: &str) -> Option<&'static str> {
    match value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "")
        .as_str()
    {
        "stdio" | "command" => Some("stdio"),
        "sse" | "serversentevents" => Some("sse"),
        "http" | "streamablehttp" | "streamable" => Some("http"),
        _ => None,
    }
}

pub fn canonicalize_spec(spec: &Value, source: &str) -> Result<Value, AppError> {
    let Some(object) = spec.as_object() else {
        return Err(invalid(format!("{source}: MCP spec must be a JSON object")));
    };

    let explicit_type = object
        .get("type")
        .or_else(|| object.get("transport"))
        .and_then(Value::as_str)
        .and_then(normalize_mcp_type);
    let inferred = if object.get("command").and_then(Value::as_str).is_some() {
        Some("stdio")
    } else if object.get("url").and_then(Value::as_str).is_some() {
        Some("http")
    } else {
        None
    };
    let typ = explicit_type
        .or(inferred)
        .ok_or_else(|| invalid(format!("{source}: MCP spec needs type, command, or url")))?;

    let mut output = object.clone();
    output.insert("type".to_string(), Value::String(typ.to_string()));
    output.remove("transport");

    match typ {
        "stdio" => {
            let command = output
                .get("command")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| invalid(format!("{source}: stdio MCP spec needs command")))?;
            output.insert("command".to_string(), Value::String(command.to_string()));
            if let Some(args) = output.get_mut("args") {
                let Some(array) = args.as_array_mut() else {
                    return Err(invalid(format!("{source}: args must be an array")));
                };
                array.retain(|value| {
                    value
                        .as_str()
                        .map(|text| !text.trim().is_empty())
                        .unwrap_or(false)
                });
                for value in array {
                    if let Some(text) = value.as_str() {
                        *value = Value::String(text.trim().to_string());
                    }
                }
            }
        }
        "http" | "sse" => {
            let url = output
                .get("url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| invalid(format!("{source}: remote MCP spec needs url")))?;
            output.insert("url".to_string(), Value::String(url.to_string()));
            if typ == "sse" && output.get("url").is_none() {
                return Err(invalid(format!("{source}: SSE MCP spec needs url")));
            }
        }
        _ => unreachable!(),
    }

    Ok(Value::Object(output))
}

pub fn app_can_host_spec(app: McpAppType, spec: &Value) -> bool {
    !(app == McpAppType::Codex && spec.get("type").and_then(Value::as_str) == Some("sse"))
}

pub fn json_to_toml(value: &Value) -> Option<toml::Value> {
    match value {
        Value::Null => None,
        Value::Bool(value) => Some(toml::Value::Boolean(*value)),
        Value::Number(value) => value
            .as_i64()
            .map(toml::Value::Integer)
            .or_else(|| value.as_f64().map(toml::Value::Float)),
        Value::String(value) => Some(toml::Value::String(value.clone())),
        Value::Array(values) => Some(toml::Value::Array(
            values.iter().filter_map(json_to_toml).collect(),
        )),
        Value::Object(values) => Some(toml::Value::Table(
            values
                .iter()
                .filter_map(|(key, value)| json_to_toml(value).map(|value| (key.clone(), value)))
                .collect(),
        )),
    }
}

pub fn toml_to_json(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(value) => Value::String(value.clone()),
        toml::Value::Integer(value) => Value::Number((*value).into()),
        toml::Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml::Value::Boolean(value) => Value::Bool(*value),
        toml::Value::Datetime(value) => Value::String(value.to_string()),
        toml::Value::Array(values) => Value::Array(values.iter().map(toml_to_json).collect()),
        toml::Value::Table(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), toml_to_json(value)))
                .collect::<Map<String, Value>>(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_stdio_from_command_shape() {
        let spec = canonicalize_spec(
            &serde_json::json!({"command":" npx ","args":[" -y ","server"]}),
            "test",
        )
        .unwrap();
        assert_eq!(spec["type"], "stdio");
        assert_eq!(spec["command"], "npx");
        assert_eq!(spec["args"], serde_json::json!(["-y", "server"]));
    }

    #[test]
    fn rejects_codex_sse() {
        assert!(!app_can_host_spec(
            McpAppType::Codex,
            &serde_json::json!({"type":"sse","url":"https://example.test"})
        ));
    }
}
