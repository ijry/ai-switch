//! Tauri command boundary for MCP settings.

use serde_json::Value;

use super::marketplace;
use super::model::{
    LocalMcpServer, McpAppType, McpMarketplaceItem, McpMarketplaceProvider,
    McpMarketplaceServerDetail,
};
use super::service;
use crate::error::ApiError;

#[tauri::command]
pub async fn mcp_scan_local() -> Result<Vec<LocalMcpServer>, ApiError> {
    service::scan_local().map_err(ApiError::from)
}

#[tauri::command]
pub async fn mcp_list_marketplaces() -> Result<Vec<McpMarketplaceProvider>, ApiError> {
    Ok(marketplace::list_marketplaces().await)
}

#[tauri::command]
pub async fn mcp_search_marketplace(
    provider_id: String,
    query: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<McpMarketplaceItem>, ApiError> {
    marketplace::search(provider_id, query, limit)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn mcp_get_marketplace_server_detail(
    provider_id: String,
    server_id: String,
) -> Result<McpMarketplaceServerDetail, ApiError> {
    marketplace::get_detail(provider_id, server_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn mcp_install_from_marketplace(
    provider_id: String,
    server_id: String,
    apps: Vec<McpAppType>,
    option_id: Option<String>,
    protocol: Option<String>,
    parameter_values: Option<Value>,
) -> Result<LocalMcpServer, ApiError> {
    marketplace::install(
        provider_id,
        server_id,
        apps,
        option_id,
        protocol,
        parameter_values,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
pub async fn mcp_upsert_local_server(
    server_id: String,
    spec: Value,
    apps: Vec<McpAppType>,
) -> Result<LocalMcpServer, ApiError> {
    service::upsert_local_server(server_id, spec, apps).map_err(ApiError::from)
}

#[tauri::command]
pub async fn mcp_set_server_apps(
    server_id: String,
    apps: Vec<McpAppType>,
) -> Result<Option<LocalMcpServer>, ApiError> {
    service::set_server_apps(server_id, apps).map_err(ApiError::from)
}

#[tauri::command]
pub async fn mcp_remove_server(
    server_id: String,
    apps: Option<Vec<McpAppType>>,
) -> Result<bool, ApiError> {
    service::remove_server(server_id, apps).map_err(ApiError::from)
}
