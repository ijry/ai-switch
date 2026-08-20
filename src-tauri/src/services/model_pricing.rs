//! Model price table for estimating request cost from token counts.
//!
//! Upstream responses from Anthropic, OpenAI, and Gemini report token counts but
//! no price, so a rate card is the only way to turn recorded usage into an
//! amount. Estimates are always tagged [`PriceSource::Estimated`] so the UI can
//! distinguish them from a price the upstream actually returned.
//!
//! Rates are US dollars per million tokens. Cache multipliers follow Anthropic's
//! published pricing: a cache write costs 1.25x the input rate and a cache read
//! 0.1x. Users can override or extend the table via
//! `~/.ai-switch/model-prices.json`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Micro-units per currency unit. 1 USD == 1_000_000 micros.
pub const PRICE_MICROS_PER_UNIT: i64 = 1_000_000;

/// CNY per USD, used to normalize upstream prices quoted in yuan.
///
/// Previously hardcoded inside the stats SQL; kept here so the rate lives in one
/// place. A coarse approximation — upstream USD prices are always preferred.
pub const CNY_PER_USD: f64 = 7.1;

/// Cache writes are billed above the base input rate.
const CACHE_WRITE_MULTIPLIER: f64 = 1.25;
/// Cache reads are billed well below the base input rate.
const CACHE_READ_MULTIPLIER: f64 = 0.1;

/// Where a recorded amount came from.
///
/// Persisted as the `price_source` column on `usage_events`; the string values
/// are the wire format the UI reads, so they must stay stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceSource {
    /// The upstream response carried an explicit price.
    Upstream,
    /// Computed locally from token counts and the rate table.
    Estimated,
}

impl PriceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upstream => "upstream",
            Self::Estimated => "estimated",
        }
    }
}

/// Per-million-token rates for one model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelRate {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

/// Substring patterns matched against a normalized model id, most specific
/// first — `claude-haiku` must be tested before `claude` would be.
///
/// Only families are listed rather than every dated snapshot, so a newly
/// released `claude-opus-6` still resolves to the Opus rate instead of silently
/// costing nothing. Users can correct any rate via the override file.
const STATIC_RATES: &[(&str, ModelRate)] = &[
    // Anthropic
    (
        "claude-haiku",
        ModelRate { input_per_mtok: 1.0, output_per_mtok: 5.0 },
    ),
    (
        "claude-sonnet",
        ModelRate { input_per_mtok: 3.0, output_per_mtok: 15.0 },
    ),
    (
        "claude-fable",
        ModelRate { input_per_mtok: 10.0, output_per_mtok: 50.0 },
    ),
    (
        "claude-mythos",
        ModelRate { input_per_mtok: 10.0, output_per_mtok: 50.0 },
    ),
    (
        "claude-opus",
        ModelRate { input_per_mtok: 5.0, output_per_mtok: 25.0 },
    ),
    // Legacy Anthropic ids ordered "3-5-haiku" before "3-opus" etc.
    (
        "haiku",
        ModelRate { input_per_mtok: 1.0, output_per_mtok: 5.0 },
    ),
    (
        "sonnet",
        ModelRate { input_per_mtok: 3.0, output_per_mtok: 15.0 },
    ),
    (
        "opus",
        ModelRate { input_per_mtok: 5.0, output_per_mtok: 25.0 },
    ),
    // OpenAI / Codex
    (
        "gpt-5",
        ModelRate { input_per_mtok: 1.25, output_per_mtok: 10.0 },
    ),
    (
        "gpt-4o-mini",
        ModelRate { input_per_mtok: 0.15, output_per_mtok: 0.6 },
    ),
    (
        "gpt-4o",
        ModelRate { input_per_mtok: 2.5, output_per_mtok: 10.0 },
    ),
    (
        "o4-mini",
        ModelRate { input_per_mtok: 1.1, output_per_mtok: 4.4 },
    ),
    // Google
    (
        "gemini-2.5-pro",
        ModelRate { input_per_mtok: 1.25, output_per_mtok: 10.0 },
    ),
    (
        "gemini-2.5-flash",
        ModelRate { input_per_mtok: 0.3, output_per_mtok: 2.5 },
    ),
    (
        "gemini",
        ModelRate { input_per_mtok: 0.3, output_per_mtok: 2.5 },
    ),
    // xAI
    (
        "grok",
        ModelRate { input_per_mtok: 3.0, output_per_mtok: 15.0 },
    ),
];

/// One entry in `~/.ai-switch/model-prices.json`.
#[derive(Debug, Clone, Deserialize)]
struct RateOverride {
    input_per_mtok: f64,
    output_per_mtok: f64,
}

static OVERRIDES: OnceLock<Mutex<HashMap<String, ModelRate>>> = OnceLock::new();

fn overrides() -> &'static Mutex<HashMap<String, ModelRate>> {
    OVERRIDES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Load user rate overrides, replacing anything loaded earlier.
///
/// A missing file is normal and not an error. A malformed file is ignored so a
/// bad edit degrades to the built-in table rather than breaking stats; the
/// reason is returned for logging.
pub fn load_overrides_from_str(contents: &str) -> Result<usize, String> {
    let table = parse_overrides(contents)?;
    let count = table.len();
    if let Ok(mut guard) = overrides().lock() {
        *guard = table;
    }
    Ok(count)
}

fn parse_overrides(contents: &str) -> Result<HashMap<String, ModelRate>, String> {
    let parsed: HashMap<String, RateOverride> = serde_json::from_str(contents)
        .map_err(|error| format!("model-prices.json is not valid JSON: {error}"))?;

    let mut table = HashMap::new();
    for (model, rate) in parsed {
        // A negative or non-finite rate would silently corrupt every total.
        if !rate.input_per_mtok.is_finite()
            || !rate.output_per_mtok.is_finite()
            || rate.input_per_mtok < 0.0
            || rate.output_per_mtok < 0.0
        {
            continue;
        }
        table.insert(
            normalize_model_id(&model).unwrap_or_else(|| model.to_ascii_lowercase()),
            ModelRate {
                input_per_mtok: rate.input_per_mtok,
                output_per_mtok: rate.output_per_mtok,
            },
        );
    }

    Ok(table)
}

/// Reduce a raw model id to a comparable key.
///
/// Session logs and gateways decorate ids in ways that must not defeat matching:
/// vendor prefixes (`anthropic/claude-opus-5-aws`), context suffixes
/// (`claude-opus-4-8[1m]`), and surrounding whitespace.
///
/// Returns `None` for ids that never correspond to billable upstream usage —
/// notably Claude Code's `<synthetic>` marker for locally generated messages.
pub fn normalize_model_id(model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() || trimmed.starts_with('<') {
        return None;
    }
    // Keep only the last path segment: "anthropic/claude-opus-5" -> "claude-opus-5".
    let tail = trimmed.rsplit('/').next().unwrap_or(trimmed);
    let lowered = tail.to_ascii_lowercase();
    // Drop a "[1m]"-style context-window suffix.
    let base = lowered.split('[').next().unwrap_or(&lowered).trim();
    (!base.is_empty()).then(|| base.to_string())
}

/// Look up the rate for a model, preferring a user override over the built-in
/// table. Returns `None` when the model is unbillable or unrecognized — callers
/// must leave the amount empty rather than assume zero cost.
pub fn rate_for_model(model: &str) -> Option<ModelRate> {
    let key = normalize_model_id(model)?;

    if let Ok(guard) = overrides().lock() {
        if let Some(rate) = lookup_in(&guard, &key) {
            return Some(rate);
        }
    }

    static_rate_for_key(&key)
}

/// Resolve `key` against an override table: exact match first, then the longest
/// matching family pattern (so an override keyed `claude-opus` still applies).
fn lookup_in(table: &HashMap<String, ModelRate>, key: &str) -> Option<ModelRate> {
    if let Some(rate) = table.get(key) {
        return Some(*rate);
    }
    table
        .iter()
        .filter(|(pattern, _)| key.contains(pattern.as_str()))
        .max_by_key(|(pattern, _)| pattern.len())
        .map(|(_, rate)| *rate)
}

fn static_rate_for_key(key: &str) -> Option<ModelRate> {
    STATIC_RATES
        .iter()
        .find(|(pattern, _)| key.contains(pattern))
        .map(|(_, rate)| *rate)
}

/// Token counts for one request, as recorded in usage logs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    /// Uncached input tokens, billed at the full input rate.
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Tokens written to the cache, billed above the input rate.
    pub cache_write_tokens: i64,
    /// Tokens served from the cache, billed well below the input rate.
    pub cache_read_tokens: i64,
}

/// Estimate the cost of `usage` on `model`, in USD micros.
///
/// Returns `None` when the model has no known rate, so an unpriced model reads
/// as "unknown" rather than as free.
pub fn estimate_cost_micros(model: &str, usage: TokenUsage) -> Option<i64> {
    let rate = rate_for_model(model)?;
    Some(estimate_cost_micros_with_rate(rate, usage))
}

pub fn estimate_cost_micros_with_rate(rate: ModelRate, usage: TokenUsage) -> i64 {
    // Negative counts would subtract from the total; treat them as absent.
    let billable = |value: i64| value.max(0) as f64;

    let dollars = (billable(usage.input_tokens) * rate.input_per_mtok
        + billable(usage.cache_write_tokens) * rate.input_per_mtok * CACHE_WRITE_MULTIPLIER
        + billable(usage.cache_read_tokens) * rate.input_per_mtok * CACHE_READ_MULTIPLIER
        + billable(usage.output_tokens) * rate.output_per_mtok)
        / 1_000_000.0;

    (dollars * PRICE_MICROS_PER_UNIT as f64).round() as i64
}

/// Convert a CNY amount in micros to USD micros.
///
/// The stats summary performs this same division inside SQL (it has to, to
/// aggregate across rows). Keeping the Rust equivalent here gives that formula a
/// unit test and a single definition of the rate, so the two cannot drift.
#[cfg_attr(not(test), allow(dead_code))]
pub fn cny_micros_to_usd_micros(cny_micros: i64) -> i64 {
    (cny_micros as f64 / CNY_PER_USD).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_vendor_prefix_and_context_suffix() {
        assert_eq!(
            normalize_model_id("anthropic/claude-opus-5-aws").as_deref(),
            Some("claude-opus-5-aws")
        );
        assert_eq!(
            normalize_model_id("claude-opus-4-8[1m]").as_deref(),
            Some("claude-opus-4-8")
        );
        assert_eq!(
            normalize_model_id("  Claude-Sonnet-5  ").as_deref(),
            Some("claude-sonnet-5")
        );
    }

    #[test]
    fn normalize_rejects_synthetic_and_empty_models() {
        // Claude Code marks locally generated messages "<synthetic>"; they are
        // never billed and must not be priced.
        assert_eq!(normalize_model_id("<synthetic>"), None);
        assert_eq!(normalize_model_id("   "), None);
        assert_eq!(rate_for_model("<synthetic>"), None);
    }

    #[test]
    fn rate_lookup_matches_observed_session_model_ids() {
        // Every id below was observed in real ~/.claude and ~/.codex session logs.
        for model in [
            "claude-opus-4-8",
            "claude-opus-5",
            "anthropic/claude-opus-5-aws",
            "anthropic/claude-opus-5-ps-aws-dst",
        ] {
            let rate = rate_for_model(model).unwrap_or_else(|| panic!("no rate for {model}"));
            assert_eq!(rate.input_per_mtok, 5.0, "input rate for {model}");
            assert_eq!(rate.output_per_mtok, 25.0, "output rate for {model}");
        }
        assert_eq!(rate_for_model("gpt-5.6-sol").map(|r| r.input_per_mtok), Some(1.25));
    }

    #[test]
    fn haiku_matches_before_generic_families() {
        // Ordering matters: a substring table must not let a broader pattern win.
        assert_eq!(rate_for_model("claude-haiku-4-5").map(|r| r.output_per_mtok), Some(5.0));
        assert_eq!(rate_for_model("claude-3-5-haiku").map(|r| r.output_per_mtok), Some(5.0));
        assert_eq!(rate_for_model("gpt-4o-mini").map(|r| r.input_per_mtok), Some(0.15));
    }

    #[test]
    fn unknown_model_has_no_rate() {
        assert_eq!(rate_for_model("some-unreleased-model"), None);
        assert_eq!(estimate_cost_micros("some-unreleased-model", TokenUsage::default()), None);
    }

    #[test]
    fn estimate_applies_cache_multipliers() {
        // 1M uncached input at $5 = $5; 1M cache writes at 1.25x = $6.25;
        // 1M cache reads at 0.1x = $0.50; 1M output at $25 = $25. Total $36.75.
        let cost = estimate_cost_micros(
            "claude-opus-5",
            TokenUsage {
                input_tokens: 1_000_000,
                output_tokens: 1_000_000,
                cache_write_tokens: 1_000_000,
                cache_read_tokens: 1_000_000,
            },
        );

        assert_eq!(cost, Some(36_750_000));
    }

    #[test]
    fn estimate_ignores_negative_counts() {
        let cost = estimate_cost_micros(
            "claude-opus-5",
            TokenUsage { input_tokens: -5, output_tokens: 1_000_000, ..TokenUsage::default() },
        );

        assert_eq!(cost, Some(25_000_000));
    }

    #[test]
    fn cny_conversion_matches_existing_stats_rate() {
        // Mirrors the assertion in the route pool repository tests.
        assert_eq!(cny_micros_to_usd_micros(7_100_000), 1_000_000);
    }

    #[test]
    fn overrides_take_precedence_and_reject_bad_values() {
        // Parsed into a local table rather than the process-wide one: these tests
        // share a process and run in parallel, so mutating global state here
        // would race against every other rate assertion.
        let table = parse_overrides(
            r#"{
                "claude-opus-5": {"input_per_mtok": 1.0, "output_per_mtok": 2.0},
                "bogus-negative": {"input_per_mtok": -1.0, "output_per_mtok": 2.0}
            }"#,
        )
        .expect("overrides parse");

        assert_eq!(table.len(), 1, "the negative rate must be dropped");
        assert_eq!(lookup_in(&table, "claude-opus-5").map(|r| r.input_per_mtok), Some(1.0));
        assert_eq!(lookup_in(&table, "bogus-negative"), None);
        // A model the override does not mention falls through to the static table.
        assert_eq!(lookup_in(&table, "claude-haiku-4-5"), None);
        assert_eq!(static_rate_for_key("claude-haiku-4-5").map(|r| r.input_per_mtok), Some(1.0));
    }

    #[test]
    fn override_can_be_keyed_by_model_family() {
        let table = parse_overrides(r#"{"claude-opus": {"input_per_mtok": 4.0, "output_per_mtok": 20.0}}"#)
            .expect("overrides parse");

        // A discounted-gateway rate keyed by family applies to every Opus id.
        assert_eq!(lookup_in(&table, "claude-opus-5-aws").map(|r| r.input_per_mtok), Some(4.0));
    }

    #[test]
    fn malformed_override_file_is_reported_not_panicked() {
        assert!(parse_overrides("not json at all").is_err());
        assert!(load_overrides_from_str("not json at all").is_err());
        // The static table remains usable after a failed load.
        assert_eq!(static_rate_for_key("claude-opus-5").map(|r| r.input_per_mtok), Some(5.0));
    }
}
