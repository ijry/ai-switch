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
use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, Offset, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{HashMap, VecDeque};

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

/// How wide one trend bucket is. Chosen from the window span so a chart never
/// has to render hundreds of bars: an hour of a day reads, a day of a decade
/// does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageBucketUnit {
    Hour,
    Day,
    Week,
    Month,
}

impl Default for UsageBucketUnit {
    fn default() -> Self {
        Self::Day
    }
}

/// One column of the trend chart: a time slice plus its totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageTrendBucket {
    /// Bucket start, RFC 3339 in the offset the bucketing used.
    pub start: String,
    /// Short axis label — `14:00`, `09-03`, `2026-09`.
    pub label: String,
    /// The full slice in words, for a tooltip heading.
    pub title: String,
    #[serde(flatten)]
    pub totals: UsageOverviewTotals,
}

/// One stacked series: a group key and its per-bucket token counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageTrendRow {
    /// Display label, or [`TREND_OTHER_KEY`] on the folded tail row.
    pub key: String,
    /// Input + output tokens per bucket, positionally aligned to `buckets`.
    ///
    /// Cache tokens are left out on purpose: on a real Claude corpus cache
    /// reads outnumber real input by an order of magnitude, so including them
    /// would draw a chart about caching rather than about work done. The
    /// per-bucket totals still carry them for the tooltip.
    pub tokens: Vec<i64>,
}

/// The same four dimensions as [`UsageOverviewGroups`], sliced over time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageTrendSeries {
    pub unit: UsageBucketUnit,
    pub buckets: Vec<UsageTrendBucket>,
    pub by_model: Vec<UsageTrendRow>,
    pub by_platform: Vec<UsageTrendRow>,
    pub by_account: Vec<UsageTrendRow>,
    pub by_source: Vec<UsageTrendRow>,
    /// Rows with no usable timestamp. They count in `totals` but sit in no
    /// bucket, so the chart has to say they are missing rather than imply its
    /// bars add up to the summary cards.
    pub undated_request_count: i64,
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
    /// Successful proxy rows with no response id. These could not be merged, so
    /// a transcript entry for the same request is still counted separately.
    ///
    /// Counts successes only: a failed request produced no assistant message,
    /// so no transcript entry exists to double count against. On a real corpus
    /// 707 of 709 id-less rows were failures — counting them would have put an
    /// alarming figure in front of the user for a risk that does not exist.
    pub unmatchable_proxy_row_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverview {
    pub totals: UsageOverviewTotals,
    pub rows: Vec<UsageOverviewRow>,
    pub groups: UsageOverviewGroups,
    pub series: UsageTrendSeries,
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
    // body preview for rows written before the column existed. Ids are not
    // unique: a bridge that has to synthesize one uses a constant, so each id
    // holds a queue and a session entry consumes one row rather than the last
    // writer silently winning and the rest vanishing from every total.
    let mut proxy_by_id: HashMap<String, VecDeque<ProxyRequestRow>> = HashMap::new();
    let mut unkeyed_proxy_rows = Vec::new();
    for row in proxy_rows {
        match resolve_proxy_response_id(&row) {
            Some(id) => proxy_by_id.entry(id).or_default().push_back(row),
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
            .and_then(|id| take_proxy_row(&mut proxy_by_id, id));
        rows.push(match paired {
            Some(proxy) => merged_row(entry, proxy),
            None => session_only_row(entry, index),
        });
    }

    // Whatever the transcripts never claimed is proxy-only: a model test, or a
    // tool other than the two scanned CLIs pointed at this proxy.
    for row in proxy_by_id
        .into_values()
        .flatten()
        .chain(unkeyed_proxy_rows)
    {
        rows.push(proxy_only_row(row));
    }

    // `id` breaks ties: the leftovers above come out of a HashMap in arbitrary
    // order, and pagination re-runs this merge per page, so without a total
    // order a row could show up on two pages or on none.
    rows.sort_by(|left, right| {
        right
            .occurred_at
            .cmp(&left.occurred_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    rows
}

/// Take one proxy row recorded under `id`, dropping the id from the index once
/// its last row has been claimed.
fn take_proxy_row(
    index: &mut HashMap<String, VecDeque<ProxyRequestRow>>,
    id: &str,
) -> Option<ProxyRequestRow> {
    let queue = index.get_mut(id)?;
    let row = queue.pop_front();
    if queue.is_empty() {
        index.remove(id);
    }
    row
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

impl UsageOverviewTotals {
    fn absorb(&mut self, row: &UsageOverviewRow) {
        self.request_count += 1;
        self.input_tokens += row.input_tokens;
        self.output_tokens += row.output_tokens;
        self.cache_write_tokens += row.cache_write_tokens;
        self.cache_read_tokens += row.cache_read_tokens;
        self.cost_micros += row.cost_micros;
    }
}

/// Totals over every row in the window — never over one page, or the summary
/// cards would change as the user pages through the list.
pub fn summarize(rows: &[UsageOverviewRow]) -> UsageOverviewTotals {
    let mut totals = UsageOverviewTotals::default();
    for row in rows {
        totals.absorb(row);
    }
    totals
}

/// Label for the "source" grouping dimension.
fn source_label(source: UsageRowSource) -> &'static str {
    match source {
        UsageRowSource::Matched => "匹配",
        UsageRowSource::SessionOnly => "仅会话",
        UsageRowSource::ProxyOnly => "仅代理",
    }
}

/// Bucket for rows with no owning account. Most merged rows are transcript-only,
/// so this needs a real label rather than an empty cell.
const NO_ACCOUNT_LABEL: &str = "未经代理";

fn group_by<'a, F>(rows: &'a [UsageOverviewRow], key: F) -> Vec<UsageOverviewGroupRow>
where
    F: Fn(&'a UsageOverviewRow) -> String,
{
    let mut buckets: HashMap<String, UsageOverviewTotals> = HashMap::new();
    for row in rows {
        buckets.entry(key(row)).or_default().absorb(row);
    }
    let mut grouped: Vec<UsageOverviewGroupRow> = buckets
        .into_iter()
        .map(|(key, totals)| UsageOverviewGroupRow { key, totals })
        .collect();
    // Highest spend first, then by request count so unpriced groups still
    // order sensibly, then by key for a stable result.
    grouped.sort_by(|left, right| {
        right
            .totals
            .cost_micros
            .cmp(&left.totals.cost_micros)
            .then_with(|| right.totals.request_count.cmp(&left.totals.request_count))
            .then_with(|| left.key.cmp(&right.key))
    });
    grouped
}

/// All four dimensions at once: their cardinality is small (single to double
/// digits), so computing them together avoids a refetch when the user flips the
/// segmented control.
pub fn group_all(rows: &[UsageOverviewRow]) -> UsageOverviewGroups {
    UsageOverviewGroups {
        by_model: group_by(rows, model_key),
        by_platform: group_by(rows, platform_key),
        by_account: group_by(rows, account_key),
        by_source: group_by(rows, source_key),
    }
}

fn model_key(row: &UsageOverviewRow) -> String {
    row.model.clone()
}

fn platform_key(row: &UsageOverviewRow) -> String {
    row.provider.clone()
}

fn account_key(row: &UsageOverviewRow) -> String {
    row.account_name
        .clone()
        .or_else(|| row.account_id.clone())
        .unwrap_or_else(|| NO_ACCOUNT_LABEL.to_string())
}

fn source_key(row: &UsageOverviewRow) -> String {
    source_label(row.source).to_string()
}

/// The widest a trend series gets before the next coarser unit takes over.
///
/// Sized for a chart in a side panel: 48 bars still read at a glance, and it
/// keeps a month of days (31) and a day of hours (25) at their natural unit.
const MAX_TREND_BUCKETS: i64 = 48;

/// Hard stop on bucket generation, only reachable with a corpus spanning
/// decades — where month is already the coarsest unit on offer.
const TREND_BUCKET_CEILING: usize = 400;

/// Series a chart can colour apart before hues start repeating. The tail folds
/// into one row rather than being dropped or handed an invented colour.
const TREND_SERIES_LIMIT: usize = 8;

/// Label of the folded tail row.
pub const TREND_OTHER_KEY: &str = "其他";

/// Where the chart's x-axis starts and ends, and in which offset to slice it.
///
/// The offset is passed in rather than assumed UTC: a bucket boundary has to be
/// local midnight, or the chart's "today" column disagrees with the 当日 window
/// the user selected.
#[derive(Debug, Clone, Copy)]
pub struct TrendFrame {
    /// Window start, or `None` for all time — then the earliest row starts it.
    pub start_ms: Option<i64>,
    /// Window end, normally now.
    pub end_ms: i64,
    pub offset: FixedOffset,
}

fn unit_span_ms(unit: UsageBucketUnit) -> i64 {
    match unit {
        UsageBucketUnit::Hour => 3_600_000,
        UsageBucketUnit::Day => 86_400_000,
        UsageBucketUnit::Week => 604_800_000,
        // Only used to rule the unit out, so a nominal 30 days is close enough.
        UsageBucketUnit::Month => 2_592_000_000,
    }
}

/// Finest unit that keeps the bar count readable.
fn choose_unit(span_ms: i64) -> UsageBucketUnit {
    let span = span_ms.max(0);
    [
        UsageBucketUnit::Hour,
        UsageBucketUnit::Day,
        UsageBucketUnit::Week,
    ]
    .into_iter()
    .find(|unit| span / unit_span_ms(*unit) + 1 <= MAX_TREND_BUCKETS)
    .unwrap_or(UsageBucketUnit::Month)
}

/// Truncate an instant down to the start of its bucket.
///
/// A fixed offset has no DST gaps, so the local datetime always resolves; the
/// fallback only exists to keep a year outside chrono's range from panicking.
fn bucket_start(unit: UsageBucketUnit, at: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
    let offset = *at.offset();
    let naive = match unit {
        UsageBucketUnit::Hour => at.date_naive().and_hms_opt(at.hour(), 0, 0),
        UsageBucketUnit::Day => at.date_naive().and_hms_opt(0, 0, 0),
        UsageBucketUnit::Week => {
            let since_monday = i64::from(at.weekday().num_days_from_monday());
            (at.date_naive() - Duration::days(since_monday)).and_hms_opt(0, 0, 0)
        }
        UsageBucketUnit::Month => at
            .date_naive()
            .with_day(1)
            .and_then(|date| date.and_hms_opt(0, 0, 0)),
    };
    naive
        .and_then(|naive| offset.from_local_datetime(&naive).single())
        .unwrap_or(at)
}

/// The next bucket boundary. Months step by the calendar, not by a fixed span,
/// so February and March start where the reader expects.
fn next_bucket(unit: UsageBucketUnit, start: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
    match unit {
        UsageBucketUnit::Hour => start + Duration::hours(1),
        UsageBucketUnit::Day => start + Duration::days(1),
        UsageBucketUnit::Week => start + Duration::days(7),
        UsageBucketUnit::Month => {
            let (year, month) = if start.month() == 12 {
                (start.year() + 1, 1)
            } else {
                (start.year(), start.month() + 1)
            };
            // The day is already 1, so neither setter can land on a date that
            // does not exist.
            start
                .with_year(year)
                .and_then(|value| value.with_month(month))
                .unwrap_or(start + Duration::days(31))
        }
    }
}

fn bucket_label(unit: UsageBucketUnit, start: DateTime<FixedOffset>) -> String {
    match unit {
        UsageBucketUnit::Hour => start.format("%H:00").to_string(),
        UsageBucketUnit::Day | UsageBucketUnit::Week => start.format("%m-%d").to_string(),
        UsageBucketUnit::Month => start.format("%Y-%m").to_string(),
    }
}

/// The slice spelled out, for a tooltip heading: the axis label alone is
/// ambiguous once the window crosses a month or a year.
fn bucket_title(
    unit: UsageBucketUnit,
    start: DateTime<FixedOffset>,
    next: DateTime<FixedOffset>,
) -> String {
    match unit {
        UsageBucketUnit::Hour => format!(
            "{} {}–{}",
            start.format("%Y-%m-%d"),
            start.format("%H:00"),
            next.format("%H:00")
        ),
        UsageBucketUnit::Day => start.format("%Y-%m-%d").to_string(),
        UsageBucketUnit::Week => format!(
            "{} – {}",
            start.format("%Y-%m-%d"),
            (next - Duration::days(1)).format("%m-%d")
        ),
        UsageBucketUnit::Month => start.format("%Y-%m").to_string(),
    }
}

/// Slice the window into buckets and stack every dimension over them.
///
/// Empty buckets are kept: a day with no requests is information, and dropping
/// it would make an idle week look like a busy one with fewer bars. The series
/// covers every row in the window, not one page, so the bars sum to the summary
/// cards apart from the undated rows it reports separately.
pub fn build_trend_series(rows: &[UsageOverviewRow], frame: TrendFrame) -> UsageTrendSeries {
    let mut dated: Vec<(&UsageOverviewRow, i64)> = Vec::with_capacity(rows.len());
    let mut undated_request_count = 0;
    for row in rows {
        match row
            .occurred_at
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        {
            Some(at) => dated.push((row, at.timestamp_millis())),
            None => undated_request_count += 1,
        }
    }

    let earliest = dated.iter().map(|(_, ms)| *ms).min();
    let Some(start_ms) = frame.start_ms.or(earliest) else {
        // No window start and nothing to derive one from: an empty chart, not a
        // chart of an arbitrary range.
        return UsageTrendSeries {
            undated_request_count,
            ..UsageTrendSeries::default()
        };
    };
    // A row can post-date `end_ms` when a clock disagrees with the database, and
    // a bar it does not fit in is a bar the user silently loses.
    let latest = dated.iter().map(|(_, ms)| *ms).max().unwrap_or(start_ms);
    let end_ms = frame.end_ms.max(latest).max(start_ms);

    let unit = choose_unit(end_ms - start_ms);
    let mut buckets = Vec::new();
    let mut index_of: HashMap<i64, usize> = HashMap::new();
    let mut cursor = bucket_start(unit, in_offset(start_ms, frame.offset));
    while cursor.timestamp_millis() <= end_ms && buckets.len() < TREND_BUCKET_CEILING {
        let next = next_bucket(unit, cursor);
        index_of.insert(cursor.timestamp_millis(), buckets.len());
        buckets.push(UsageTrendBucket {
            start: cursor.to_rfc3339(),
            label: bucket_label(unit, cursor),
            title: bucket_title(unit, cursor, next),
            totals: UsageOverviewTotals::default(),
        });
        cursor = next;
    }

    // Clamping rather than skipping keeps every row on the chart: an unmapped
    // timestamp — a leftover from the ceiling above, or a boundary the offset
    // rounded differently — would otherwise vanish from the bars while still
    // counting in the cards above them.
    let last = buckets.len().saturating_sub(1);
    let placed: Vec<(&UsageOverviewRow, usize)> = dated
        .into_iter()
        .map(|(row, ms)| {
            let at = in_offset(ms, frame.offset);
            let index = index_of
                .get(&bucket_start(unit, at).timestamp_millis())
                .copied()
                .unwrap_or(if ms <= start_ms { 0 } else { last });
            (row, index)
        })
        .collect();

    for (row, index) in &placed {
        buckets[*index].totals.absorb(row);
    }

    let count = buckets.len();
    UsageTrendSeries {
        unit,
        by_model: stack_series(&placed, count, model_key),
        by_platform: stack_series(&placed, count, platform_key),
        by_account: stack_series(&placed, count, account_key),
        by_source: stack_series(&placed, count, source_key),
        buckets,
        undated_request_count,
    }
}

fn in_offset(millis: i64, offset: FixedOffset) -> DateTime<FixedOffset> {
    let utc: DateTime<Utc> =
        DateTime::from_timestamp_millis(millis).unwrap_or_else(|| Utc.timestamp_nanos(0));
    utc.with_timezone(&offset)
}

fn stack_series<F>(
    placed: &[(&UsageOverviewRow, usize)],
    bucket_count: usize,
    key: F,
) -> Vec<UsageTrendRow>
where
    F: Fn(&UsageOverviewRow) -> String,
{
    let mut stacks: HashMap<String, Vec<i64>> = HashMap::new();
    for (row, index) in placed {
        let stack = stacks
            .entry(key(row))
            .or_insert_with(|| vec![0; bucket_count]);
        stack[*index] += row.input_tokens + row.output_tokens;
    }
    fold_tail(stacks)
}

/// Biggest series first, with everything past the colour budget folded into one
/// row. Ordering is by the plotted metric — tokens — so the tallest stack
/// segments are the named ones.
fn fold_tail(stacks: HashMap<String, Vec<i64>>) -> Vec<UsageTrendRow> {
    let mut ranked: Vec<(i64, UsageTrendRow)> = stacks
        .into_iter()
        .map(|(key, tokens)| (tokens.iter().sum(), UsageTrendRow { key, tokens }))
        .collect();
    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.key.cmp(&right.1.key))
    });
    let mut rows: Vec<UsageTrendRow> = ranked.into_iter().map(|(_, row)| row).collect();

    if rows.len() <= TREND_SERIES_LIMIT {
        return rows;
    }
    let tail = rows.split_off(TREND_SERIES_LIMIT);
    let mut other = vec![0; tail.first().map_or(0, |row| row.tokens.len())];
    for row in tail {
        for (slot, value) in other.iter_mut().zip(row.tokens) {
            *slot += value;
        }
    }
    rows.push(UsageTrendRow {
        key: TREND_OTHER_KEY.to_string(),
        tokens: other,
    });
    rows
}

/// One page of rows. A page past the end is empty rather than an error: the
/// list shrinks between refreshes as rows age out of the window.
pub fn paginate(rows: &[UsageOverviewRow], page: i64, page_size: i64) -> Vec<UsageOverviewRow> {
    let offset = ((page - 1).max(0) as usize).saturating_mul(page_size.max(1) as usize);
    rows.iter()
        .skip(offset)
        .take(page_size.max(1) as usize)
        .cloned()
        .collect()
}

fn integrity_of(
    rows: &[UsageOverviewRow],
    scanned_file_count: i64,
    truncated: bool,
    unmatchable_proxy_row_count: i64,
) -> UsageOverviewIntegrity {
    UsageOverviewIntegrity {
        scanned_file_count,
        truncated,
        unpriced_request_count: rows.iter().filter(|row| row.price_source.is_none()).count() as i64,
        estimated_price_request_count: rows
            .iter()
            .filter(|row| row.price_source.as_deref() == Some("estimated"))
            .count() as i64,
        unmatchable_proxy_row_count,
    }
}

/// Assemble the full overview: merge, summarize, group, and page.
///
/// The transcript scan is blocking file IO over a corpus that can reach
/// gigabytes, so it runs on a blocking thread. Warm scans hit the per-file parse
/// cache in [`session_usage_service`].
pub async fn build_usage_overview(
    pool: &SqlitePool,
    since: Option<&str>,
    page: i64,
    page_size: i64,
    window: TimeWindow,
    utc_offset_minutes: Option<i32>,
) -> Result<UsageOverview, AppError> {
    let proxy_rows = RoutePoolRepository::list_request_events(pool, since).await?;

    let (session_entries, scanned_file_count, truncated) =
        tokio::task::spawn_blocking(move || session_usage_service::collect_session_entries(window))
            .await
            .map_err(|error| AppError::Filesystem {
                code: "filesystem.session_usage_scan_failed",
                message: format!("Failed to scan session usage: {error}"),
                details: None,
                recoverable: true,
            })?;

    Ok(assemble_overview(
        session_entries,
        proxy_rows,
        scanned_file_count,
        truncated,
        page,
        page_size,
        // The chart's x-axis is the window the caller asked for, sliced at the
        // *caller's* midnight so its columns line up with the 当日 / 本周 presets
        // the UI computes from its own clock. Falling back to the server's offset
        // only matters for a caller that reports none.
        //
        // Still one fixed offset for the whole window, so a DST region is off by an
        // hour on the far side of a transition and the transition day itself is
        // 23h or 25h rather than 24h. Correcting that needs a real timezone
        // database (the client reports an offset, not a zone name), which is a
        // dependency this has not earned yet — the daily and hourly labels either
        // side of the change are still the right buckets, only the boundary moves.
        TrendFrame {
            start_ms: window.start_ms,
            end_ms: window
                .end_ms
                .unwrap_or_else(|| Utc::now().timestamp_millis()),
            offset: utc_offset_minutes
                .and_then(|minutes| FixedOffset::east_opt(minutes * 60))
                .unwrap_or_else(|| Local::now().offset().fix()),
        },
    ))
}

/// Merge both sides and shape the result, with no IO of its own.
///
/// Split out from [`build_usage_overview`] so the shaping rules — totals over
/// the window rather than the page, groups over every row, integrity counts —
/// can be tested without reading a multi-gigabyte transcript corpus.
fn assemble_overview(
    session_entries: Vec<SessionUsageEntry>,
    proxy_rows: Vec<ProxyRequestRow>,
    scanned_file_count: i64,
    truncated: bool,
    page: i64,
    page_size: i64,
    frame: TrendFrame,
) -> UsageOverview {
    // Only successes can double count: a failed request never produced an
    // assistant message, so the transcripts hold nothing to pair it with.
    let unmatchable_proxy_row_count = proxy_rows
        .iter()
        .filter(|row| resolve_proxy_response_id(row).is_none() && proxy_facts(row).success)
        .count() as i64;

    let rows = merge_entries(session_entries, proxy_rows);
    let totals = summarize(&rows);
    let groups = group_all(&rows);
    let series = build_trend_series(&rows, frame);
    let integrity = integrity_of(
        &rows,
        scanned_file_count,
        truncated,
        unmatchable_proxy_row_count,
    );

    UsageOverview {
        totals,
        row_count: rows.len() as i64,
        rows: paginate(&rows, page, page_size),
        groups,
        series,
        page,
        page_size,
        integrity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed +08:00 frame around the fixtures' 2026-08-19 timestamps.
    ///
    /// Every trend assertion pins the offset rather than reading the machine's:
    /// bucket boundaries are local midnights, so a test that used `Local` would
    /// pass in Shanghai and fail in CI.
    fn test_frame() -> TrendFrame {
        TrendFrame {
            start_ms: Some(millis("2026-08-19T00:00:00+08:00")),
            end_ms: millis("2026-08-19T23:59:59+08:00"),
            offset: FixedOffset::east_opt(8 * 3600).expect("offset"),
        }
    }

    fn millis(rfc3339: &str) -> i64 {
        DateTime::parse_from_rfc3339(rfc3339)
            .expect("timestamp")
            .timestamp_millis()
    }

    /// Build one proxied request row carrying an upstream price.
    fn priced_proxy_row(index: usize, response_id: Option<&str>) -> ProxyRequestRow {
        ProxyRequestRow {
            id: format!("proxy-{index}"),
            platform: "claude".to_string(),
            account_id: Some("cred-1".to_string()),
            account_name: Some("Team Account".to_string()),
            source_label: "route_proxy".to_string(),
            metadata_json: r#"{"path":"/v1/messages","status":200,"success":true,"upstream_model":"claude-opus-5"}"#.to_string(),
            created_at: format!("2026-08-19T14:{:02}:00Z", index % 60),
            input_tokens: Some(100),
            output_tokens: Some(10),
            cache_tokens: None,
            price_usd_micros: Some(1_000),
            price_cny_micros: None,
            price_currency: Some("usd".to_string()),
            price_source: Some("upstream".to_string()),
            upstream_response_id: response_id.map(str::to_string),
        }
    }

    #[test]
    fn totals_and_groups_cover_the_window_while_rows_cover_one_page() {
        // The summary cards answer "what did I spend in this period", so paging
        // through the list must not change them.
        let proxy_rows: Vec<ProxyRequestRow> = (0..25)
            .map(|index| {
                let id = format!("msg_seed_{index}");
                priced_proxy_row(index, Some(&id))
            })
            .collect();

        let first = assemble_overview(
            Vec::new(),
            proxy_rows.clone(),
            1_186,
            false,
            1,
            20,
            test_frame(),
        );
        let second = assemble_overview(Vec::new(), proxy_rows, 1_186, false, 2, 20, test_frame());

        assert_eq!(first.rows.len(), 20, "one page of rows");
        assert_eq!(second.rows.len(), 5);
        assert_eq!(first.row_count, 25, "the whole window is reported");
        assert_eq!(
            first.totals.request_count, 25,
            "totals must not shrink to the page"
        );
        assert_eq!(first.totals.cost_micros, 25_000);
        assert_eq!(
            first.groups.by_account[0].totals.request_count, 25,
            "groups must not shrink to the page either"
        );
        assert_eq!(
            second.totals, first.totals,
            "the totals are identical on every page"
        );
        assert_eq!(second.groups, first.groups);
    }

    #[test]
    fn integrity_reports_proxy_rows_that_carry_no_response_id() {
        // These rows could not be merged, so a transcript entry for the same
        // request is still counted separately. The UI has to be able to say so.
        let overview = assemble_overview(
            Vec::new(),
            vec![
                priced_proxy_row(0, None),
                priced_proxy_row(1, Some("msg_ok")),
            ],
            1_186,
            false,
            1,
            20,
            test_frame(),
        );

        assert_eq!(overview.integrity.unmatchable_proxy_row_count, 1);
        assert_eq!(overview.integrity.scanned_file_count, 1_186);
    }

    #[test]
    fn a_failed_id_less_proxy_row_is_not_reported_as_a_double_count_risk() {
        // A failed request produced no assistant message, so the transcripts
        // hold nothing to pair it with. On a real corpus 707 of 709 id-less
        // rows were failures; counting them would have shown the user an
        // alarming number for a risk that does not exist.
        let mut failed = priced_proxy_row(0, None);
        failed.metadata_json =
            r#"{"path":"/v1/messages","status":524,"success":false}"#.to_string();

        let overview = assemble_overview(Vec::new(), vec![failed], 0, false, 1, 20, test_frame());

        assert_eq!(overview.integrity.unmatchable_proxy_row_count, 0);
    }

    fn row_with(
        source: UsageRowSource,
        provider: &str,
        model: &str,
        account: Option<&str>,
        cost: i64,
    ) -> UsageOverviewRow {
        UsageOverviewRow {
            id: format!("{model}-{cost}"),
            source,
            occurred_at: Some("2026-08-19T14:04:50Z".to_string()),
            provider: provider.to_string(),
            model: model.to_string(),
            account_id: account.map(|_| "cred-1".to_string()),
            account_name: account.map(str::to_string),
            source_label: None,
            path: None,
            status: None,
            success: true,
            input_tokens: 100,
            output_tokens: 10,
            cache_write_tokens: 2,
            cache_read_tokens: 3,
            cost_micros: cost,
            price_source: Some("upstream".to_string()),
            upstream_response_id: None,
            metadata_json: None,
        }
    }

    #[test]
    fn totals_add_up_every_row_exactly_once() {
        let rows = vec![
            row_with(
                UsageRowSource::Matched,
                "claude",
                "claude-opus-5",
                Some("A"),
                1_000,
            ),
            row_with(
                UsageRowSource::SessionOnly,
                "claude",
                "claude-opus-5",
                None,
                2_000,
            ),
            row_with(
                UsageRowSource::ProxyOnly,
                "codex",
                "gpt-5.6-sol",
                Some("B"),
                3_000,
            ),
        ];

        let totals = summarize(&rows);

        assert_eq!(totals.request_count, 3);
        assert_eq!(totals.input_tokens, 300);
        assert_eq!(totals.output_tokens, 30);
        assert_eq!(totals.cache_write_tokens, 6);
        assert_eq!(totals.cache_read_tokens, 9);
        assert_eq!(totals.cost_micros, 6_000);
    }

    #[test]
    fn groups_cover_all_four_dimensions() {
        let rows = vec![
            row_with(
                UsageRowSource::Matched,
                "claude",
                "claude-opus-5",
                Some("A"),
                1_000,
            ),
            row_with(
                UsageRowSource::SessionOnly,
                "claude",
                "claude-haiku-4-5",
                None,
                2_000,
            ),
            row_with(
                UsageRowSource::ProxyOnly,
                "codex",
                "gpt-5.6-sol",
                Some("B"),
                3_000,
            ),
        ];

        let groups = group_all(&rows);

        assert_eq!(groups.by_model.len(), 3);
        assert_eq!(groups.by_platform.len(), 2);
        assert_eq!(groups.by_source.len(), 3);
        // Two named accounts plus one bucket for the rows with none.
        assert_eq!(groups.by_account.len(), 3);
    }

    #[test]
    fn account_grouping_buckets_rows_that_never_went_through_the_proxy() {
        // Most merged rows come from transcripts and have no account, so the
        // bucket has to be an explicit, named row rather than a blank label.
        let rows = vec![
            row_with(
                UsageRowSource::SessionOnly,
                "claude",
                "claude-opus-5",
                None,
                2_000,
            ),
            row_with(
                UsageRowSource::SessionOnly,
                "claude",
                "claude-opus-5",
                None,
                3_000,
            ),
        ];

        let groups = group_all(&rows);

        assert_eq!(groups.by_account.len(), 1);
        assert_eq!(groups.by_account[0].key, "未经代理");
        assert_eq!(groups.by_account[0].totals.request_count, 2);
        assert_eq!(groups.by_account[0].totals.cost_micros, 5_000);
    }

    #[test]
    fn groups_are_ordered_by_cost_so_the_biggest_spend_reads_first() {
        let rows = vec![
            row_with(
                UsageRowSource::Matched,
                "claude",
                "cheap-model",
                Some("A"),
                10,
            ),
            row_with(
                UsageRowSource::Matched,
                "claude",
                "pricey-model",
                Some("A"),
                9_000,
            ),
        ];

        let groups = group_all(&rows);

        assert_eq!(groups.by_model[0].key, "pricey-model");
    }

    #[test]
    fn source_group_keys_are_human_readable() {
        let rows = vec![
            row_with(UsageRowSource::Matched, "claude", "m", Some("A"), 1),
            row_with(UsageRowSource::SessionOnly, "claude", "m", None, 1),
            row_with(UsageRowSource::ProxyOnly, "codex", "m", Some("B"), 1),
        ];

        let groups = group_all(&rows);
        let keys: Vec<&str> = groups
            .by_source
            .iter()
            .map(|row| row.key.as_str())
            .collect();

        assert!(keys.contains(&"匹配"));
        assert!(keys.contains(&"仅会话"));
        assert!(keys.contains(&"仅代理"));
    }

    #[test]
    fn paging_slices_rows_without_shrinking_the_totals() {
        // The cards answer "what did I spend in this period", so they must not
        // change as the user walks through pages.
        let rows: Vec<UsageOverviewRow> = (0..25)
            .map(|index| {
                row_with(
                    UsageRowSource::SessionOnly,
                    "claude",
                    &format!("model-{index}"),
                    None,
                    100,
                )
            })
            .collect();

        let first = paginate(&rows, 1, 20);
        let second = paginate(&rows, 2, 20);

        assert_eq!(first.len(), 20);
        assert_eq!(second.len(), 5);
        assert_eq!(summarize(&rows).cost_micros, 2_500);
    }

    #[test]
    fn a_page_past_the_end_yields_no_rows_rather_than_an_error() {
        let rows = vec![row_with(
            UsageRowSource::SessionOnly,
            "claude",
            "m",
            None,
            1,
        )];

        assert!(paginate(&rows, 99, 20).is_empty());
    }

    #[test]
    fn integrity_counts_unpriced_and_estimated_rows() {
        let mut unpriced = row_with(UsageRowSource::SessionOnly, "codex", "unknown", None, 0);
        unpriced.price_source = None;
        let mut estimated = row_with(UsageRowSource::SessionOnly, "claude", "m", None, 500);
        estimated.price_source = Some("estimated".to_string());
        let upstream = row_with(UsageRowSource::Matched, "claude", "m", Some("A"), 700);

        let integrity = integrity_of(&[unpriced, estimated, upstream], 1_186, false, 4);

        assert_eq!(integrity.unpriced_request_count, 1);
        assert_eq!(integrity.estimated_price_request_count, 1);
        assert_eq!(integrity.scanned_file_count, 1_186);
        assert_eq!(integrity.unmatchable_proxy_row_count, 4);
        assert!(!integrity.truncated);
    }

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
    fn two_proxy_rows_sharing_one_response_id_both_survive() {
        // Bridges that have to synthesize a response id use a constant, so the
        // same id legitimately appears on unrelated rows. Keying the index by id
        // alone let the last writer win and dropped the rest out of every total.
        let mut first = proxy_row(Some("resp_ai_switch"));
        first.id = "proxy-a".to_string();
        let mut second = proxy_row(Some("resp_ai_switch"));
        second.id = "proxy-b".to_string();

        let rows = merge_entries(
            vec![session_entry(Some("resp_ai_switch"), "claude-opus-5", 10)],
            vec![first, second],
        );

        assert_eq!(rows.len(), 2);
        // One row paired with the transcript entry; the other stands on its own
        // rather than disappearing.
        assert_eq!(
            rows.iter()
                .filter(|row| row.source == UsageRowSource::Matched)
                .count(),
            1
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row.source == UsageRowSource::ProxyOnly)
                .count(),
            1
        );
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

    /// One transcript-only row with an explicit timestamp and token count.
    fn dated_row(model: &str, occurred_at: &str, input_tokens: i64) -> UsageOverviewRow {
        let mut row = row_with(UsageRowSource::SessionOnly, "claude", model, None, 100);
        row.id = format!("{model}@{occurred_at}");
        row.occurred_at = Some(occurred_at.to_string());
        row.input_tokens = input_tokens;
        row.output_tokens = 0;
        row
    }

    fn frame(start: &str, end: &str) -> TrendFrame {
        TrendFrame {
            start_ms: Some(millis(start)),
            end_ms: millis(end),
            offset: FixedOffset::east_opt(8 * 3600).expect("offset"),
        }
    }

    #[test]
    fn a_single_day_is_sliced_into_hours() {
        // 当日 as one daily bar is a single block that says nothing; the useful
        // shape inside one day is hourly.
        let series = build_trend_series(
            &[dated_row("m", "2026-08-19T09:30:00+08:00", 10)],
            frame("2026-08-19T00:00:00+08:00", "2026-08-19T23:59:00+08:00"),
        );

        assert_eq!(series.unit, UsageBucketUnit::Hour);
        assert_eq!(series.buckets.len(), 24);
        assert_eq!(series.buckets[0].label, "00:00");
        assert_eq!(series.buckets[9].label, "09:00");
        assert_eq!(series.buckets[9].totals.request_count, 1);
        assert_eq!(series.by_model[0].tokens[9], 10);
    }

    #[test]
    fn a_month_is_sliced_into_days_and_keeps_the_empty_ones() {
        // A quiet day is information. Dropping it would leave the axis lying
        // about how far apart the remaining bars are.
        let series = build_trend_series(
            &[
                dated_row("m", "2026-08-01T10:00:00+08:00", 10),
                dated_row("m", "2026-08-03T10:00:00+08:00", 20),
            ],
            frame("2026-08-01T00:00:00+08:00", "2026-08-05T12:00:00+08:00"),
        );

        assert_eq!(series.unit, UsageBucketUnit::Day);
        assert_eq!(series.buckets.len(), 5);
        assert_eq!(series.buckets[1].label, "08-02");
        assert_eq!(series.by_model[0].tokens, vec![10, 0, 20, 0, 0]);
    }

    #[test]
    fn a_span_too_wide_for_days_steps_up_to_weeks_then_months() {
        let weekly = build_trend_series(
            &[dated_row("m", "2026-03-02T10:00:00+08:00", 10)],
            frame("2026-01-01T00:00:00+08:00", "2026-06-01T00:00:00+08:00"),
        );
        let monthly = build_trend_series(
            &[dated_row("m", "2024-03-02T10:00:00+08:00", 10)],
            frame("2020-01-01T00:00:00+08:00", "2026-06-01T00:00:00+08:00"),
        );

        // Five months of daily bars is 150 columns in a side panel; the point of
        // stepping up is that the chart stays readable at any range.
        assert_eq!(weekly.unit, UsageBucketUnit::Week);
        assert!(weekly.buckets.len() <= MAX_TREND_BUCKETS as usize);
        assert_eq!(monthly.unit, UsageBucketUnit::Month);
        assert_eq!(monthly.buckets[0].label, "2020-01");
        // Calendar stepping, not a fixed 30 days: month two must start on the 1st.
        assert_eq!(monthly.buckets[1].label, "2020-02");
    }

    #[test]
    fn buckets_break_at_midnight_in_the_supplied_offset() {
        // 16:30 UTC is 00:30 the next morning in +08:00. Bucketing in UTC would
        // file it under the previous day and disagree with the 当日 window the
        // UI computed from the same local clock.
        let series = build_trend_series(
            &[dated_row("m", "2026-08-19T16:30:00Z", 10)],
            frame("2026-08-19T00:00:00+08:00", "2026-08-22T12:00:00+08:00"),
        );

        assert_eq!(series.unit, UsageBucketUnit::Day);
        assert_eq!(series.buckets[1].label, "08-20");
        assert_eq!(series.by_model[0].tokens, vec![0, 10, 0, 0]);
    }

    #[test]
    fn every_bucket_carries_its_own_totals_and_they_add_up_to_the_window() {
        // The chart sits under the summary cards, so its bars have to account
        // for the same requests the cards counted.
        let rows = vec![
            dated_row("a", "2026-08-19T09:00:00+08:00", 10),
            dated_row("b", "2026-08-19T09:30:00+08:00", 20),
            dated_row("a", "2026-08-19T11:00:00+08:00", 30),
        ];
        let totals = summarize(&rows);

        let series = build_trend_series(
            &rows,
            frame("2026-08-19T00:00:00+08:00", "2026-08-19T23:59:00+08:00"),
        );

        let bucketed: i64 = series
            .buckets
            .iter()
            .map(|bucket| bucket.totals.request_count)
            .sum();
        let stacked: i64 = series
            .by_model
            .iter()
            .flat_map(|row| row.tokens.iter())
            .sum();
        assert_eq!(bucketed, totals.request_count);
        assert_eq!(stacked, totals.input_tokens + totals.output_tokens);
        assert_eq!(series.buckets[9].totals.request_count, 2);
    }

    #[test]
    fn the_tail_of_a_long_series_folds_into_one_row() {
        // Past eight hues a reader cannot tell two segments apart, and inventing
        // a ninth colour is worse than saying "everything else".
        let rows: Vec<UsageOverviewRow> = (0..12)
            .map(|index| {
                dated_row(
                    &format!("model-{index:02}"),
                    "2026-08-19T09:00:00+08:00",
                    // Descending, so the folded tail is the smallest models.
                    100 - i64::from(index),
                )
            })
            .collect();

        let series = build_trend_series(
            &rows,
            frame("2026-08-19T00:00:00+08:00", "2026-08-19T23:59:00+08:00"),
        );

        assert_eq!(series.by_model.len(), 9);
        assert_eq!(series.by_model[0].key, "model-00");
        let other = series.by_model.last().expect("folded row");
        assert_eq!(other.key, TREND_OTHER_KEY);
        // The four folded models: 92 + 91 + 90 + 89.
        assert_eq!(other.tokens[9], 362);
    }

    #[test]
    fn a_ninth_series_folds_rather_than_taking_a_ninth_colour() {
        let rows: Vec<UsageOverviewRow> = (0..9)
            .map(|index| {
                dated_row(
                    &format!("model-{index}"),
                    "2026-08-19T09:00:00+08:00",
                    100 - i64::from(index),
                )
            })
            .collect();

        let series = build_trend_series(
            &rows,
            frame("2026-08-19T00:00:00+08:00", "2026-08-19T23:59:00+08:00"),
        );

        // The chart has exactly eight hues a reader can separate; the ninth
        // series has to be 其他 rather than a colour that impersonates another.
        assert_eq!(series.by_model.len(), 9);
        assert_eq!(series.by_model[7].key, "model-7");
        assert_eq!(series.by_model[8].key, TREND_OTHER_KEY);
        assert_eq!(series.by_model[8].tokens[9], 92);
    }

    #[test]
    fn undated_rows_are_reported_instead_of_being_bucketed() {
        // A transcript entry can carry no timestamp. Filing it under an
        // arbitrary bucket would put a bar where nothing happened, so the chart
        // has to be able to say the figure is short by this many requests.
        let mut undated = dated_row("m", "2026-08-19T09:00:00+08:00", 10);
        undated.occurred_at = None;

        let series = build_trend_series(
            &[dated_row("m", "2026-08-19T09:00:00+08:00", 10), undated],
            frame("2026-08-19T00:00:00+08:00", "2026-08-19T23:59:00+08:00"),
        );

        assert_eq!(series.undated_request_count, 1);
        assert_eq!(series.by_model[0].tokens.iter().sum::<i64>(), 10);
    }

    #[test]
    fn an_all_time_window_starts_at_the_earliest_row() {
        // 累计 sends no `since`, so the axis has to come from the data rather
        // than from an arbitrary epoch that would draw thousands of empty bars.
        let series = build_trend_series(
            &[dated_row("m", "2026-08-18T09:00:00+08:00", 10)],
            TrendFrame {
                start_ms: None,
                end_ms: millis("2026-08-25T12:00:00+08:00"),
                offset: FixedOffset::east_opt(8 * 3600).expect("offset"),
            },
        );

        assert_eq!(series.buckets.len(), 8);
        assert_eq!(series.buckets[0].label, "08-18");
    }

    #[test]
    fn an_empty_all_time_window_yields_no_buckets_rather_than_a_guess() {
        let series = build_trend_series(
            &[],
            TrendFrame {
                start_ms: None,
                end_ms: millis("2026-08-19T12:00:00+08:00"),
                offset: FixedOffset::east_opt(8 * 3600).expect("offset"),
            },
        );

        assert!(series.buckets.is_empty());
        assert!(series.by_model.is_empty());
    }
}
