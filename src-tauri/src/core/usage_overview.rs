//! Shared usage-overview logic for the Tauri commands and the web dispatcher.
//!
//! Mirrors [`crate::core::usage_stats`]: keeping the implementation here means a
//! change cannot land on one surface and be forgotten on the other.

use crate::error::AppError;
use crate::services::usage_overview_service::{self, UsageOverview};
use sqlx::SqlitePool;

const DEFAULT_PAGE: i64 = 1;
const DEFAULT_PAGE_SIZE: i64 = 20;
const MAX_PAGE_SIZE: i64 = 100;

/// Merge transcript usage with proxied requests and return one page of the
/// combined list plus window-wide totals and groups.
///
/// `since` is an optional RFC 3339 timestamp, matching `get_route_pool` and
/// `get_session_usage_stats` so the UI can reuse its period selector.
pub async fn get_usage_overview_core(
    pool: &SqlitePool,
    since: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<UsageOverview, AppError> {
    let window = super::usage_stats::parse_window(since.as_deref())?;
    let (page, page_size) = normalize_pagination(page, page_size);
    usage_overview_service::build_usage_overview(
        pool,
        since
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        page,
        page_size,
        window,
    )
    .await
}

/// Clamp paging into a usable range rather than rejecting it: a stale page
/// number from the UI should show an empty page, not an error dialog.
fn normalize_pagination(page: Option<i64>, page_size: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(DEFAULT_PAGE).max(1);
    let page_size = page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    (page, page_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_defaults_match_the_previous_request_list() {
        assert_eq!(normalize_pagination(None, None), (1, 20));
    }

    #[test]
    fn pagination_clamps_rather_than_rejecting() {
        // A stale page number or a hand-crafted web request must not error out.
        assert_eq!(normalize_pagination(Some(0), Some(0)), (1, 1));
        assert_eq!(normalize_pagination(Some(-5), Some(9_999)), (1, 100));
    }

    #[tokio::test]
    async fn an_invalid_since_is_rejected_rather_than_widened() {
        // Silently treating a bad timestamp as "all time" would inflate the
        // figures shown for a narrow period.
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let error = get_usage_overview_core(&pool, Some("last tuesday".to_string()), None, None)
            .await
            .expect_err("must reject");

        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.invalid_timestamp",
                ..
            }
        ));
    }
}
