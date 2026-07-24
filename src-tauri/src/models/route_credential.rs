use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub const ANTHROPIC_API_KEY_FIELD: &str = "ANTHROPIC_API_KEY";
pub const ANTHROPIC_AUTH_TOKEN_FIELD: &str = "ANTHROPIC_AUTH_TOKEN";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct RouteCredential {
    pub id: String,
    pub platform: String,
    pub kind: String,
    pub display_name: String,
    pub email: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub batch_id: Option<String>,
    pub secret_payload_json: String,
    pub config_json: String,
    pub preview_json: String,
    pub subscription_type: Option<String>,
    pub primary_remain: Option<i64>,
    pub weekly_remain: Option<i64>,
    pub reset_primary: Option<String>,
    pub reset_weekly: Option<String>,
    // Legacy single-window fields kept for existing DBs/migrations.
    pub quota_remaining: Option<i64>,
    pub quota_limit: Option<i64>,
    pub quota_used: Option<i64>,
    pub quota_updated_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateApiRouteCredentialInput {
    pub platform: String,
    pub display_name: String,
    pub api_key: String,
    pub base_url: String,
    pub interface_format: String,
    pub model_mappings_json: String, // JSON array
    #[serde(default)]
    pub api_key_field: Option<String>,
    pub preview_json: Option<String>,
    pub batch_id: Option<String>,
    #[serde(default)]
    pub responses_custom_tool_compat: Option<bool>,
    #[serde(default)]
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateRouteCredentialInput {
    pub display_name: String,
    pub email: Option<String>,
    pub status: String,
    pub secret_payload_json: String,
    pub config_json: String,
    pub preview_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportOfficialTextInput {
    pub platform: String,
    pub text: String,
    pub batch_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportOfficialFilesInput {
    pub platform: String,
    pub file_paths: Vec<String>,
    pub batch_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialImportFailure {
    pub label: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialImportResult {
    pub imported: Vec<RouteCredential>,
    pub failed: Vec<RouteCredentialImportFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelMapping {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_1m: Option<bool>,
}

pub fn normalize_anthropic_api_key_field(value: Option<&str>) -> Result<&'static str, String> {
    match value.map(str::trim).filter(|item| !item.is_empty()) {
        None => Ok(ANTHROPIC_API_KEY_FIELD),
        Some(ANTHROPIC_API_KEY_FIELD) => Ok(ANTHROPIC_API_KEY_FIELD),
        Some(ANTHROPIC_AUTH_TOKEN_FIELD) => Ok(ANTHROPIC_AUTH_TOKEN_FIELD),
        Some(other) => Err(format!(
            "Unsupported Anthropic api_key_field: {other}. Expected {ANTHROPIC_API_KEY_FIELD} or {ANTHROPIC_AUTH_TOKEN_FIELD}"
        )),
    }
}
