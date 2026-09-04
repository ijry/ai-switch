use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::error::AppError;
use crate::models::route_credential::RouteCredential;
use crate::models::route_relay_balance::{
    RelayBalanceConfig, RelayBalanceProvider, RelayBalanceSnapshot, DEFAULT_NEW_API_QUOTA_PER_UNIT,
    RELAY_BALANCE_ACCESS_TOKEN_KEY, RELAY_BALANCE_ACCESS_TOKEN_USER_ID_KEY,
    RELAY_BALANCE_SNAPSHOT_KEY,
};
use crate::services::deeplink_service::mask_api_key;
use crate::services::http_client::build_outbound_http_client;
use crate::services::route_proxy_service::credential_user_agent;
use chrono::{TimeZone, Utc};
use futures_util::StreamExt;
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
    ///
    /// Accounts are swept a few at a time rather than strictly one after another.
    /// A slow relay costs a full per-request timeout per candidate, and both the
    /// single-account and the batch button stay disabled for the whole run with no
    /// way to cancel, so a serial sweep over a handful of accounts left the UI
    /// frozen for minutes on one unresponsive panel. The cap stays small because
    /// several accounts often share one panel host.
    pub async fn refresh_platform(
        pool: &SqlitePool,
        platform: String,
    ) -> Result<Vec<RelayBalanceRefreshOutcome>, AppError> {
        let credentials =
            RouteCredentialRepository::list_by_platform(pool, platform.trim()).await?;
        let queued = credentials.into_iter().filter(|credential| {
            credential.kind == "api"
                && credential.archived_at.is_none()
                && RelayBalanceConfig::from_config_json(&credential.config_json).is_some()
        });
        let outcomes = futures_util::stream::iter(queued.map(|credential| async move {
            match refresh_credential(pool, credential.clone()).await {
                Ok(outcome) => outcome,
                Err(err) => RelayBalanceRefreshOutcome {
                    credential,
                    updated: false,
                    source: "error".to_string(),
                    message: Some(describe_failure(&err)),
                },
            }
        }))
        // Ordered, so a future consumer that assumes list order is not surprised;
        // today both (the batch summary's counts and the per-row status writeback,
        // which keys by credential id) are order-independent anyway.
        .buffered(RELAY_BALANCE_BATCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        Ok(outcomes)
    }
}

/// How many accounts a batch balance refresh sweeps at once.
const RELAY_BALANCE_BATCH_CONCURRENCY: usize = 4;

/// Renders a failure for the batch path, which has no error channel of its own
/// and folds each one into a per-account string. `Display` is only the message,
/// and for a relay panel that message is deliberately generic ("余额查询请求失败") —
/// the URL tried and the panel's own answer live in the detail. Dropping it
/// leaves a batch refresh reporting "失败 3" with nothing to act on, while the
/// single-account path shows the same failure in full.
fn describe_failure(err: &AppError) -> String {
    match err.details() {
        // Same shape as the front end's `formatApiError`, so one failure reads
        // identically whether it arrived through the row action or the batch.
        Some(details) => format!("{err} ({details})"),
        None => err.to_string(),
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
        // The dialect that actually answered, which is not always the one the
        // account selected — see `fetch_with_dialect_fallback`.
        source: snapshot.provider.as_str().to_string(),
        message: None,
    })
}

/// Everything an adapter needs from the account row.
struct BalanceRequest {
    base_url: String,
    api_key: String,
    /// The panel account's own access token, when the user supplied one.
    ///
    /// A New API token with `unlimited_quota` has no allowance of its own, so the
    /// token-scoped endpoint can only report what it spent. The balance that
    /// actually runs out belongs to the account, and only this token can read it.
    access_token: Option<String>,
    /// The numeric panel user id that goes with `access_token`, for the panels that
    /// insist on a matching `New-Api-User` header.
    access_token_user_id: Option<String>,
    interface_format: String,
    /// The account's own `User-Agent`, when it configures one. Relays that gate
    /// a group on the client fingerprint reject anything else, so a balance
    /// query has to introduce itself the same way the proxy does.
    user_agent: Option<String>,
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
        let secret =
            serde_json::from_str::<Value>(&credential.secret_payload_json).unwrap_or(Value::Null);
        let api_key = secret_string(&secret, "api_key").unwrap_or_default();
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
            access_token: secret_string(&secret, RELAY_BALANCE_ACCESS_TOKEN_KEY),
            access_token_user_id: secret_string(&secret, RELAY_BALANCE_ACCESS_TOKEN_USER_ID_KEY),
            interface_format,
            user_agent: credential_user_agent(config).map(str::to_string),
        })
    }
}

/// Reads one trimmed, non-empty string out of a secret payload.
fn secret_string(secret: &Value, key: &str) -> Option<String> {
    let value = secret.get(key).and_then(Value::as_str)?.trim();
    (!value.is_empty()).then(|| value.to_string())
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
    let headers = balance_request_headers(
        &request.api_key,
        &request.interface_format,
        request.user_agent.as_deref(),
    )
    .map_err(|err| AppError::Validation {
        code: "validation.route_relay_balance_headers",
        message: "无法构造余额查询请求头".to_string(),
        details: Some(err),
        recoverable: true,
    })?;

    fetch_with_dialect_fallback(&client, headers, config, request).await
}

/// Runs the selected dialect, and on "no such endpoint anywhere" runs the other
/// built-in one.
///
/// Only "every candidate 404'd" earns the retry. A 401, a 403, or a panel that
/// answered with an error envelope all mean the endpoint is there and the problem
/// lies elsewhere; retrying the other dialect would bury the real reason under a
/// second, less relevant failure.
async fn fetch_with_dialect_fallback(
    client: &Client,
    headers: HeaderMap,
    config: &RelayBalanceConfig,
    request: &BalanceRequest,
) -> Result<RelayBalanceSnapshot, AppError> {
    let chosen = config.provider;
    let err = match fetch_with_dialect(client, headers.clone(), config, request, chosen).await {
        Ok(snapshot) => return Ok(snapshot),
        Err(err) => err,
    };
    let Some(other) = chosen.other_built_in() else {
        return Err(err);
    };
    // Only when the panel answered every candidate and none of them served the
    // endpoint. `..._unreachable` — anything that never answered at all — must not
    // reach here: the timeout below is reqwest's *per-request* limit, not a budget
    // shared with the first round, so a wall-blocked or dead host would spend a
    // fresh 15s per candidate proving the same thing a second time.
    if err.code() != "validation.route_relay_balance_all_failed" {
        return Err(err);
    }
    // The retry deliberately reuses the same client and the same per-request 15s
    // limit. A shorter leash was tried and rejected: at 8s the real kktoken.cc and
    // worldclawpro.ai panels — both of which do answer `/api/usage/token/` —
    // timed out and reported a balance of "查询失败", which is the exact failure
    // this fallback exists to remove. A slow relay is normal.
    match fetch_with_dialect(client, headers, config, request, other).await {
        Ok(mut snapshot) => {
            // Say so rather than silently answering a different question: the
            // account's setting still reads `sub2api` while the number came from
            // new-api, and the user is the one who can fix that.
            snapshot.notes.push(format!(
                "面板实际按 {} 应答（账号里选的是 {}）",
                other.label(),
                chosen.label()
            ));
            Ok(snapshot)
        }
        Err(fallback_err) => {
            // A fallback failure that is not "no such endpoint here" means the
            // other dialect *found* the endpoint and something else went wrong —
            // most often a 401/403, i.e. the key is the problem. That is the real
            // diagnosis, so surfacing the selected dialect's "no address worked"
            // instead (and asserting the other dialect had nothing either) both
            // discards the answer and contradicts it.
            if !matches!(
                fallback_err.code(),
                "validation.route_relay_balance_all_failed"
                    | "validation.route_relay_balance_unreachable"
            ) {
                return Err(with_dialect_note(fallback_err, other));
            }
            Err(with_fallback_note(err, other))
        }
    }
}

/// Marks an error as coming from the dialect the user did not select.
fn with_dialect_note(err: AppError, other: RelayBalanceProvider) -> AppError {
    let AppError::Validation {
        code,
        message,
        details,
        recoverable,
    } = err
    else {
        return err;
    };
    let note = format!("（按 {} 的地址查到了接口）", other.label());
    AppError::Validation {
        code,
        message,
        details: Some(match details {
            Some(details) => format!("{details}{note}"),
            None => note,
        }),
        recoverable,
    }
}

async fn fetch_with_dialect(
    client: &Client,
    headers: HeaderMap,
    config: &RelayBalanceConfig,
    request: &BalanceRequest,
    provider: RelayBalanceProvider,
) -> Result<RelayBalanceSnapshot, AppError> {
    match provider {
        RelayBalanceProvider::NewApi => {
            fetch_new_api_balance(client, headers, config, request).await
        }
        RelayBalanceProvider::Sub2Api => fetch_sub2api_balance(client, headers, request).await,
        RelayBalanceProvider::Custom => {
            fetch_custom_balance(client, headers, config, &request.api_key).await
        }
    }
}

fn with_fallback_note(err: AppError, other: RelayBalanceProvider) -> AppError {
    let AppError::Validation {
        code,
        message,
        details,
        recoverable,
    } = err
    else {
        return err;
    };
    let note = format!("也试过 {} 的地址，同样没有", other.label());
    AppError::Validation {
        code,
        message,
        details: Some(match details {
            Some(details) => format!("{details}；{note}"),
            None => note,
        }),
        recoverable,
    }
}

/// Both built-in panels document `Authorization: Bearer <key>`, whatever dialect
/// the relay itself speaks, so that header is unconditional. `x-api-key` rides
/// along for Anthropic-dialect accounts because some gateways only read that
/// one; panels that don't know it ignore it.
///
/// `user_agent` carries the account's own override when it has one: a relay that
/// gates a group on the client fingerprint answers 403 to anything else, and a
/// healthy account reading as "query failed" is worse than no badge at all.
fn balance_request_headers(
    api_key: &str,
    interface_format: &str,
    user_agent: Option<&str>,
) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    // The outbound client has no decompression support; ask for plain bytes.
    headers.insert("accept-encoding", HeaderValue::from_static("identity"));
    headers.insert(
        USER_AGENT,
        match user_agent {
            Some(value) => HeaderValue::from_str(value)
                .map_err(|err| format!("Invalid user-agent header: {err}"))?,
            None => HeaderValue::from_static(USER_AGENT_VALUE),
        },
    );
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

/// Headers for new-api's account-scoped routes, which authenticate as the panel
/// user rather than as one key.
///
/// `Bearer ` is kept because that is the form cc-switch ships and users have proved
/// against these panels; new-api strips the prefix before looking the token up, and
/// its newer releases accept either spelling.
///
/// `New-Api-User` carries the numeric user id. Every New API release up to v0.13.2
/// (the current stable line) refuses an access-token request without it and checks it
/// against the token's owner; `main` dropped the check, where sending it is harmless.
/// It is omitted when unknown rather than guessed — a wrong id is rejected exactly
/// like a missing one, and a literal placeholder is how cc-switch turns an unfilled
/// box into an unexplained 401.
///
/// `x-api-key` is deliberately absent: this is not a relay key, and a panel that
/// reads that header would authenticate the wrong identity.
fn account_request_headers(
    access_token: &str,
    user_id: Option<&str>,
    user_agent: Option<&str>,
) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("accept-encoding", HeaderValue::from_static("identity"));
    headers.insert(
        USER_AGENT,
        match user_agent {
            Some(value) => HeaderValue::from_str(value)
                .map_err(|err| format!("Invalid user-agent header: {err}"))?,
            None => HeaderValue::from_static(USER_AGENT_VALUE),
        },
    );
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}"))
            .map_err(|err| format!("Invalid authorization header: {err}"))?,
    );
    if let Some(user_id) = user_id {
        // The panel parses it with strconv.Atoi, so anything else is a typo worth
        // naming rather than a 401 to decipher.
        if !user_id.chars().all(|character| character.is_ascii_digit()) {
            return Err(format!("面板用户 ID 必须是数字，收到的是 {user_id}"));
        }
        headers.insert(
            "New-Api-User",
            HeaderValue::from_str(user_id)
                .map_err(|err| format!("Invalid New-Api-User header: {err}"))?,
        );
    }
    Ok(headers)
}

/// Why one candidate URL did not produce a balance.
///
/// The distinction drives the dialect fallback: "this panel answered and does not
/// serve that endpoint" is a reason to try the other dialect, while "the panel
/// never answered" is not — retrying a dead or wall-blocked host under a second
/// dialect only spends another full timeout per candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateFailure {
    /// Never got a response: DNS, TLS, connect, or the request timeout.
    Transport,
    /// The panel answered that this path is not there (404/405, or a non-JSON 200
    /// from its single-page-app catch-all route).
    Absent,
}

/// Tries each candidate URL in order. Transport errors, 404, 405 and a 200 whose
/// body is not JSON move on to the next one; any other non-success status is
/// reported as-is, because a 401 or 403 means the endpoint exists and the key is
/// the problem.
///
/// On total failure every candidate's own reason is reported, not just the last
/// one: for a Base URL that already ends in `/v1` the last candidate is the
/// double-prefix `…/v1/v1/…` address this module derives itself, so keeping only
/// that one pointed the user at a URL that was never meant to exist while hiding
/// the answer the real endpoint gave.
async fn get_json_from_candidates(
    client: &Client,
    headers: &HeaderMap,
    candidates: &[String],
    secret: &str,
) -> Result<(String, Value), AppError> {
    if candidates.is_empty() {
        return Err(validation_error(
            "validation.route_relay_balance_endpoint",
            "无法根据 Base URL 推导余额查询地址",
            None,
        ));
    }
    let mut failures: Vec<(CandidateFailure, String)> = Vec::new();
    for url in candidates {
        let response = match client.get(url).headers(headers.clone()).send().await {
            Ok(response) => response,
            Err(err) => {
                failures.push((
                    CandidateFailure::Transport,
                    redact_secret(format!("{url}: {err}"), secret),
                ));
                continue;
            }
        };
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            match serde_json::from_str::<Value>(&body) {
                Ok(parsed) => return Ok((url.clone(), parsed)),
                // A relay panel is a single-page app, and its catch-all route
                // answers 200 with `index.html` for every path it does not know.
                // So a non-JSON 200 means "no such endpoint here", not "the panel
                // is broken": treat it like the 404 it morally is and keep going,
                // or a real endpoint sitting behind an SPA route never gets asked.
                Err(err) => {
                    failures.push((
                        CandidateFailure::Absent,
                        redact_secret(
                            format!(
                                "{url}: HTTP {status}，但响应不是 JSON（{err}）: {}",
                                truncate_body(&body)
                            ),
                            secret,
                        ),
                    ));
                    continue;
                }
            }
        }
        let message = redact_secret(
            format!("{url}: HTTP {status}: {}", truncate_body(&body)),
            secret,
        );
        if status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED {
            failures.push((CandidateFailure::Absent, message));
            continue;
        }
        return Err(validation_error(
            "validation.route_relay_balance_http",
            "余额查询请求失败",
            Some(message),
        ));
    }
    // Every candidate answered, and none of them served the endpoint: the caller
    // may usefully try the other dialect. A transport failure anywhere makes this
    // inconclusive instead, so the caller must not spend a second round.
    let all_absent = failures
        .iter()
        .all(|(kind, _)| *kind == CandidateFailure::Absent);
    let details = failures
        .into_iter()
        .map(|(_, message)| message)
        .collect::<Vec<_>>()
        .join("；");
    Err(validation_error(
        if all_absent {
            "validation.route_relay_balance_all_failed"
        } else {
            "validation.route_relay_balance_unreachable"
        },
        if all_absent {
            "所有余额查询地址都失败了"
        } else {
            "余额查询地址都没有应答"
        },
        (!details.is_empty()).then_some(details),
    ))
}

const NEW_API_USAGE_PATH: &str = "/api/usage/token/";
const NEW_API_USER_SELF_PATH: &str = "/api/user/self";

/// new-api's token-scoped usage endpoint, plus the account-scoped one behind it.
///
/// `/api/usage/token/` authenticates with the relay key the account already
/// stores, so the common case needs no extra input. It answers a token-scoped
/// question though, and a token the panel marked `unlimited_quota` has no
/// allowance to report — only what it spent. What runs out for those keys is the
/// panel *account*'s balance, and reading that means `/api/user/self` with the
/// account's own access token (个人设置 → 系统访问令牌), which is why that token is
/// worth an input box even though the rest of this provider needs none.
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
    let (url, body) =
        match get_json_from_candidates(client, &headers, &candidates, &request.api_key).await {
            Ok(found) => found,
            // Some new-api forks dropped the token-scoped route entirely. With an
            // access token there is still an account-scoped answer to be had, so a
            // panel that serves no token usage is worth one more question — but only
            // when the sweep proved the path absent rather than the host unreachable,
            // for the same reason the dialect fallback insists on that distinction.
            Err(err)
                if request.access_token.is_some()
                    && err.code() == "validation.route_relay_balance_all_failed" =>
            {
                return fetch_new_api_account_balance(client, config, request, None, None)
                    .await
                    .map_err(|account_err| merge_account_failure(err, account_err));
            }
            Err(err) => return Err(err),
        };
    let usage = parse_new_api_token_usage(&body).map_err(|message| {
        validation_error(
            "validation.route_relay_balance_parse",
            message,
            Some(redact_secret(
                format!("{url}: {}", truncate_body(&body.to_string())),
                &request.api_key,
            )),
        )
    })?;

    let panel_root = url
        .strip_suffix(NEW_API_USAGE_PATH)
        .unwrap_or(url.as_str())
        .to_string();

    // An uncapped token hides the only number the user is asking for. Reading the
    // account instead is the whole point of storing an access token, so a failure
    // here is reported rather than quietly downgraded to "余额 不限" — that is the
    // reading this path exists to replace.
    if usage.unlimited && request.access_token.is_some() {
        return fetch_new_api_account_balance(
            client,
            config,
            request,
            Some(&panel_root),
            Some(&usage),
        )
        .await;
    }

    let (divisor, divisor_source) = new_api_divisor(client, config, &panel_root).await;

    let mut notes = Vec::new();
    if let Some(name) = &usage.name {
        notes.push(format!("令牌 {name}"));
    }
    if usage.unlimited {
        notes.push("面板标记为不限额度".to_string());
        // The badge would otherwise read "余额 不限" forever on a panel whose
        // account balance is the thing that actually empties.
        notes.push("填写面板访问令牌后可显示账户剩余额度".to_string());
    }
    // Always stated, including for the shipped default: the divisor scales the
    // whole figure, so "the panel could not be read, this is a guess" is exactly
    // what the user needs when the amount looks wrong by a factor of a hundred.
    notes.push(format!(
        "额度换算 {}",
        format_divisor(divisor, divisor_source)
    ));

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
        account_level: false,
        expires_at: usage.expires_at.and_then(unix_seconds_to_rfc3339),
        source_url: url,
        checked_at: Utc::now().to_rfc3339(),
        notes,
    })
}

/// Reads the panel account's balance with its access token.
///
/// `panel_root` is passed when the token endpoint already identified the panel, so
/// the sweep is not repeated; without it every candidate root is tried, which is
/// the case for forks that serve only this route.
async fn fetch_new_api_account_balance(
    client: &Client,
    config: &RelayBalanceConfig,
    request: &BalanceRequest,
    panel_root: Option<&str>,
    token_usage: Option<&NewApiTokenUsage>,
) -> Result<RelayBalanceSnapshot, AppError> {
    let access_token = request.access_token.as_deref().unwrap_or_default();
    let headers = account_request_headers(
        access_token,
        request.access_token_user_id.as_deref(),
        request.user_agent.as_deref(),
    )
    .map_err(|err| AppError::Validation {
        code: "validation.route_relay_balance_account_headers",
        message: "无法构造账户额度查询请求头".to_string(),
        details: Some(err),
        recoverable: true,
    })?;
    let roots = match panel_root {
        Some(root) => vec![root.to_string()],
        None => panel_root_candidates(&request.base_url),
    };
    let candidates: Vec<String> = roots
        .into_iter()
        .map(|root| format!("{root}{NEW_API_USER_SELF_PATH}"))
        .collect();
    let (url, body) = get_json_from_candidates(client, &headers, &candidates, access_token)
        .await
        .map_err(|err| with_user_id_hint(err, request))?;
    let account = parse_new_api_user_self(&body).map_err(|message| {
        with_user_id_hint(
            validation_error(
                "validation.route_relay_balance_account_parse",
                message,
                Some(redact_secret(
                    format!("{url}: {}", truncate_body(&body.to_string())),
                    access_token,
                )),
            ),
            request,
        )
    })?;

    let panel_root = url
        .strip_suffix(NEW_API_USER_SELF_PATH)
        .unwrap_or(url.as_str())
        .to_string();
    let (divisor, divisor_source) = new_api_divisor(client, config, &panel_root).await;

    let mut notes = Vec::new();
    match token_usage {
        Some(usage) => {
            notes.push("令牌不限额度，显示的是面板账户余额".to_string());
            if let Some(name) = &usage.name {
                notes.push(format!("令牌 {name}"));
            }
            if let Some(used) = usage.total_used {
                notes.push(format!("本令牌已用 {}", format_usd(used / divisor)));
            }
        }
        // Reached only when the panel had no token-scoped route at all; saying so
        // keeps the account-wide figure from reading as this key's allowance.
        None => notes.push("面板没有令牌级余额接口，显示的是面板账户余额".to_string()),
    }
    if let Some(username) = &account.username {
        notes.push(match account.id {
            Some(id) => format!("面板账户 {username}（ID {id}）"),
            None => format!("面板账户 {username}"),
        });
    }
    notes.push(format!(
        "额度换算 {}",
        format_divisor(divisor, divisor_source)
    ));

    Ok(RelayBalanceSnapshot {
        provider: RelayBalanceProvider::NewApi,
        // new-api calls it a group; it is what decides this account's pricing, so
        // it lands where every other provider's plan name does.
        plan_name: account.group,
        remaining: account.quota.map(|value| value / divisor),
        used: account.used_quota.map(|value| value / divisor),
        // The panel reports what is left and what was spent, never an allowance.
        // Adding the two would invent a "total" that means "topped up so far".
        limit: None,
        unit: "USD".to_string(),
        // The account has a real number, whatever the token's own cap says. Keeping
        // `unlimited` here would hide it behind the "不限" badge again.
        unlimited: false,
        account_level: true,
        // The key's own expiry still ends its usefulness, account balance or not.
        expires_at: token_usage
            .and_then(|usage| usage.expires_at)
            .and_then(unix_seconds_to_rfc3339),
        source_url: url,
        checked_at: Utc::now().to_rfc3339(),
        notes,
    })
}

/// Folds an account-endpoint failure into the token-endpoint failure that led to
/// it.
///
/// The distinction that matters is who gets the last word. When the account
/// endpoint answered and refused us — a rejected access token, most often — that
/// is the diagnosis, and it must not be replaced by "no address worked". When it
/// was simply not there either, the token failure keeps its code so the caller can
/// still try the other dialect.
fn merge_account_failure(token_err: AppError, account_err: AppError) -> AppError {
    if !matches!(
        account_err.code(),
        "validation.route_relay_balance_all_failed" | "validation.route_relay_balance_unreachable"
    ) {
        return account_err;
    }
    let note = match account_err.details() {
        Some(details) => format!("也试过面板账户额度接口：{details}"),
        None => format!("也试过面板账户额度接口：{account_err}"),
    };
    append_details(token_err, note)
}

/// Names the likeliest cause when the account route refused us and no user id was
/// on file.
///
/// Every New API release through v0.13.2 answers 401 to an access-token request with
/// no `New-Api-User` header, and its own wording talks about a header the user has
/// never heard of. Pointing at the box that fixes it beats making them find that
/// out. Left alone once an id is configured: then the refusal is about something
/// else, and a wrong guess would send them after the wrong field.
fn with_user_id_hint(err: AppError, request: &BalanceRequest) -> AppError {
    if request.access_token_user_id.is_some() {
        return err;
    }
    append_details(
        err,
        "多数 new-api 面板还要求随访问令牌一起发送用户 ID（New-Api-User 头），请在余额查询里补填面板用户 ID"
            .to_string(),
    )
}

fn append_details(err: AppError, note: String) -> AppError {
    let AppError::Validation {
        code,
        message,
        details,
        recoverable,
    } = err
    else {
        return err;
    };
    AppError::Validation {
        code,
        message,
        details: Some(match details {
            Some(details) => format!("{details}；{note}"),
            None => note,
        }),
        recoverable,
    }
}

/// The divisor that turns new-api's integer quota into money, and where it came
/// from. Both the token-scoped and the account-scoped reading need it.
async fn new_api_divisor(
    client: &Client,
    config: &RelayBalanceConfig,
    panel_root: &str,
) -> (f64, DivisorSource) {
    match config.divisor {
        Some(divisor) => (divisor, DivisorSource::UserPinned),
        None => match fetch_new_api_quota_per_unit(client, panel_root).await {
            Some(divisor) => (divisor, DivisorSource::Panel),
            None => (DEFAULT_NEW_API_QUOTA_PER_UNIT, DivisorSource::Default),
        },
    }
}

fn format_usd(amount: f64) -> String {
    format!("${amount:.2}")
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
        DivisorSource::Default => "默认（未能读取面板 quota_per_unit）",
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

/// What `/api/user/self` says about the panel account behind a key.
#[derive(Debug, Clone, PartialEq)]
struct NewApiUserAccount {
    id: Option<i64>,
    username: Option<String>,
    group: Option<String>,
    /// Remaining account quota, in the panel's integer unit.
    quota: Option<f64>,
    used_quota: Option<f64>,
}

/// Reads new-api's account-scoped response.
///
/// This route stamps the envelope on `success`, not the `code` that
/// `/api/usage/token/` uses. Probed against real panels on 2026-09-04
/// (api.justwoker.icu, worldclawpro.ai), a rejected access token comes back as
/// HTTP 401 with `{"code":"AUTH_UNAUTHORIZED","message":"Unauthorized, invalid
/// access token","success":false}` — so the refusal is normally caught by the
/// status check upstream, and `code` there is a *string*, which is why reading it
/// as the envelope bool has to stay a fallback that yields nothing on those
/// bodies. The flag is still checked before the numbers because new-api does hand
/// out 200-with-`false`: that is exactly how `/api/usage/token/` refuses a key.
fn parse_new_api_user_self(body: &Value) -> Result<NewApiUserAccount, String> {
    let envelope_ok = body
        .get("success")
        .and_then(Value::as_bool)
        .or_else(|| body.get("code").and_then(Value::as_bool));
    if envelope_ok == Some(false) {
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|message| !message.is_empty())
            .unwrap_or("面板拒绝了账户额度查询");
        return Err(format!("面板返回失败：{message}"));
    }
    let Some(data) = body.get("data").filter(|data| data.is_object()) else {
        return Err("面板响应里没有 data 对象".to_string());
    };

    let account = NewApiUserAccount {
        id: number_field(data, "id").map(|value| value as i64),
        username: string_field(data, "username").or_else(|| string_field(data, "display_name")),
        group: string_field(data, "group"),
        quota: number_field(data, "quota"),
        used_quota: number_field(data, "used_quota"),
    };
    if account.quota.is_none() {
        return Err("面板响应里没有账户剩余额度字段 quota".to_string());
    }
    Ok(account)
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
        // Same reason as the balance request: nothing here can decompress, and a
        // gzipped body would fall through as "panel unreadable" and quietly cost
        // the reading its real divisor.
        .header("accept-encoding", "identity")
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

    let (url, body) =
        get_json_from_candidates(client, &headers, &candidates, &request.api_key).await?;
    let usage = parse_sub2api_usage(&body).map_err(|message| {
        validation_error(
            "validation.route_relay_balance_parse",
            message,
            Some(redact_secret(
                format!("{url}: {}", truncate_body(&body.to_string())),
                &request.api_key,
            )),
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
        account_level: false,
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
    secret: &str,
) -> Result<RelayBalanceSnapshot, AppError> {
    let candidates = vec![config.endpoint.trim().to_string()];
    let (url, body) = get_json_from_candidates(client, &headers, &candidates, secret).await?;

    let divisor = config.divisor.unwrap_or(1.0);
    let remaining_path = config.remaining_path.trim();
    let Some(remaining) = json_path_number(&body, remaining_path) else {
        return Err(validation_error(
            "validation.route_relay_balance_parse",
            format!("响应里 {remaining_path} 取不到数值"),
            Some(redact_secret(
                format!("{url}: {}", truncate_body(&body.to_string())),
                secret,
            )),
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
        account_level: false,
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

/// Mask the account's own key wherever it appears in an error detail.
///
/// Details keep the URL and a slice of the response body on purpose: they are the
/// only useful signal when a relay misbehaves, and they are shown verbatim in the
/// UI. The key must not ride along — a panel that echoes the token back in its
/// error, or a custom endpoint that carries it in the query string, would put it
/// on screen. Short values are left alone: they are not keys, and replacing a
/// common substring would only garble the text.
fn redact_secret(detail: String, secret: &str) -> String {
    let secret = secret.trim();
    if secret.chars().count() < 8 {
        return detail;
    }
    detail.replace(secret, &mask_api_key(secret))
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
    use axum::http::HeaderMap as AxumHeaderMap;
    use axum::http::StatusCode as HttpStatus;
    use axum::response::Html;
    use axum::response::IntoResponse;
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
    fn error_details_mask_the_accounts_own_key() {
        // Details are shown verbatim in the UI. A panel that echoes the token in
        // its error, or a custom endpoint carrying it in the query string, must
        // not put it on screen — but everything else stays, because that is what
        // makes a relay failure diagnosable.
        let key = "sk-relay-abcdef123456";
        let detail = redact_secret(
            format!(
                "https://panel.example.com/api/usage?token={key}: HTTP 401: {{\"key\":\"{key}\"}}"
            ),
            key,
        );
        assert!(!detail.contains(key), "{detail}");
        assert!(detail.contains(&mask_api_key(key)), "{detail}");
        assert!(detail.contains("HTTP 401"), "{detail}");
        assert!(detail.contains("panel.example.com"), "{detail}");

        // Too short to be a key: replacing a common substring would only garble
        // the message.
        assert_eq!(
            redact_secret("abc: HTTP 500".to_string(), "abc"),
            "abc: HTTP 500"
        );
        assert_eq!(redact_secret("nothing".to_string(), ""), "nothing");
    }

    #[test]
    fn balance_headers_prefer_the_accounts_own_user_agent() {
        // A relay that gates a group on the client fingerprint answers 403 to any
        // other agent, so a healthy account would report "query failed" forever.
        let spoofed = balance_request_headers("sk-test", "anthropic", Some("claude-cli/2.0.1"))
            .expect("headers");
        assert_eq!(spoofed[USER_AGENT], "claude-cli/2.0.1");
        // Nothing configured falls back to this app's own agent.
        let default = balance_request_headers("sk-test", "openai", None).expect("headers");
        assert_eq!(default[USER_AGENT], USER_AGENT_VALUE);
        // Unchanged by the override: the panels document Bearer, and the outbound
        // client cannot decompress.
        assert_eq!(spoofed[AUTHORIZATION], "Bearer sk-test");
        assert_eq!(spoofed["accept-encoding"], "identity");
        assert_eq!(spoofed["x-api-key"], "sk-test");
        assert!(!default.contains_key("x-api-key"));
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
    fn new_api_user_self_reads_the_account_wide_numbers() {
        let body = json!({
            "success": true,
            "message": "",
            "data": {
                "id": 114_514,
                "username": "pooluser",
                "display_name": "Pool User",
                "group": "vip",
                "quota": 6_150_000,
                "used_quota": 3_850_000,
                "request_count": 42,
            }
        });
        let account = parse_new_api_user_self(&body).expect("parses");
        assert_eq!(account.id, Some(114_514));
        assert_eq!(account.username.as_deref(), Some("pooluser"));
        assert_eq!(account.group.as_deref(), Some("vip"));
        assert_eq!(account.quota, Some(6_150_000.0));
        assert_eq!(account.used_quota, Some(3_850_000.0));
    }

    /// The account route stamps its envelope on `success`, and new-api does refuse
    /// with 200 plus a false flag on its other routes, so a status-only check would
    /// read such a refusal as a balance of zero.
    #[test]
    fn new_api_user_self_surfaces_a_rejected_access_token() {
        let body = json!({"success": false, "message": "无权进行此操作，access token 无效"});
        let error = parse_new_api_user_self(&body).expect_err("must not look like success");
        assert!(error.contains("access token 无效"), "{error}");
    }

    /// The body real panels actually send with their 401 (probed 2026-09-04 against
    /// api.justwoker.icu and worldclawpro.ai). `code` is a string here, so reading
    /// it as the envelope flag must not turn the refusal into a success.
    #[test]
    fn new_api_user_self_reads_the_shape_real_panels_refuse_with() {
        let body = json!({
            "code": "AUTH_UNAUTHORIZED",
            "message": "Unauthorized, invalid access token",
            "success": false,
        });
        let error = parse_new_api_user_self(&body).expect_err("must not look like success");
        assert!(error.contains("invalid access token"), "{error}");
    }

    #[test]
    fn new_api_user_self_without_a_quota_field_is_an_error() {
        let body = json!({"success": true, "data": {"username": "pooluser"}});
        let error = parse_new_api_user_self(&body).expect_err("no balance to show");
        assert!(error.contains("quota"), "{error}");

        let body = json!({"success": true, "data": "nope"});
        let error = parse_new_api_user_self(&body).expect_err("data must be an object");
        assert!(error.contains("data"), "{error}");
    }

    /// The account route authenticates the panel *user*, so it carries the access
    /// token rather than the relay key, and `x-api-key` would only offer a second,
    /// wrong identity. `New-Api-User` goes along when the id is known: New API's
    /// stable line rejects the request without it.
    #[test]
    fn account_headers_send_the_access_token_and_the_user_id() {
        let headers = account_request_headers("pat-abcdef123456", None, None).expect("headers");
        assert_eq!(headers[AUTHORIZATION], "Bearer pat-abcdef123456");
        assert!(!headers.contains_key("x-api-key"));
        assert!(
            !headers.contains_key("New-Api-User"),
            "an unknown id is left out, not guessed"
        );
        assert_eq!(headers[USER_AGENT], USER_AGENT_VALUE);
        assert_eq!(headers["accept-encoding"], "identity");

        let identified =
            account_request_headers("pat-abcdef123456", Some("114514"), Some("claude-cli/2.0.1"))
                .expect("headers");
        assert_eq!(identified["New-Api-User"], "114514");
        assert_eq!(identified[USER_AGENT], "claude-cli/2.0.1");
    }

    /// The panel parses the id with `strconv.Atoi`, so a typo there comes back as a
    /// bare 401. Naming the field beats decoding that.
    #[test]
    fn a_non_numeric_user_id_is_rejected_before_the_request() {
        let error = account_request_headers("pat-abcdef123456", Some("user-7"), None)
            .expect_err("not a number");
        assert!(error.contains("用户 ID"), "{error}");
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
            account_level: false,
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
        format!("http://{}/v1", serve(app).await)
    }

    /// Binds the router on a loopback port and returns its `host:port`.
    async fn serve(app: Router) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind panel");
        let address = listener.local_addr().expect("panel address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve panel");
        });
        address.to_string()
    }

    /// A panel that serves its SPA shell — 200, but `text/html` — from a
    /// catch-all, the way every relay console does, and optionally the real JSON
    /// endpoint on `/v1/usage`. Returns the bare `host:port`, because these tests
    /// drive the candidate sweep directly rather than through a Base URL.
    async fn start_spa_panel(json_endpoint: Option<Value>) -> String {
        let mut app =
            Router::new().fallback(|| async { Html("<!doctype html><title>New API</title>") });
        if let Some(body) = json_endpoint {
            app = app.route(
                "/v1/usage",
                get(move || {
                    let body = body.clone();
                    async move { Json(body) }
                }),
            );
        }
        serve(app).await
    }

    /// Runs the candidate sweep with the headers a real query would carry.
    async fn sweep(candidates: &[String]) -> Result<(String, Value), AppError> {
        let client = build_outbound_http_client(Some(Duration::from_secs(BALANCE_TIMEOUT_SECS)))
            .expect("client");
        let headers = balance_request_headers("sk-relay-key", "openai", None).expect("headers");
        get_json_from_candidates(&client, &headers, candidates, "sk-relay-key").await
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
        seed_relay_credential_with_access_token(pool, base_url, relay_balance, None).await
    }

    /// Same, but the secret also carries the panel account's access token — what the
    /// form writes when the user fills that box. `user_id` rides along for the panels
    /// that demand a matching `New-Api-User` header.
    async fn seed_relay_credential_with_access_token(
        pool: &SqlitePool,
        base_url: &str,
        relay_balance: Option<Value>,
        access_token: Option<&str>,
    ) -> RouteCredential {
        seed_relay_credential_with_panel_account(pool, base_url, relay_balance, access_token, None)
            .await
    }

    async fn seed_relay_credential_with_panel_account(
        pool: &SqlitePool,
        base_url: &str,
        relay_balance: Option<Value>,
        access_token: Option<&str>,
        user_id: Option<&str>,
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
                relay_balance_access_token: access_token.map(str::to_string),
                relay_balance_access_token_user_id: user_id.map(str::to_string),
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

    /// A relay panel is a single-page app: its catch-all route answers 200 with
    /// `index.html` for every path it does not recognise. Reading that as a hard
    /// failure both mis-reports the reason ("the endpoint returned no JSON")
    /// and abandons the candidates behind it, so a panel whose real endpoint
    /// sits after an SPA route never gets asked at all.
    #[tokio::test]
    async fn a_non_json_200_falls_through_to_the_next_candidate() {
        let address = start_spa_panel(Some(json!({
            "isValid": true, "remaining": 4.25, "unit": "USD"
        })))
        .await;
        let candidates = vec![
            format!("http://{address}/usage"),
            format!("http://{address}/v1/usage"),
        ];

        let (url, body) = sweep(&candidates).await.expect("the JSON endpoint answers");
        assert!(url.ends_with("/v1/usage"), "{url}");
        assert_eq!(body["remaining"].as_f64(), Some(4.25));
    }

    /// When every candidate is the SPA, the detail has to say the body was not
    /// JSON — "所有余额查询地址都失败了" alone reads as a network problem, and the
    /// user's next move (switch provider, or use the custom slot) depends on
    /// knowing the panel answered with a web page.
    #[tokio::test]
    async fn a_sweep_that_only_finds_the_spa_says_the_body_was_not_json() {
        let address = start_spa_panel(None).await;
        let candidates = vec![
            format!("http://{address}/usage"),
            format!("http://{address}/v1/usage"),
        ];

        let error = sweep(&candidates).await.expect_err("no JSON anywhere");
        let AppError::Validation {
            code,
            details: Some(details),
            ..
        } = error
        else {
            panic!("expected a validation error with details");
        };
        assert_eq!(code, "validation.route_relay_balance_all_failed");
        assert!(details.contains("不是 JSON"), "{details}");
        assert!(details.contains("/v1/usage"), "{details}");
    }

    /// The batch path has no error channel, so whatever it puts in `message` is
    /// all the user gets. Keeping only `Display` would report "余额查询请求失败" for
    /// every kind of panel problem — the URL tried and the panel's own answer are
    /// the whole diagnosis.
    #[tokio::test]
    async fn a_batch_failure_keeps_the_url_and_the_panels_answer() {
        let address = start_spa_panel(None).await;
        let pool = memory_pool().await;
        seed_relay_credential(
            &pool,
            &format!("http://{address}"),
            Some(json!({"provider": "sub2api"})),
        )
        .await;

        let outcomes = RouteRelayBalanceService::refresh_platform(&pool, "codex".to_string())
            .await
            .expect("a per-account failure must not fail the batch");
        let [outcome] = outcomes.as_slice() else {
            panic!("expected exactly one outcome, got {}", outcomes.len());
        };
        assert_eq!(outcome.source, "error");
        let message = outcome.message.as_deref().expect("explains itself");
        assert!(message.contains("所有余额查询地址都失败了"), "{message}");
        assert!(message.contains("/usage"), "{message}");
        assert!(message.contains("不是 JSON"), "{message}");
    }

    /// The 2026-09-03 reading of the real account pool: three accounts set to
    /// `sub2api` were pointed at New API panels, so every query failed on a
    /// healthy key. Nothing in a Base URL says which panel software is behind it,
    /// so the setting is a guess — and the app can check the other zero-config
    /// dialect for free instead of reporting a failure.
    #[tokio::test]
    async fn a_panel_answering_the_other_dialect_is_read_anyway() {
        let base_url = start_panel(vec![("/api/usage/token/", new_api_usage_body())]).await;
        let pool = memory_pool().await;
        let credential =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "sub2api"}))).await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("the New API endpoint answers even though sub2api was selected");
        assert_eq!(outcome.source, "new_api", "the dialect that answered");

        let snapshot = snapshot_of(&outcome.credential);
        assert_eq!(snapshot.provider, RelayBalanceProvider::NewApi);
        assert_eq!(snapshot.remaining, Some(0.5));
        assert!(
            snapshot
                .notes
                .iter()
                .any(|note| note.contains("new-api") && note.contains("sub2api")),
            "the mismatch is stated, not silently corrected: {:?}",
            snapshot.notes
        );
    }

    /// A host that never answers is not evidence that the endpoint is elsewhere.
    /// The dialect fallback reuses the same per-request 15s reqwest limit — it is
    /// not a budget shared with the first round — so switching dialects on a dead
    /// host spends a fresh timeout per candidate to learn the same thing twice.
    #[tokio::test]
    async fn an_unreachable_panel_is_not_retried_under_the_other_dialect() {
        let pool = memory_pool().await;
        // Bind and drop, so the port is closed and connections are refused fast.
        let closed = {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
            listener.local_addr().expect("addr").to_string()
        };
        let credential = seed_relay_credential(
            &pool,
            &format!("http://{closed}/v1"),
            Some(json!({"provider": "sub2api"})),
        )
        .await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("nothing answered");
        let AppError::Validation { code, details, .. } = &error else {
            panic!("expected a validation error, got {error:?}");
        };
        assert_eq!(
            *code, "validation.route_relay_balance_unreachable",
            "a transport failure is its own class, not \"no address worked\""
        );
        let details = details.clone().unwrap_or_default();
        assert!(
            !details.contains("也试过"),
            "the other dialect must not have been swept: {details}"
        );
    }

    /// Keeping only the last candidate's reason pointed the user at a URL this
    /// module derives rather than one the panel ever advertised: for a Base URL
    /// that already ends in `/v1`, the last candidate is the double-prefix
    /// `…/v1/v1/…` address, and the answer the real path gave was discarded.
    #[tokio::test]
    async fn every_candidate_reports_its_own_reason() {
        let host = start_spa_panel(None).await;
        let first = format!("http://{host}/v1/usage");
        let second = format!("http://{host}/v1/v1/usage");

        let error = sweep(&[first.clone(), second.clone()])
            .await
            .expect_err("the SPA answers both");
        let AppError::Validation { details, .. } = &error else {
            panic!("expected a validation error, got {error:?}");
        };
        let details = details.clone().unwrap_or_default();
        assert!(
            details.contains(&first),
            "the meaningful candidate is missing: {details}"
        );
        assert!(
            details.contains(&second),
            "the derived candidate is missing: {details}"
        );
    }

    /// When the other dialect finds the endpoint and it answers 401, that is the
    /// real diagnosis. Reporting the selected dialect's "no address worked" and
    /// appending "the other one had nothing either" discards the answer and then
    /// contradicts it.
    #[tokio::test]
    async fn a_fallback_that_finds_a_rejected_key_reports_the_rejection() {
        let pool = memory_pool().await;
        let host = serve(Router::new().route(
            NEW_API_USAGE_PATH,
            get(|| async {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"message": "invalid token"})),
                )
            }),
        ))
        .await;
        // sub2api is selected but serves nothing here, so its sweep ends in
        // `all_failed` and the new-api address is tried next.
        let credential = seed_relay_credential(
            &pool,
            &format!("http://{host}/v1"),
            Some(json!({"provider": "sub2api"})),
        )
        .await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("the key is rejected");
        let AppError::Validation { code, details, .. } = &error else {
            panic!("expected a validation error, got {error:?}");
        };
        assert_eq!(*code, "validation.route_relay_balance_http");
        let details = details.clone().unwrap_or_default();
        assert!(
            details.contains("401"),
            "the upstream status is the diagnosis: {details}"
        );
        assert!(
            !details.contains("同样没有"),
            "the rejection must not be reported as an absent endpoint: {details}"
        );
    }

    #[tokio::test]
    async fn the_selected_dialect_wins_when_both_answer() {
        let base_url = start_panel(vec![
            ("/api/usage/token/", new_api_usage_body()),
            (
                "/v1/usage",
                json!({"isValid": true, "remaining": 37.7, "unit": "USD"}),
            ),
        ])
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
        assert!(snapshot.notes.is_empty(), "{:?}", snapshot.notes);
    }

    /// A rejected key means the endpoint is there and the dialect was right, so
    /// the other one must not be tried: it would answer 404 and replace "面板说这
    /// 把 key 不能用" with "找不到接口", which sends the user after the wrong thing.
    #[tokio::test]
    async fn a_rejected_key_is_not_retried_with_the_other_dialect() {
        let app = Router::new()
            .route(
                "/v1/usage",
                get(|| async {
                    (
                        HttpStatus::UNAUTHORIZED,
                        Json(json!({"error": "invalid api key"})),
                    )
                }),
            )
            // Available, and deliberately never reached.
            .route(
                "/api/usage/token/",
                get(|| async { Json(new_api_usage_body()) }),
            );
        let base_url = format!("http://{}/v1", serve(app).await);
        let pool = memory_pool().await;
        let credential =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "sub2api"}))).await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("a 401 is the answer, not a miss");
        assert_eq!(error.code(), "validation.route_relay_balance_http");
        assert!(error
            .details()
            .is_some_and(|details| details.contains("401")));
    }

    #[tokio::test]
    async fn a_panel_with_neither_dialect_says_both_were_tried() {
        let pool = memory_pool().await;
        let base_url = start_panel(vec![("/unrelated", json!({}))]).await;
        let credential =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "new_api"}))).await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("nothing answered");
        assert_eq!(error.code(), "validation.route_relay_balance_all_failed");
        let details = error.details().expect("explains itself");
        assert!(details.contains("sub2api"), "{details}");
    }

    /// The custom slot is a URL the user typed. Guessing a built-in endpoint
    /// after it fails would query an address they never asked for.
    #[tokio::test]
    async fn a_custom_endpoint_is_never_second_guessed() {
        let base_url = start_panel(vec![("/api/usage/token/", new_api_usage_body())]).await;
        let pool = memory_pool().await;
        let panel_root = base_url.trim_end_matches("/v1").to_string();
        let credential = seed_relay_credential(
            &pool,
            &base_url,
            Some(json!({
                "provider": "custom",
                "endpoint": format!("{panel_root}/billing/summary"),
                "remaining_path": "result.left",
            })),
        )
        .await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("the declared endpoint is absent");
        assert_eq!(error.code(), "validation.route_relay_balance_all_failed");
        let details = error.details().expect("explains itself");
        assert!(!details.contains("new-api"), "{details}");
    }

    /// A New API panel with both routes, each checking who is asking.
    ///
    /// The token-scoped route only accepts the relay key and the account-scoped one
    /// only accepts the panel access token, because "the account balance was fetched
    /// with the wrong credential" is the mistake these two endpoints invite, and a
    /// fixture that answers either header would not catch it. Passing `None` for a
    /// route leaves the panel without it, which is how the new-api forks that
    /// dropped token usage behave.
    ///
    /// `demands_user_id` mirrors New API through v0.13.2, which also refuses an
    /// access-token request that carries no matching `New-Api-User` header.
    async fn start_new_api_panel(token_usage: Option<Value>, account: Option<Value>) -> String {
        start_new_api_panel_with_user_id(token_usage, account, false).await
    }

    async fn start_new_api_panel_with_user_id(
        token_usage: Option<Value>,
        account: Option<Value>,
        demands_user_id: bool,
    ) -> String {
        let mut app = Router::new();
        if let Some(body) = token_usage {
            app = app.route(
                NEW_API_USAGE_PATH,
                get(move |headers: AxumHeaderMap| {
                    let body = body.clone();
                    async move {
                        answer_if_authorized(&headers, "Bearer sk-relay-key", body, "令牌无效")
                    }
                }),
            );
        }
        if let Some(body) = account {
            app = app.route(
                NEW_API_USER_SELF_PATH,
                get(move |headers: AxumHeaderMap| {
                    let body = body.clone();
                    async move {
                        if demands_user_id
                            && header_value(&headers, "New-Api-User") != Some(PANEL_USER_ID)
                        {
                            return Json(
                                json!({"success": false, "message": "无权访问此用户信息"}),
                            )
                            .into_response();
                        }
                        answer_if_authorized(
                            &headers,
                            &format!("Bearer {PANEL_ACCESS_TOKEN}"),
                            body,
                            "无权进行此操作，access token 无效",
                        )
                    }
                }),
            );
        }
        format!("http://{}/v1", serve(app).await)
    }

    /// The panel access token a test account stores. Deliberately unlike the relay
    /// key: New API hands these out from 个人设置 without an `sk-` prefix.
    const PANEL_ACCESS_TOKEN: &str = "pat-panel-account-token";
    const PANEL_USER_ID: &str = "7";

    fn header_value<'a>(headers: &'a AxumHeaderMap, name: &str) -> Option<&'a str> {
        headers.get(name).and_then(|value| value.to_str().ok())
    }

    fn answer_if_authorized(
        headers: &AxumHeaderMap,
        expected: &str,
        body: Value,
        refusal: &str,
    ) -> axum::response::Response {
        if header_value(headers, "authorization") != Some(expected) {
            // 200 with a false flag, the way new-api actually refuses.
            return Json(json!({"success": false, "message": refusal})).into_response();
        }
        Json(body).into_response()
    }

    /// What `/api/user/self` answers: `quota` is what is left, in the panel's
    /// integer unit, and there is no "granted" figure at all.
    fn new_api_account_body() -> Value {
        json!({
            "success": true,
            "message": "",
            "data": {
                "id": 7,
                "username": "pooluser",
                "group": "vip",
                "quota": 6_150_000,
                "used_quota": 3_850_000,
            }
        })
    }

    fn new_api_unlimited_usage_body() -> Value {
        json!({
            "code": true,
            "message": "ok",
            "data": {
                "object": "token_usage",
                "name": "pool key",
                "total_granted": 0,
                "total_used": 500_000,
                "total_available": 0,
                "unlimited_quota": true,
                "expires_at": 0,
            }
        })
    }

    /// The reading this whole path exists for: New API panels mostly hand out
    /// `unlimited_quota` keys, whose token-scoped usage has a spend and no balance.
    /// What runs out is the account behind the key, so that is what the badge has to
    /// show once the panel's access token is on file.
    #[tokio::test]
    async fn an_unlimited_key_reports_the_panel_accounts_balance() {
        let base_url = start_new_api_panel(
            Some(new_api_unlimited_usage_body()),
            Some(new_api_account_body()),
        )
        .await;
        let pool = memory_pool().await;
        let credential = seed_relay_credential_with_access_token(
            &pool,
            &base_url,
            Some(json!({"provider": "new_api"})),
            Some(PANEL_ACCESS_TOKEN),
        )
        .await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("refresh");
        assert_eq!(outcome.source, "new_api");

        let snapshot = snapshot_of(&outcome.credential);
        assert_eq!(snapshot.remaining, Some(12.3), "6150000 / 500000");
        assert_eq!(
            snapshot.used,
            Some(7.7),
            "the account's spend, not the key's"
        );
        assert_eq!(snapshot.limit, None, "the panel never reports an allowance");
        assert!(
            !snapshot.unlimited,
            "the account has a real number even though the key has no cap"
        );
        assert!(snapshot.account_level, "the figures are not this key's");
        assert_eq!(snapshot.plan_name.as_deref(), Some("vip"));
        assert!(snapshot.source_url.ends_with(NEW_API_USER_SELF_PATH));
        assert!(
            snapshot
                .notes
                .iter()
                .any(|note| note.contains("令牌不限额度")),
            "the badge has to say whose money it is: {:?}",
            snapshot.notes
        );
        assert!(
            snapshot.notes.iter().any(|note| note.contains("$1.00")),
            "the key's own spend is still worth keeping: {:?}",
            snapshot.notes
        );
    }

    /// Without the access token nothing changed: an unlimited key still reports only
    /// its spend, and the badge says so rather than inventing a balance.
    #[tokio::test]
    async fn an_unlimited_key_without_an_access_token_still_reads_as_unlimited() {
        let base_url = start_new_api_panel(
            Some(new_api_unlimited_usage_body()),
            Some(new_api_account_body()),
        )
        .await;
        let pool = memory_pool().await;
        let credential =
            seed_relay_credential(&pool, &base_url, Some(json!({"provider": "new_api"}))).await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("refresh");
        let snapshot = snapshot_of(&outcome.credential);
        assert!(snapshot.unlimited);
        assert!(!snapshot.account_level);
        assert_eq!(snapshot.remaining, None);
        assert_eq!(snapshot.used, Some(1.0), "500000 / 500000");
        assert!(
            snapshot
                .notes
                .iter()
                .any(|note| note.contains("面板访问令牌")),
            "the way out is named: {:?}",
            snapshot.notes
        );
    }

    /// A capped key answers its own question, so the account is left alone: the
    /// panel's balance is shared by every key on it, and reporting it for a key that
    /// has an allowance of its own would overstate what this account can spend.
    #[tokio::test]
    async fn a_capped_key_keeps_its_own_numbers_even_with_an_access_token() {
        let base_url =
            start_new_api_panel(Some(new_api_usage_body()), Some(new_api_account_body())).await;
        let pool = memory_pool().await;
        let credential = seed_relay_credential_with_access_token(
            &pool,
            &base_url,
            Some(json!({"provider": "new_api"})),
            Some(PANEL_ACCESS_TOKEN),
        )
        .await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("refresh");
        let snapshot = snapshot_of(&outcome.credential);
        assert!(!snapshot.account_level);
        assert_eq!(snapshot.remaining, Some(0.5), "250000 / 500000");
        assert_eq!(snapshot.limit, Some(1.0));
        assert!(snapshot.source_url.ends_with(NEW_API_USAGE_PATH));
    }

    /// The AR-style fork: `/api/usage/token/` was removed, so the account route is
    /// the only balance the panel serves. Worth one extra question when an access
    /// token is on file, because the alternative is reporting a healthy account as
    /// "查询失败".
    #[tokio::test]
    async fn a_panel_without_token_usage_falls_back_to_the_account_route() {
        let base_url = start_new_api_panel(None, Some(new_api_account_body())).await;
        let pool = memory_pool().await;
        let credential = seed_relay_credential_with_access_token(
            &pool,
            &base_url,
            Some(json!({"provider": "new_api"})),
            Some(PANEL_ACCESS_TOKEN),
        )
        .await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("the account route answers");
        let snapshot = snapshot_of(&outcome.credential);
        assert!(snapshot.account_level);
        assert_eq!(snapshot.remaining, Some(12.3));
        assert!(
            snapshot
                .notes
                .iter()
                .any(|note| note.contains("没有令牌级余额接口")),
            "an account-wide figure must not read as this key's: {:?}",
            snapshot.notes
        );
    }

    /// A rejected access token is a configuration mistake the user has to see. The
    /// tempting alternative — keep the token-scoped reading and note the failure —
    /// puts "余额 不限" back on the row, which is the exact reading this path was
    /// added to replace.
    #[tokio::test]
    async fn a_rejected_access_token_fails_the_query_instead_of_reading_as_unlimited() {
        let base_url = start_new_api_panel(
            Some(new_api_unlimited_usage_body()),
            Some(new_api_account_body()),
        )
        .await;
        let pool = memory_pool().await;
        let credential = seed_relay_credential_with_access_token(
            &pool,
            &base_url,
            Some(json!({"provider": "new_api"})),
            Some("pat-stale"),
        )
        .await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id.clone())
            .await
            .expect_err("the panel refused the access token");
        assert_eq!(error.code(), "validation.route_relay_balance_account_parse");
        let details = error.details().expect("explains itself");
        assert!(details.contains("access token 无效"), "{details}");
        assert!(
            details.contains(NEW_API_USER_SELF_PATH),
            "the address tried is part of the diagnosis: {details}"
        );
        // A failed query leaves the previous reading in place rather than writing a
        // blank one.
        let stored = RouteCredentialRepository::get(&pool, &credential.id)
            .await
            .expect("reload");
        assert!(RelayBalanceSnapshot::from_config_json(&stored.config_json).is_none());
    }

    /// The account route is reached only with the account's own token. Reusing the
    /// relay key would look like it works on panels that accept either, and this
    /// fixture refuses to.
    #[tokio::test]
    async fn the_account_route_is_not_queried_with_the_relay_key() {
        let base_url = start_new_api_panel(None, Some(new_api_account_body())).await;
        let pool = memory_pool().await;
        let credential = seed_relay_credential_with_access_token(
            &pool,
            &base_url,
            Some(json!({"provider": "new_api"})),
            Some("sk-relay-key"),
        )
        .await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("the relay key is not the account's token");
        assert!(error
            .details()
            .is_some_and(|details| details.contains("access token 无效")));
    }

    /// New API through v0.13.2 — every stable release — also insists on a
    /// `New-Api-User` header matching the token's owner.
    #[tokio::test]
    async fn a_panel_that_demands_a_user_id_is_answered_with_one() {
        let base_url = start_new_api_panel_with_user_id(
            Some(new_api_unlimited_usage_body()),
            Some(new_api_account_body()),
            true,
        )
        .await;
        let pool = memory_pool().await;
        let credential = seed_relay_credential_with_panel_account(
            &pool,
            &base_url,
            Some(json!({"provider": "new_api"})),
            Some(PANEL_ACCESS_TOKEN),
            Some(PANEL_USER_ID),
        )
        .await;

        let outcome = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect("refresh");
        let snapshot = snapshot_of(&outcome.credential);
        assert!(snapshot.account_level);
        assert_eq!(snapshot.remaining, Some(12.3));
    }

    /// Without the id the same panel refuses, in wording that never mentions the box
    /// the user has to fill. The failure has to name it, or the token looks broken.
    #[tokio::test]
    async fn a_refusal_without_a_user_id_points_at_the_missing_field() {
        let base_url = start_new_api_panel_with_user_id(
            Some(new_api_unlimited_usage_body()),
            Some(new_api_account_body()),
            true,
        )
        .await;
        let pool = memory_pool().await;
        let credential = seed_relay_credential_with_access_token(
            &pool,
            &base_url,
            Some(json!({"provider": "new_api"})),
            Some(PANEL_ACCESS_TOKEN),
        )
        .await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("the panel wants a user id");
        let details = error.details().expect("explains itself");
        assert!(details.contains("New-Api-User"), "{details}");
        assert!(details.contains("用户 ID"), "{details}");
    }

    /// With an id on file a refusal is about something else, so the hint stays out of
    /// the way instead of sending the user after a field they already filled.
    #[tokio::test]
    async fn a_refusal_with_a_user_id_on_file_keeps_the_panels_own_reason() {
        let base_url = start_new_api_panel(
            Some(new_api_unlimited_usage_body()),
            Some(new_api_account_body()),
        )
        .await;
        let pool = memory_pool().await;
        let credential = seed_relay_credential_with_panel_account(
            &pool,
            &base_url,
            Some(json!({"provider": "new_api"})),
            Some("pat-stale"),
            Some(PANEL_USER_ID),
        )
        .await;

        let error = RouteRelayBalanceService::refresh_one(&pool, credential.id)
            .await
            .expect_err("the access token is stale");
        let details = error.details().expect("explains itself");
        assert!(details.contains("access token 无效"), "{details}");
        assert!(!details.contains("New-Api-User"), "{details}");
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
