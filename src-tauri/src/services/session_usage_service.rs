//! Aggregate token usage and cost from local CLI session transcripts.
//!
//! Complements the proxy-route statistics: the route stats only see traffic that
//! went through this app's proxy, while Claude Code and Codex also record every
//! request they made directly. Reading their session logs gives a complete
//! picture of spend, including work done outside the proxy.
//!
//! Two provider formats are supported, each with a counting rule that must not
//! be got wrong:
//!
//! * **Claude Code** (`~/.claude/projects/**/*.jsonl`) — one JSON object per
//!   line; assistant messages carry `message.usage`. The same message is
//!   re-serialized into multiple files by resume and compaction, so rows must be
//!   deduplicated by `message.id`. On a real machine this cut a 4020-row scan to
//!   2008 unique messages — counting raw lines overstated cost by 93%.
//! * **Codex CLI** (`~/.codex/sessions/**/*.jsonl`) — `token_count` events whose
//!   `total_token_usage` is **cumulative for the session**, not per-turn. Only
//!   the last event in a file may be counted. Summing them overstated one real
//!   file by 350x (28.1B tokens against an actual 80.5M).

use crate::services::model_pricing::{self, TokenUsage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Usage statistics scans read the full history rather than a recent window, so
/// the cap only exists to bound pathological directories. Well above the ~1.2k
/// files a heavy user accumulates; when it does trip, the truncation is reported
/// rather than passed off as a complete total.
const USAGE_SCAN_FILE_LIMIT: usize = 50_000;

/// Directory depth limit, matching the session list's traversal.
const USAGE_SCAN_DEPTH: usize = 8;

/// Rolled-up usage for one grouping key (a model, a provider, or the total).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUsageTotals {
    /// Billable requests counted (deduplicated).
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_read_tokens: i64,
    /// Estimated cost in USD micros (1 USD = 1_000_000).
    pub cost_micros: i64,
    /// Requests whose model had no known rate, so they contribute no cost.
    /// Surfaced so a partial total is never mistaken for a complete one.
    pub unpriced_request_count: i64,
}

impl SessionUsageTotals {
    fn add(&mut self, other: &SessionUsageTotals) {
        self.request_count += other.request_count;
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cost_micros += other.cost_micros;
        self.unpriced_request_count += other.unpriced_request_count;
    }

    pub fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.cache_write_tokens + self.cache_read_tokens
    }
}

/// Per-model breakdown row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUsageModelRow {
    /// Provider id: `claude` or `codex`.
    pub provider: String,
    /// Model id as recorded, before price-table normalization.
    pub model: String,
    /// Whether a rate was found for this model.
    pub priced: bool,
    #[serde(flatten)]
    pub totals: SessionUsageTotals,
}

/// Full result of a session usage scan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUsageStats {
    pub totals: SessionUsageTotals,
    /// Per-provider rollup, keyed by provider id.
    pub by_provider: Vec<SessionUsageModelRow>,
    /// Per-model rollup, highest cost first.
    pub by_model: Vec<SessionUsageModelRow>,
    /// Session files read.
    pub scanned_file_count: i64,
    /// True when the file cap was hit and the totals are therefore incomplete.
    pub truncated: bool,
}

/// Inclusive-start, exclusive-end epoch-millisecond filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct TimeWindow {
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

impl TimeWindow {
    fn contains(&self, timestamp_ms: Option<i64>) -> bool {
        // Entries without a timestamp are only counted for an unbounded window,
        // so a period filter cannot silently absorb undated rows.
        let Some(timestamp) = timestamp_ms else {
            return self.start_ms.is_none() && self.end_ms.is_none();
        };
        if self.start_ms.is_some_and(|start| timestamp < start) {
            return false;
        }
        if self.end_ms.is_some_and(|end| timestamp >= end) {
            return false;
        }
        true
    }
}

/// One billable request extracted from a transcript, before time filtering and
/// cross-file deduplication.
#[derive(Debug, Clone, PartialEq, Eq)]
struct UsageEntry {
    provider: &'static str,
    model: String,
    /// Dedup key (Claude `message.id`); `None` when the transcript has none.
    dedup_key: Option<String>,
    timestamp_ms: Option<i64>,
    usage: TokenUsage,
}

/// Parsed contents of one session file.
///
/// Cached per file so a refresh only re-reads files that changed. Entries are
/// stored un-filtered and un-deduplicated so one cache entry serves every time
/// window and participates in cross-file dedup.
#[derive(Debug, Clone, Default)]
struct ParsedFile {
    entries: Vec<UsageEntry>,
}

/// Identity of a file version: changing either field invalidates the cache.
/// Session transcripts are append-only, so size alone would nearly suffice;
/// mtime also catches rewrites (compaction) that keep the length the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileVersion {
    modified_ms: i64,
    size: u64,
}

fn file_version(path: &Path) -> Option<FileVersion> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default();
    Some(FileVersion {
        modified_ms,
        size: metadata.len(),
    })
}

/// Process-wide parse cache.
///
/// Statistics refresh on a timer while the panel is open, and the transcript
/// corpus reaches multiple gigabytes; without this every refresh would re-read
/// all of it. Keyed by path, invalidated by [`FileVersion`].
type ParseCache = HashMap<PathBuf, (FileVersion, Arc<ParsedFile>)>;

static PARSE_CACHE: OnceLock<Mutex<ParseCache>> = OnceLock::new();

fn parse_cache() -> &'static Mutex<ParseCache> {
    PARSE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Return the parsed contents of `path`, reusing the cached parse when the file
/// is unchanged.
fn parsed_file(path: &Path, provider: Provider) -> Arc<ParsedFile> {
    let version = file_version(path);

    if let (Some(version), Ok(cache)) = (version, parse_cache().lock()) {
        if let Some((cached_version, parsed)) = cache.get(path) {
            if *cached_version == version {
                return Arc::clone(parsed);
            }
        }
    }

    let parsed = Arc::new(match provider {
        Provider::Claude => parse_claude_file(path),
        Provider::Codex => parse_codex_file(path),
    });

    if let (Some(version), Ok(mut cache)) = (version, parse_cache().lock()) {
        // Bound the map so a long-lived process cannot grow it without limit.
        if cache.len() >= MAX_CACHED_FILES {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), (version, Arc::clone(&parsed)));
    }

    parsed
}

/// Cap on cached file parses; cleared wholesale when exceeded.
const MAX_CACHED_FILES: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Provider {
    Claude,
    Codex,
}

fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Resolve an env var that may hold a path, expanding a leading `~`.
///
/// Mirrors the MCP client helper of the same name; duplicated here rather than
/// widening the private `mcp::clients` API for a services-layer caller.
fn env_path(name: &str, fallback: PathBuf) -> PathBuf {
    let Some(value) = std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return fallback;
    };
    if value == "~" {
        return home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(value)
}

/// Roots to scan for Claude Code transcripts.
///
/// `CLAUDE_CONFIG_DIR` is honored because a relocated install would otherwise be
/// missed entirely.
fn claude_roots() -> Vec<PathBuf> {
    let home = home_dir();
    let configured = env_path("CLAUDE_CONFIG_DIR", home.join(".claude"));
    let mut roots = vec![configured.join("projects")];
    let fallback = home.join(".cache").join("claude").join("projects");
    if !roots.contains(&fallback) {
        roots.push(fallback);
    }
    roots
}

/// Roots to scan for Codex CLI transcripts. `CODEX_HOME` is honored to match the
/// rest of the app's Codex handling.
fn codex_roots() -> Vec<PathBuf> {
    let codex_home = env_path("CODEX_HOME", home_dir().join(".codex"));
    vec![codex_home.join("sessions")]
}

/// Scan local session transcripts and aggregate usage within `window`.
///
/// Blocking file IO — call from `spawn_blocking`.
pub fn scan_session_usage(window: TimeWindow) -> SessionUsageStats {
    let mut accumulator = Accumulator::default();
    let mut scanned = 0_i64;
    let mut truncated = false;

    // One dedup set across every root and provider: the same Claude message can
    // appear in both the primary projects directory and the cache mirror.
    let mut seen_dedup_keys = HashSet::new();

    let roots = claude_roots()
        .into_iter()
        .map(|root| (root, Provider::Claude))
        .chain(
            codex_roots()
                .into_iter()
                .map(|root| (root, Provider::Codex)),
        );

    for (root, provider) in roots {
        if !root.exists() {
            continue;
        }
        let files = collect_files(&root, &mut truncated);
        scanned += files.len() as i64;
        for path in files {
            let parsed = parsed_file(&path, provider);
            accumulator.absorb(&parsed, window, &mut seen_dedup_keys);
        }
    }

    let mut stats = accumulator.finish();
    stats.scanned_file_count = scanned;
    stats.truncated = truncated;
    stats
}

fn collect_files(root: &Path, truncated: &mut bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    crate::session_manager::collect_session_files(
        root,
        &["jsonl"],
        USAGE_SCAN_DEPTH,
        USAGE_SCAN_FILE_LIMIT,
        &mut files,
    );
    if files.len() >= USAGE_SCAN_FILE_LIMIT {
        *truncated = true;
    }
    files
}

#[derive(Default)]
struct Accumulator {
    /// Keyed by (provider, model).
    by_model: HashMap<(String, String), SessionUsageTotals>,
}

impl Accumulator {
    /// Fold one parsed file into the running totals, applying the time filter
    /// and cross-file deduplication.
    fn absorb(
        &mut self,
        parsed: &ParsedFile,
        window: TimeWindow,
        seen_dedup_keys: &mut HashSet<String>,
    ) {
        for entry in &parsed.entries {
            if !window.contains(entry.timestamp_ms) {
                continue;
            }
            if let Some(key) = &entry.dedup_key {
                if !seen_dedup_keys.insert(key.clone()) {
                    continue;
                }
            }
            self.record(entry.provider, &entry.model, entry.usage);
        }
    }

    fn record(&mut self, provider: &str, model: &str, usage: TokenUsage) {
        let priced_cost = model_pricing::estimate_cost_micros(model, usage);
        let entry = self
            .by_model
            .entry((provider.to_string(), model.to_string()))
            .or_default();

        entry.request_count += 1;
        entry.input_tokens += usage.input_tokens.max(0);
        entry.output_tokens += usage.output_tokens.max(0);
        entry.cache_write_tokens += usage.cache_write_tokens.max(0);
        entry.cache_read_tokens += usage.cache_read_tokens.max(0);
        match priced_cost {
            Some(cost) => entry.cost_micros += cost,
            None => entry.unpriced_request_count += 1,
        }
    }

    fn finish(self) -> SessionUsageStats {
        let mut totals = SessionUsageTotals::default();
        let mut provider_totals: HashMap<String, SessionUsageTotals> = HashMap::new();
        let mut by_model = Vec::with_capacity(self.by_model.len());

        for ((provider, model), model_totals) in self.by_model {
            totals.add(&model_totals);
            provider_totals
                .entry(provider.clone())
                .or_default()
                .add(&model_totals);
            by_model.push(SessionUsageModelRow {
                priced: model_pricing::rate_for_model(&model).is_some(),
                provider,
                model,
                totals: model_totals,
            });
        }

        // Highest cost first, then by tokens so unpriced rows still order sensibly.
        by_model.sort_by(|left, right| {
            right
                .totals
                .cost_micros
                .cmp(&left.totals.cost_micros)
                .then_with(|| right.totals.total_tokens().cmp(&left.totals.total_tokens()))
                .then_with(|| left.model.cmp(&right.model))
        });

        let mut by_provider: Vec<SessionUsageModelRow> = provider_totals
            .into_iter()
            .map(|(provider, provider_total)| SessionUsageModelRow {
                provider,
                model: String::new(),
                priced: true,
                totals: provider_total,
            })
            .collect();
        by_provider.sort_by(|left, right| {
            right
                .totals
                .cost_micros
                .cmp(&left.totals.cost_micros)
                .then_with(|| left.provider.cmp(&right.provider))
        });

        SessionUsageStats {
            totals,
            by_provider,
            by_model,
            scanned_file_count: 0,
            truncated: false,
        }
    }
}

fn read_lines(path: &Path) -> Option<impl Iterator<Item = String>> {
    let file = File::open(path).ok()?;
    Some(
        BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter(|line| !line.trim().is_empty()),
    )
}

/// Parse one Claude Code transcript into its billable entries.
///
/// Sidechain (subagent) messages are included: their tokens are real spend, even
/// though the session *list* hides them. Deduplication and time filtering happen
/// later so this parse can be cached once and reused for any time window.
fn parse_claude_file(path: &Path) -> ParsedFile {
    let Some(lines) = read_lines(path) else {
        return ParsedFile::default();
    };

    let mut entries = Vec::new();
    for line in lines {
        // Transcripts are dominated by user turns and tool results that carry no
        // usage. A substring check is far cheaper than parsing every line, and
        // these files run to gigabytes.
        if !line.contains("\"usage\"") {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(message) = entry.get("message") else {
            continue;
        };
        let Some(usage) = message.get("usage") else {
            continue;
        };

        // `<synthetic>` marks locally generated messages that were never billed;
        // `normalize_model_id` rejects them, and skipping here also keeps them
        // out of the request count.
        let Some(model) = message.get("model").and_then(Value::as_str) else {
            continue;
        };
        if model_pricing::normalize_model_id(model).is_none() {
            continue;
        }

        entries.push(UsageEntry {
            provider: "claude",
            model: model.to_string(),
            // Resume and compaction rewrite the same assistant message into
            // several files; without this key the totals roughly double. Rows
            // with no id are kept unconditionally — every observed id-less row
            // was a distinct request, and undercounting spend is worse.
            dedup_key: message
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string),
            timestamp_ms: entry_timestamp_ms(&entry),
            usage: claude_token_usage(usage),
        });
    }

    ParsedFile { entries }
}

fn claude_token_usage(usage: &Value) -> TokenUsage {
    TokenUsage {
        input_tokens: json_i64(usage.get("input_tokens")),
        output_tokens: json_i64(usage.get("output_tokens")),
        cache_write_tokens: json_i64(usage.get("cache_creation_input_tokens")),
        cache_read_tokens: json_i64(usage.get("cache_read_input_tokens")),
    }
}

/// Parse one Codex CLI rollout file into its single billable entry.
///
/// `total_token_usage` accumulates over the session, so only the final
/// `token_count` event is counted. The model lives on separate `turn_context`
/// records, so it is tracked as the file is read.
fn parse_codex_file(path: &Path) -> ParsedFile {
    let Some(lines) = read_lines(path) else {
        return ParsedFile::default();
    };

    let mut model: Option<String> = None;
    let mut last_total: Option<TokenUsage> = None;
    let mut last_timestamp: Option<i64> = None;

    for line in lines {
        // Same cheap pre-filter as the Claude path: only `turn_context` records
        // (which carry the model) and `token_count` events matter.
        if !line.contains("token_count") && !line.contains("\"model\"") {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = entry.get("payload").unwrap_or(&Value::Null);

        if let Some(found) = payload
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            model = Some(found.to_string());
        }

        if payload.get("type").and_then(Value::as_str) != Some("token_count") {
            continue;
        }
        let Some(total) = payload.pointer("/info/total_token_usage") else {
            continue;
        };

        last_total = Some(codex_token_usage(total));
        last_timestamp = entry_timestamp_ms(&entry).or(last_timestamp);
    }

    let Some(usage) = last_total else {
        return ParsedFile::default();
    };

    ParsedFile {
        entries: vec![UsageEntry {
            provider: "codex",
            // A rollout without a recorded model still represents real spend;
            // attribute it to a placeholder so it appears as unpriced rather
            // than vanishing from the totals.
            model: model.unwrap_or_else(|| "unknown".to_string()),
            // One entry per file already, and Codex has no cross-file id.
            dedup_key: None,
            timestamp_ms: last_timestamp,
            usage,
        }],
    }
}

fn codex_token_usage(total: &Value) -> TokenUsage {
    // Codex reports `input_tokens` inclusive of `cached_input_tokens`, so the
    // cached portion is subtracted to avoid billing it at the full input rate.
    let raw_input = json_i64(total.get("input_tokens"));
    let cached = json_i64(total.get("cached_input_tokens"));
    // `reasoning_output_tokens` is already part of `output_tokens`; adding it
    // would double-count reasoning.
    TokenUsage {
        input_tokens: (raw_input - cached).max(0),
        output_tokens: json_i64(total.get("output_tokens")),
        cache_write_tokens: json_i64(total.get("cache_write_input_tokens")),
        cache_read_tokens: cached,
    }
}

fn json_i64(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|number| number as i64))
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .unwrap_or(0)
        .max(0)
}

/// Epoch milliseconds for a transcript line. Claude uses RFC 3339 `timestamp`;
/// Codex uses the same field on the envelope.
fn entry_timestamp_ms(entry: &Value) -> Option<i64> {
    let raw = entry
        .get("timestamp")
        .or_else(|| entry.get("created_at"))
        .or_else(|| entry.get("createdAt"))?;

    if let Some(number) = raw.as_i64() {
        // Heuristic: values this small are seconds, not milliseconds.
        return Some(if number < 100_000_000_000 {
            number * 1_000
        } else {
            number
        });
    }
    let text = raw.as_str()?;
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|parsed| parsed.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let mut file = File::create(&path).expect("create fixture");
        for line in lines {
            writeln!(file, "{line}").expect("write fixture");
        }
        path
    }

    fn claude_line(msg_id: &str, model: &str, input: i64, output: i64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"2026-08-19T14:04:50.011Z","message":{{"id":"{msg_id}","model":"{model}","usage":{{"input_tokens":{input},"output_tokens":{output},"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#
        )
    }

    /// Aggregate a set of files the same way [`scan_session_usage`] does, without
    /// touching the real home directory.
    fn aggregate(files: &[(&Path, Provider)], window: TimeWindow) -> SessionUsageStats {
        let mut accumulator = Accumulator::default();
        let mut seen = HashSet::new();
        for (path, provider) in files {
            let parsed = match provider {
                Provider::Claude => parse_claude_file(path),
                Provider::Codex => parse_codex_file(path),
            };
            accumulator.absorb(&parsed, window, &mut seen);
        }
        accumulator.finish()
    }

    fn aggregate_claude(path: &Path) -> SessionUsageStats {
        aggregate(&[(path, Provider::Claude)], TimeWindow::default())
    }

    fn aggregate_codex(path: &Path) -> SessionUsageStats {
        aggregate(&[(path, Provider::Codex)], TimeWindow::default())
    }

    #[test]
    fn claude_rows_are_deduplicated_by_message_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        // The same message id appears three times, as resume/compaction produces.
        let path = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[
                &claude_line("msg_1", "claude-opus-5", 1_000_000, 0),
                &claude_line("msg_1", "claude-opus-5", 1_000_000, 0),
                &claude_line("msg_2", "claude-opus-5", 0, 1_000_000),
            ],
        );

        let stats = aggregate_claude(&path);

        assert_eq!(
            stats.totals.request_count, 2,
            "duplicate id must be dropped"
        );
        assert_eq!(stats.totals.input_tokens, 1_000_000);
        assert_eq!(stats.totals.output_tokens, 1_000_000);
        // 1M input at $5 + 1M output at $25.
        assert_eq!(stats.totals.cost_micros, 30_000_000);
    }

    #[test]
    fn claude_dedup_spans_multiple_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[&claude_line("msg_shared", "claude-opus-5", 500, 0)],
        );
        let second = write_jsonl(
            dir.path(),
            "b.jsonl",
            &[&claude_line("msg_shared", "claude-opus-5", 500, 0)],
        );

        let stats = aggregate(
            &[(&first, Provider::Claude), (&second, Provider::Claude)],
            TimeWindow::default(),
        );

        assert_eq!(stats.totals.request_count, 1);
    }

    #[test]
    fn claude_skips_synthetic_messages() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[
                &claude_line("msg_1", "<synthetic>", 999, 999),
                &claude_line("msg_2", "claude-opus-5", 10, 10),
            ],
        );

        let stats = aggregate_claude(&path);

        assert_eq!(stats.totals.request_count, 1);
        assert_eq!(stats.totals.input_tokens, 10);
    }

    #[test]
    fn claude_counts_vendor_prefixed_models() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[&claude_line(
                "msg_1",
                "anthropic/claude-opus-5-aws",
                1_000_000,
                0,
            )],
        );

        let stats = aggregate_claude(&path);

        assert_eq!(stats.totals.cost_micros, 5_000_000);
        assert_eq!(stats.totals.unpriced_request_count, 0);
    }

    #[test]
    fn codex_counts_only_the_final_cumulative_total() {
        let dir = tempfile::tempdir().expect("tempdir");
        // total_token_usage grows over the session; summing would triple it.
        let path = write_jsonl(
            dir.path(),
            "rollout.jsonl",
            &[
                r#"{"timestamp":"2026-08-19T03:41:50.476Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
                r#"{"timestamp":"2026-08-19T03:42:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10}}}}"#,
                r#"{"timestamp":"2026-08-19T03:43:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"cached_input_tokens":0,"output_tokens":30}}}}"#,
                r#"{"timestamp":"2026-08-19T03:44:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":200,"output_tokens":50}}}}"#,
            ],
        );

        let stats = aggregate_codex(&path);

        assert_eq!(stats.totals.request_count, 1);
        // Final event only: 1000 input of which 200 cached -> 800 uncached.
        assert_eq!(stats.totals.input_tokens, 800);
        assert_eq!(stats.totals.cache_read_tokens, 200);
        assert_eq!(stats.totals.output_tokens, 50);
        assert_eq!(stats.by_model[0].model, "gpt-5.6-sol");
    }

    #[test]
    fn codex_rollout_without_model_is_reported_as_unpriced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "rollout.jsonl",
            &[
                r#"{"timestamp":"2026-08-19T03:42:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":10}}}}"#,
            ],
        );

        let stats = aggregate_codex(&path);

        // The tokens are still counted; only the cost is unknown.
        assert_eq!(stats.totals.input_tokens, 100);
        assert_eq!(stats.totals.cost_micros, 0);
        assert_eq!(stats.totals.unpriced_request_count, 1);
        assert!(!stats.by_model[0].priced);
    }

    #[test]
    fn time_window_filters_by_timestamp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[
                &format!(
                    r#"{{"timestamp":"2026-08-01T00:00:00.000Z","message":{{"id":"old","model":"claude-opus-5","usage":{{"input_tokens":5,"output_tokens":0}}}}}}"#
                ),
                &format!(
                    r#"{{"timestamp":"2026-08-20T00:00:00.000Z","message":{{"id":"new","model":"claude-opus-5","usage":{{"input_tokens":7,"output_tokens":0}}}}}}"#
                ),
            ],
        );

        let cutoff = chrono::DateTime::parse_from_rfc3339("2026-08-10T00:00:00Z")
            .unwrap()
            .timestamp_millis();

        let stats = aggregate(
            &[(&path, Provider::Claude)],
            TimeWindow {
                start_ms: Some(cutoff),
                end_ms: None,
            },
        );

        assert_eq!(stats.totals.request_count, 1);
        assert_eq!(stats.totals.input_tokens, 7);
    }

    #[test]
    fn malformed_lines_do_not_abort_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[
                "not json at all",
                r#"{"message":{"id":"no_usage","model":"claude-opus-5"}}"#,
                &claude_line("msg_ok", "claude-opus-5", 42, 0),
                "{\"truncated\": ",
            ],
        );

        let stats = aggregate_claude(&path);

        assert_eq!(stats.totals.request_count, 1);
        assert_eq!(stats.totals.input_tokens, 42);
    }

    #[test]
    fn provider_and_model_rollups_are_separated() {
        let dir = tempfile::tempdir().expect("tempdir");
        let claude = write_jsonl(
            dir.path(),
            "c.jsonl",
            &[
                &claude_line("m1", "claude-opus-5", 1_000_000, 0),
                &claude_line("m2", "claude-haiku-4-5", 1_000_000, 0),
            ],
        );
        let codex = write_jsonl(
            dir.path(),
            "x.jsonl",
            &[
                r#"{"timestamp":"2026-08-19T03:41:50.476Z","payload":{"model":"gpt-5.6-sol"}}"#,
                r#"{"timestamp":"2026-08-19T03:42:00.000Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000000,"output_tokens":0}}}}"#,
            ],
        );

        let stats = aggregate(
            &[(&claude, Provider::Claude), (&codex, Provider::Codex)],
            TimeWindow::default(),
        );

        assert_eq!(stats.by_model.len(), 3);
        assert_eq!(stats.by_provider.len(), 2);
        // Opus at $5/MTok is the most expensive row and must sort first.
        assert_eq!(stats.by_model[0].model, "claude-opus-5");
        let claude_total = stats
            .by_provider
            .iter()
            .find(|row| row.provider == "claude")
            .expect("claude rollup");
        assert_eq!(claude_total.totals.request_count, 2);
        assert_eq!(claude_total.totals.cost_micros, 6_000_000); // $5 + $1
    }

    #[test]
    fn empty_window_yields_no_usage() {
        // A window in the far future matches nothing, so the aggregate must be
        // empty regardless of what the corpus contains. Uses fixtures rather
        // than `scan_session_usage` so the test never reads the real home
        // directory (which would make it machine-dependent and slow).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[&claude_line("msg_1", "claude-opus-5", 100, 0)],
        );

        let stats = aggregate(
            &[(&path, Provider::Claude)],
            TimeWindow {
                start_ms: Some(i64::MAX - 1),
                end_ms: Some(i64::MAX),
            },
        );

        assert_eq!(stats.totals, SessionUsageTotals::default());
        assert!(!stats.truncated);
    }

    #[test]
    fn cached_parse_is_reused_and_invalidated_on_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[&claude_line("msg_1", "claude-opus-5", 100, 0)],
        );

        let first = parsed_file(&path, Provider::Claude);
        let second = parsed_file(&path, Provider::Claude);
        assert!(
            Arc::ptr_eq(&first, &second),
            "unchanged file should hit the cache"
        );
        assert_eq!(first.entries.len(), 1);

        // Appending changes both size and mtime, so the cache must re-parse.
        // Sleep past filesystem mtime granularity: a same-millisecond append
        // would otherwise be indistinguishable if the size also matched.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("reopen fixture");
        writeln!(file, "{}", claude_line("msg_2", "claude-opus-5", 200, 0))
            .expect("append fixture");
        drop(file);

        let third = parsed_file(&path, Provider::Claude);
        assert!(
            !Arc::ptr_eq(&first, &third),
            "changed file must invalidate the cache"
        );
        assert_eq!(third.entries.len(), 2);
    }
}

/// Manual validation against the real session corpus on this machine.
///
/// Ignored by default: it reads the user's actual `~/.claude` and `~/.codex`
/// directories, so results vary per machine and a cold run touches gigabytes.
/// Run with `cargo test --lib real_corpus -- --ignored --nocapture` to check the
/// counting rules against known-good figures and to see cold/warm timings.
#[cfg(test)]
mod real_corpus {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore]
    fn report_real_usage_and_cache_speedup() {
        let cold_start = Instant::now();
        let cold = scan_session_usage(TimeWindow::default());
        let cold_elapsed = cold_start.elapsed();

        let warm_start = Instant::now();
        let warm = scan_session_usage(TimeWindow::default());
        let warm_elapsed = warm_start.elapsed();

        println!("files scanned      : {}", cold.scanned_file_count);
        println!("truncated          : {}", cold.truncated);
        println!("requests           : {}", cold.totals.request_count);
        println!("input tokens       : {}", cold.totals.input_tokens);
        println!("output tokens      : {}", cold.totals.output_tokens);
        println!("cache write tokens : {}", cold.totals.cache_write_tokens);
        println!("cache read tokens  : {}", cold.totals.cache_read_tokens);
        println!(
            "estimated cost     : ${:.2}",
            cold.totals.cost_micros as f64 / 1_000_000.0
        );
        println!(
            "unpriced requests  : {}",
            cold.totals.unpriced_request_count
        );
        println!("cold scan          : {:.2}s", cold_elapsed.as_secs_f64());
        println!("warm scan          : {:.2}s", warm_elapsed.as_secs_f64());
        for row in cold.by_provider.iter() {
            println!(
                "  provider {:<8} requests={:<6} cost=${:.2}",
                row.provider,
                row.totals.request_count,
                row.totals.cost_micros as f64 / 1_000_000.0
            );
        }
        for row in cold.by_model.iter().take(8) {
            println!(
                "  model {:<38} priced={:<5} requests={:<6} cost=${:.2}",
                row.model,
                row.priced,
                row.totals.request_count,
                row.totals.cost_micros as f64 / 1_000_000.0
            );
        }

        // The corpus is live and append-only — an agent may write to its own
        // transcript while this runs — so the warm scan can legitimately see
        // more. It must never see less.
        assert!(
            warm.totals.request_count >= cold.totals.request_count,
            "warm scan lost requests: {} -> {}",
            cold.totals.request_count,
            warm.totals.request_count
        );
        assert!(
            warm.totals.cost_micros >= cold.totals.cost_micros,
            "warm scan lost cost"
        );
    }
}
