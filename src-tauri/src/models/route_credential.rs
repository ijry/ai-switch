use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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
    pub preview_json: Option<String>,
    pub batch_id: Option<String>,
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
}
