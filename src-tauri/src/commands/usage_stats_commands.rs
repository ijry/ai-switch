use crate::core::usage_stats::{get_session_usage_stats_core, reload_model_price_overrides_core};
use crate::error::ApiError;
use crate::services::session_usage_service::SessionUsageStats;

/// Aggregate token usage and estimated cost from local Claude Code and Codex CLI
/// session transcripts.
///
/// `since` is an optional RFC 3339 timestamp; `None` reports the full history.
#[tauri::command]
pub async fn get_session_usage_stats(since: Option<String>) -> Result<SessionUsageStats, ApiError> {
    get_session_usage_stats_core(since)
        .await
        .map_err(ApiError::from)
}

/// Reload model price overrides from `~/.ai-switch/model-prices.json` and return
/// how many entries were loaded.
#[tauri::command]
pub async fn reload_model_price_overrides() -> Result<usize, ApiError> {
    reload_model_price_overrides_core()
        .await
        .map_err(ApiError::from)
}
