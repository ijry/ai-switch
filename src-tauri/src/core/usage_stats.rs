//! Shared usage-statistics logic for the Tauri commands and the web dispatcher.
//!
//! Both surfaces must expose identical behavior; keeping the implementation here
//! (the pattern established by [`crate::core::sessions`]) means a change cannot
//! land on one and be forgotten on the other.

use crate::error::AppError;
use crate::services::model_pricing;
use crate::services::session_usage_service::{self, SessionUsageStats, TimeWindow};

/// Scan local session transcripts and aggregate usage.
///
/// `since` is an optional RFC 3339 timestamp; entries before it are excluded.
/// `None` reports the full history. Matches the `since` convention used by
/// `get_route_pool` so the UI can reuse its period selector.
pub async fn get_session_usage_stats_core(
    since: Option<String>,
) -> Result<SessionUsageStats, AppError> {
    let window = parse_window(since.as_deref())?;

    // Blocking file IO over a corpus that can reach gigabytes — keep it off the
    // async runtime's worker threads.
    tokio::task::spawn_blocking(move || session_usage_service::scan_session_usage(window))
        .await
        .map_err(|error| AppError::Filesystem {
            code: "filesystem.session_usage_scan_failed",
            message: format!("Failed to scan session usage: {error}"),
            details: None,
            recoverable: true,
        })
}

/// Reload model price overrides from `~/.ai-switch/model-prices.json`.
///
/// Returns the number of entries loaded. A missing file is not an error: it
/// clears any previously loaded overrides and falls back to the built-in table.
pub async fn reload_model_price_overrides_core() -> Result<usize, AppError> {
    let path = crate::paths::AppPaths::resolve()?
        .data_dir
        .join("model-prices.json");

    let contents = match tokio::fs::read_to_string(&path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".to_string(),
        Err(error) => return Err(AppError::from(error)),
    };

    model_pricing::load_overrides_from_str(&contents).map_err(|message| AppError::Validation {
        code: "validation.model_prices_invalid",
        message,
        details: Some(path.to_string_lossy().to_string()),
        recoverable: true,
    })
}

fn parse_window(since: Option<&str>) -> Result<TimeWindow, AppError> {
    let Some(since) = since.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(TimeWindow::default());
    };

    let start = chrono::DateTime::parse_from_rfc3339(since)
        .map(|parsed| parsed.timestamp_millis())
        .map_err(|error| AppError::Validation {
            code: "validation.invalid_timestamp",
            message: format!("`since` must be an RFC 3339 timestamp: {error}"),
            details: Some(since.to_string()),
            recoverable: true,
        })?;

    Ok(TimeWindow {
        start_ms: Some(start),
        end_ms: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_since_scans_full_history() {
        let window = parse_window(None).expect("window");
        assert_eq!(window.start_ms, None);
        assert_eq!(window.end_ms, None);
    }

    #[test]
    fn blank_since_is_treated_as_unbounded() {
        // The UI sends null for the "all time" period, but an empty string
        // arriving over the web transport must not be rejected.
        let window = parse_window(Some("   ")).expect("window");
        assert_eq!(window.start_ms, None);
    }

    #[test]
    fn since_is_parsed_as_rfc3339() {
        let window = parse_window(Some("2026-08-20T00:00:00Z")).expect("window");
        assert_eq!(
            window.start_ms,
            Some(
                chrono::DateTime::parse_from_rfc3339("2026-08-20T00:00:00Z")
                    .unwrap()
                    .timestamp_millis()
            )
        );
    }

    #[test]
    fn invalid_since_is_rejected_rather_than_ignored() {
        // Silently treating a bad timestamp as "all time" would inflate the
        // numbers shown for a narrow period.
        let error = parse_window(Some("last tuesday")).expect_err("must reject");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.invalid_timestamp",
                ..
            }
        ));
    }
}
