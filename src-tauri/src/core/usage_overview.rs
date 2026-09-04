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
///
/// `utc_offset_minutes` is the *caller's* offset east of UTC, used to slice the
/// chart's buckets. The caller has to supply it: the window start is computed
/// from the client's own calendar (its midnight, its start-of-week), so bucketing
/// with the server's offset instead cut the first and last bucket at a different
/// instant and shifted every hour label. That is invisible on the desktop, where
/// both clocks are the same machine, and wrong for every browser and paired phone
/// in another timezone. `None` falls back to the server's offset, which is right
/// for a caller that has no clock of its own to report.
pub async fn get_usage_overview_core(
    pool: &SqlitePool,
    since: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
    utc_offset_minutes: Option<i32>,
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
        normalize_utc_offset(utc_offset_minutes),
    )
    .await
}

/// Rejects an offset no real timezone uses rather than trusting the wire.
///
/// The value comes from a browser, and `FixedOffset::east_opt` refuses anything
/// past ±24h by returning `None` — which would turn a hand-crafted request into a
/// panic at the unwrap, or a silent fallback that is harder to explain than a
/// clamp. Real offsets run from −12:00 to +14:00.
fn normalize_utc_offset(minutes: Option<i32>) -> Option<i32> {
    minutes.filter(|value| (-12 * 60..=14 * 60).contains(value))
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

        let error =
            get_usage_overview_core(&pool, Some("last tuesday".to_string()), None, None, None)
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
