use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::error::AppError;
use crate::models::route_credential::RouteCredential;
use crate::services::http_client::build_outbound_http_client;
use crate::services::official_agent_identity_service::{
    resolve_agent_identity_headers, AgentIdentityHeaders,
};
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
const QUOTA_BODY_SNIPPET_CHARS: usize = 240;
const CODEX_QUOTA_CLI_ORIGINATOR: &str = "codex_cli_rs";
const CODEX_QUOTA_AGENT_IDENTITY_ORIGINATOR: &str = "Codex Desktop";
const CODEX_QUOTA_CLI_USER_AGENT: &str = "codex_cli_rs/0.1.0";
const CODEX_QUOTA_AGENT_IDENTITY_USER_AGENT: &str = "Codex Desktop/0.1.0";
const CODEX_QUOTA_OPENAI_BETA: &str = "codex-1";
const CODEX_QUOTA_LANGUAGE: &str = "zh-CN";
const CODEX_QUOTA_SEC_FETCH_SITE: &str = "none";
const CODEX_QUOTA_SEC_FETCH_MODE: &str = "no-cors";
const CODEX_QUOTA_SEC_FETCH_DEST: &str = "empty";
const CODEX_QUOTA_PRIORITY: &str = "u=4, i";

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
        status: credential.status.clone(),
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

    let Some((snapshot, source)) = fetch.snapshot else {
        // Token refresh may still have updated secret/config.
        let latest = RouteCredentialRepository::get(pool, &credential.id).await?;
        return Ok(QuotaRefreshOutcome {
            updated: latest.updated_at != credential.updated_at
                || latest.secret_payload_json != credential.secret_payload_json
                || latest.config_json != credential.config_json,
            credential: latest,
            source: "none".to_string(),
            message: Some(fetch.message.unwrap_or_else(|| {
                "No official quota endpoint returned usable remaining data".to_string()
            })),
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

#[derive(Debug, Clone)]
struct QuotaFetchResult {
    snapshot: Option<(OfficialQuotaSnapshot, String)>,
    message: Option<String>,
}

async fn fetch_official_quota_snapshot(
    platform: &str,
    secret_payload_json: &str,
    config_json: &str,
) -> Result<QuotaFetchResult, AppError> {
    let secret = parse_json_object(secret_payload_json, "secret")?;
    let config = parse_json_object(config_json, "config")?;
    let auth = quota_request_auth(&secret, &config)?;

    let platform_key = normalize_platform(platform)?;
    let candidates = quota_endpoint_candidates(&platform_key, &config);
    if candidates.is_empty() {
        return Ok(QuotaFetchResult {
            snapshot: None,
            message: Some("No official quota endpoint is configured for this platform".to_string()),
        });
    }

    let client = build_outbound_http_client(Some(QUOTA_HTTP_TIMEOUT)).map_err(|message| {
        AppError::Validation {
            code: "validation.route_quota_http_client",
            message,
            details: None,
            recoverable: true,
        }
    })?;

    let mut diagnostics: Vec<String> = Vec::new();
    for candidate in candidates {
        match request_quota_snapshot(&client, &platform_key, &auth, &secret, &candidate)
            .await
        {
            Ok(Some(snapshot)) => {
                return Ok(QuotaFetchResult {
                    snapshot: Some((snapshot, candidate.source)),
                    message: None,
                });
            }
            Ok(None) => continue,
            Err(err) => {
                diagnostics.push(err);
            }
        }
    }

    // Endpoint may be unavailable/unknown for this account shape; keep remain NULL.
    Ok(QuotaFetchResult {
        snapshot: None,
        message: quota_fetch_failure_message(&diagnostics),
    })
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

#[derive(Debug, Clone)]
enum QuotaRequestAuth {
    Bearer(String),
    AgentIdentity(AgentIdentityHeaders),
}

fn quota_request_auth(secret: &Value, config: &Value) -> Result<QuotaRequestAuth, AppError> {
    if let Some(agent_identity) =
        resolve_agent_identity_headers(secret, config).map_err(|message| AppError::Validation {
            code: "validation.route_quota_agent_identity",
            message,
            details: None,
            recoverable: true,
        })?
    {
        return Ok(QuotaRequestAuth::AgentIdentity(agent_identity));
    }

    string_value(secret, "access_token")
        .map(|value| QuotaRequestAuth::Bearer(value.to_string()))
        .ok_or_else(|| AppError::Validation {
            code: "validation.route_quota_access_token",
            message: "Official account is missing access_token for quota refresh".to_string(),
            details: None,
            recoverable: true,
        })
}

fn quota_endpoint_candidates(platform: &str, config: &Value) -> Vec<QuotaEndpointCandidate> {
    let mut out = Vec::new();
    match platform {
        "codex" => {
            if let Some(base_url) = string_value(config, "base_url") {
                push_codex_usage_candidate(&mut out, base_url, "codex.config_usage");
            } else {
                push_quota_candidate(
                    &mut out,
                    "https://chatgpt.com/backend-api/wham/usage".to_string(),
                    "codex.default_wham_usage",
                    QuotaEndpointStyle::CodexWham,
                );
            }
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

fn push_codex_usage_candidate(
    out: &mut Vec<QuotaEndpointCandidate>,
    base_url: &str,
    source: &str,
) {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return;
    }
    let lower = base.to_ascii_lowercase();
    let url = if lower.contains("/backend-api") {
        let prefix = backend_api_prefix(base).unwrap_or(base);
        format!("{prefix}/wham/usage")
    } else if lower.starts_with("https://chatgpt.com")
        || lower.starts_with("https://chat.openai.com")
    {
        format!("{base}/backend-api/wham/usage")
    } else {
        format!("{base}/api/codex/usage")
    };
    push_quota_candidate(out, url, source, QuotaEndpointStyle::CodexWham);
}

fn backend_api_prefix(base: &str) -> Option<&str> {
    let lower = base.to_ascii_lowercase();
    let idx = lower.find("/backend-api")?;
    Some(&base[..idx + "/backend-api".len()])
}

fn push_quota_candidate(
    out: &mut Vec<QuotaEndpointCandidate>,
    url: String,
    source: &str,
    style: QuotaEndpointStyle,
) {
    if out.iter().any(|candidate| candidate.url == url) {
        return;
    }
    out.push(QuotaEndpointCandidate {
        url,
        source: source.to_string(),
        style,
    });
}

async fn request_quota_snapshot(
    client: &reqwest::Client,
    platform: &str,
    auth: &QuotaRequestAuth,
    secret: &Value,
    candidate: &QuotaEndpointCandidate,
) -> Result<Option<OfficialQuotaSnapshot>, String> {
    let mut request = client.get(&candidate.url).header("accept", "application/json");
    request = match auth {
        QuotaRequestAuth::Bearer(access_token) => {
            request.header("authorization", format!("Bearer {access_token}"))
        }
        QuotaRequestAuth::AgentIdentity(agent_identity) => {
            request.header("authorization", agent_identity.authorization.as_str())
        }
    };

    match candidate.style {
        QuotaEndpointStyle::CodexWham => {
            request = request
                .header("openai-beta", CODEX_QUOTA_OPENAI_BETA)
                .header("oai-language", CODEX_QUOTA_LANGUAGE)
                .header("sec-fetch-site", CODEX_QUOTA_SEC_FETCH_SITE)
                .header("sec-fetch-mode", CODEX_QUOTA_SEC_FETCH_MODE)
                .header("sec-fetch-dest", CODEX_QUOTA_SEC_FETCH_DEST)
                .header("priority", CODEX_QUOTA_PRIORITY);
            match auth {
                QuotaRequestAuth::Bearer(_) => {
                    if let Some(account_id) = codex_account_id(secret) {
                        request = request.header("chatgpt-account-id", account_id);
                    }
                    request = request
                        .header("originator", CODEX_QUOTA_CLI_ORIGINATOR)
                        .header("user-agent", CODEX_QUOTA_CLI_USER_AGENT);
                }
                QuotaRequestAuth::AgentIdentity(agent_identity) => {
                    request = request
                        .header("chatgpt-account-id", agent_identity.chatgpt_account_id.as_str())
                        .header("originator", CODEX_QUOTA_AGENT_IDENTITY_ORIGINATOR)
                        .header("user-agent", CODEX_QUOTA_AGENT_IDENTITY_USER_AGENT);
                    if agent_identity.is_fedramp_account {
                        request = request.header("x-openai-fedramp", "true");
                    }
                }
            }
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
        .map_err(|err| {
            format!(
                "Quota request failed for {} ({}): {err}",
                candidate.source, candidate.url
            )
        })?;
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
        return Err(format_quota_http_error(
            &candidate.source,
            &candidate.url,
            status.as_u16(),
            &body,
        ));
    }

    let value = match serde_json::from_str::<Value>(&body) {
        Ok(value) => value,
        Err(_) => {
            if let Some(blocked) = chatgpt_network_block_message(&body) {
                return Err(format!(
                    "Quota response from {} ({}) was blocked by ChatGPT network policy: {}",
                    candidate.source, candidate.url, blocked
                ));
            }
            return Err(format!(
                "Quota response from {} ({}) was not JSON: {}",
                candidate.source,
                candidate.url,
                quota_response_snippet(&body)
            ));
        }
    };

    let snapshot = match candidate.style {
        QuotaEndpointStyle::CodexWham => parse_codex_quota_snapshot(&value),
        QuotaEndpointStyle::ClaudeOauthUsage => parse_claude_quota_snapshot(&value),
        QuotaEndpointStyle::GrokStatus => parse_grok_quota_snapshot(&value),
    };

    let _ = platform;
    if snapshot.is_none() {
        return Err(format!(
            "Quota response from {} ({}) had no supported quota fields: {}",
            candidate.source,
            candidate.url,
            quota_response_snippet(&body)
        ));
    }
    Ok(snapshot)
}

fn codex_account_id(secret: &Value) -> Option<&str> {
    string_value(secret, "account_id")
        .or_else(|| string_value(secret, "chatgpt_account_id"))
        .or_else(|| string_value(secret, "workspace_id"))
}

fn quota_fetch_failure_message(diagnostics: &[String]) -> Option<String> {
    if diagnostics.is_empty() {
        return None;
    }
    let joined = diagnostics.join(" | ");
    Some(format!(
        "No official quota endpoint returned usable remaining data: {}",
        joined.chars().take(720).collect::<String>()
    ))
}

fn format_quota_http_error(source: &str, url: &str, status: u16, body: &str) -> String {
    if let Some(blocked) = chatgpt_network_block_message(body) {
        return format!(
            "Quota request failed for {source} ({url}) with status {status}: {blocked}"
        );
    }
    format!(
        "Quota request failed for {source} ({url}) with status {status}: {}",
        quota_response_snippet(body)
    )
}

fn chatgpt_network_block_message(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let blocked = lower.contains("unable to load site")
        || lower.contains("if you are using a vpn")
        || (lower.contains("blocked-icon") && lower.contains("status.openai.com"));
    if !blocked {
        return None;
    }

    let mut message = String::from(
        "ChatGPT blocked this network egress (often VPN/proxy IP). Switch to another exit node or disable VPN, then retry quota refresh.",
    );
    if let Some(ip) = extract_bracketed_field(body, "IP:") {
        message.push_str(&format!(" Detected IP: {ip}."));
    }
    if let Some(ray_id) = extract_bracketed_field(body, "Ray ID:") {
        message.push_str(&format!(" Ray ID: {ray_id}."));
    }
    Some(message)
}

fn extract_bracketed_field(body: &str, label: &str) -> Option<String> {
    let start = body.find(label)? + label.len();
    let rest = body[start..].trim_start();
    let token = rest
        .split(|ch: char| ch == '|' || ch == ']' || ch.is_whitespace())
        .find(|part| !part.is_empty())?;
    Some(
        token
            .trim_matches(|ch: char| ch == '[' || ch == ']' || ch == ',')
            .to_string(),
    )
}

fn quota_response_snippet(body: &str) -> String {
    let snippet = body
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(QUOTA_BODY_SNIPPET_CHARS)
        .collect::<String>();
    if snippet.is_empty() {
        "<empty body>".to_string()
    } else {
        snippet
    }
}

pub fn parse_codex_quota_snapshot(value: &Value) -> Option<OfficialQuotaSnapshot> {
    if let Some(rate_limits) = value.get("rate_limits").or_else(|| value.get("rateLimits")) {
        if let Some(snapshot) = parse_codex_quota_snapshot(rate_limits) {
            return Some(snapshot);
        }
    }

    let subscription_type = subscription_type_from(value);

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
            return Some(merge_windows(primary, weekly, subscription_type));
        }
        if codex_rate_limit_exhausted(value, rate_limit) {
            return Some(snapshot_with_primary_remaining(subscription_type, Some(0)));
        }
    }

    if let Some(snapshot) = parse_codex_additional_rate_limits(value, subscription_type.clone()) {
        return Some(snapshot);
    }
    if let Some(snapshot) = parse_codex_spend_control(value, subscription_type.clone()) {
        return Some(snapshot);
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
        return Some(merge_windows(primary, weekly, subscription_type));
    }
    if subscription_type.is_some() && has_codex_quota_signal(value) {
        return Some(snapshot_with_primary_remaining(subscription_type, None));
    }
    None
}

fn parse_codex_additional_rate_limits(
    value: &Value,
    subscription_type: Option<String>,
) -> Option<OfficialQuotaSnapshot> {
    let items = value
        .get("additional_rate_limits")
        .or_else(|| value.get("additionalRateLimits"))?
        .as_array()?;
    let mut fallback: Option<OfficialQuotaSnapshot> = None;

    for item in items {
        let Some(rate_limit) = item.get("rate_limit").or_else(|| item.get("rateLimit")) else {
            continue;
        };
        let primary = window_remain(
            rate_limit
                .get("primary_window")
                .or_else(|| rate_limit.get("primaryWindow")),
        );
        let weekly = window_remain(
            rate_limit
                .get("secondary_window")
                .or_else(|| rate_limit.get("secondaryWindow")),
        );
        let snapshot = if primary.is_some() || weekly.is_some() {
            merge_windows(primary, weekly, subscription_type.clone())
        } else if codex_rate_limit_exhausted(value, rate_limit) {
            snapshot_with_primary_remaining(subscription_type.clone(), Some(0))
        } else {
            continue;
        };

        let feature = string_value(item, "metered_feature")
            .or_else(|| string_value(item, "meteredFeature"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if feature == "codex" {
            return Some(snapshot);
        }
        if fallback.is_none() {
            fallback = Some(snapshot);
        }
    }

    fallback
}

fn parse_codex_spend_control(
    value: &Value,
    subscription_type: Option<String>,
) -> Option<OfficialQuotaSnapshot> {
    let spend_control = value
        .get("spend_control")
        .or_else(|| value.get("spendControl"))?;
    if bool_value(spend_control, "reached").unwrap_or(false) {
        return Some(snapshot_with_primary_remaining(subscription_type, Some(0)));
    }
    let individual_limit = spend_control
        .get("individual_limit")
        .or_else(|| spend_control.get("individualLimit"));
    let primary = window_remain(individual_limit);
    primary.map(|primary| merge_windows(Some(primary), None, subscription_type))
}

fn codex_rate_limit_exhausted(root: &Value, rate_limit: &Value) -> bool {
    bool_value(rate_limit, "limit_reached").unwrap_or(false)
        || bool_value(rate_limit, "limitReached").unwrap_or(false)
        || bool_value(rate_limit, "allowed").is_some_and(|allowed| !allowed)
        || root
            .get("rate_limit_reached_type")
            .or_else(|| root.get("rateLimitReachedType"))
            .is_some_and(|value| !value.is_null())
}

fn has_codex_quota_signal(value: &Value) -> bool {
    value.get("rate_limit").is_some()
        || value.get("rateLimit").is_some()
        || value.get("credits").is_some()
        || value.get("spend_control").is_some()
        || value.get("spendControl").is_some()
        || value.get("additional_rate_limits").is_some()
        || value.get("additionalRateLimits").is_some()
}

fn snapshot_with_primary_remaining(
    subscription_type: Option<String>,
    primary_remain: Option<i64>,
) -> OfficialQuotaSnapshot {
    OfficialQuotaSnapshot {
        subscription_type,
        primary_remain,
        weekly_remain: None,
        reset_primary: None,
        reset_weekly: None,
        quota_remaining: primary_remain,
        quota_limit: None,
        quota_used: None,
    }
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
            "remaining_percent",
            "remainingPercent",
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

fn bool_value(value: &Value, key: &str) -> Option<bool> {
    match value.get(key)? {
        Value::Bool(value) => Some(*value),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
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
    fn codex_candidates_use_official_chatgpt_wham_endpoint() {
        let candidates = quota_endpoint_candidates(
            "codex",
            &json!({"base_url":"https://chatgpt.com/backend-api/codex"}),
        );
        let urls = candidates
            .iter()
            .map(|candidate| candidate.url.as_str())
            .collect::<Vec<_>>();

        assert_eq!(urls, vec!["https://chatgpt.com/backend-api/wham/usage"]);
        assert!(!urls.contains(&"https://chatgpt.com/backend-api/codex/usage"));
    }

    #[test]
    fn codex_candidates_normalize_chatgpt_host_to_backend_api_wham() {
        let candidates = quota_endpoint_candidates("codex", &json!({"base_url":"https://chatgpt.com"}));
        let urls = candidates
            .iter()
            .map(|candidate| candidate.url.as_str())
            .collect::<Vec<_>>();

        assert_eq!(urls, vec!["https://chatgpt.com/backend-api/wham/usage"]);
        assert!(!urls.contains(&"https://chatgpt.com/api/codex/usage"));
    }

    #[test]
    fn codex_candidates_use_codex_api_usage_for_non_chatgpt_base() {
        let candidates = quota_endpoint_candidates("codex", &json!({"base_url":"https://example.test"}));
        let urls = candidates
            .iter()
            .map(|candidate| candidate.url.as_str())
            .collect::<Vec<_>>();

        assert_eq!(urls, vec!["https://example.test/api/codex/usage"]);
        assert!(!urls.contains(&"https://chatgpt.com/backend-api/codex/usage"));
    }

    #[test]
    fn parse_codex_official_percent_windows() {
        let value = json!({
            "plan_type": "k12",
            "rate_limit": {
                "allowed": true,
                "limit_reached": false,
                "primary_window": {
                    "used_percent": 42,
                    "limit_window_seconds": 18000,
                    "reset_after_seconds": 1000,
                    "reset_at": 1780000000
                },
                "secondary_window": {
                    "used_percent": 84,
                    "limit_window_seconds": 604800,
                    "reset_after_seconds": 2000,
                    "reset_at": 1780600000
                }
            },
            "additional_rate_limits": []
        });

        let snapshot = parse_codex_quota_snapshot(&value).expect("snapshot");
        assert_eq!(snapshot.subscription_type.as_deref(), Some("k12"));
        assert_eq!(snapshot.primary_remain, Some(58));
        assert_eq!(snapshot.weekly_remain, Some(16));
        assert!(snapshot.reset_primary.as_deref().is_some_and(|value| value.contains('T')));
        assert!(snapshot.reset_weekly.as_deref().is_some_and(|value| value.contains('T')));
    }

    #[test]
    fn parse_codex_exhausted_without_windows() {
        let value = json!({
            "plan_type": "free",
            "rate_limit": {
                "allowed": false,
                "limit_reached": true
            },
            "rate_limit_reached_type": { "type": "rate_limit_reached" }
        });

        let snapshot = parse_codex_quota_snapshot(&value).expect("snapshot");
        assert_eq!(snapshot.subscription_type.as_deref(), Some("free"));
        assert_eq!(snapshot.primary_remain, Some(0));
        assert_eq!(snapshot.quota_remaining, Some(0));
    }

    #[test]
    fn parse_codex_subscription_only_payload() {
        let value = json!({
            "plan_type": "k12",
            "rate_limit": null,
            "credits": {
                "has_credits": true,
                "unlimited": true
            }
        });

        let snapshot = parse_codex_quota_snapshot(&value).expect("snapshot");
        assert_eq!(snapshot.subscription_type.as_deref(), Some("k12"));
        assert_eq!(snapshot.primary_remain, None);
        assert_eq!(snapshot.weekly_remain, None);
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

    fn assert_quota_header(headers: &axum::http::HeaderMap, name: &str, expected: &str) {
        assert_eq!(
            headers.get(name).and_then(|value| value.to_str().ok()),
            Some(expected),
            "header {name}"
        );
    }

    fn assert_common_codex_quota_headers(
        headers: &axum::http::HeaderMap,
        originator: &str,
        user_agent: &str,
    ) {
        assert_quota_header(headers, "user-agent", user_agent);
        assert_quota_header(headers, "originator", originator);
        assert_quota_header(headers, "openai-beta", CODEX_QUOTA_OPENAI_BETA);
        assert_quota_header(headers, "oai-language", CODEX_QUOTA_LANGUAGE);
        assert_quota_header(headers, "sec-fetch-site", CODEX_QUOTA_SEC_FETCH_SITE);
        assert_quota_header(headers, "sec-fetch-mode", CODEX_QUOTA_SEC_FETCH_MODE);
        assert_quota_header(headers, "sec-fetch-dest", CODEX_QUOTA_SEC_FETCH_DEST);
        assert_quota_header(headers, "priority", CODEX_QUOTA_PRIORITY);
    }

    #[tokio::test]
    async fn codex_quota_request_uses_official_headers() {
        use axum::http::HeaderMap;
        use axum::routing::get;
        use axum::{Json, Router};
        use std::sync::{Arc, Mutex};
        use tokio::net::TcpListener;

        let seen_headers: Arc<Mutex<Option<HeaderMap>>> = Arc::new(Mutex::new(None));
        let captured = Arc::clone(&seen_headers);
        let app = Router::new().route(
            "/usage",
            get(move |headers: HeaderMap| {
                let captured = Arc::clone(&captured);
                async move {
                    *captured.lock().expect("headers lock") = Some(headers);
                    Json(json!({
                        "plan_type": "plus",
                        "rate_limit": {
                            "primary_window": { "remaining": 12 }
                        }
                    }))
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = build_outbound_http_client(Some(Duration::from_secs(5))).expect("client");
        let secret = json!({
            "access_token": "at-test",
            "chatgpt_account_id": "account-123"
        });
        let candidate = QuotaEndpointCandidate {
            url: format!("http://{address}/usage"),
            source: "test.codex".to_string(),
            style: QuotaEndpointStyle::CodexWham,
        };

        let snapshot = request_quota_snapshot(
            &client,
            "codex",
            &QuotaRequestAuth::Bearer("at-test".to_string()),
            &secret,
            &candidate,
        )
        .await
        .expect("request")
        .expect("snapshot");

        assert_eq!(snapshot.primary_remain, Some(12));
        let headers = seen_headers
            .lock()
            .expect("headers lock")
            .clone()
            .expect("captured headers");
        assert_common_codex_quota_headers(&headers, CODEX_QUOTA_CLI_ORIGINATOR, CODEX_QUOTA_CLI_USER_AGENT);
        assert_quota_header(&headers, "authorization", "Bearer at-test");
        assert_quota_header(&headers, "chatgpt-account-id", "account-123");
    }

    #[tokio::test]
    async fn codex_quota_request_uses_agent_identity_headers() {
        use axum::http::HeaderMap;
        use axum::routing::get;
        use axum::{Json, Router};
        use std::sync::{Arc, Mutex};
        use tokio::net::TcpListener;

        let seen_headers: Arc<Mutex<Option<HeaderMap>>> = Arc::new(Mutex::new(None));
        let captured = Arc::clone(&seen_headers);
        let app = Router::new().route(
            "/usage",
            get(move |headers: HeaderMap| {
                let captured = Arc::clone(&captured);
                async move {
                    *captured.lock().expect("headers lock") = Some(headers);
                    Json(json!({
                        "plan_type": "k12",
                        "rate_limit": {
                            "primary_window": { "remaining": 88 }
                        }
                    }))
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = build_outbound_http_client(Some(Duration::from_secs(5))).expect("client");
        let secret = json!({});
        let candidate = QuotaEndpointCandidate {
            url: format!("http://{address}/usage"),
            source: "test.codex_agent_identity".to_string(),
            style: QuotaEndpointStyle::CodexWham,
        };

        let snapshot = request_quota_snapshot(
            &client,
            "codex",
            &QuotaRequestAuth::AgentIdentity(AgentIdentityHeaders {
                authorization: "AgentAssertion token-test".to_string(),
                chatgpt_account_id: "account-agent-1".to_string(),
                is_fedramp_account: true,
            }),
            &secret,
            &candidate,
        )
        .await
        .expect("request")
        .expect("snapshot");

        assert_eq!(snapshot.primary_remain, Some(88));
        let headers = seen_headers
            .lock()
            .expect("headers lock")
            .clone()
            .expect("captured headers");
        assert_common_codex_quota_headers(&headers, CODEX_QUOTA_AGENT_IDENTITY_ORIGINATOR, CODEX_QUOTA_AGENT_IDENTITY_USER_AGENT);
        assert_quota_header(&headers, "authorization", "AgentAssertion token-test");
        assert_quota_header(&headers, "chatgpt-account-id", "account-agent-1");
        assert_quota_header(&headers, "x-openai-fedramp", "true");
    }

    #[test]
    fn detects_chatgpt_vpn_block_page() {
        let body = r#"<html><div class="blocked-icon"></div><p>Unable to load site</p>
        <span>Please try again later. If you are using a VPN, try turning it off.
        Check the <a href="https://status.openai.com/">status page</a>
        [IP:45.144.136.244 | Ray ID:a1f774a29f0f04c7]</span></html>"#;
        let message = chatgpt_network_block_message(body).expect("blocked page");
        assert!(message.contains("VPN/proxy IP"));
        assert!(message.contains("45.144.136.244"));
        assert!(message.contains("a1f774a29f0f04c7"));

        let err = format_quota_http_error(
            "codex.default_wham_usage",
            "https://chatgpt.com/backend-api/wham/usage",
            403,
            body,
        );
        assert!(err.contains("status 403"));
        assert!(err.contains("VPN/proxy IP"));
        assert!(!err.contains("<html>"));
    }
}
