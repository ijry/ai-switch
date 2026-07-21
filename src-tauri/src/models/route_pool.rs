use serde::{Deserialize, Serialize};

// Pool public fields keep stable names for UI compatibility.
// Selected/member ids are route_credentials.id values, not official_accounts.id.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePoolUsageLog {
    pub id: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub source_label: String,
    pub metric_type: String,
    pub amount: i64,
    pub unit: String,
    pub metadata_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePoolStats {
    pub member_count: i64,
    pub request_count: i64,
    pub token_count: i64,
    pub cost_micros: i64,
    pub recent_logs: Vec<RoutePoolUsageLog>,
    pub requests: Vec<RoutePoolUsageLog>,
    pub request_row_count: i64,
    pub request_page: i64,
    pub request_page_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePoolState {
    pub platform: String,
    /// Selected route_credentials.id values for this platform pool.
    pub account_ids: Vec<String>,
    pub stats: RoutePoolStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetRoutePoolMembersInput {
    pub platform: String,
    /// route_credentials.id values to set as pool members.
    pub account_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePoolRouteRequest {
    pub platform: String,
    pub token_count: Option<i64>,
    pub cost_micros: Option<i64>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePoolRouteOutcome {
    pub platform: String,
    /// Selected route_credentials.id.
    pub selected_account_id: String,
    pub selected_account_name: String,
    pub stats: RoutePoolStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePoolModelTestRequest {
    pub platform: String,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePoolModelTestOutcome {
    pub platform: String,
    /// Selected route_credentials.id.
    pub selected_account_id: String,
    pub selected_account_name: String,
    pub interface_format: String,
    pub request_path: String,
    pub base_url: Option<String>,
    pub target_url: Option<String>,
    pub request_body_json: String,
    pub response_status: Option<u16>,
    pub response_body: String,
    pub response_text: Option<String>,
    pub error_message: Option<String>,
    pub success: bool,
    pub duration_ms: i64,
    pub stats: RoutePoolStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteModelsFetchRequest {
    pub base_url: String,
    pub api_key: String,
    #[serde(default)]
    pub interface_format: Option<String>,
    #[serde(default)]
    pub api_key_field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FetchedRouteModel {
    pub id: String,
    #[serde(default)]
    pub owned_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_1m: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePoolMemberAccount {
    /// route_credentials.id
    pub id: String,
    pub display_name: String,
}
