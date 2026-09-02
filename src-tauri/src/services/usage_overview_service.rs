//! Merge local CLI transcript usage with proxied request rows into one list.
//!
//! The two sources overlap: a CLI request routed through this app's proxy is
//! recorded on both sides. The upstream response id joins them — on a real
//! corpus 2905 of 2933 proxy rows (99.0%) matched a transcript entry. Merging
//! on that key is what lets a single set of totals mean "my total spend"
//! instead of double counting the overlap.

use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::route_pool::ProxyRequestRow;
use crate::services::model_pricing::{self, TokenUsage};
use crate::services::session_usage_service::{self, SessionUsageEntry, TimeWindow};
use crate::services::upstream_response_id::extract_upstream_response_id;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Where a merged row's data came from. Doubles as the "source" grouping key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageRowSource {
    /// Present on both sides: a CLI request that went through this proxy.
    Matched,
    /// Transcript only: the CLI reached the upstream directly.
    SessionOnly,
    /// Proxy only: the caller is not one of the scanned CLIs (model test, or
    /// another tool pointed at this proxy).
    ProxyOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewRow {
    /// Stable id for React keys: the proxy row id, else the response id, else a
    /// synthesized `session:<index>`.
    pub id: String,
    pub source: UsageRowSource,
    /// RFC 3339. Proxy `created_at` for matched and proxy-only rows, the
    /// transcript timestamp for session-only rows.
    pub occurred_at: Option<String>,
    pub provider: String,
    pub model: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub source_label: Option<String>,
    pub path: Option<String>,
    /// HTTP status, only ever present on a row with a proxy side.
    pub status: Option<String>,
    pub success: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_micros: i64,
    /// `upstream` when a real upstream price was used, `estimated` when the
    /// local price table was, `null` when the model has no known rate.
    pub price_source: Option<String>,
    pub upstream_response_id: Option<String>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewTotals {
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_micros: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewGroupRow {
    /// Display label: the model id, platform id, account name, or source name.
    pub key: String,
    #[serde(flatten)]
    pub totals: UsageOverviewTotals,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewGroups {
    pub by_model: Vec<UsageOverviewGroupRow>,
    pub by_platform: Vec<UsageOverviewGroupRow>,
    pub by_account: Vec<UsageOverviewGroupRow>,
    pub by_source: Vec<UsageOverviewGroupRow>,
}

/// Facts the UI needs to state how complete the totals are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewIntegrity {
    pub scanned_file_count: i64,
    /// True when the transcript file cap was hit, so the totals are a floor.
    pub truncated: bool,
    /// Requests whose model has no rate, contributing no cost.
    pub unpriced_request_count: i64,
    /// Requests priced from the local table rather than an upstream price.
    pub estimated_price_request_count: i64,
    /// Proxy rows with no response id, which therefore could not be merged and
    /// may double count against a transcript entry for the same request.
    pub unmatchable_proxy_row_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverview {
    pub totals: UsageOverviewTotals,
    pub rows: Vec<UsageOverviewRow>,
    pub groups: UsageOverviewGroups,
    pub row_count: i64,
    pub page: i64,
    pub page_size: i64,
    pub integrity: UsageOverviewIntegrity,
}

/// Merge the two sides on the upstream response id.
///
/// A row with no id on either side stays unmerged: a missing key is not
/// evidence of a shared request, and treating it as one would collapse
/// unrelated requests into a single row.
pub fn merge_entries(
    session_entries: Vec<SessionUsageEntry>,
    proxy_rows: Vec<ProxyRequestRow>,
) -> Vec<UsageOverviewRow> {
    // Index the proxy side by response id, falling back to parsing the stored
    // body preview for rows written before the column existed.
    let mut proxy_by_id: HashMap<String, ProxyRequestRow> = HashMap::new();
    let mut unkeyed_proxy_rows = Vec::new();
    for row in proxy_rows {
        match resolve_proxy_response_id(&row) {
            Some(id) => {
                proxy_by_id.insert(id, row);
            }
            None => unkeyed_proxy_rows.push(row),
        }
    }

    let mut rows = Vec::new();
    for (index, entry) in session_entries.into_iter().enumerate() {
        // Blank is filtered on both sides: a present-but-empty id is not a key,
        // and letting it act as one would merge every id-less row into a single
        // request. The proxy side does the same in `resolve_proxy_response_id`.
        let paired = entry
            .response_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .and_then(|id| proxy_by_id.remove(id));
        rows.push(match paired {
            Some(proxy) => merged_row(entry, proxy),
            None => session_only_row(entry, index),
        });
    }

    // Whatever the transcripts never claimed is proxy-only: a model test, or a
    // tool other than the two scanned CLIs pointed at this proxy.
    for row in proxy_by_id.into_values().chain(unkeyed_proxy_rows) {
        rows.push(proxy_only_row(row));
    }

    rows.sort_by(|left, right| right.occurred_at.cmp(&left.occurred_at));
    rows
}

fn resolve_proxy_response_id(row: &ProxyRequestRow) -> Option<String> {
    if let Some(id) = row
        .upstream_response_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return Some(id.to_string());
    }
    // Pre-migration rows: the id may still be inside the stored preview.
    let metadata = serde_json::from_str::<serde_json::Value>(&row.metadata_json).ok()?;
    let body = metadata.get("response_body")?.as_str()?;
    extract_upstream_response_id(body.as_bytes())
}

struct ProxyFacts {
    path: Option<String>,
    status: Option<String>,
    success: bool,
    model: Option<String>,
}

fn proxy_facts(row: &ProxyRequestRow) -> ProxyFacts {
    let metadata = serde_json::from_str::<serde_json::Value>(&row.metadata_json).ok();
    let field = |key: &str| -> Option<String> {
        let value = metadata.as_ref()?.get(key)?;
        match value {
            serde_json::Value::String(text) if !text.trim().is_empty() => {
                Some(text.trim().to_string())
            }
            serde_json::Value::Number(number) => Some(number.to_string()),
            _ => None,
        }
    };
    ProxyFacts {
        path: field("path"),
        status: field("status"),
        // Absent `success` means a legacy row that only recorded successes.
        success: metadata
            .as_ref()
            .and_then(|value| value.get("success"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        model: field("upstream_model").or_else(|| field("requested_model")),
    }
}

/// The upstream's own price in USD micros, when it reported one.
fn upstream_cost_micros(row: &ProxyRequestRow) -> Option<i64> {
    if row.price_source.as_deref() != Some("upstream") {
        return None;
    }
    match row.price_currency.as_deref() {
        Some("usd") => row.price_usd_micros,
        Some("cny") => row
            .price_cny_micros
            .map(model_pricing::cny_micros_to_usd_micros),
        _ => None,
    }
}

fn merged_row(entry: SessionUsageEntry, proxy: ProxyRequestRow) -> UsageOverviewRow {
    let facts = proxy_facts(&proxy);
    // An upstream price is real billing data; a local estimate is a guess.
    let (cost_micros, price_source) = match upstream_cost_micros(&proxy) {
        Some(cost) => (cost, Some("upstream".to_string())),
        None => estimated_cost(&entry.model, entry.usage),
    };
    UsageOverviewRow {
        id: proxy.id,
        source: UsageRowSource::Matched,
        occurred_at: Some(proxy.created_at),
        provider: entry.provider.to_string(),
        // The transcript records what the CLI itself used, and its cache split
        // is finer than the proxy's single combined figure.
        model: entry.model,
        account_id: proxy.account_id,
        account_name: proxy.account_name,
        source_label: Some(proxy.source_label),
        path: facts.path,
        status: facts.status,
        success: facts.success,
        input_tokens: entry.usage.input_tokens.max(0),
        output_tokens: entry.usage.output_tokens.max(0),
        cache_write_tokens: entry.usage.cache_write_tokens.max(0),
        cache_read_tokens: entry.usage.cache_read_tokens.max(0),
        cost_micros,
        price_source,
        upstream_response_id: entry.response_id,
        metadata_json: Some(proxy.metadata_json),
    }
}

fn session_only_row(entry: SessionUsageEntry, index: usize) -> UsageOverviewRow {
    let (cost_micros, price_source) = estimated_cost(&entry.model, entry.usage);
    UsageOverviewRow {
        id: entry
            .response_id
            .clone()
            .unwrap_or_else(|| format!("session:{index}")),
        source: UsageRowSource::SessionOnly,
        occurred_at: entry.timestamp_ms.and_then(rfc3339_from_millis),
        provider: entry.provider.to_string(),
        model: entry.model,
        account_id: None,
        account_name: None,
        source_label: None,
        path: None,
        // A transcript has no HTTP status; an entry exists only for a request
        // that returned usage, so it succeeded.
        status: None,
        success: true,
        input_tokens: entry.usage.input_tokens.max(0),
        output_tokens: entry.usage.output_tokens.max(0),
        cache_write_tokens: entry.usage.cache_write_tokens.max(0),
        cache_read_tokens: entry.usage.cache_read_tokens.max(0),
        cost_micros,
        price_source,
        upstream_response_id: entry.response_id,
        metadata_json: None,
    }
}

fn proxy_only_row(row: ProxyRequestRow) -> UsageOverviewRow {
    let facts = proxy_facts(&row);
    let model = facts.model.clone().unwrap_or_else(|| "unknown".to_string());
    let usage = TokenUsage {
        input_tokens: row.input_tokens.unwrap_or(0).max(0),
        output_tokens: row.output_tokens.unwrap_or(0).max(0),
        // The proxy stores one combined cache figure; attributing it to reads
        // is the cheaper of the two rates, so an estimate stays a lower bound.
        cache_write_tokens: 0,
        cache_read_tokens: row.cache_tokens.unwrap_or(0).max(0),
    };
    let (cost_micros, price_source) = match upstream_cost_micros(&row) {
        Some(cost) => (cost, Some("upstream".to_string())),
        None => estimated_cost(&model, usage),
    };
    UsageOverviewRow {
        id: row.id,
        source: UsageRowSource::ProxyOnly,
        occurred_at: Some(row.created_at),
        provider: row.platform.clone(),
        model,
        account_id: row.account_id,
        account_name: row.account_name,
        source_label: Some(row.source_label),
        path: facts.path,
        status: facts.status,
        success: facts.success,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        cost_micros,
        price_source,
        upstream_response_id: row.upstream_response_id,
        metadata_json: Some(row.metadata_json),
    }
}

/// Price from the local table. A `None` source means the model has no known
/// rate, so the row reads as unpriced rather than free.
fn estimated_cost(model: &str, usage: TokenUsage) -> (i64, Option<String>) {
    match model_pricing::estimate_cost_micros(model, usage) {
        Some(cost) => (cost, Some("estimated".to_string())),
        None => (0, None),
    }
}

fn rfc3339_from_millis(millis: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(millis).map(|value| value.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_entry(response_id: Option<&str>, model: &str, input: i64) -> SessionUsageEntry {
        SessionUsageEntry {
            provider: "claude",
            model: model.to_string(),
            response_id: response_id.map(str::to_string),
            timestamp_ms: Some(1_787_000_000_000),
            usage: TokenUsage {
                input_tokens: input,
                output_tokens: 20,
                cache_write_tokens: 5,
                cache_read_tokens: 7,
            },
        }
    }

    fn proxy_row(response_id: Option<&str>) -> ProxyRequestRow {
        ProxyRequestRow {
            id: "proxy-1".to_string(),
            platform: "claude".to_string(),
            account_id: Some("cred-1".to_string()),
            account_name: Some("Team Account".to_string()),
            source_label: "route_proxy".to_string(),
            metadata_json: r#"{"path":"/v1/messages","status":200,"success":true,"upstream_model":"claude-opus-5"}"#.to_string(),
            created_at: "2026-08-19T14:04:50Z".to_string(),
            // Deliberately different from the session entry so the field
            // precedence is observable.
            input_tokens: Some(999),
            output_tokens: Some(999),
            cache_tokens: Some(999),
            price_usd_micros: Some(4_200),
            price_cny_micros: None,
            price_currency: Some("usd".to_string()),
            price_source: Some("upstream".to_string()),
            upstream_response_id: response_id.map(str::to_string),
        }
    }

    #[test]
    fn a_matched_pair_becomes_one_row() {
        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![proxy_row(Some("msg_a"))],
        );

        assert_eq!(rows.len(), 1, "the overlap must not be counted twice");
        assert_eq!(rows[0].source, UsageRowSource::Matched);
    }

    #[test]
    fn a_matched_row_takes_tokens_from_the_transcript() {
        // The transcript splits cache into write and read, which price 12.5x
        // apart, and it does not lose the final delta of a truncated stream.
        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![proxy_row(Some("msg_a"))],
        );

        assert_eq!(rows[0].input_tokens, 120);
        assert_eq!(rows[0].output_tokens, 20);
        assert_eq!(rows[0].cache_write_tokens, 5);
        assert_eq!(rows[0].cache_read_tokens, 7);
    }

    #[test]
    fn a_matched_row_takes_account_and_status_from_the_proxy() {
        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![proxy_row(Some("msg_a"))],
        );

        assert_eq!(rows[0].account_name.as_deref(), Some("Team Account"));
        assert_eq!(rows[0].status.as_deref(), Some("200"));
        assert_eq!(rows[0].path.as_deref(), Some("/v1/messages"));
        assert!(rows[0].success);
    }

    #[test]
    fn an_upstream_price_wins_over_a_local_estimate() {
        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![proxy_row(Some("msg_a"))],
        );

        assert_eq!(rows[0].cost_micros, 4_200);
        assert_eq!(rows[0].price_source.as_deref(), Some("upstream"));
    }

    #[test]
    fn a_cny_upstream_price_is_converted_to_usd() {
        let mut row = proxy_row(Some("msg_a"));
        row.price_usd_micros = None;
        row.price_cny_micros = Some(7_100_000);
        row.price_currency = Some("cny".to_string());

        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![row],
        );

        // 7.1 CNY at the fixed 7.1 rate is exactly 1 USD.
        assert_eq!(rows[0].cost_micros, 1_000_000);
    }

    #[test]
    fn a_matched_row_without_an_upstream_price_is_estimated_from_transcript_tokens() {
        let mut row = proxy_row(Some("msg_a"));
        row.price_usd_micros = None;
        row.price_cny_micros = None;
        row.price_currency = None;
        row.price_source = None;

        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 1_000_000)],
            vec![row],
        );

        assert_eq!(rows[0].price_source.as_deref(), Some("estimated"));
        // Priced off the transcript's 1M input tokens, not the proxy's 999.
        assert!(
            rows[0].cost_micros > 1_000_000,
            "1M input tokens must cost more than $1, got {}",
            rows[0].cost_micros
        );
    }

    #[test]
    fn unmatched_rows_from_each_side_are_kept_and_labelled() {
        let rows = merge_entries(
            vec![session_entry(Some("msg_only_session"), "claude-opus-5", 10)],
            vec![proxy_row(Some("msg_only_proxy"))],
        );

        assert_eq!(rows.len(), 2);
        let sources: Vec<UsageRowSource> = rows.iter().map(|row| row.source).collect();
        assert!(sources.contains(&UsageRowSource::SessionOnly));
        assert!(sources.contains(&UsageRowSource::ProxyOnly));
    }

    #[test]
    fn rows_without_a_response_id_never_merge_with_each_other() {
        // Two id-less rows are not evidence of the same request. Treating a
        // missing key as a shared key would collapse unrelated requests and
        // undercount spend.
        let rows = merge_entries(
            vec![session_entry(None, "claude-opus-5", 10)],
            vec![proxy_row(None)],
        );

        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn a_blank_response_id_is_not_treated_as_a_key() {
        // An empty string is a present-but-useless id. If it were allowed
        // through, every blank-id row on both sides would collapse into one
        // request, and merging two unrelated requests undercounts spend.
        let rows = merge_entries(
            vec![
                session_entry(Some("   "), "claude-opus-5", 10),
                session_entry(Some(""), "claude-opus-5", 10),
            ],
            vec![proxy_row(Some(""))],
        );

        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.source != UsageRowSource::Matched));
    }

    #[test]
    fn every_id_less_proxy_row_survives_as_its_own_row() {
        // Id-less proxy rows are held in a list, not a map: keying them by a
        // placeholder would let the second one overwrite the first and silently
        // drop real spend from the totals.
        let mut first = proxy_row(None);
        first.id = "proxy-a".to_string();
        let mut second = proxy_row(None);
        second.id = "proxy-b".to_string();

        let rows = merge_entries(Vec::new(), vec![first, second]);

        assert_eq!(rows.len(), 2);
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert!(ids.contains(&"proxy-a") && ids.contains(&"proxy-b"));
    }

    #[test]
    fn a_proxy_row_falls_back_to_parsing_its_stored_body_for_an_id() {
        // Pre-migration rows have no upstream_response_id column value; the id
        // is still recoverable from the stored response preview.
        let mut row = proxy_row(None);
        row.metadata_json = r#"{"path":"/v1/messages","status":200,"success":true,"response_body":"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_legacy\"}}\n\n"}"#.to_string();

        let rows = merge_entries(
            vec![session_entry(Some("msg_legacy"), "claude-opus-5", 120)],
            vec![row],
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, UsageRowSource::Matched);
    }

    #[test]
    fn rows_are_ordered_newest_first() {
        let mut older = session_entry(Some("msg_old"), "claude-opus-5", 10);
        older.timestamp_ms = Some(1_786_000_000_000);
        let mut newer = session_entry(Some("msg_new"), "claude-opus-5", 10);
        newer.timestamp_ms = Some(1_788_000_000_000);

        let rows = merge_entries(vec![older, newer], Vec::new());

        assert_eq!(rows[0].upstream_response_id.as_deref(), Some("msg_new"));
    }
}
