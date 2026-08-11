//! Per-client MCP configuration adapters.

use std::collections::BTreeMap;

use serde_json::Value;

use super::model::McpAppType;
use crate::error::AppError;

pub mod claude_code;
pub mod cline;
pub mod code_buddy;
pub mod codex;
pub mod common;
pub mod cursor;
pub mod gemini;
pub mod grok;
pub mod hermes;
pub mod kimi_code;
pub mod openclaw;
pub mod opencode;

pub trait McpClientAdapter: Send + Sync {
    fn app(&self) -> McpAppType;
    fn read_servers(&self) -> Result<BTreeMap<String, Value>, AppError>;
    fn upsert_server(&self, id: &str, spec: &Value) -> Result<(), AppError>;
    fn remove_server(&self, id: &str) -> Result<bool, AppError>;
}
