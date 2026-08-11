//! Cross-client MCP orchestration.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::error::AppError;

use super::clients::McpClientAdapter;
use super::clients::{
    claude_code::ClaudeCodeAdapter, cline::ClineAdapter, code_buddy::CodeBuddyAdapter,
    codex::CodexAdapter, cursor::CursorAdapter, gemini::GeminiAdapter, grok::GrokAdapter,
    hermes::HermesAdapter, kimi_code::KimiCodeAdapter, openclaw::OpenClawAdapter,
    opencode::OpenCodeAdapter,
};
use super::model::{LocalMcpServer, McpAppType};
use super::normalize::{app_can_host_spec, canonicalize_spec};

fn invalid(code: &'static str, message: impl Into<String>) -> AppError {
    AppError::Validation {
        code,
        message: message.into(),
        details: None,
        recoverable: true,
    }
}

fn adapters() -> Vec<Box<dyn McpClientAdapter>> {
    vec![
        Box::new(ClaudeCodeAdapter::default()),
        Box::new(CodexAdapter::default()),
        Box::new(GeminiAdapter::default()),
        Box::new(OpenClawAdapter::default()),
        Box::new(OpenCodeAdapter::default()),
        Box::new(HermesAdapter::default()),
        Box::new(ClineAdapter::default()),
        Box::new(CursorAdapter::default()),
        Box::new(KimiCodeAdapter::default()),
        Box::new(CodeBuddyAdapter::default()),
        Box::new(GrokAdapter::default()),
    ]
}

pub fn scan_local() -> Result<Vec<LocalMcpServer>, AppError> {
    scan_with_adapters(&adapters())
}

pub(crate) fn scan_with_adapters(
    adapters: &[Box<dyn McpClientAdapter>],
) -> Result<Vec<LocalMcpServer>, AppError> {
    let mut grouped: BTreeMap<String, (Value, BTreeSet<McpAppType>)> = BTreeMap::new();
    for adapter in adapters {
        let entries = adapter.read_servers()?;
        for (id, spec) in entries {
            let normalized = canonicalize_spec(&spec, &format!("{} MCP config", adapter.app()))?;
            let entry = grouped
                .entry(id)
                .or_insert_with(|| (normalized.clone(), BTreeSet::new()));
            entry.1.insert(adapter.app());
        }
    }

    Ok(grouped
        .into_iter()
        .map(|(id, (spec, apps))| LocalMcpServer {
            id,
            spec,
            apps: apps.into_iter().collect(),
        })
        .collect())
}

pub(crate) fn preflight_apps(
    apps: &[McpAppType],
    spec: &Value,
) -> Result<Vec<McpAppType>, AppError> {
    let compatible = apps
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|app| app_can_host_spec(*app, spec))
        .collect::<Vec<_>>();
    if compatible.is_empty() {
        return Err(invalid(
            "mcp.no_compatible_client",
            "None of the selected clients can host this MCP transport",
        ));
    }
    Ok(compatible)
}

pub fn upsert_local_server(
    id: String,
    spec: Value,
    apps: Vec<McpAppType>,
) -> Result<LocalMcpServer, AppError> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(invalid(
            "mcp.invalid_server_id",
            "MCP server id is required",
        ));
    }
    let canonical = canonicalize_spec(&spec, "MCP save")?;
    let selected = preflight_apps(&apps, &canonical)?;
    for adapter in adapters() {
        if selected.contains(&adapter.app()) {
            adapter.upsert_server(&id, &canonical)?;
        } else {
            let _ = adapter.remove_server(&id)?;
        }
    }
    scan_local()?
        .into_iter()
        .find(|server| server.id == id)
        .ok_or_else(|| {
            invalid(
                "mcp.config_invalid",
                "MCP server was written but could not be reloaded",
            )
        })
}

pub fn set_server_apps(
    id: String,
    apps: Vec<McpAppType>,
) -> Result<Option<LocalMcpServer>, AppError> {
    let current = scan_local()?
        .into_iter()
        .find(|server| server.id == id)
        .ok_or_else(|| invalid("mcp.server_not_found", "MCP server was not found"))?;
    let selected = if apps.is_empty() {
        Vec::new()
    } else {
        preflight_apps(&apps, &current.spec)?
    };
    for adapter in adapters() {
        if selected.contains(&adapter.app()) {
            adapter.upsert_server(&id, &current.spec)?;
        } else {
            let _ = adapter.remove_server(&id)?;
        }
    }
    Ok(scan_local()?.into_iter().find(|server| server.id == id))
}

pub fn remove_server(id: String, apps: Option<Vec<McpAppType>>) -> Result<bool, AppError> {
    let selected = apps.unwrap_or_else(|| McpAppType::ALL.to_vec());
    let mut removed = false;
    for adapter in adapters() {
        if selected.contains(&adapter.app()) {
            removed |= adapter.remove_server(&id)?;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_rejects_codex_sse() {
        let error = preflight_apps(
            &[McpAppType::Codex],
            &serde_json::json!({"type":"sse","url":"https://example.test"}),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AppError::Validation {
                code: "mcp.no_compatible_client",
                ..
            }
        ));
    }
}
