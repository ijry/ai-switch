use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub const ANTHROPIC_API_KEY_FIELD: &str = "ANTHROPIC_API_KEY";
pub const ANTHROPIC_AUTH_TOKEN_FIELD: &str = "ANTHROPIC_AUTH_TOKEN";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq)]
pub struct RouteCredential {
    pub id: String,
    pub platform: String,
    pub kind: String,
    pub display_name: String,
    pub email: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub route_priority: i64,
    pub max_concurrency: i64,
    pub batch_id: Option<String>,
    #[sqlx(default)]
    pub batch_name: Option<String>,
    pub secret_payload_json: String,
    pub config_json: String,
    pub preview_json: String,
    pub subscription_type: Option<String>,
    pub primary_remain: Option<i64>,
    pub weekly_remain: Option<i64>,
    pub reset_primary: Option<String>,
    pub reset_weekly: Option<String>,
    #[sqlx(default)]
    pub transient_failure_count: i64,
    #[sqlx(default)]
    pub next_retry_at: Option<String>,
    #[sqlx(default)]
    pub cooldown_until: Option<String>,
    #[sqlx(default)]
    pub last_failure_kind: Option<String>,
    #[sqlx(default)]
    pub last_failure_message: Option<String>,
    #[sqlx(default)]
    pub last_failure_response_json: Option<String>,
    #[sqlx(default)]
    pub active_request_count: i64,
    #[sqlx(default)]
    pub request_count: i64,
    #[sqlx(default)]
    pub success_count: i64,
    #[sqlx(default)]
    pub failure_count: i64,
    #[sqlx(default)]
    pub success_rate: Option<f64>,
    // Legacy single-window fields kept for existing DBs/migrations.
    pub quota_remaining: Option<i64>,
    pub quota_limit: Option<i64>,
    pub quota_used: Option<i64>,
    pub quota_updated_at: Option<String>,
    pub archived_at: Option<String>,
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
    pub route_priority: i64,
    pub max_concurrency: i64,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteCredentialImportResult {
    pub imported: Vec<RouteCredential>,
    pub failed: Vec<RouteCredentialImportFailure>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteCredentialPoolScope {
    InPool,
    OutOfPool,
    Archived,
}

impl Default for RouteCredentialPoolScope {
    fn default() -> Self {
        Self::OutOfPool
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialPageRequest {
    pub platform: String,
    pub page: i64,
    pub page_size: i64,
    #[serde(default)]
    pub filters: Vec<String>,
    #[serde(default)]
    pub pool_scope: RouteCredentialPoolScope,
}

impl RouteCredentialPageRequest {
    pub fn normalized_page_size(&self) -> Result<i64, String> {
        match self.page_size {
            20 | 50 | 100 => Ok(self.page_size),
            _ => Err("page_size must be 20, 50, or 100".to_string()),
        }
    }

    pub fn normalized_page(&self) -> i64 {
        self.page.max(1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialFilterOption {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteCredentialPage {
    pub items: Vec<RouteCredential>,
    pub total: i64,
    pub page: i64,
    pub page_count: i64,
    pub page_size: i64,
    pub previous_page_account_id: Option<String>,
    pub next_page_account_id: Option<String>,
    pub filter_options: Vec<RouteCredentialFilterOption>,
    pub official_account_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReorderRouteCredentialInput {
    pub platform: String,
    pub moved_account_id: String,
    pub previous_account_id: Option<String>,
    pub next_account_id: Option<String>,
    #[serde(default)]
    pub filters: Vec<String>,
    #[serde(default)]
    pub pool_scope: RouteCredentialPoolScope,
    pub page_size: i64,
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
