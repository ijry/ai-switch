use serde::{Deserialize, Serialize};

use crate::services::route_config_service::RouteConfigWriteOutcome;
use crate::services::route_proxy_service::RouteProxyStatus;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RouteProxyHttpsConfig {
    #[serde(default)]
    pub enabled: bool,
}

impl Default for RouteProxyHttpsConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RouteProxyTrustStatus {
    SystemTrusted,
    NssTrusted,
    PartiallyTrusted,
    Untrusted,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RouteProxyTrustRecord {
    pub status: RouteProxyTrustStatus,
    pub adapter: Option<String>,
    pub message: Option<String>,
    #[serde(default)]
    pub manual_instructions: Vec<String>,
}

impl Default for RouteProxyTrustRecord {
    fn default() -> Self {
        Self {
            status: RouteProxyTrustStatus::Untrusted,
            adapter: None,
            message: None,
            manual_instructions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteProxyHttpsStatus {
    pub enabled: bool,
    pub cert_ready: bool,
    pub trust_status: RouteProxyTrustStatus,
    pub trust_adapter: Option<String>,
    pub root_fingerprint: Option<String>,
    pub expires_at: Option<String>,
    pub certificate_dir: String,
    pub root_certificate_path: Option<String>,
    pub proxy_base_url: Option<String>,
    pub message: Option<String>,
    pub manual_instructions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteProxyHttpsOperationOutcome {
    pub https: RouteProxyHttpsStatus,
    pub route_proxy: RouteProxyStatus,
    pub config_writes: Vec<RouteConfigWriteOutcome>,
}
