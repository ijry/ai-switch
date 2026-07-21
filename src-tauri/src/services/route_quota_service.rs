use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::error::AppError;
use crate::models::route_credential::RouteCredential;
use crate::services::http_client::build_outbound_http_client;
use crate::services::route_proxy_service::{
    apply_official_quota_snapshot, maybe_refresh_official_credential, OfficialQuotaSnapshot,
    SelectedCredential,
};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::time::Duration;

const QUOTA_HTTP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaRefreshOutcome {
    pub credential: RouteCredential,
    pub updated: bool,
    pub source: String,
    pub message: Option<String>,
}

pub struct RouteQuotaService;

impl RouteQuotaService {
    pub async fn refresh_one(
        pool: &SqlitePool,
        id: String,
    ) -> Result<QuotaRefreshOutcome, AppError> {
        let credential = RouteCredentialRepository::get(pool, &id).await?;
        refresh_credential(pool, credential).await
    }

    pub async fn refresh_platform(
        pool: &SqlitePool,
        platform: String,
    ) -> Result<Vec<QuotaRefreshOutcome>, AppError> {
        let platform = normalize_platform(&platform)?;
        let credentials = RouteCredentialRepository::list_by_platform(pool, &platform).await?;
        let mut outcomes = Vec::with_capacity(credentials.len());
        for credential in credentials {
            if credential.kind != "official" {
                continue;
            }
            match refresh_credential(pool, credential.clone()).await {
                Ok(outcome) => outcomes.push(outcome),
                Err(err) => outcomes.push(QuotaRefreshOutcome {
                    credential,
                    updated: false,
                    source: "error".to_string(),
                    message: Some(err.to_string()),
                }),
            }
        }
        Ok(outcomes)
    }
}

async fn refresh_credential(
    pool: &SqlitePool,
    credential: RouteCredential,
) -> Result<QuotaRefreshOutcome, AppError> {
    if credential.kind != "official" {
        return Ok(QuotaRefreshOutcome {
            credential,
            updated: false,
            source: "skipped".to_string(),
            message: Some("Only official accounts support quota refresh".to_string()),
        });
    }

    let selected = SelectedCredential {
        id: credential.id.clone(),
        platform: credential.platform.clone(),
        kind: credential.kind.clone(),
        display_name: credential.display_name.clone(),
        secret_payload_json: credential.secret_payload_json.clone(),
        config_json: credential.config_json.clone(),
    };

    let refreshed = maybe_refresh_official_credential(pool, &selected)
        .await
        .map_err(|message| AppError::Validation {
            code: "validation.route_quota_oauth_refresh",
            message,
            details: Some(credential.id.clone()),
            recoverable: true,
        })?;

    let fetch = fetch_official_quota_snapshot(
        &refreshed.platform,
        &refreshed.secret_payload_json,
        &refreshed.config_json,
    )
    .await?;

    let Some((snapshot, source)) = fetch else {
        // Token refresh may still have updated secret/config.
        let latest = RouteCredentialRepository::get(pool, &credential.id).await?;
        return Ok(QuotaRefreshOutcome {
            updated: latest.updated_at != credential.updated_at
                || latest.secret_payload_json != credential.secret_payload_json
                || latest.config_json != credential.config_json,
            credential: latest,
            source: "none".to_string(),
            message: Some("No official quota endpoint returned usable remaining data".to_string()),
        });
    };

    let next_config =
        apply_official_quota_snapshot(&refreshed.config_json, &snapshot).map_err(|message| {
            AppError::Validation {
                code: "validation.route_quota_apply",
                message,
                details: Some(credential.id.clone()),
                recoverable: true,
            }
        })?;

    if next_config == refreshed.config_json
        && refreshed.secret_payload_json == credential.secret_payload_json
    {
        let latest = RouteCredentialRepository::get(pool, &credential.id).await?;
        return Ok(QuotaRefreshOutcome {
            credential: latest,
            updated: false,
            source,
            message: None,
        });
    }

    RouteCredentialRepository::update_secret_and_config(
        pool,
        &credential.id,
        &refreshed.secret_payload_json,
        &next_config,
    )
    .await?;

    let latest = RouteCredentialRepository::get(pool, &credential.id).await?;
    Ok(QuotaRefreshOutcome {
        credential: latest,
        updated: true,
        source,
        message: None,
    })
}

async fn fetch_official_quota_snapshot(
    platform: &str,
    secret_payload_json: &str,
    config_json: &str,
) -> Result<Option<(OfficialQuotaSnapshot, String)>, AppError> {
    let secret = parse_json_object(secret_payload_json, "secret")?;
    let config = parse_json_object(config_json, "config")?;
    let access_token = string_value(&secret, "access_token")
        .ok_or_else(|| AppError::Validation {
            code: "validation.route_quota_access_token",
            message: "Official account is missing access_token for quota refresh".to_string(),
            details: None,
            recoverable: true,
        })?
        .to_string();

    let platform_key = normalize_platform(platform)?;
    let candidates = quota_endpoint_candidates(&platform_key, &config);
    if candidates.is_empty() {
        return Ok(None);
    }

    let client = build_outbound_http_client(Some(QUOTA_HTTP_TIMEOUT)).map_err(|message| {
        AppError::Validation {
            code: "validation.route_quota_http_client",
            message,
            details: None,
            recoverable: true,
        }
    })?;

    let mut last_error: Option<String> = None;
    for candidate in candidates {
        match request_quota_snapshot(&client, &platform_key, &access_token, &secret, &candidate)
            .await
        {
            Ok(Some(snapshot)) => return Ok(Some((snapshot, candidate.source))),
            Ok(None) => continue,
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    // Endpoint may be unavailable/unknown for this account shape; keep remain NULL.
    if last_error.is_some() {
        return Ok(None);
    }
    Ok(None)
}

#[derive(Debug, Clone)]
struct QuotaEndpointCandidate {
    url: String,
    source: String,
    style: QuotaEndpointStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuotaEndpointStyle {
    CodexWham,
    ClaudeOauthUsage,
    GrokStatus,
}

fn quota_endpoint_candidates(platform: &str, config: &Value) -> Vec<QuotaEndpointCandidate> {
    let mut out = Vec::new();
    match platform {
        "codex" => {
            out.push(QuotaEndpointCandidate {
                url: "https://chatgpt.com/backend-api/wham/usage".to_string(),
                source: "codex.wham_usage".to_string(),
                style: QuotaEndpointStyle::CodexWham,
            });
        }
        "claude" => {
            out.push(QuotaEndpointCandidate {
                url: "https://api.anthropic.com/api/oauth/usage".to_string(),
                source: "claude.oauth_usage".to_string(),
                style: QuotaEndpointStyle::ClaudeOauthUsage,
            });
            // Some account tokens are bound to claude.ai host instead of api.anthropic.com.
            out.push(QuotaEndpointCandidate {
                url: "https://claude.ai/api/oauth/usage".to_string(),
                source: "claude.web_oauth_usage".to_string(),
                style: QuotaEndpointStyle::ClaudeOauthUsage,
            });
        }
        "grok" => {
            let base = string_value(config, "base_url")
                .unwrap_or("https://cli-chat-proxy.grok.com/v1")
                .trim_end_matches('/')
                .to_string();
            // Best-effort status endpoints; parsers keep remain NULL unless fields are present.
            out.push(QuotaEndpointCandidate {
                url: format!("{base}/usage"),
                source: "grok.usage".to_string(),
                style: QuotaEndpointStyle::GrokStatus,
            });
            out.push(QuotaEndpointCandidate {
                url: format!("{base}/me"),
                source: "grok.me".to_string(),
                style: QuotaEndpointStyle::GrokStatus,
            });
            if !base.contains("api.x.ai") {
                out.push(QuotaEndpointCandidate {
                    url: "https://api.x.ai/v1/usage".to_string(),
                    source: "grok.api_usage".to_string(),
                    style: QuotaEndpointStyle::GrokStatus,
                });
            }
        }
        _ => {}
    }
    out
}

async fn request_quota_snapshot(
    client: &reqwest::Client,
    platform: &str,
    access_token: &str,
    secret: &Value,
    candidate: &QuotaEndpointCandidate,
) -> Result<Option<OfficialQuotaSnapshot>, String> {
    let mut request = client
        .get(&candidate.url)
        .header("authorization", format!("Bearer {access_token}"))
        .header("accept", "application/json");

    match candidate.style {
        QuotaEndpointStyle::CodexWham => {
            if let Some(account_id) = string_value(secret, "account_id") {
                request = request.header("chatgpt-account-id", account_id);
            }
            request = request.header("user-agent", "ai-switch/0.1");
        }
        QuotaEndpointStyle::ClaudeOauthUsage => {
            request = request
                .header("anthropic-beta", "oauth-2025-04-20")
                .header("user-agent", "ai-switch/0.1");
        }
        QuotaEndpointStyle::GrokStatus => {
            request = request
                .header("user-agent", "grok-cli")
                .header("x-client-name", "grok-cli")
                .header("x-app-version", "0.2.93")
                .header("x-token-auth", "xai-grok-cli");
        }
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("Quota request failed for {}: {err}", candidate.url))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("Quota response read failed for {}: {err}", candidate.url))?;

    if status.as_u16() == 404 || status.as_u16() == 405 {
        return Ok(None);
    }
    if !status.is_success() {
        // Auth/network failures should surface for single-endpoint attempts, but callers
        // may continue to the next candidate.
        return Err(format!(
            "Quota request failed for {} with status {}: {}",
            candidate.url,
            status.as_u16(),
            body.chars().take(240).collect::<String>()
        ));
    }

    let value = match serde_json::from_str::<Value>(&body) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let snapshot = match candidate.style {
        QuotaEndpointStyle::CodexWham => parse_codex_quota_snapshot(&value),
        QuotaEndpointStyle::ClaudeOauthUsage => parse_claude_quota_snapshot(&value),
        QuotaEndpointStyle::GrokStatus => parse_grok_quota_snapshot(&value),
    };

    let _ = platform;
    Ok(snapshot)
}

pub fn parse_codex_quota_snapshot(value: &Value) -> Option<OfficialQuotaSnapshot> {
    // Preferred ChatGPT/Codex WHAM shape.
    if let Some(rate_limit) = value.get("rate_limit").or_else(|| value.get("rateLimit")) {
        let primary = window_remain(
            rate_limit
                .get("primary_window")
                .or_else(|| rate_limit.get("primaryWindow"))
                .or_else(|| rate_limit.get("five_hour"))
                .or_else(|| rate_limit.get("fiveHour")),
        );
        let weekly = window_remain(
            rate_limit
                .get("secondary_window")
                .or_else(|| rate_limit.get("secondaryWindow"))
                .or_else(|| rate_limit.get("weekly"))
                .or_else(|| rate_limit.get("seven_day"))
                .or_else(|| rate_limit.get("sevenDay")),
        );
        if primary.is_some() || weekly.is_some() {
            return Some(merge_windows(primary, weekly, subscription_type_from(value)));
        }
    }

    // Flat five_hour / weekly objects.
    let primary = window_remain(
        value
            .get("five_hour")
            .or_else(|| value.get("fiveHour"))
            .or_else(|| value.get("primary_window"))
            .or_else(|| value.get("primaryWindow")),
    );
    let weekly = window_remain(
        value
            .get("weekly")
            .or_else(|| value.get("seven_day"))
            .or_else(|| value.get("sevenDay"))
            .or_else(|| value.get("secondary_window"))
            .or_else(|| value.get("secondaryWindow")),
    );
    if primary.is_some() || weekly.is_some() {
        return Some(merge_windows(primary, weekly, subscription_type_from(value)));
    }
    None
}

pub fn parse_claude_quota_snapshot(value: &Value) -> Option<OfficialQuotaSnapshot> {
    // Claude oauth usage often exposes five_hour / seven_day utilization.
    let primary = window_remain(
        value
            .get("five_hour")
            .or_else(|| value.get("fiveHour"))
            .or_else(|| value.pointer("/rate_limit/five_hour"))
            .or_else(|| value.pointer("/rateLimit/fiveHour")),
    );
    let weekly = window_remain(
        value
            .get("seven_day")
            .or_else(|| value.get("sevenDay"))
            .or_else(|| value.get("weekly"))
            .or_else(|| value.pointer("/rate_limit/seven_day"))
            .or_else(|| value.pointer("/rateLimit/sevenDay")),
    );
    if primary.is_some() || weekly.is_some() {
        return Some(merge_windows(primary, weekly, subscription_type_from(value)));
    }
    // Some payloads nest under "usage".
    if let Some(usage) = value.get("usage") {
        return parse_claude_quota_snapshot(usage);
    }
    None
}

pub fn parse_grok_quota_snapshot(value: &Value) -> Option<OfficialQuotaSnapshot> {
    // Prefer explicit remaining fields; never invent 0 unless exhausted is proven.
    if let Some(snapshot) = parse_codex_quota_snapshot(value) {
        return Some(snapshot);
    }

    let subscription_type = subscription_type_from(value).or_else(|| {
        string_value(value, "plan")
            .or_else(|| string_value(value, "tier"))
            .map(str::to_string)
    });

    let primary = first_i64(
        value,
        &[
            "primary_remain",
            "quota_remaining",
            "remaining",
            "remaining_tokens",
            "tokens_remaining",
        ],
    )
    .or_else(|| {
        let used = first_i64(value, &["quota_used", "used", "tokens_used", "actual"]);
        let limit = first_i64(value, &["quota_limit", "limit", "tokens_limit"]);
        match (used, limit) {
            (Some(used), Some(limit)) => Some((limit - used).max(0)),
            _ => None,
        }
    });
    let weekly = first_i64(value, &["weekly_remain", "weekly_remaining"]);
    let reset_primary = first_time(
        value,
        &["reset_primary", "resets_at", "reset_at", "resetAt", "expires_at"],
    );
    let reset_weekly = first_time(value, &["reset_weekly", "weekly_reset_at", "weeklyResetAt"]);

    if primary.is_none() && weekly.is_none() && subscription_type.is_none() {
        return None;
    }

    let quota_limit = first_i64(value, &["quota_limit", "limit", "tokens_limit"]);
    let quota_used = first_i64(value, &["quota_used", "used", "tokens_used", "actual"]);

    Some(OfficialQuotaSnapshot {
        subscription_type,
        primary_remain: primary,
        weekly_remain: weekly,
        reset_primary,
        reset_weekly,
        quota_remaining: primary,
        quota_limit,
        quota_used,
    })
}

#[derive(Debug, Clone)]
struct WindowRemain {
    remain: Option<i64>,
    limit: Option<i64>,
    used: Option<i64>,
    reset_at: Option<String>,
}

fn window_remain(value: Option<&Value>) -> Option<WindowRemain> {
    let value = value?;
    if value.is_null() {
        return None;
    }

    let limit = first_i64(value, &["limit", "allowed", "quota", "max", "total"]);
    let used = first_i64(value, &["used", "usage", "consumed"]);
    let remain = first_i64(
        value,
        &[
            "remaining",
            "remain",
            "left",
            "tokens_remaining",
            "remaining_tokens",
        ],
    )
    .or_else(|| match (limit, used) {
        (Some(limit), Some(used)) => Some((limit - used).max(0)),
        _ => None,
    })
    .or_else(|| {
        // utilization / used_percent -> remaining percent in 0..=100.
        let used_percent = first_f64(
            value,
            &[
                "used_percent",
                "usedPercent",
                "utilization",
                "utilized",
                "percent_used",
            ],
        )?;
        let percent = if used_percent <= 1.0 {
            used_percent * 100.0
        } else {
            used_percent
        };
        Some((100.0 - percent).round().clamp(0.0, 100.0) as i64)
    });

    let reset_at = first_time(
        value,
        &[
            "reset_at",
            "resetAt",
            "resets_at",
            "resetsAt",
            "reset_time",
            "resetTime",
            "expires_at",
            "expiresAt",
        ],
    );

    if remain.is_none() && limit.is_none() && used.is_none() && reset_at.is_none() {
        return None;
    }

    Some(WindowRemain {
        remain,
        limit,
        used,
        reset_at,
    })
}

fn merge_windows(
    primary: Option<WindowRemain>,
    weekly: Option<WindowRemain>,
    subscription_type: Option<String>,
) -> OfficialQuotaSnapshot {
    let primary_remain = primary.as_ref().and_then(|item| item.remain);
    let weekly_remain = weekly.as_ref().and_then(|item| item.remain);
    OfficialQuotaSnapshot {
        subscription_type,
        primary_remain,
        weekly_remain,
        reset_primary: primary.as_ref().and_then(|item| item.reset_at.clone()),
        reset_weekly: weekly.as_ref().and_then(|item| item.reset_at.clone()),
        quota_remaining: primary_remain,
        quota_limit: primary.as_ref().and_then(|item| item.limit),
        quota_used: primary.as_ref().and_then(|item| item.used),
    }
}

fn subscription_type_from(value: &Value) -> Option<String> {
    string_value(value, "subscription_type")
        .or_else(|| string_value(value, "subscriptionType"))
        .or_else(|| string_value(value, "plan"))
        .or_else(|| string_value(value, "plan_type"))
        .or_else(|| string_value(value, "planType"))
        .or_else(|| string_value(value, "tier"))
        .or_else(|| value.pointer("/subscription/type").and_then(Value::as_str))
        .or_else(|| value.pointer("/account/plan").and_then(Value::as_str))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

fn first_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(item) = value.get(*key) {
            if let Some(number) = as_i64(item) {
                return Some(number);
            }
        }
        // Support nested common containers.
        if let Some(item) = value.pointer(&format!("/{key}")) {
            if let Some(number) = as_i64(item) {
                return Some(number);
            }
        }
    }
    None
}

fn first_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(item) = value.get(*key) {
            if let Some(number) = as_f64(item) {
                return Some(number);
            }
        }
    }
    None
}

fn first_time(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(item) = value.get(*key) {
            if let Some(text) = as_time_string(item) {
                return Some(text);
            }
        }
    }
    None
}

fn as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|n| n.round() as i64)),
        Value::String(text) => text.trim().parse::<i64>().ok().or_else(|| {
            text.trim()
                .parse::<f64>()
                .ok()
                .map(|n| n.round() as i64)
        }),
        _ => None,
    }
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn as_time_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            Some(text.to_string())
        }
        Value::Number(number) => {
            let raw = number.as_i64().or_else(|| number.as_f64().map(|n| n as i64))?;
            // Accept seconds or milliseconds epoch.
            let seconds = if raw > 10_000_000_000 { raw / 1000 } else { raw };
            Utc.timestamp_opt(seconds, 0)
                .single()
                .map(|dt| dt.to_rfc3339())
        }
        _ => None,
    }
}

fn parse_json_object(raw: &str, label: &str) -> Result<Value, AppError> {
    let value = serde_json::from_str::<Value>(raw).map_err(|err| AppError::Validation {
        code: "validation.route_quota_json",
        message: format!("Route credential {label} JSON is invalid: {err}"),
        details: None,
        recoverable: true,
    })?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(AppError::Validation {
            code: "validation.route_quota_json_object",
            message: format!("Route credential {label} JSON must be an object"),
            details: None,
            recoverable: true,
        })
    }
}

fn string_value<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
}

fn normalize_platform(platform: &str) -> Result<String, AppError> {
    let platform = platform.trim();
    if platform.is_empty() {
        return Err(AppError::Validation {
            code: "validation.platform_required",
            message: "Platform is required".to_string(),
            details: None,
            recoverable: true,
        });
    }
    let lower = platform.to_lowercase();
    if lower.contains("grok") || lower == "xai" || lower.contains("x.ai") || lower == "x-ai" {
        return Ok("grok".to_string());
    }
    if lower.contains("claude") || lower.contains("anthropic") {
        return Ok("claude".to_string());
    }
    if lower.contains("codex") || lower.contains("openai") || lower.contains("chatgpt") {
        return Ok("codex".to_string());
    }
    Ok(platform.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::open_migrated_pool;
    use crate::services::route_proxy_service::is_route_credential_quota_available;
    use assert_fs::TempDir;

    #[test]
    fn parse_codex_primary_weekly_windows() {
        let value = json!({
            "plan": "plus",
            "rate_limit": {
                "primary_window": {
                    "limit": 100,
                    "used": 35,
                    "reset_at": 1780000000
                },
                "secondary_window": {
                    "limit": 1000,
                    "used": 200,
                    "reset_at": 1780600000
                }
            }
        });
        let snapshot = parse_codex_quota_snapshot(&value).expect("snapshot");
        assert_eq!(snapshot.subscription_type.as_deref(), Some("plus"));
        assert_eq!(snapshot.primary_remain, Some(65));
        assert_eq!(snapshot.weekly_remain, Some(800));
        assert_eq!(snapshot.quota_limit, Some(100));
        assert_eq!(snapshot.quota_used, Some(35));
        assert!(snapshot.reset_primary.as_deref().is_some_and(|value| value.contains('T')));
    }

    #[test]
    fn parse_claude_utilization_percent() {
        let value = json!({
            "five_hour": { "utilization": 0.25, "resets_at": "2026-07-22T12:00:00Z" },
            "seven_day": { "used_percent": 10.0, "resets_at": "2026-07-28T12:00:00Z" },
            "subscription_type": "pro"
        });
        let snapshot = parse_claude_quota_snapshot(&value).expect("snapshot");
        assert_eq!(snapshot.subscription_type.as_deref(), Some("pro"));
        assert_eq!(snapshot.primary_remain, Some(75));
        assert_eq!(snapshot.weekly_remain, Some(90));
        assert_eq!(
            snapshot.reset_primary.as_deref(),
            Some("2026-07-22T12:00:00Z")
        );
        assert_eq!(
            snapshot.reset_weekly.as_deref(),
            Some("2026-07-28T12:00:00Z")
        );
    }

    #[test]
    fn parse_grok_remaining_fields() {
        let value = json!({
            "plan": "free",
            "remaining_tokens": 120,
            "quota_limit": 1000,
            "quota_used": 880,
            "reset_at": "2026-07-22T18:00:00Z"
        });
        let snapshot = parse_grok_quota_snapshot(&value).expect("snapshot");
        assert_eq!(snapshot.subscription_type.as_deref(), Some("free"));
        assert_eq!(snapshot.primary_remain, Some(120));
        assert_eq!(snapshot.quota_limit, Some(1000));
        assert_eq!(snapshot.quota_used, Some(880));
        assert_eq!(
            snapshot.reset_primary.as_deref(),
            Some("2026-07-22T18:00:00Z")
        );
    }

    #[test]
    fn apply_snapshot_keeps_zero_out_of_pool() {
        let snapshot = OfficialQuotaSnapshot {
            subscription_type: Some("free".to_string()),
            primary_remain: Some(0),
            weekly_remain: Some(12),
            reset_primary: Some("2026-07-22T00:00:00Z".to_string()),
            reset_weekly: Some("2026-07-28T00:00:00Z".to_string()),
            quota_remaining: Some(0),
            quota_limit: Some(1000),
            quota_used: Some(1000),
        };
        let next = apply_official_quota_snapshot("{}", &snapshot).expect("config");
        assert!(!is_route_credential_quota_available(&next));
        assert!(next.contains("\"primary_remain\":0"));
    }

    #[tokio::test]
    async fn refresh_one_skips_non_official_accounts() {
        let temp = TempDir::new().unwrap();
        let db = temp.path().join("quota.db");
        let pool = open_migrated_pool(&db, temp.path()).await.unwrap();
        let created = RouteCredentialRepository::create(
            &pool,
            "grok",
            "api",
            "api-only",
            None,
            "ok",
            None,
            r#"{"api_key":"k"}"#,
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}"#,
            "{}",
        )
        .await
        .unwrap();

        let outcome = RouteQuotaService::refresh_one(&pool, created.id.clone())
            .await
            .unwrap();
        assert!(!outcome.updated);
        assert_eq!(outcome.source, "skipped");
        assert_eq!(outcome.credential.id, created.id);
    }
}
