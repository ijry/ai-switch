use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::error::AppError;
use crate::models::route_credential::RouteCredential;
use crate::models::route_relay_balance::{
    RelayBalanceConfig, RelayBalanceProvider, RelayBalanceSnapshot, DEFAULT_NEW_API_QUOTA_PER_UNIT,
    RELAY_BALANCE_SNAPSHOT_KEY,
};
use crate::services::http_client::build_outbound_http_client;
use chrono::{TimeZone, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::time::Duration;

const BALANCE_TIMEOUT_SECS: u64 = 15;
/// The panel-settings probe is a nicety, not the query itself, so it gets a
/// shorter leash than the balance request.
const PANEL_STATUS_TIMEOUT_SECS: u64 = 6;
const ERROR_BODY_MAX_CHARS: usize = 512;
const USER_AGENT_VALUE: &str = "ai-switch/0.1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelayBalanceRefreshOutcome {
    pub credential: RouteCredential,
    /// False when the numbers came back identical to the stored snapshot.
    pub updated: bool,
    /// The provider that answered, or `skipped` when the account is not set up
    /// for balance queries at all.
    pub source: String,
    pub message: Option<String>,
}

pub struct RouteRelayBalanceService;

impl RouteRelayBalanceService {
    pub async fn refresh_one(
        pool: &SqlitePool,
        id: String,
    ) -> Result<RelayBalanceRefreshOutcome, AppError> {
        let credential = RouteCredentialRepository::get(pool, &id).await?;
        if credential.archived_at.is_some() {
            return Err(AppError::Validation {
                code: "validation.route_credential_archived",
                message: "Archived route credentials cannot refresh balance".to_string(),
                details: Some(id),
                recoverable: true,
            });
        }
        refresh_credential(pool, credential).await
    }

    /// Refreshes every relay account on a platform that has querying turned on.
    /// Per-account failures become `source: "error"` entries instead of failing
    /// the whole batch, mirroring `RouteQuotaService::refresh_platform`.
    pub async fn refresh_platform(
        pool: &SqlitePool,
        platform: String,
    ) -> Result<Vec<RelayBalanceRefreshOutcome>, AppError> {
        let credentials =
            RouteCredentialRepository::list_by_platform(pool, platform.trim()).await?;
        let mut outcomes = Vec::new();
        for credential in credentials {
            if credential.kind != "api" || credential.archived_at.is_some() {
                continue;
            }
            if RelayBalanceConfig::from_config_json(&credential.config_json).is_none() {
                continue;
            }
            match refresh_credential(pool, credential.clone()).await {
                Ok(outcome) => outcomes.push(outcome),
                Err(err) => outcomes.push(RelayBalanceRefreshOutcome {
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
) -> Result<RelayBalanceRefreshOutcome, AppError> {
    if credential.kind != "api" {
        return Ok(RelayBalanceRefreshOutcome {
            credential,
            updated: false,
            source: "skipped".to_string(),
            message: Some("只有中转站账号支持余额查询".to_string()),
        });
    }

    let config_value = serde_json::from_str::<Value>(&credential.config_json).map_err(|err| {
        AppError::Validation {
            code: "validation.route_relay_balance_config",
            message: format!("Route credential config JSON is invalid: {err}"),
            details: Some(credential.id.clone()),
            recoverable: true,
        }
    })?;
    let Some(config) = RelayBalanceConfig::from_config_value(&config_value).map_err(|message| {
        AppError::Validation {
            code: "validation.route_relay_balance_config",
            message,
            details: Some(credential.id.clone()),
            recoverable: true,
        }
    })?
    else {
        return Ok(RelayBalanceRefreshOutcome {
            credential,
            updated: false,
            source: "skipped".to_string(),
            message: Some("该账号未开启余额查询".to_string()),
        });
    };

    let request = BalanceRequest::from_credential(&credential, &config_value)?;
    let snapshot = fetch_snapshot(&config, &request).await?;

    let previous = RelayBalanceSnapshot::from_config_json(&credential.config_json);
    let changed = match &previous {
        Some(previous) => {
            (
                previous.remaining,
                previous.used,
                previous.limit,
                previous.unlimited,
            ) != (
                snapshot.remaining,
                snapshot.used,
                snapshot.limit,
                snapshot.unlimited,
            )
        }
        None => true,
    };
    let next_config =
        apply_relay_balance_snapshot(&credential.config_json, &snapshot).map_err(|message| {
            AppError::Validation {
                code: "validation.route_relay_balance_apply",
                message,
                details: Some(credential.id.clone()),
                recoverable: true,
            }
        })?;

    RouteCredentialRepository::update_secret_and_config(
        pool,
        &credential.id,
        &credential.secret_payload_json,
        &next_config,
    )
    .await?;

    let latest = RouteCredentialRepository::get(pool, &credential.id).await?;
    Ok(RelayBalanceRefreshOutcome {
        credential: latest,
        updated: changed,
        source: config.provider.as_str().to_string(),
        message: None,
    })
}

/// Everything an adapter needs from the account row.
struct BalanceRequest {
    base_url: String,
    api_key: String,
    interface_format: String,
}

impl BalanceRequest {
    fn from_credential(credential: &RouteCredential, config: &Value) -> Result<Self, AppError> {
        let base_url = config
            .get("base_url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if base_url.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_relay_balance_base_url_required",
                message: "账号没有 Base URL，无法查询余额".to_string(),
                details: Some(credential.id.clone()),
                recoverable: true,
            });
        }
        let api_key = serde_json::from_str::<Value>(&credential.secret_payload_json)
            .ok()
            .and_then(|secret| {
                secret
                    .get("api_key")
                    .and_then(Value::as_str)
                    .map(|key| key.trim().to_string())
            })
            .unwrap_or_default();
        if api_key.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_relay_balance_api_key_required",
                message: "账号没有 API Key，无法查询余额".to_string(),
                details: Some(credential.id.clone()),
                recoverable: true,
            });
        }
        let interface_format = config
            .get("interface_format")
            .and_then(Value::as_str)
            .unwrap_or("openai")
            .trim()
            .to_string();
        Ok(Self {
            base_url,
            api_key,
            interface_format,
        })
    }
}

/// Writes the snapshot into a credential's `config_json`.
pub fn apply_relay_balance_snapshot(
    config_json: &str,
    snapshot: &RelayBalanceSnapshot,
) -> Result<String, String> {
    let mut config = serde_json::from_str::<Value>(config_json)
        .map_err(|err| format!("Route credential config JSON is invalid: {err}"))?;
    let Some(object) = config.as_object_mut() else {
        return Err("Route credential config JSON must be an object".to_string());
    };
    let value = serde_json::to_value(snapshot)
        .map_err(|err| format!("Could not serialize the balance snapshot: {err}"))?;
    object.insert(RELAY_BALANCE_SNAPSHOT_KEY.to_string(), value);
    Ok(config.to_string())
}

/// Relay panels answer on their own root, but an account's Base URL usually
/// points at the API prefix (`https://panel.example.com/v1`). Naively joining
/// gives `…/v1/api/usage/token/`, which 404s — this is the single most common
/// cause of "balance query failed" in clients that skip this step. Candidates
/// are ordered panel-root first, raw Base URL second.
pub fn panel_root_candidates(base_url: &str) -> Vec<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let lowered = trimmed.to_ascii_lowercase();
    for suffix in ["/openai/v1", "/v1beta", "/v1"] {
        if lowered.ends_with(suffix) {
            candidates.push(trimmed[..trimmed.len() - suffix.len()].to_string());
            break;
        }
    }
    candidates.push(trimmed.to_string());
    deduplicate(candidates)
}

fn deduplicate(values: Vec<String>) -> Vec<String> {
    let mut seen = Vec::new();
    for value in values {
        if value.is_empty() || seen.contains(&value) {
            continue;
        }
        seen.push(value);
    }
    seen
}

async fn fetch_snapshot(
    config: &RelayBalanceConfig,
    request: &BalanceRequest,
) -> Result<RelayBalanceSnapshot, AppError> {
    let client = build_outbound_http_client(Some(Duration::from_secs(BALANCE_TIMEOUT_SECS)))
        .map_err(|err| AppError::Validation {
            code: "validation.route_relay_balance_client",
            message: "无法初始化余额查询客户端".to_string(),
            details: Some(err),
            recoverable: true,
        })?;
    let headers =
        balance_request_headers(&request.api_key, &request.interface_format).map_err(|err| {
            AppError::Validation {
                code: "validation.route_relay_balance_headers",
                message: "无法构造余额查询请求头".to_string(),
                details: Some(err),
                recoverable: true,
            }
        })?;

    match config.provider {
        RelayBalanceProvider::NewApi => {
            fetch_new_api_balance(&client, headers, config, request).await
        }
        RelayBalanceProvider::Sub2Api => fetch_sub2api_balance(&client, headers, request).await,
        RelayBalanceProvider::Custom => fetch_custom_balance(&client, headers, config).await,
    }
}

/// Both built-in panels document `Authorization: Bearer <key>`, whatever dialect
/// the relay itself speaks, so that header is unconditional. `x-api-key` rides
/// along for Anthropic-dialect accounts because some gateways only read that
/// one; panels that don't know it ignore it.
fn balance_request_headers(api_key: &str, interface_format: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    // The outbound client has no decompression support; ask for plain bytes.
    headers.insert("accept-encoding", HeaderValue::from_static("identity"));
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|err| format!("Invalid authorization header: {err}"))?,
    );
    if interface_format == "anthropic" {
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(api_key)
                .map_err(|err| format!("Invalid x-api-key header: {err}"))?,
        );
    }
    Ok(headers)
}

/// Tries each candidate URL in order. Transport errors, 404 and 405 move on to
/// the next one; any other non-success status is reported as-is, because a 401
/// or 403 means the endpoint exists and the key is the problem.
async fn get_json_from_candidates(
    client: &Client,
    headers: &HeaderMap,
    candidates: &[String],
) -> Result<(String, Value), AppError> {
    if candidates.is_empty() {
        return Err(validation_error(
            "validation.route_relay_balance_endpoint",
            "无法根据 Base URL 推导余额查询地址",
            None,
        ));
    }
    let mut last_err: Option<String> = None;
    for url in candidates {
        let response = match client.get(url).headers(headers.clone()).send().await {
            Ok(response) => response,
            Err(err) => {
                last_err = Some(format!("{url}: {err}"));
                continue;
            }
        };
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            let parsed = serde_json::from_str::<Value>(&body).map_err(|err| {
                validation_error(
                    "validation.route_relay_balance_parse",
                    "余额接口没有返回 JSON",
                    Some(format!("{url}: {err}; response: {}", truncate_body(&body))),
                )
            })?;
            return Ok((url.clone(), parsed));
        }
        let message = format!("{url}: HTTP {status}: {}", truncate_body(&body));
        if status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED {
            last_err = Some(message);
            continue;
        }
        return Err(validation_error(
            "validation.route_relay_balance_http",
            "余额查询请求失败",
            Some(message),
        ));
    }
    Err(validation_error(
        "validation.route_relay_balance_all_failed",
        "所有余额查询地址都失败了",
        last_err,
    ))
}

const NEW_API_USAGE_PATH: &str = "/api/usage/token/";

/// new-api's token-scoped usage endpoint. It authenticates with the relay key
/// the account already stores, so this provider needs no extra input — unlike
/// `/api/user/self`, which wants a user PAT (and, on releases before 0.14, a
/// `New-Api-User` header carrying the numeric user id).
async fn fetch_new_api_balance(
    client: &Client,
    headers: HeaderMap,
    config: &RelayBalanceConfig,
    request: &BalanceRequest,
) -> Result<RelayBalanceSnapshot, AppError> {
    let candidates: Vec<String> = panel_root_candidates(&request.base_url)
        .into_iter()
        .map(|root| format!("{root}{NEW_API_USAGE_PATH}"))
        .collect();
    let (url, body) = get_json_from_candidates(client, &headers, &candidates).await?;
    let usage = parse_new_api_token_usage(&body).map_err(|message| {
        validation_error(
            "validation.route_relay_balance_parse",
            message,
            Some(format!("{url}: {}", truncate_body(&body.to_string()))),
        )
    })?;

    let panel_root = url
        .strip_suffix(NEW_API_USAGE_PATH)
        .unwrap_or(url.as_str())
        .to_string();
    let (divisor, divisor_source) = match config.divisor {
        Some(divisor) => (divisor, DivisorSource::UserPinned),
        None => match fetch_new_api_quota_per_unit(client, &panel_root).await {
            Some(divisor) => (divisor, DivisorSource::Panel),
            None => (DEFAULT_NEW_API_QUOTA_PER_UNIT, DivisorSource::Default),
        },
    };

    let mut notes = Vec::new();
    if let Some(name) = &usage.name {
        notes.push(format!("令牌 {name}"));
    }
    if usage.unlimited {
        notes.push("面板标记为不限额度".to_string());
    }
    if divisor_source != DivisorSource::Default || divisor != DEFAULT_NEW_API_QUOTA_PER_UNIT {
        notes.push(format!(
            "额度换算 {}",
            format_divisor(divisor, divisor_source)
        ));
    }

    Ok(RelayBalanceSnapshot {
        provider: RelayBalanceProvider::NewApi,
        plan_name: None,
        // An unlimited key's granted/available numbers carry no meaning; what it
        // has spent still does.
        remaining: if usage.unlimited {
            None
        } else {
            usage.total_available.map(|value| value / divisor)
        },
        used: usage.total_used.map(|value| value / divisor),
        limit: if usage.unlimited {
            None
        } else {
            usage.total_granted.map(|value| value / divisor)
        },
        unit: "USD".to_string(),
        unlimited: usage.unlimited,
        expires_at: usage.expires_at.and_then(unix_seconds_to_rfc3339),
        source_url: url,
        checked_at: Utc::now().to_rfc3339(),
        notes,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DivisorSource {
    UserPinned,
    Panel,
    Default,
}

fn format_divisor(divisor: f64, source: DivisorSource) -> String {
    let label = match source {
        DivisorSource::UserPinned => "手填",
        DivisorSource::Panel => "面板 quota_per_unit",
        DivisorSource::Default => "默认",
    };
    format!("{label} {divisor:.0}")
}

#[derive(Debug, Clone, PartialEq)]
struct NewApiTokenUsage {
    name: Option<String>,
    total_granted: Option<f64>,
    total_used: Option<f64>,
    total_available: Option<f64>,
    unlimited: bool,
    expires_at: Option<i64>,
}

fn parse_new_api_token_usage(body: &Value) -> Result<NewApiTokenUsage, String> {
    // `/api/usage/token/` stamps the envelope flag on `code`; other new-api
    // routes use `success`. Several auth failures answer HTTP 200 with the flag
    // set to false, so the flag has to be read before the numbers.
    let envelope_ok = body
        .get("code")
        .and_then(Value::as_bool)
        .or_else(|| body.get("success").and_then(Value::as_bool));
    if envelope_ok == Some(false) {
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|message| !message.is_empty())
            .unwrap_or("面板拒绝了余额查询");
        return Err(format!("面板返回失败：{message}"));
    }
    let Some(data) = body.get("data").filter(|data| data.is_object()) else {
        return Err("面板响应里没有 data 对象".to_string());
    };

    let unlimited = data
        .get("unlimited_quota")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let usage = NewApiTokenUsage {
        name: string_field(data, "name"),
        total_granted: number_field(data, "total_granted"),
        total_used: number_field(data, "total_used"),
        total_available: number_field(data, "total_available"),
        unlimited,
        expires_at: number_field(data, "expires_at")
            .map(|value| value as i64)
            .filter(|value| *value > 0),
    };
    if !unlimited && usage.total_available.is_none() && usage.total_used.is_none() {
        return Err("面板响应里没有可用的额度字段".to_string());
    }
    Ok(usage)
}

/// `GET <panel>/api/status` is unauthenticated and reports the panel's real
/// `quota_per_unit`. Admins do change it, and hard-coding 500000 silently
/// reports the wrong dollar figure when they have. Best-effort: any failure
/// falls back to the shipped default.
async fn fetch_new_api_quota_per_unit(client: &Client, panel_root: &str) -> Option<f64> {
    let response = client
        .get(format!("{panel_root}/api/status"))
        .timeout(Duration::from_secs(PANEL_STATUS_TIMEOUT_SECS))
        .header(ACCEPT, "application/json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.json::<Value>().await.ok()?;
    number_field(body.get("data")?, "quota_per_unit")
        .filter(|value| value.is_finite() && *value > 0.0)
}

/// sub2api's gateway usage endpoint. Values are already USD floats, so there is
/// no divisor. Three response shapes share one set of top-level fields
/// (`isValid` / `planName` / `remaining` / `unit`); `mode` only decides what
/// extra detail is worth showing.
async fn fetch_sub2api_balance(
    client: &Client,
    headers: HeaderMap,
    request: &BalanceRequest,
) -> Result<RelayBalanceSnapshot, AppError> {
    let trimmed = request.base_url.trim().trim_end_matches('/');
    let mut candidates: Vec<String> = panel_root_candidates(&request.base_url)
        .into_iter()
        .map(|root| format!("{root}/v1/usage"))
        .collect();
    // Covers relays whose Base URL already points inside a versioned prefix
    // other than /v1 (e.g. …/api).
    candidates.push(format!("{trimmed}/usage"));
    let candidates = deduplicate(candidates);

    let (url, body) = get_json_from_candidates(client, &headers, &candidates).await?;
    let usage = parse_sub2api_usage(&body).map_err(|message| {
        validation_error(
            "validation.route_relay_balance_parse",
            message,
            Some(format!("{url}: {}", truncate_body(&body.to_string()))),
        )
    })?;

    Ok(RelayBalanceSnapshot {
        provider: RelayBalanceProvider::Sub2Api,
        plan_name: usage.plan_name,
        remaining: usage.remaining,
        used: usage.used,
        limit: usage.limit,
        unit: usage.unit,
        unlimited: usage.unlimited,
        expires_at: usage.expires_at,
        source_url: url,
        checked_at: Utc::now().to_rfc3339(),
        notes: usage.notes,
    })
}

#[derive(Debug, Clone, PartialEq)]
struct Sub2ApiUsage {
    plan_name: Option<String>,
    remaining: Option<f64>,
    used: Option<f64>,
    limit: Option<f64>,
    unit: String,
    unlimited: bool,
    expires_at: Option<String>,
    notes: Vec<String>,
}

fn parse_sub2api_usage(body: &Value) -> Result<Sub2ApiUsage, String> {
    if body.get("isValid").and_then(Value::as_bool) == Some(false) {
        let message = ["invalidMessage", "message", "status"]
            .into_iter()
            .find_map(|key| string_field(body, key))
            .unwrap_or_else(|| "面板报告这把 key 不可用".to_string());
        return Err(format!("面板返回失败：{message}"));
    }

    let quota = body.get("quota");
    let remaining = number_field(body, "remaining")
        .or_else(|| quota.and_then(|quota| number_field(quota, "remaining")))
        .or_else(|| number_field(body, "balance"));
    let used = quota.and_then(|quota| number_field(quota, "used"));
    let limit = quota.and_then(|quota| number_field(quota, "limit"));
    let unit = string_field(body, "unit")
        .or_else(|| quota.and_then(|quota| string_field(quota, "unit")))
        .unwrap_or_else(|| "USD".to_string());
    let mode = string_field(body, "mode").unwrap_or_default();

    let mut notes = Vec::new();
    if let Some(windows) = body.get("rate_limits").and_then(Value::as_array) {
        for window in windows {
            let label = string_field(window, "window").unwrap_or_else(|| "窗口".to_string());
            match (
                number_field(window, "remaining"),
                number_field(window, "limit"),
            ) {
                (Some(remaining), Some(limit)) => {
                    notes.push(format!("{label} 窗口剩余 {remaining:.0}/{limit:.0}"));
                }
                (Some(remaining), None) => notes.push(format!("{label} 窗口剩余 {remaining:.0}")),
                _ => {}
            }
        }
    }
    if let Some(subscription) = body.get("subscription") {
        for (label, used_key, limit_key) in [
            ("日", "daily_usage_usd", "daily_limit_usd"),
            ("周", "weekly_usage_usd", "weekly_limit_usd"),
            ("月", "monthly_usage_usd", "monthly_limit_usd"),
        ] {
            let Some(window_limit) = number_field(subscription, limit_key) else {
                continue;
            };
            if window_limit <= 0.0 {
                continue;
            }
            let window_used = number_field(subscription, used_key).unwrap_or(0.0);
            notes.push(format!(
                "{label}用量 {window_used:.2}/{window_limit:.2} {unit}"
            ));
        }
    }

    // An unrestricted key with no cap and no wallet balance has nothing to count
    // down; saying so beats showing a blank badge.
    let unlimited = mode == "unrestricted" && remaining.is_none() && limit.is_none();
    if remaining.is_none() && used.is_none() && !unlimited {
        return Err("响应里没有可用的余额字段".to_string());
    }

    Ok(Sub2ApiUsage {
        plan_name: string_field(body, "planName"),
        remaining,
        used,
        limit,
        unit,
        unlimited,
        expires_at: string_field(body, "expires_at"),
        notes,
    })
}

/// The escape hatch: the user names the URL and where the numbers live. No code
/// is executed on our side — a dotted path can only read, which is the whole
/// reason this is declarative instead of a scripting engine.
async fn fetch_custom_balance(
    client: &Client,
    headers: HeaderMap,
    config: &RelayBalanceConfig,
) -> Result<RelayBalanceSnapshot, AppError> {
    let candidates = vec![config.endpoint.trim().to_string()];
    let (url, body) = get_json_from_candidates(client, &headers, &candidates).await?;

    let divisor = config.divisor.unwrap_or(1.0);
    let remaining_path = config.remaining_path.trim();
    let Some(remaining) = json_path_number(&body, remaining_path) else {
        return Err(validation_error(
            "validation.route_relay_balance_parse",
            format!("响应里 {remaining_path} 取不到数值"),
            Some(format!("{url}: {}", truncate_body(&body.to_string()))),
        ));
    };

    let mut notes = Vec::new();
    if divisor != 1.0 {
        notes.push(format!("额度换算 手填 {divisor:.0}"));
    }

    Ok(RelayBalanceSnapshot {
        provider: RelayBalanceProvider::Custom,
        plan_name: json_path_value(&body, &config.plan_path).and_then(display_string),
        remaining: Some(remaining / divisor),
        used: json_path_number(&body, &config.used_path).map(|value| value / divisor),
        limit: json_path_number(&body, &config.limit_path).map(|value| value / divisor),
        unit: config.display_unit(),
        unlimited: false,
        expires_at: None,
        source_url: url,
        checked_at: Utc::now().to_rfc3339(),
        notes,
    })
}

/// Walks a dotted path such as `data.total_available`. Numeric segments index
/// into arrays, so `data.plans.0.remaining` works too.
fn json_path_value<'a>(body: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let mut current = body;
    for segment in path.split('.') {
        let segment = segment.trim();
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

fn json_path_number(body: &Value, path: &str) -> Option<f64> {
    json_number(json_path_value(body, path)?)
}

/// Panels are inconsistent about quoting numbers, so a numeric string counts.
fn json_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn number_field(value: &Value, key: &str) -> Option<f64> {
    json_number(value.get(key)?)
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    display_string(value.get(key)?)
}

fn display_string(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn unix_seconds_to_rfc3339(seconds: i64) -> Option<String> {
    Utc.timestamp_opt(seconds, 0)
        .single()
        .map(|stamp| stamp.to_rfc3339())
}

fn truncate_body(body: &str) -> String {
    if body.chars().count() <= ERROR_BODY_MAX_CHARS {
        return body.to_string();
    }
    let truncated: String = body.chars().take(ERROR_BODY_MAX_CHARS).collect();
    format!("{truncated}…")
}

fn validation_error(
    code: &'static str,
    message: impl Into<String>,
    details: Option<String>,
) -> AppError {
    AppError::Validation {
        code,
        message: message.into(),
        details,
        recoverable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::route_credential::CreateApiRouteCredentialInput;
    use crate::services::route_credential_service::RouteCredentialService;
    use axum::routing::get;
    use axum::{Json, Router};
    use tokio::net::TcpListener;

    #[test]
    fn panel_root_is_tried_before_the_raw_base_url() {
        assert_eq!(
            panel_root_candidates("https://panel.example.com/v1"),
            vec![
                "https://panel.example.com".to_string(),
                "https://panel.example.com/v1".to_string(),
            ]
        );
    }

    #[test]
    fn panel_root_ignores_trailing_slashes_and_other_api_prefixes() {
        assert_eq!(
            panel_root_candidates("https://panel.example.com/v1/"),
            panel_root_candidates("https://panel.example.com/v1")
        );
        assert_eq!(
            panel_root_candidates("https://panel.example.com/openai/v1")[0],
            "https://panel.example.com"
        );
        assert_eq!(
            panel_root_candidates("https://panel.example.com/v1beta")[0],
            "https://panel.example.com"
        );
    }

    #[test]
    fn a_base_url_without_an_api_prefix_yields_one_candidate() {
        assert_eq!(
            panel_root_candidates("https://panel.example.com"),
            vec!["https://panel.example.com".to_string()]
        );
        assert!(panel_root_candidates("   ").is_empty());
    }

    #[test]
    fn a_relay_path_prefix_survives_stripping_the_api_version() {
        assert_eq!(
            panel_root_candidates("https://host.example.com/relay/v1")[0],
            "https://host.example.com/relay"
        );
    }

    #[test]
    fn new_api_token_usage_reads_the_token_scoped_numbers() {
        let body = json!({
            "code": true,
            "message": "ok",
            "data": {
                "object": "token_usage",
                "name": "my key",
                "total_granted": 500_000,
                "total_used": 12_345,
                "total_available": 487_655,
                "unlimited_quota": false,
                "expires_at": 0,
            }
        });
        let usage = parse_new_api_token_usage(&body).expect("parses");
        assert_eq!(usage.total_granted, Some(500_000.0));
        assert_eq!(usage.total_used, Some(12_345.0));
        assert_eq!(usage.total_available, Some(487_655.0));
        assert_eq!(usage.name.as_deref(), Some("my key"));
        // 0 means "never expires", not "expired at the epoch".
        assert_eq!(usage.expires_at, None);
        assert!(!usage.unlimited);
    }

    #[test]
    fn new_api_auth_failures_arrive_as_http_200() {
        // The panel answers 200 with the envelope flag false on several auth
        // branches, so a status-only check would read this as a balance of zero.
        let body =
            json!({"success": false, "message": "无权进行此操作，未登录且未提供 access token"});
        let error = parse_new_api_token_usage(&body).expect_err("must not look like success");
        assert!(error.contains("未登录"), "{error}");

        let body = json!({"code": false, "message": "令牌已禁用"});
        let error = parse_new_api_token_usage(&body).expect_err("must not look like success");
        assert!(error.contains("令牌已禁用"), "{error}");
    }

    #[test]
    fn new_api_usage_without_numbers_is_an_error() {
        let body = json!({"code": true, "data": {"object": "token_usage"}});
        let error = parse_new_api_token_usage(&body).expect_err("no usable fields");
        assert!(error.contains("额度字段"), "{error}");

        let body = json!({"code": true, "data": "nope"});
        let error = parse_new_api_token_usage(&body).expect_err("data must be an object");
        assert!(error.contains("data"), "{error}");
    }

    #[test]
    fn new_api_unlimited_keys_keep_only_what_they_spent() {
        let body = json!({
            "code": true,
            "data": {"unlimited_quota": true, "total_used": 1_000_000, "total_available": 0}
        });
        let usage = parse_new_api_token_usage(&body).expect("parses");
        assert!(usage.unlimited);
        assert_eq!(usage.total_used, Some(1_000_000.0));
    }

    #[test]
    fn quoted_numbers_are_accepted() {
        let body = json!({"code": true, "data": {"total_available": "487655.5"}});
        let usage = parse_new_api_token_usage(&body).expect("parses");
        assert_eq!(usage.total_available, Some(487_655.5));
    }

    #[test]
    fn sub2api_quota_limited_keys_report_dollars_directly() {
        let body = json!({
            "mode": "quota_limited",
            "isValid": true,
            "status": "active",
            "quota": {"limit": 50, "used": 12.3, "remaining": 37.7, "unit": "USD"},
            "remaining": 37.7,
            "unit": "USD",
            "rate_limits": [{"window": "5h", "limit": 20, "used": 3, "remaining": 17}],
            "expires_at": "2026-10-01T00:00:00Z",
        });
        let usage = parse_sub2api_usage(&body).expect("parses");
        assert_eq!(usage.remaining, Some(37.7));
        assert_eq!(usage.used, Some(12.3));
        assert_eq!(usage.limit, Some(50.0));
        assert_eq!(usage.unit, "USD");
        assert!(!usage.unlimited);
        assert_eq!(usage.expires_at.as_deref(), Some("2026-10-01T00:00:00Z"));
        assert_eq!(usage.notes, vec!["5h 窗口剩余 17/20".to_string()]);
    }

    #[test]
    fn sub2api_subscription_groups_list_each_window() {
        let body = json!({
            "mode": "unrestricted",
            "isValid": true,
            "planName": "Claude Max",
            "unit": "USD",
            "remaining": 8.5,
            "subscription": {
                "daily_usage_usd": 1.5,
                "daily_limit_usd": 10.0,
                "weekly_usage_usd": 12.0,
                "weekly_limit_usd": 50.0,
                "monthly_limit_usd": 0.0,
            }
        });
        let usage = parse_sub2api_usage(&body).expect("parses");
        assert_eq!(usage.plan_name.as_deref(), Some("Claude Max"));
        assert_eq!(usage.remaining, Some(8.5));
        assert_eq!(
            usage.notes,
            vec![
                "日用量 1.50/10.00 USD".to_string(),
                "周用量 12.00/50.00 USD".to_string(),
            ],
            "a zero limit means the window is not configured"
        );
    }

    #[test]
    fn sub2api_wallet_groups_fall_back_to_balance() {
        let body = json!({
            "mode": "unrestricted",
            "isValid": true,
            "planName": "钱包余额",
            "unit": "USD",
            "balance": 4.25,
        });
        let usage = parse_sub2api_usage(&body).expect("parses");
        assert_eq!(usage.remaining, Some(4.25));
        assert!(!usage.unlimited);
    }

    #[test]
    fn sub2api_unrestricted_without_any_cap_is_unlimited() {
        let body = json!({"mode": "unrestricted", "isValid": true, "planName": "内部组"});
        let usage = parse_sub2api_usage(&body).expect("parses");
        assert!(usage.unlimited);
        assert_eq!(usage.remaining, None);
    }

    #[test]
    fn sub2api_invalid_keys_surface_the_panel_message() {
        let body = json!({"isValid": false, "invalidMessage": "订阅已过期"});
        let error = parse_sub2api_usage(&body).expect_err("invalid key");
        assert!(error.contains("订阅已过期"), "{error}");
    }

    #[test]
    fn dotted_paths_walk_objects_and_arrays() {
        let body = json!({
            "data": {"total_available": 12.5, "plans": [{"remaining": "3.5"}]},
        });
        assert_eq!(json_path_number(&body, "data.total_available"), Some(12.5));
        assert_eq!(json_path_number(&body, "data.plans.0.remaining"), Some(3.5));
        assert_eq!(json_path_number(&body, "data.missing"), None);
        assert_eq!(json_path_number(&body, "data.plans.9.remaining"), None);
        assert_eq!(json_path_number(&body, ""), None);
    }

    #[test]
    fn error_bodies_are_truncated() {
        let body = "x".repeat(ERROR_BODY_MAX_CHARS + 10);
        let truncated = truncate_body(&body);
        assert_eq!(truncated.chars().count(), ERROR_BODY_MAX_CHARS + 1);
        assert!(truncated.ends_with('…'));
        assert_eq!(truncate_body("short"), "short");
    }

    #[test]
    fn applying_a_snapshot_keeps_the_rest_of_the_config() {
        let config_json = json!({
            "base_url": "https://panel.example.com/v1",
            "relay_balance": {"provider": "new_api"},
        })
        .to_string();
        let snapshot = RelayBalanceSnapshot {
            provider: RelayBalanceProvider::NewApi,
            plan_name: None,
            remaining: Some(1.5),
            used: None,
            limit: None,
            unit: "USD".to_string(),
            unlimited: false,
            expires_at: None,
            source_url: "https://panel.example.com/api/usage/token/".to_string(),
            checked_at: "2026-09-02T12:00:00Z".to_string(),
            notes: Vec::new(),
        };
        let next = apply_relay_balance_snapshot(&config_json, &snapshot).expect("applies");
        let value = serde_json::from_str::<Value>(&next).expect("json");
        assert_eq!(
            value["base_url"].as_str(),
            Some("https://panel.example.com/v1")
        );
        assert_eq!(value["relay_balance"]["provider"].as_str(), Some("new_api"));
        assert_eq!(
            value["relay_balance_snapshot"]["remaining"].as_f64(),
            Some(1.5)
        );

        let error = apply_relay_balance_snapshot("[]", &snapshot).expect_err("must be an object");
        assert!(error.contains("object"), "{error}");
    }

    /// Serves the given paths as JSON and 404s everything else, then hands back
    /// an API-prefixed base URL — which is what accounts actually store, so the
    /// panel-root derivation is exercised on every integration test.
    async fn start_panel(routes: Vec<(&'static str, Value)>) -> String {
        let mut app = Router::new();
        for (path, body) in routes {
            app = app.route(
                path,
                get(move || {
                    let body = body.clone();
                    async move { Json(body) }
                }),
            );
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind panel");
        let address = listener.local_addr().expect("panel address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve panel");
        });
        format!("http://{address}/v1")
    }

    async fn memory_pool() -> SqlitePool {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        pool
    }

    async fn seed_relay_credential(
        pool: &SqlitePool,
        base_url: &str,
        relay_balance: Option<Value>,
    ) -> RouteCredential {
        let created = RouteCredentialService::create_api(
            pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Relay".into(),
                api_key: "sk-relay-key".into(),
                base_url: base_url.into(),
                interface_format: "openai".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
                relay_balance_provider: None,
            },
        )
        .await
        .expect("create");

        let Some(relay_balance) = relay_balance else {
            return created;
        };
        let mut config = serde_json::from_str::<Value>(&created.config_json).expect("config");
        config[crate::models::route_relay_balance::RELAY_BALANCE_CONFIG_KEY] = relay_balance;
        RouteCredentialRepository::update_secret_and_config(
            pool,
            &created.id,
            &created.secret_payload_json,
            &config.to_string(),
        )
        .await
        .expect("seed relay balance config");
        RouteCredentialRepository::get(pool, &created.id)
            .await
            .expect("reload")
    }

    fn snapshot_of(credential: &RouteCredential) -> RelayBalanceSnapshot {
        RelayBalanceSnapshot::from_config_json(&credential.config_json)
            .expect("snapshot was persisted")
    }

    fn new_api_usage_body() -> Value {
        json!({
            "code": true,
            "message": "ok",
            "data": {
                "object": "token_usage",
                "name": "pool key",
                "total_granted": 500_000,
                "total_used": 250_000,
                "total_available": 250_000,
                "unlimited_quota": false,
                "expires_at": 0,
            }
        })
    }

    #[tokio::test]
    async fn new_api_refresh_converts_quota_with_the_panel_divisor() {
        let base_url = start_panel(vec![
            ("/api/usage/token/", new_api_usage_body()),
            // A panel whose admin changed QuotaPerUnit. Hard-coding 500000 here
            // would report $0.50 instead of $250.
            (
                "/api/status",
                json!({"success": true, "data": {"quota_per_unit": 1000}}),
            ),
        ])
        .await;
        let pool = memory_pool().await;
        let credential =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "new_api"}))).await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id.clone())
            .await
            .expect("refresh");
        assert_eq!(outcome.source, "new_api");
        assert!(outcome.updated);

        let snapshot = snapshot_of(&outcome.credential);
        assert_eq!(snapshot.remaining, Some(250.0));
        assert_eq!(snapshot.used, Some(250.0));
        assert_eq!(snapshot.limit, Some(500.0));
        assert_eq!(snapshot.unit, "USD");
        assert!(snapshot.source_url.ends_with("/api/usage/token/"));
        assert!(
            snapshot.notes.iter().any(|note| note.contains("1000")),
            "the divisor is recorded for diagnosis: {:?}",
            snapshot.notes
        );
    }

    #[tokio::test]
    async fn new_api_refresh_falls_back_to_the_shipped_divisor() {
        let base_url = start_panel(vec![("/api/usage/token/", new_api_usage_body())]).await;
        let pool = memory_pool().await;
        let credential =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "new_api"}))).await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("refresh");
        assert_eq!(snapshot_of(&outcome.credential).remaining, Some(0.5));
    }

    #[tokio::test]
    async fn a_second_refresh_with_the_same_numbers_reports_no_change() {
        let base_url = start_panel(vec![("/api/usage/token/", new_api_usage_body())]).await;
        let pool = memory_pool().await;
        let credential =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "new_api"}))).await;

        RouteRelayBalanceService::refresh_one(&pool, credential.id.clone())
            .await
            .expect("first refresh");
        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("second refresh");
        assert!(!outcome.updated, "the balance did not move");
        // The reading still happened, so the timestamp advances.
        assert!(!snapshot_of(&outcome.credential).checked_at.is_empty());
    }

    #[tokio::test]
    async fn sub2api_refresh_stores_the_dollar_figures_as_reported() {
        let base_url = start_panel(vec![(
            "/v1/usage",
            json!({
                "mode": "quota_limited",
                "isValid": true,
                "quota": {"limit": 50, "used": 12.3, "remaining": 37.7, "unit": "USD"},
                "remaining": 37.7,
                "unit": "USD",
            }),
        )])
        .await;
        let pool = memory_pool().await;
        let credential =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "sub2api"}))).await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("refresh");
        assert_eq!(outcome.source, "sub2api");
        let snapshot = snapshot_of(&outcome.credential);
        assert_eq!(snapshot.remaining, Some(37.7));
        assert_eq!(snapshot.limit, Some(50.0));
        assert!(snapshot.source_url.ends_with("/v1/usage"));
    }

    #[tokio::test]
    async fn custom_refresh_reads_the_declared_paths() {
        let base_url = start_panel(vec![(
            "/billing/summary",
            json!({"result": {"left": 24_000, "spent": 6_000, "cap": 30_000, "tier": "vip"}}),
        )])
        .await;
        let pool = memory_pool().await;
        let panel_root = base_url.trim_end_matches("/v1").to_string();
        let credential = seed_relay_credential(
            &pool,
            &base_url,
            Some(json!({
                "provider": "custom",
                "endpoint": format!("{panel_root}/billing/summary"),
                "remaining_path": "result.left",
                "used_path": "result.spent",
                "limit_path": "result.cap",
                "plan_path": "result.tier",
                "unit": "CNY",
                "divisor": 1000,
            })),
        )
        .await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("refresh");
        assert_eq!(outcome.source, "custom");
        let snapshot = snapshot_of(&outcome.credential);
        assert_eq!(snapshot.remaining, Some(24.0));
        assert_eq!(snapshot.used, Some(6.0));
        assert_eq!(snapshot.limit, Some(30.0));
        assert_eq!(snapshot.plan_name.as_deref(), Some("vip"));
        assert_eq!(snapshot.unit, "CNY");
    }

    #[tokio::test]
    async fn accounts_without_the_setting_are_skipped_rather_than_failed() {
        let pool = memory_pool().await;
        let credential = seed_relay_credential(&pool, "https://panel.example.com/v1", None).await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("skipping is not an error");
        assert_eq!(outcome.source, "skipped");
        assert!(!outcome.updated);
        assert!(outcome.message.expect("explains itself").contains("未开启"));
    }

    #[tokio::test]
    async fn archived_accounts_cannot_refresh() {
        let pool = memory_pool().await;
        let credential = seed_relay_credential(
            &pool,
            "https://panel.example.com/v1",
            Some(json!({"provider": "new_api"})),
        )
        .await;
        sqlx::query("UPDATE route_credentials SET archived_at = ? WHERE id = ?")
            .bind("2026-09-01T00:00:00Z")
            .bind(&credential.id)
            .execute(&pool)
            .await
            .expect("archive");

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("archived accounts are out of scope");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.route_credential_archived",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn a_panel_that_rejects_the_key_reports_the_upstream_status() {
        let pool = memory_pool().await;
        // No route for /api/usage/token/, so every candidate 404s.
        let base_url = start_panel(vec![("/unrelated", json!({}))]).await;
        let credential =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "new_api"}))).await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("nothing answered");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.route_relay_balance_all_failed",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn refresh_platform_only_touches_configured_relay_accounts() {
        let base_url = start_panel(vec![("/api/usage/token/", new_api_usage_body())]).await;
        let pool = memory_pool().await;
        seed_relay_credential(&pool, &base_url, None).await;
        let configured =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "new_api"}))).await;

        let outcomes = RouteRelayBalanceService::refresh_platform(&pool, "codex".to_string())
            .await
            .expect("refresh platform");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].credential.id, configured.id);
        assert_eq!(outcomes[0].source, "new_api");
    }

    #[tokio::test]
    async fn official_accounts_are_not_relay_accounts() {
        let pool = memory_pool().await;
        let credential = seed_relay_credential(
            &pool,
            "https://panel.example.com/v1",
            Some(json!({"provider": "new_api"})),
        )
        .await;
        sqlx::query("UPDATE route_credentials SET kind = 'official' WHERE id = ?")
            .bind(&credential.id)
            .execute(&pool)
            .await
            .expect("flip kind");

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("skipping is not an error");
        assert_eq!(outcome.source, "skipped");
        assert!(outcome
            .message
            .expect("explains itself")
            .contains("中转站账号"));
    }
}
