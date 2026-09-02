use crate::app_state::AppState;
use crate::core::usage_overview::get_usage_overview_core;
use crate::core::usage_stats::{get_session_usage_stats_core, reload_model_price_overrides_core};
use crate::error::ApiError;
use crate::services::session_usage_service::SessionUsageStats;
use crate::services::usage_overview_service::UsageOverview;
use tauri::State;

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

/// Merge local CLI transcript usage with proxied requests into one deduplicated
/// list, with window-wide totals and per-dimension groups.
///
/// Spans every platform: the figures answer "my total spend", so a per-platform
/// view comes from the returned groups rather than from a filter.
// `rename_all` because `page_size` travels over the wire as-is: the web
// dispatcher reads that exact key, and the default camelCase rewrite would look
// for `pageSize`, quietly fall back to the default page size on the desktop, and
// leave the two transports disagreeing.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_usage_overview(
    state: State<'_, AppState>,
    since: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<UsageOverview, ApiError> {
    get_usage_overview_core(&state.pool, since, page, page_size)
        .await
        .map_err(ApiError::from)
}
