use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::mcp::{McpServer, NewMcpServer, SetMcpServerEnabledRequest};
use crate::services::mcp_service::McpService;
use tauri::State;

#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServer>, ApiError> {
    McpService::list_mcp_servers(&state.pool)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_mcp_server(
    state: State<'_, AppState>,
    request: NewMcpServer,
) -> Result<McpServer, ApiError> {
    McpService::create_mcp_server(&state.pool, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn set_mcp_server_enabled(
    state: State<'_, AppState>,
    request: SetMcpServerEnabledRequest,
) -> Result<McpServer, ApiError> {
    McpService::set_mcp_server_enabled(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
