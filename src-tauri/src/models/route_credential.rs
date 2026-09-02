use crate::models::route_credential_model::RouteCredentialModelState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

pub const ANTHROPIC_API_KEY_FIELD: &str = "ANTHROPIC_API_KEY";
pub const ANTHROPIC_AUTH_TOKEN_FIELD: &str = "ANTHROPIC_AUTH_TOKEN";
pub const DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT: u32 = 2;
pub const DEFAULT_ROUTE_CREDENTIAL_RETRY_INTERVAL_MS: u32 = 200;
pub const DEFAULT_ROUTE_CREDENTIAL_SEMANTIC_ERROR_THRESHOLD: u32 = 10;
/// Transient-failure backoff is opt-in: a failing account stays immediately
/// selectable unless the user turns cooldown on for it.
pub const DEFAULT_ROUTE_CREDENTIAL_COOLDOWN_ENABLED: bool = false;
/// How long a failing account waits before it may be selected again. Kept
/// short on purpose: a single hiccup should cost seconds, not minutes, and
/// accounts that are really down keep re-triggering the same short window.
pub const DEFAULT_ROUTE_CREDENTIAL_COOLDOWN_SECONDS: u32 = 10;
/// Flipping an account to `error` stays on by default — the streak conditions
/// are strict enough that reaching them usually means the account is dead.
pub const DEFAULT_ROUTE_CREDENTIAL_ERROR_STATUS_ENABLED: bool = true;
/// Concurrency ceiling given to a freshly created account.
///
/// The column default is still 1 (changing it would mean rebuilding the table),
/// so every insert path binds this value explicitly.
pub const DEFAULT_ROUTE_CREDENTIAL_MAX_CONCURRENCY: i64 = 5;
pub const DEFAULT_ROUTE_CREDENTIAL_PRIORITY: i64 = 3;
pub const MAX_ROUTE_CREDENTIAL_RETRY_COUNT: u32 = 10;
pub const MAX_ROUTE_CREDENTIAL_RETRY_INTERVAL_MS: u32 = 60_000;
pub const MAX_ROUTE_CREDENTIAL_SEMANTIC_ERROR_THRESHOLD: u32 = 1_000;
pub const MAX_ROUTE_CREDENTIAL_COOLDOWN_SECONDS: u32 = 86_400;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RouteCredentialFailurePolicy {
    pub retry_count: u32,
    pub retry_interval_ms: u32,
    pub semantic_error_threshold: u32,
    pub cooldown_enabled: bool,
    pub cooldown_seconds: u32,
    pub error_status_enabled: bool,
}

impl Default for RouteCredentialFailurePolicy {
    fn default() -> Self {
        Self {
            retry_count: DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT,
            retry_interval_ms: DEFAULT_ROUTE_CREDENTIAL_RETRY_INTERVAL_MS,
            semantic_error_threshold: DEFAULT_ROUTE_CREDENTIAL_SEMANTIC_ERROR_THRESHOLD,
            cooldown_enabled: DEFAULT_ROUTE_CREDENTIAL_COOLDOWN_ENABLED,
            cooldown_seconds: DEFAULT_ROUTE_CREDENTIAL_COOLDOWN_SECONDS,
            error_status_enabled: DEFAULT_ROUTE_CREDENTIAL_ERROR_STATUS_ENABLED,
        }
    }
}

impl RouteCredentialFailurePolicy {
    pub fn from_config_json(config_json: &str) -> Self {
        serde_json::from_str::<Value>(config_json)
            .ok()
            .and_then(|config| Self::from_config_value(&config).ok())
            .unwrap_or_default()
    }

    pub fn from_config_value(config: &Value) -> Result<Self, String> {
        let Some(raw_policy) = config.get("failure_policy") else {
            return Ok(Self::default());
        };
        let policy = serde_json::from_value::<Self>(raw_policy.clone())
            .map_err(|error| format!("failure_policy must contain integer values: {error}"))?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.retry_count > MAX_ROUTE_CREDENTIAL_RETRY_COUNT {
            return Err(format!(
                "retry_count must be between 0 and {MAX_ROUTE_CREDENTIAL_RETRY_COUNT}"
            ));
        }
        if self.retry_interval_ms > MAX_ROUTE_CREDENTIAL_RETRY_INTERVAL_MS {
            return Err(format!(
                "retry_interval_ms must be between 0 and {MAX_ROUTE_CREDENTIAL_RETRY_INTERVAL_MS}"
            ));
        }
        if self.cooldown_seconds == 0
            || self.cooldown_seconds > MAX_ROUTE_CREDENTIAL_COOLDOWN_SECONDS
        {
            return Err(format!(
                "cooldown_seconds must be between 1 and {MAX_ROUTE_CREDENTIAL_COOLDOWN_SECONDS}"
            ));
        }
        if self.semantic_error_threshold == 0
            || self.semantic_error_threshold > MAX_ROUTE_CREDENTIAL_SEMANTIC_ERROR_THRESHOLD
        {
            return Err(format!(
                "semantic_error_threshold must be between 1 and {MAX_ROUTE_CREDENTIAL_SEMANTIC_ERROR_THRESHOLD}"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq)]
pub struct RouteCredential {
    pub id: String,
    pub platform: String,
    pub kind: String,
    pub display_name: String,
    pub email: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub route_priority: i64,
    pub max_concurrency: i64,
    pub batch_id: Option<String>,
    #[sqlx(default)]
    pub batch_name: Option<String>,
    pub secret_payload_json: String,
    pub config_json: String,
    pub preview_json: String,
    pub subscription_type: Option<String>,
    pub primary_remain: Option<i64>,
    pub weekly_remain: Option<i64>,
    pub reset_primary: Option<String>,
    pub reset_weekly: Option<String>,
    #[sqlx(default)]
    pub transient_failure_count: i64,
    #[sqlx(default)]
    pub next_retry_at: Option<String>,
    #[sqlx(default)]
    pub cooldown_until: Option<String>,
    #[sqlx(default)]
    pub last_failure_kind: Option<String>,
    #[sqlx(default)]
    pub last_failure_message: Option<String>,
    #[sqlx(default)]
    pub last_failure_response_json: Option<String>,
    #[sqlx(default)]
    pub active_request_count: i64,
    /// Per-model failure state, one entry per known model. Filled by the service
    /// layer from `route_credential_models` plus the account's mappings, so a
    /// healthy model the user may want to pause is listed too.
    #[sqlx(skip)]
    #[serde(default)]
    pub model_states: Vec<RouteCredentialModelState>,
    #[sqlx(default)]
    pub request_count: i64,
    #[sqlx(default)]
    pub success_count: i64,
    #[sqlx(default)]
    pub failure_count: i64,
    #[sqlx(default)]
    pub success_rate: Option<f64>,
    // Legacy single-window fields kept for existing DBs/migrations.
    pub quota_remaining: Option<i64>,
    pub quota_limit: Option<i64>,
    pub quota_used: Option<i64>,
    pub quota_updated_at: Option<String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Lightweight projection used by the auto-recovery scheduler to evaluate each
/// account's recovery rule without loading the full aggregate row.
#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct RecoveryCandidate {
    pub id: String,
    pub platform: String,
    pub status: String,
    pub config_json: String,
    pub next_retry_at: Option<String>,
    pub cooldown_until: Option<String>,
    /// 1 when the account has non-paused model rows. An account can look healthy
    /// at the account level while one of its models is parked, and that case must
    /// still reach the scheduler.
    pub has_model_failures: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateApiRouteCredentialInput {
    pub platform: String,
    pub display_name: String,
    pub api_key: String,
    pub base_url: String,
    pub interface_format: String,
    pub model_mappings_json: String, // JSON array
    #[serde(default)]
    pub fetched_models_json: Option<String>,
    #[serde(default)]
    pub api_key_field: Option<String>,
    pub preview_json: Option<String>,
    pub batch_id: Option<String>,
    #[serde(default)]
    pub responses_custom_tool_compat: Option<bool>,
    #[serde(default)]
    pub user_agent: Option<String>,
    /// Which relay panel dialect the account's balance is read with. Only the
    /// provider name is settable at creation time; the custom variant's endpoint
    /// and paths are configured from the edit drawer afterwards.
    #[serde(default)]
    pub relay_balance_provider: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CopyRouteCredentialInput {
    #[serde(default)]
    pub target_platform: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateRouteCredentialInput {
    pub display_name: String,
    pub email: Option<String>,
    pub status: String,
    pub route_priority: i64,
    pub max_concurrency: i64,
    pub secret_payload_json: String,
    pub config_json: String,
    pub preview_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportOfficialTextInput {
    pub platform: String,
    pub text: String,
    pub batch_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportOfficialFilesInput {
    pub platform: String,
    pub file_paths: Vec<String>,
    pub batch_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialImportFailure {
    pub label: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteCredentialImportResult {
    pub imported: Vec<RouteCredential>,
    pub failed: Vec<RouteCredentialImportFailure>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteCredentialPoolScope {
    InPool,
    OutOfPool,
    Archived,
}

impl Default for RouteCredentialPoolScope {
    fn default() -> Self {
        Self::OutOfPool
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialPageRequest {
    pub platform: String,
    pub page: i64,
    pub page_size: i64,
    #[serde(default)]
    pub filters: Vec<String>,
    #[serde(default)]
    pub pool_scope: RouteCredentialPoolScope,
}

impl RouteCredentialPageRequest {
    pub fn normalized_page_size(&self) -> Result<i64, String> {
        match self.page_size {
            20 | 50 | 100 => Ok(self.page_size),
            _ => Err("page_size must be 20, 50, or 100".to_string()),
        }
    }

    pub fn normalized_page(&self) -> i64 {
        self.page.max(1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialFilterOption {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteCredentialPage {
    pub items: Vec<RouteCredential>,
    pub total: i64,
    pub page: i64,
    pub page_count: i64,
    pub page_size: i64,
    pub previous_page_account_id: Option<String>,
    pub next_page_account_id: Option<String>,
    pub filter_options: Vec<RouteCredentialFilterOption>,
    pub official_account_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReorderRouteCredentialInput {
    pub platform: String,
    pub moved_account_id: String,
    pub previous_account_id: Option<String>,
    pub next_account_id: Option<String>,
    #[serde(default)]
    pub filters: Vec<String>,
    #[serde(default)]
    pub pool_scope: RouteCredentialPoolScope,
    pub page_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelMapping {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_1m: Option<bool>,
}

/// Catch-all `from` alias: the account accepts any requested model and rewrites
/// it to this entry's `to` when no specific mapping matched. Not a model id, so
/// it is never advertised. The routing code is platform-agnostic even though the
/// name reads Claude-specific — only the Claude editor offers the row.
///
/// It shares the `claude-` prefix with the real role aliases, which means
/// `claude_route_lookup_model` treats it as a Claude route model. That is inert
/// only because both `supports_requested_model` and `resolve_mapping_target`
/// test `is_fallback_mapping` before they ever reach `model_mapping_matches` —
/// drop either guard and the catch-all starts matching by name.
pub const FALLBACK_MODEL_ALIAS: &str = "claude-model";

/// Generic alias written into Claude Code's `CLAUDE_CODE_SUBAGENT_MODEL`. It is
/// a real routable alias — the pool rewrites it per account — so unlike the
/// fallback it *is* advertised.
pub const CLAUDE_SUBAGENT_MODEL_ALIAS: &str = "claude-subagent";

/// The four model slots Claude Code exposes in its `/model` menu. Each pins the
/// generic alias the client should request; the proxy then rewrites that alias
/// per account. Writing them makes the client→proxy contract explicit instead of
/// relying on Claude Code's built-in defaults happening to match our mapping
/// keys — a client-side version bump would otherwise strand every account.
pub const CLAUDE_MODEL_SLOTS: &[ClaudeModelSlot] = &[
    ClaudeModelSlot {
        alias: "claude-sonnet-alias",
        model_env_key: "ANTHROPIC_DEFAULT_SONNET_MODEL",
        name_env_key: "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
    },
    ClaudeModelSlot {
        alias: "claude-opus-alias",
        model_env_key: "ANTHROPIC_DEFAULT_OPUS_MODEL",
        name_env_key: "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
    },
    ClaudeModelSlot {
        alias: "claude-fable-alias",
        model_env_key: "ANTHROPIC_DEFAULT_FABLE_MODEL",
        name_env_key: "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
    },
    ClaudeModelSlot {
        alias: "claude-haiku-alias",
        model_env_key: "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        name_env_key: "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaudeModelSlot {
    pub alias: &'static str,
    pub model_env_key: &'static str,
    pub name_env_key: &'static str,
}

/// Suffix Claude Code appends to a model value to request the 1M context window.
pub const CLAUDE_ONE_M_SUFFIX: &str = "[1M]";

/// What to write for one `/model` slot. `None` for either field clears that key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeSlotWrite {
    /// Alias plus the `[1M]` suffix when every pool account supports it.
    pub model: Option<String>,
    /// Display name shown in the `/model` menu, when the pool agrees on one.
    pub display_name: Option<String>,
}

pub fn is_fallback_mapping(mapping: &ModelMapping) -> bool {
    mapping.from.trim() == FALLBACK_MODEL_ALIAS
}

/// Aliases this app invents rather than receives from a vendor. Official
/// credentials must never be routed one: their bodies are forwarded without
/// model rewriting, so the fake name would reach the vendor verbatim.
pub fn is_synthetic_route_alias(model: &str) -> bool {
    let model = model.trim();
    model == FALLBACK_MODEL_ALIAS || model == CLAUDE_SUBAGENT_MODEL_ALIAS
}

pub fn normalize_anthropic_api_key_field(value: Option<&str>) -> Result<&'static str, String> {
    match value.map(str::trim).filter(|item| !item.is_empty()) {
        None => Ok(ANTHROPIC_API_KEY_FIELD),
        Some(ANTHROPIC_API_KEY_FIELD) => Ok(ANTHROPIC_API_KEY_FIELD),
        Some(ANTHROPIC_AUTH_TOKEN_FIELD) => Ok(ANTHROPIC_AUTH_TOKEN_FIELD),
        Some(other) => Err(format!(
            "Unsupported Anthropic api_key_field: {other}. Expected {ANTHROPIC_API_KEY_FIELD} or {ANTHROPIC_AUTH_TOKEN_FIELD}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RouteCredentialFailurePolicy, DEFAULT_ROUTE_CREDENTIAL_COOLDOWN_SECONDS,
        DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT, DEFAULT_ROUTE_CREDENTIAL_RETRY_INTERVAL_MS,
        DEFAULT_ROUTE_CREDENTIAL_SEMANTIC_ERROR_THRESHOLD, MAX_ROUTE_CREDENTIAL_COOLDOWN_SECONDS,
    };
    use serde_json::json;

    #[test]
    fn failure_policy_uses_defaults_for_existing_account_configs() {
        assert_eq!(
            RouteCredentialFailurePolicy::from_config_json(r#"{"base_url":"https://example.com"}"#),
            RouteCredentialFailurePolicy::default()
        );
    }

    #[test]
    fn failure_policy_supports_partial_account_overrides() {
        let policy = RouteCredentialFailurePolicy::from_config_value(&json!({
            "failure_policy": { "retry_count": 4 }
        }))
        .expect("policy");

        assert_eq!(policy.retry_count, 4);
        assert_eq!(
            policy.retry_interval_ms,
            DEFAULT_ROUTE_CREDENTIAL_RETRY_INTERVAL_MS
        );
        assert_eq!(
            policy.semantic_error_threshold,
            DEFAULT_ROUTE_CREDENTIAL_SEMANTIC_ERROR_THRESHOLD
        );
        assert_ne!(policy.retry_count, DEFAULT_ROUTE_CREDENTIAL_RETRY_COUNT);
    }

    #[test]
    fn failure_policy_cooldown_seconds_defaults_to_ten_for_existing_configs() {
        let policy = RouteCredentialFailurePolicy::from_config_json(
            r#"{"failure_policy":{"retry_count":1}}"#,
        );

        assert_eq!(policy.cooldown_seconds, 10);
        assert_eq!(
            policy.cooldown_seconds,
            DEFAULT_ROUTE_CREDENTIAL_COOLDOWN_SECONDS
        );
    }

    #[test]
    fn failure_policy_keeps_configured_cooldown_seconds() {
        let policy = RouteCredentialFailurePolicy::from_config_value(&json!({
            "failure_policy": { "cooldown_seconds": 45 }
        }))
        .expect("policy");

        assert_eq!(policy.cooldown_seconds, 45);
    }

    #[test]
    fn failure_policy_rejects_cooldown_seconds_outside_safe_bounds() {
        let zero = RouteCredentialFailurePolicy::from_config_value(&json!({
            "failure_policy": { "cooldown_seconds": 0 }
        }))
        .expect_err("zero cooldown");
        assert!(zero.contains("cooldown_seconds"));

        let too_long = RouteCredentialFailurePolicy::from_config_value(&json!({
            "failure_policy": { "cooldown_seconds": MAX_ROUTE_CREDENTIAL_COOLDOWN_SECONDS + 1 }
        }))
        .expect_err("cooldown above ceiling");
        assert!(too_long.contains("cooldown_seconds"));
    }

    #[test]
    fn failure_policy_rejects_values_outside_safe_bounds() {
        let error = RouteCredentialFailurePolicy::from_config_value(&json!({
            "failure_policy": { "semantic_error_threshold": 0 }
        }))
        .expect_err("invalid policy");

        assert!(error.contains("semantic_error_threshold"));
    }
}
